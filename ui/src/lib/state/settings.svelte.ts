// Settings store (spec §5.3.4 rule 7): view mode, sort (primary + secondary),
// grid column override, color ramp (UI-17) and last filter are read once at
// startup via app_get_settings and written via app_set_setting, debounced 300 ms.
// localStorage is never used (DR-44); a failed settings write is silent (a lost
// preference is not a user-facing error).

import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import * as api from "../../api";
import { debounce } from "../debounce";
import type { FilterState, GridOverride, Theme, ViewMode } from "../types";

/** Values restored from the settings table at startup. */
export interface RestoredQuery {
  sort: string;
  dir: string;
  sort_secondary: string;
  last_filter: string;
}

/** Manual grid counts are clamped to the range the View menu offers (UI-08). */
const GRID_MIN = 2;
const GRID_MAX = 10;

function parseGridColumns(value: string): GridOverride {
  const count = Number(value);
  return Number.isInteger(count) && count >= GRID_MIN && count <= GRID_MAX ? count : "auto";
}

/** The EPUB reading text-size ladder (SPEC-v0.0.9 §3.13, DR-303 item 5),
 *  restated here exactly as the View-menu clamp above restates the range it
 *  guards: the Rust validator (`is_valid_text_size`) is the authority, this
 *  only keeps a corrupt row from reaching the reader. */
const TEXT_SIZE_STEPS = [14, 16, 18, 20, 24, 28];
const DEFAULT_TEXT_SIZE = 18;

function parseTextSize(value: string): number {
  const size = Number(value);
  return TEXT_SIZE_STEPS.includes(size) ? size : DEFAULT_TEXT_SIZE;
}

/** The application log level (FR-26, SPEC-v0.0.11 §3.2): the same
 *  restatement pattern as the two validators above — the Rust side
 *  (`service::set_setting` + `applog::set_level`) is the authority; this
 *  only keeps a corrupt row from reaching the Log section. Anything that
 *  is not `info` reads as `error`, the released default. */
export type LogLevel = "error" | "info";

function parseLogLevel(value: string): LogLevel {
  return value === "info" ? "info" : "error";
}

/** The reader view mode (FR-29, SPEC-v0.0.12 §3.2): the four names as the
 *  Rust validator accepts them (`service::set_setting` is the authority);
 *  anything else reads as the released default (continuous, M2). Structurally
 *  the same union `reader.svelte.ts` defines as `ReaderViewMode` — the
 *  stores assign it directly, no import cycle. */
export type StoredReaderViewMode = "single" | "continuous" | "facing" | "horizontal";

function parseReaderViewMode(value: string): StoredReaderViewMode {
  return value === "single" || value === "facing" || value === "horizontal"
    ? value
    : "continuous";
}

/** The skipped update version (FR-25, SPEC-v0.0.13 §3.2): the same
 *  restatement pattern as the validators above — the Rust side
 *  (`service::is_valid_skipped_version`) is the authority; this only keeps
 *  a corrupt row from reaching the update check. The empty string means
 *  "nothing skipped", the released behavior for an absent key. */
function parseSkippedVersion(value: string): string {
  return value === "" || /^\d+\.\d+\.\d+$/.test(value) ? value : "";
}

class SettingsStore {
  view = $state<ViewMode>("list");
  gridColumns = $state<GridOverride>("auto");
  theme = $state<Theme>("system");
  /** The EPUB reading text size (§3.13): the ladder step the reader windows
   *  render at; only the EPUB strip writes it. */
  epubTextSize = $state<number>(DEFAULT_TEXT_SIZE);
  /** The application log level (UI-53): mirrored from the settings store;
   *  only the Settings *Log* section writes it. */
  logLevel = $state<LogLevel>("error");
  /** The reader view mode (FR-29): the mode new reader windows open in.
   *  Live reader windows are unaffected by later changes (per-window choice
   *  stands, UI-34 unchanged) — only a new window's store reads this. */
  readerViewMode = $state<StoredReaderViewMode>("continuous");
  /** The skipped update version (FR-25): the version a startup check must
   *  not offer again; only the update modal's *Skip this version* writes it. */
  updateSkippedVersion = $state("");
  loaded = $state(false);

