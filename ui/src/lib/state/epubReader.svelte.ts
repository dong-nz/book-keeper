// The EPUB reader window store (SPEC-v0.0.9 §3.13, DR-303). Per-window by
// construction like `reader.svelte.ts` — one instance per reader webview, no
// UI state crosses windows. The difference is stated, not hidden: flow content
// has no data-derived geometry (the PDF no-measurement rule is not
// transportable to reflowable text), so chapter heights are measured once and
// kept; everything that CAN be computed from data — the position model from
// the zip's uncompressed entry sizes, chapter resolution, the mount window —
// is computed here, pure, like the released store's math.

import { indexAt, stackOffsets, window as mountWindow, type WindowRange } from "$lib/virtual";
import {
  PAGE_GAP,
  clampFraction,
  encodeReaderPosition,
  parseReaderPosition,
  POSITION_WRITE_MS,
  isTocParentOf,
  tocParentsOf,
  tocVisibleOf,
  type ReaderPosition,
  type ReaderStatus,
} from "$lib/state/reader.svelte";
import { debounce } from "../debounce";
import { settings } from "$lib/state/settings.svelte";
import * as api from "../../api";
import { asAppError, type AppErrorPayload, type EpubManifestDto, type OutlineEntry } from "../types";

/** The text-size ladder (§3.13): six px steps, default 18. The Rust validator
 *  (`is_valid_text_size`) is the authority on what persists; this is the walk
 *  the strip's − / + control performs. */
export const TEXT_SIZE_STEPS = [14, 16, 18, 20, 24, 28] as const;
export const DEFAULT_TEXT_SIZE = 18;

/** The next step of the − / + walk (the zoom pattern, DR-221): the nearest
 *  ladder value in the delta direction, clamped at the ends. */
export function textSizeStep(current: number, delta: number): number {
  const steps = TEXT_SIZE_STEPS as readonly number[];
  const at = steps.indexOf(current);
  if (at === -1) return DEFAULT_TEXT_SIZE;
  const next = at + Math.sign(delta);
  return steps[Math.min(Math.max(next, 0), steps.length - 1)];
}

/** The mount window's overscan for flow content: viewport ± 1 chapter —
 *  §3.12 rule 3's O(3-chapters) DOM bound, restated as the store's own
 *  constant the way `reader.svelte.ts` restates its windowing inputs. */
export const CHAPTER_OVERSCAN = 1;

/** A chapter's height is unknown until it is mounted (DR-303 item 3). An
 *  unmeasured chapter is estimated from its uncompressed entry size at
 *  0.03 px per byte — the 68ch column at the default step fits roughly
 *  600 bytes of XHTML per line-height — with a 100 px floor so a stub
 *  chapter still reads as a block. The estimate exists only to keep the
 *  scrollbar honest (NFR-01) until the first measurement replaces it. */
export const ESTIMATE_PX_PER_BYTE = 0.03;
export const MIN_ESTIMATE_PX = 100;

/** Chunked mounting (§3.12 rule 3, DR-303 item 4): chapters above this
 *  uncompressed size mount in parsed-node chunks. */
export const CHUNKED_MOUNT_BYTES = 200_000;
export const CHUNK_NODES = 250;

export function estimateHeight(size: number): number {
  return Math.max(MIN_ESTIMATE_PX, Math.round(size * ESTIMATE_PX_PER_BYTE));
}

/** Cumulative uncompressed spine-entry sizes → chapter start offsets in bytes
 *  and the book total (§3.13 position model: the sizes the archive already
 *  carries, computed once at open). Index-aligned with the spine. */
export function chapterStarts(spine: { size: number }[]): { starts: number[]; total: number } {
  const starts: number[] = [];
  let total = 0;
  for (const item of spine) {
    starts.push(total);
    total += item.size;
  }
  return { starts, total };
}

/** The chapter containing a book percent (the jump field's resolution,
 *  §3.13): the last chapter whose byte start is at or before the target.
 *  A zero-total book (no sizes declared) resolves to the first chapter. */
export function chapterForPercent(
  starts: number[],
  total: number,
  percent: number,
): number {
  if (starts.length === 0) return 0;
  if (total <= 0) return 0;
  const clamped = Math.min(Math.max(percent, 0), 100);
  const target = (total * clamped) / 100;
  let index = 0;
  for (let i = 0; i < starts.length; i++) {
    if (starts[i] <= target) index = i;
    else break;
  }
  return index;
}