  // Settings writes are debounced (R4.3) as one batch: a 300 ms window collects
  // every key changed in it and writes each distinct key once with its last
  // value. A query change touches sort, dir, sort_secondary and last_filter in
  // the same tick, and `debounce` is trailing-edge — one debounced function per
  // key would keep only the last call and silently drop the other three.
  #pending = new Map<string, string>();
  #flush = debounce(() => {
    void this.#writeBatch();
  }, 300);
  #unlisten: UnlistenFn[] = [];

  /** Subscribe to the settings broadcast (DR-230, design §3.18): every window
   *  re-applies the keys this store owns from the `settings_changed` event —
   *  the same subscription pattern the library events use. Must run before any
   *  command (the startup order in `App.svelte`). */
  async initEvents(): Promise<void> {
    this.#unlisten.push(
      await listen<{ key: string; value: string }>("settings_changed", (event) => {
        this.#applyRemote(event.payload.key, event.payload.value);
      }),
    );
  }

  /** Re-apply one broadcast key. The keys this store owns are the rendered
   *  preferences (view, grid columns, theme); the query keys are per-window
   *  browsing state read once at startup (spec §5.3.4 rule 7) and are not
   *  re-applied live. An echo of this window's own save re-applies the same
   *  value — idempotent, and it never writes back. */
  #applyRemote(key: string, value: string): void {
    switch (key) {
      case "view":
        this.view = value === "grid" ? "grid" : "list";
        break;
      case "grid_columns":
        this.gridColumns = parseGridColumns(value);
        break;
      case "theme":
        this.theme = value === "light" || value === "dark" ? value : "system";
        break;
      case "epub_text_size":
        this.epubTextSize = parseTextSize(value);
        break;
      case "log_level":
        this.logLevel = parseLogLevel(value);
        break;
      case "view_mode":
        // FR-29: re-applied like every owned key — it decides the next
        // window's start, never a live window's mode (the store's `viewMode`
        // is its own state, not a derivation of this).
        this.readerViewMode = parseReaderViewMode(value);
        break;
      case "update_skipped_version":
        this.updateSkippedVersion = parseSkippedVersion(value);
        break;
      default:
        break;
    }
  }

  async #writeBatch(): Promise<void> {
    const batch = [...this.#pending];
    this.#pending.clear();
    await Promise.all(
      batch.map(([key, value]) =>
        api.appSetSetting(key, value).catch(() => {
          /* silent by spec: a lost preference is not an error */
        }),
      ),
    );
  }

  /** True while a batch is still waiting — the window close has to wait for it
   *  (AC-28: a choice made and the window closed moments later is still the
   *  choice the next start reads). */
  hasPendingWrites(): boolean {
    return this.#pending.size > 0;
  }

  /** Writes everything pending now. */
  async flushNow(): Promise<void> {
    this.#flush.cancel();
    await this.#writeBatch();
  }

  #persist(key: string, value: string): void {
    this.#pending.set(key, value);
    this.#flush();
  }

  /** Startup read; undefined on failure (defaults stand). */
  async load(): Promise<RestoredQuery | undefined> {
    try {
      const settings = await api.appGetSettings();
      this.view = settings.view === "grid" ? "grid" : "list";
      this.gridColumns = parseGridColumns(settings.grid_columns);
      // The Rust side validates this key; anything unexpected is the released
      // behavior — follow the OS (UI-17).
      this.theme =
        settings.theme === "light" || settings.theme === "dark" ? settings.theme : "system";
      this.epubTextSize = parseTextSize(settings.epub_text_size);
      this.logLevel = parseLogLevel(settings.log_level);
      this.readerViewMode = parseReaderViewMode(settings.view_mode);
      this.updateSkippedVersion = parseSkippedVersion(settings.update_skipped_version);
      this.loaded = true;
      return {
        sort: settings.sort,
        dir: settings.dir,
        sort_secondary: settings.sort_secondary,
        last_filter: settings.last_filter,
      };
    } catch {
      return undefined;
    }
  }

  setView(view: ViewMode): void {
    this.view = view;
    this.#persist("view", view);
  }

  /** View menu Columns control (UI-08): "auto" or a clamped manual count. */
  setGridColumns(value: GridOverride): void {
    const count = value === "auto" ? "auto" : Math.min(GRID_MAX, Math.max(GRID_MIN, value));
    this.gridColumns = count;
    this.#persist("grid_columns", String(count));
  }

  /** Settings menu / Settings dialog theme control (UI-17). The ramp itself is
   *  applied by `App.svelte` from this value; the store only holds the choice. */
  setTheme(value: Theme): void {
    this.theme = value;
    this.#persist("theme", value);
  }

  /** The EPUB strip's text-size control (§3.13, DR-303 item 5): the only
   *  writer of this key. The value is a ladder step — the reader store walks
   *  the ladder, this store persists the choice globally and the
   *  `settings_changed` broadcast re-applies it in every reader window. */
  setEpubTextSize(value: number): void {
    const size = TEXT_SIZE_STEPS.includes(value) ? value : DEFAULT_TEXT_SIZE;
    this.epubTextSize = size;
    this.#persist("epub_text_size", String(size));
  }

  /** The reader strip's mode control (FR-29): the only writer of this key.
   *  The value is persisted through the released debounced batch exactly as
   *  `view`/`theme` are; the `settings_changed` broadcast re-applies it in
   *  every window, where it only decides the NEXT reader window's start. */
  setReaderViewMode(value: StoredReaderViewMode): void {
    const mode = parseReaderViewMode(value);
    this.readerViewMode = mode;
    this.#persist("view_mode", mode);
  }

  /** The update modal's *Skip this version* action (FR-25, SPEC-v0.0.13
   *  §3.2): the only writer of this key. Written immediately, not through
   *  the debounced batch — the skip has to survive a restart made right
   *  after it (AC-186's restart assertion); the released write path, the
   *  broadcast to the other windows, and the silent-failure rule are
   *  unchanged. */
  skipUpdateVersion(value: string): void {
    const version = parseSkippedVersion(value);
    this.updateSkippedVersion = version;
    this.#persist("update_skipped_version", version);
    void this.flushNow();
  }

  /** The Log section's level selector (UI-53, AC-156): written through the
   *  dedicated `log_level_set` command, not the debounced batch — the Rust
   *  side stores it through the settings store and broadcasts
   *  `settings_changed`, so the choice takes effect in every window without
   *  a restart and the local mirror rides the same broadcast as every other
   *  key (DR-230). A failed write is silent by the settings pattern; the
   *  boundary logs it (FR-26). */
  setLogLevel(value: LogLevel): void {
    this.logLevel = value;
    void api.logLevelSet(value).catch(() => {
      /* silent by spec: a lost preference is not a user-facing error */
    });
  }

  /** Called from the library store on every query change — the single
   *  settings-write path for sort and navigation selection. */
  rememberQuery(filter: FilterState): void {
    this.#persist("sort", filter.sort);
    this.#persist("dir", filter.dir);
    this.#persist("sort_secondary", filter.sort_secondary);
    this.#persist("last_filter", encodeFilter(filter));
  }
}