/** The live readout value (§3.13): the chapter's slice of the percent mapped
 *  through the chapter's own scroll range — chapter start → chapter end
 *  interpolates within `size` bytes. `localFraction` is the scroll fraction
 *  inside the mounted chapter, 0..1. */
export function percentAt(
  starts: number[],
  sizes: number[],
  total: number,
  chapterIndex: number,
  localFraction: number,
): number {
  if (total <= 0) return 0;
  const fraction = Math.min(Math.max(localFraction, 0), 1);
  const bytes = starts[chapterIndex] + sizes[chapterIndex] * fraction;
  return Math.min(100, (bytes / total) * 100);
}

/** Where an in-content href points inside the book (§3.13 link routing):
 *  a protocol URL resolves to its spine chapter (the sanitizer's rewritten
 *  form carries the percent-encoded entry and keeps the fragment, DR-302);
 *  a bare `#fragment` stays in the current chapter; anything else is not
 *  internal. Returns a 0-based chapter index or null. */
export function resolveChapterHref(
  href: string,
  baseUrl: string,
  spine: { entry: string }[],
): { chapter: number | null; fragment: string } | null {
  // A bare `#fragment` carries no path at all — it stays in the current
  // chapter. Checked on the raw href: the split below removes the marker.
  if (href.startsWith("#")) {
    return { chapter: null, fragment: href.slice(1) };
  }
  const [target, fragment = ""] = splitFragment(href);
  if (!target.startsWith(baseUrl)) return null;
  let path: string;
  try {
    path = decodeURIComponent(target.slice(baseUrl.length));
  } catch {
    return null;
  }
  const index = spine.findIndex((item) => item.entry === path);
  if (index === -1) return null;
  return { chapter: index, fragment };
}

function splitFragment(href: string): [string, string | undefined] {
  const at = href.indexOf("#");
  return at === -1 ? [href, undefined] : [href.slice(0, at), href.slice(at + 1)];
}

/** The ToC entry that "labels" a chapter: the first entry targeting it
 *  (the chapter header line's label source, §3.13 — content only, no
 *  decoration). Null when nothing targets the chapter. */
export function tocLabelFor(toc: OutlineEntry[], chapterIndex0: number): string | null {
  const hit = toc.find((entry) => entry.page === chapterIndex0 + 1);
  return hit?.title ?? null;
}

/** The current-entry derivation for the panel highlight (§3.13): the last
 *  entry (document order — the nearest, deepest one) whose target chapter
 *  the reader has reached. -1 when the scroll sits before any entry. */
export function currentTocIndexFor(toc: OutlineEntry[], currentChapter1: number): number {
  let index = -1;
  for (let i = 0; i < toc.length; i++) {
    if (toc[i].page <= currentChapter1) index = i;
    else break;
  }
  return index;
}

export class EpubReaderStore {
  status = $state<ReaderStatus>("loading");
  error = $state<AppErrorPayload | null>(null);
  documentName = $state("");
  manifest = $state<EpubManifestDto | null>(null);
  /** The ToC tree in the released flat shape (`page` = 1-based spine index,
   *  §3.9 consequence 8). Empty for a package without one — the strip then
   *  renders no ToC control, the present-when-present rule (§3.13). */
  outline = $state<OutlineEntry[]>([]);
  /** The ToC collapse state: the released initialization and rules
   *  (DR-218/DR-223), through the shared tree functions (DR-303 item 7). */
  tocCollapsed = $state<ReadonlySet<number>>(new Set());
  tocExpandMode = $state(true);
  /** Whether the ToC panel is pinned as a docked layout panel at the window's
   *  left edge (UI-48 MODIFIED / UI-49, SPEC-v0.0.10 §3.8 — the same state
   *  the PDF store carries; the shared panel reads it through `TocStore`).
   *  Default `false`; per-window session scope, not inherited by new windows
   *  (OQ-60 closed → A, DR-327). */
  tocPinned = $state(false);
  /** Whether the docked panel is shown (DR-337): the same visibility state
   *  the PDF store carries — the strip control and `Esc` toggle it, the pin
   *  control owns the mode. Session scope like the pin; default `false`. */
  tocDockedOpen = $state(false);
  /** Chapter heights in px: 0 = never measured (the estimate is used),
   *  > 0 = measured and KEPT across recycling — a recycled chapter renders
   *  its header line inside a box of this height (DR-303 item 3). */
  heights = $state<number[]>([]);
  /** Fetched chapter documents (raw sanitized XHTML as the protocol serves
   *  it), keyed by 1-based spine index — the released `images` shape, the
   *  same copy-on-write (DR-303 item 2). */
  chapterText = $state(new Map<number, string>());
  /** Chapters whose fetch failed — rendered by the view's chapter error
   *  surface, not a window error (a failed chapter never takes the book
   *  down; a failed manifest does). */
  chapterErrors = $state(new Set<number>());
  /** The chapter's own first heading, reported by the view after parsing
   *  (§3.13 header-line fallback). Keyed by 1-based spine index. */
  chapterHeadings = $state(new Map<number, string>());