function encodeFilter(filter: FilterState): string {
  const mode = filter.favorites_only
    ? "favorites"
    : filter.missing_only
      ? "missing"
      : filter.reading_only
        ? "reading"
        : "all";
  // Grammar (spec §3.2 as extended by FR-19 and UI-42): the mode and the
  // tag set are encoded separately, because both may be active at once
  // (FR-02 as modified, DR-82). The current list token is `&tags=` (AC-113).
  return filter.tag_ids.length > 0 ? `${mode}&tags=${filter.tag_ids.join(",")}` : mode;
}

/** Decoded `last_filter`. A v0.0.1 value (`cat:<id>`) and a v0.0.7 value
 *  (`&cats=`) decode as the same tag set with no mode, so an existing saved
 *  filter survives the upgrade (AC-113); only new writes use `&tags=`. */
export interface RestoredFilter {
  mode: "all" | "favorites" | "missing" | "reading";
  tagIds: number[];
}

export function decodeFilter(encoded: string): RestoredFilter {
  const split = (token: string) => encoded.split(token);
  const [head, tags] = split("&tags=").length > 1 ? split("&tags=") : split("&cats=");
  const mode: RestoredFilter["mode"] =
    head === "favorites" || head === "missing" || head === "reading" ? head : "all";
  const raw =
    tags !== undefined ? tags : head.startsWith("cat:") ? head.slice(4) : "";
  const ids = raw
    .split(",")
    .map((part) => Number(part))
    .filter((id) => Number.isInteger(id) && id > 0);
  return { mode, tagIds: [...new Set(ids)].sort((a, b) => a - b) };
}

export const settings = new SettingsStore();