  readonly docId: number;
  /** Set by the view: jump the scroll to an absolute stack offset. */
  onScrollTo: ((offsetPx: number) => void) | null = null;
  /** Set by the view: shift the scroll by a pixel delta. */
  onScrollBy: ((deltaPx: number) => void) | null = null;
  /** Written by the view on every measure (DR-155): the single source the
   *  readout, the mount window, and the panel highlight all derive from. */
  scrollTop = $state(0);
  viewportHeight = $state(0);

  #pending = new Map<number, Promise<void>>();

  /** FR-28 (DR-406 1/4): the position read at load. The landing is applied
   *  by the view once the store has real geometry (`applyRestoredPosition`);
   *  while it is pending the view mounts ONLY the landing chapter (its text
   *  was fetched in `load`), so no other chapter is fetched or painted
   *  first — and the landing scrolls against a stack whose landing-chapter
   *  span has a real height under it, never one whose only content is a
   *  byte estimate that can be smaller than what it stands for.
   *  `#reanchor` is the exact second landing once a height arrives. */
  #restore: ReaderPosition | null = null;
  #reanchor: { index: number; fraction: number } | null = null;
  restorePending = $state(false);
  /** The 0-based spine index the pending landing targets (the view's mount
   *  window while `restorePending` is exactly this chapter). */
  get restoreTarget(): number {
    return Math.max(0, (this.#restore?.page ?? 1) - 1);
  }
  /** FR-28 write cadence (1 s, §3.2): one write per settle, last value
   *  wins; `flushPosition` cancels it on close. */
  #writePosition = debounce(() => this.#writePositionNow(), POSITION_WRITE_MS);

  constructor(docId: number) {
    this.docId = docId;
  }

  /** Load (the released current-page-first rule, §3.13 "First paint"): the
   *  row for the name, the manifest for everything else, and the FIRST
   *  spine item's document — the window reaches `ready` only when chapter 1
   *  is in hand, never "window first, content later". A typed manifest
   *  failure (malformed package, missing file) becomes the window's error
   *  state (AC-133). */
  async load(): Promise<void> {
    this.status = "loading";
    this.error = null;
    try {
      const row = await api.libraryGetDocument(this.docId);
      this.documentName = row.filename;
      const manifest = await api.appEpubManifest(this.docId);
      this.manifest = manifest;
      this.outline = manifest.toc;
      this.heights = manifest.spine.map(() => 0);
      this.tocCollapsed = new Set(tocParentsOf(this.outline));
      this.tocExpandMode = true;
      // FR-28 (AC-167): the position is read before `ready`; the view
      // applies it on its first measured step, before anything mounts
      // (DR-406 1). A failed read is the released default; a stale chapter
      // is clamped inside `parseReaderPosition`.
      try {
        const stored = await api.appReaderPositionGet(this.docId);
        this.#restore = parseReaderPosition(stored, manifest.spine.length);
      } catch {
        this.#restore = null;
      }
      this.restorePending = this.#restore !== null;
      // The released current-page-first rule (NFR-08/DR-150): the current
      // chapter IS the restored one when there is one — chapter 1 leads only
      // when it is the current chapter. `ready` still means "the current
      // chapter is in hand", never "window first, content later".
      await this.requestChapter(this.#restore?.page ?? 1);
      this.status = "ready";
    } catch (error) {
      this.error = asAppError(error);
      this.status = "error";
    }
  }

  /** Fetch one chapter document through the content protocol (DR-303 item 2),
   *  with the released request-dedup rule: scrolling back and forth never
   *  fires the same fetch twice before the first lands. A failed fetch marks
   *  the chapter for the view's chapter error surface. */
  requestChapter(index: number): Promise<void> {
    const existing = this.#pending.get(index);
    if (existing) return existing;
    const spine = this.manifest?.spine;
    const item = spine?.[index - 1];
    if (!item) return Promise.resolve();
    const url = `${this.manifest!.base_url}${encodeURI(item.entry)}`;
    const request = fetch(url)
      .then((response) => {
        if (!response.ok) {
          const failed = new Set(this.chapterErrors);
          failed.add(index);
          this.chapterErrors = failed;
          return;
        }
        return response.text().then((text) => {
          const next = new Map(this.chapterText);
          next.set(index, text);
          this.chapterText = next;
          const cleared = new Set(this.chapterErrors);
          cleared.delete(index);
          this.chapterErrors = cleared;
        });
      })
      .finally(() => {
        this.#pending.delete(index);
      });
    this.#pending.set(index, request);
    return request;
  }

  /** A chapter's current stack height: measured px, or the byte estimate
   *  until first mount (DR-303 item 3). 0-based index. */
  chapterHeight(index: number): number {
    const measured = this.heights[index];
    if (measured && measured > 0) return measured;
    return estimateHeight(this.manifest?.spine[index]?.size ?? 0);
  }

  /** Stack prefix offsets including the gap — the input `window()` expects,
   *  the same construction as the released `pageOffsets`. */
  chapterOffsets(): number[] {
    if (!this.manifest) return [];
    return stackOffsets(
      this.manifest.spine.map((_, index) => this.chapterHeight(index)),
      PAGE_GAP,
    );
  }

  /** The mount window (§3.12 rule 3): the chapters the viewport touches,
   *  plus one on each side; `padTop`/`padBottom` reproduce the full height
   *  so the scrollbar stays honest. */
  mountRange(): WindowRange {
    if (!this.manifest) return { start: 0, end: 0, padTop: 0, padBottom: 0, totalHeight: 0 };
    return mountWindow(
      this.scrollTop,
      this.viewportHeight,
      this.chapterOffsets(),
      CHAPTER_OVERSCAN,
    );
  }

  /** The chapter the reader is on: the top-edge rule, the released
   *  `pageAt` semantics (DR-155: derived from the store's own scroll
   *  source). 1-based. */
  get currentChapter(): number {
    const offsets = this.chapterOffsets();
    if (offsets.length < 2 || this.viewportHeight <= 0) return 1;
    return indexAt(offsets, this.scrollTop) + 1;
  }

  /** The live position readout (§3.13): the current chapter's byte slice
   *  interpolated by its own scroll range. 0..100. */
  get bookPercent(): number {
    const manifest = this.manifest;
    if (!manifest) return 0;
    const { starts, total } = chapterStarts(manifest.spine);
    const offsets = this.chapterOffsets();
    if (offsets.length < 2 || total <= 0) return 0;
    const index = indexAt(offsets, this.scrollTop);
    const height = this.chapterHeight(index);
    // End-of-book rule (§3.13: chapter end maps the chapter's slice end):
    // once the viewport bottom reaches the end of the last chapter's range
    // the whole book is on screen — the readout reads 100. (The stack's
    // bottom pad would otherwise cap the last chapter's own fraction below
    // 1 whenever the final chapter is shorter than the viewport.)
    if (
      index === manifest.spine.length - 1 &&
      this.scrollTop + this.viewportHeight >= offsets[index] + height
    ) {
      return 100;
    }
    const fraction = height > 0 ? (this.scrollTop - offsets[index]) / height : 0;
    return percentAt(
      starts,
      manifest.spine.map((item) => item.size),
      total,
      index,
      fraction,
    );
  }

  /** The header-line / readout label for a chapter (§3.13): the ToC label,
   *  else the chapter's own first heading the view reported, else null —
   *  "else none, content only"; the callers render the i18n chapter-number
   *  fallback where a label is REQUIRED (the readout). 1-based index. */
  chapterLabel(index1: number): string | null {
    return (
      tocLabelFor(this.outline, index1 - 1) ?? this.chapterHeadings.get(index1) ?? null
    );
  }

  /** Whether a chapter must mount in chunks (§3.12 rule 3). 0-based. */
  isChunked(index: number): boolean {
    return (this.manifest?.spine[index]?.size ?? 0) > CHUNKED_MOUNT_BYTES;
  }

  /** Record a measured chapter height (DR-303 item 3). Returns the delta
   *  and whether the change happened above the reader's place — the view
   *  applies the delta to the scroll when it did (lazy images must not
   *  move what the user is looking at). The delta is against the height
   *  the layout was ACTUALLY using — the byte estimate until the first
   *  measurement, the previous measurement after — so the first real
   *  height corrects the estimate, not zero. The reader's position is
   *  judged against the model BEFORE this height enters it: a change above
   *  the reader moves the chapter boundary under them, and judging after
   *  the update would silence the compensation that exists exactly for
   *  that shift — and read a waiting landing as a reader who left it. */
  recordHeight(index: number, height: number): { delta: number; aboveReader: boolean } {
    if (height <= 0) return { delta: 0, aboveReader: false };
    const current = this.chapterHeight(index);
    if (Math.abs(height - current) < 1) return { delta: 0, aboveReader: false };
    const readerIndex = indexAt(this.chapterOffsets(), this.scrollTop);
    const next = [...this.heights];
    next[index] = height;
    this.heights = next;
    // DR-406 4: the restored landing is exact at the first measurement of
    // its chapter — the absolute landing replaces the delta compensation for
    // that one measurement (the reader stays at `o` within the real height).
    // A landing the reader has already left is not re-anchored at all.
    const reanchor = this.#reanchor;
    if (reanchor) {
      if (reanchor.index === index && readerIndex === index) {
        this.#reanchor = null;
        const target = this.chapterOffsets()[index] + 1 + reanchor.fraction * height;
        this.scrollTop = target;
        this.onScrollTo?.(target);
        return { delta: 0, aboveReader: false };
      }
      if (readerIndex !== reanchor.index) this.#reanchor = null; // the reader left the landing
    }
    return { delta: height - current, aboveReader: index < readerIndex };
  }

  /** `PgUp`/`PgDn` (UI-30 semantics, flow version): one viewport through the
   *  view's scroller. */
  scrollViewport(viewports: number): void {
    this.onScrollBy?.(viewports * this.viewportHeight);
  }

  /** `ArrowDown`/`ArrowUp` (UI-39/DR-228): one wheel notch. */
  scrollNotch(delta: number): void {
    this.onScrollBy?.(delta);
  }

  /** `End` (UI-39 semantics, flow version): book end is the bottom of the
   *  flow, not the start of the last chapter — the view's scroller clamps
   *  the overshoot to its maximum. */
  scrollToEnd(): void {
    if (!this.manifest) return;
    this.onScrollTo?.(Number.MAX_SAFE_INTEGER);
  }

  /** Jump to a chapter (the ToC panel, percent jump, internal links,
   *  `Home`): clamped, landed at the chapter's start through the
   *  view's scroller — the released `goToPage` path, +1 px for the same
   *  DR-148 snap tolerance. 1-based spine index. */
  goToPage(spineIndex: number): void {
    const count = this.manifest?.spine.length ?? 0;
    if (count === 0) return;
    const clamped = Math.min(Math.max(1, Math.trunc(spineIndex)), count);
    const offsets = this.chapterOffsets();
    this.onScrollTo?.(offsets[clamped - 1] + 1);
  }

  /** The position control's jump field (§3.13): a percent resolves to the
   *  chapter containing it and lands at its start. */
  jumpToPercent(percent: number): void {
    if (!Number.isFinite(percent)) return;
    const { starts, total } = chapterStarts(this.manifest?.spine ?? []);
    this.goToPage(chapterForPercent(starts, total, percent) + 1);
  }

  /** Where an in-content href points (§3.13): resolves through the manifest.
   *  Returns null for anything the reader does not route (external links are
   *  the view's opener call). */
  resolveHref(href: string): { chapter: number | null; fragment: string } | null {
    if (!this.manifest) return null;
    return resolveChapterHref(href, this.manifest.base_url, this.manifest.spine);
  }

  // ------------------------------------------------- position memory (FR-28)

  /** Apply the restored position (DR-406 1/4): the view calls this once it
   *  has real geometry. The landing is at the target chapter's start plus
   *  `o ×` its CURRENT height — the byte estimate until the chapter is
   *  mounted and measured — and `recordHeight` re-anchors exactly once when
   *  the real height arrives. While `restorePending` the view mounts only
   *  the landing chapter (`restoreTarget`), so no other chapter is fetched
   *  or painted first. */
  applyRestoredPosition(): void {
    if (this.status !== "ready" || !this.manifest) return; // no geometry to land against
    const restore = this.#restore;
    if (!restore) return;
    this.#restore = null;
    const index = restore.page - 1;
    const target = this.chapterOffsets()[index] + 1 + restore.fraction * this.chapterHeight(index);
    this.scrollTop = target;
    this.onScrollTo?.(target);
    this.#reanchor = { index, fraction: restore.fraction };
    this.restorePending = false;
  }

  /** Schedule the debounced position write (DR-406 3): the view's `measure`
   *  is the only hook the flow model needs — every position change here is
   *  a scroll, and every scroll reaches `measure`. The trailing debounce is
   *  the write discipline (AC-168); a pending landing is not a position. */
  schedulePositionRecord(): void {
    if (this.status !== "ready" || this.restorePending) return;
    this.#writePosition();
  }

  /** The current position in the §3.2 `{c,o}` grammar, from the store's own
   *  sources (DR-155): the current chapter is `currentChapter`, the fraction
   *  is the scroll against its current height — the same arithmetic the
   *  readout already performs. */
  positionValue(): string | null {
    const manifest = this.manifest;
    if (!manifest || manifest.spine.length === 0 || this.status !== "ready") return null;
    const index = this.currentChapter - 1;
    const height = this.chapterHeight(index);
    const fraction = height > 0 ? clampFraction((this.scrollTop - this.chapterOffsets()[index]) / height) : 0;
    return encodeReaderPosition(index + 1, fraction, "c");
  }

  #writePositionNow(): void {
    const value = this.positionValue();
    if (value === null) return;
    void api.appReaderPositionSet(this.docId, value).catch(() => {
      /* a lost position is not a user-facing error (the settings pattern);
         the next settle re-tries */
    });
  }

  /** Flush on window close (FR-28/AC-168, DR-406 3): cancel the debounce
   *  and write the last value; the caller awaits it before the window
   *  closes. */
  flushPosition(): Promise<void> {
    this.#writePosition.cancel();
    const value = this.positionValue();
    if (value === null) return Promise.resolve();
    return api
      .appReaderPositionSet(this.docId, value)
      .then(() => undefined)
      .catch(() => {
        /* the last settle's value stands instead — stated in §3.2 */
      });
  }

  // ------------------------------------------------------------- ToC tree

  /** The panel's current-entry highlight (§3.13): derived from the store's
   *  scroll source, never a relayed copy. Outline index, -1 before any entry. */
  currentTocIndex(): number {
    return currentTocIndexFor(this.outline, this.currentChapter);
  }

  tocParents(): number[] {
    return tocParentsOf(this.outline);
  }

  isTocParent(index: number): boolean {
    return isTocParentOf(this.outline, index);
  }

  tocVisible(): { entry: OutlineEntry; index: number }[] {
    return tocVisibleOf(this.outline, this.tocCollapsed);
  }

  toggleTocEntry(index: number): void {
    if (!this.isTocParent(index)) return;
    const next = new Set(this.tocCollapsed);
    if (next.has(index)) next.delete(index);
    else next.add(index);
    this.tocCollapsed = next;
  }

  tocExpandAll(): void {
    this.tocCollapsed = new Set();
  }

  tocCollapseAll(): void {
    this.tocCollapsed = new Set(this.tocParents());
  }

  tocToggleAll(): void {
    if (this.tocExpandMode) this.tocExpandAll();
    else this.tocCollapseAll();
    this.tocExpandMode = !this.tocExpandMode;
  }

  /** The pin's single write path (UI-49/§3.8, revised by DR-337): the panel's
   *  pin control reaches it alone, and it writes the docked panel's visibility
   *  with the mode — the same rule as the PDF store (§3.8 EPUB host). */
  setTocPinned(pinned: boolean): void {
    this.tocPinned = pinned;
    this.tocDockedOpen = pinned;
  }

  /** The visibility path (DR-337): the strip control in the pinned state and
   *  the docked panel's `Esc` write it; it never changes the pin. */
  setTocDockedOpen(open: boolean): void {
    this.tocDockedOpen = open;
  }

  // ------------------------------------------------------------- text size

  /** The strip's − / + walk (the zoom pattern, DR-221): the ladder step is
   *  applied live through the settings store — the single source every reader
   *  window re-applies from the `settings_changed` broadcast (§3.9
   *  consequence 6, DR-303 item 5). */
  changeTextSize(delta: number): void {
    settings.setEpubTextSize(textSizeStep(settings.epubTextSize, delta));
  }
}
