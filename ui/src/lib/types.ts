// DTOs mirroring the Rust serde types one-to-one (spec §5.3.3; the
// tags/cover_path fields are the DR-51 extension, renamed by UI-42; the
// reading field is FR-19). Field names match the Rust serde output exactly
// (snake_case — the Rust structs carry no `rename_all`); only command
// ARGUMENTS use camelCase, per Tauri's own conversion (see api.ts).

export interface DocumentDto {
  id: number;
  path: string;
  filename: string;
  title: string;
  author: string;
  notes: string;
  favorite: boolean;
  size: number;
  added_at: number;
  missing: boolean;
  title_source: string;
  reading: boolean;
  tags: string[];
  cover_path: string;
}

export interface TagDto {
  id: number;
  name: string;
  count: number;
}

export interface CountsDto {
  total: number;
  favorites: number;
  missing: number;
  // Live count of the Reading collection (FR-19 / UI-43).
  reading: number;
}

export interface ImportResult {
  imported: number;
  skipped: string[];
}

export interface RescanResult {
  missing_found: number;
  restored: number;
}

export interface JobProgress {
  kind: string; // "import" | "rescan" | "cover" | "update" (UI-52: the update download, fed from the frontend)
  processed: number;
  total: number;
}

export interface SearchPage {
  rows: DocumentDto[];
  total: number;
}

export interface SettingsDto {
  view: string;
  sort: string;
  dir: string;
  /** Secondary sort key, `"none"` when unset (FR-13). */
  sort_secondary: string;
  /** Grid column override, `"auto"` or a manual count as a string (UI-08). */
  grid_columns: string;
  /** Color ramp override, `"system" | "light" | "dark"` (UI-17). */
  theme: string;
  last_filter: string;
  /** EPUB reading text size, one ladder step in px as a string
   *  (SPEC-v0.0.9 §3.9 consequence 6, DR-303 item 5). */
  epub_text_size: string;
  /** Application log level, `"error" | "info"` (FR-26, SPEC-v0.0.11 §3.2). */
  log_level: string;
  /** Reader view mode, one of the four mode names (FR-29, SPEC-v0.0.12
   *  §3.2): the mode new reader windows open in. */
  view_mode: string;
  /** Skipped update version, `x.y.z` or `""` — nothing skipped (FR-25,
   *  SPEC-v0.0.13 §3.2). An absent key reads as `""` at the mirror. */
  update_skipped_version: string;
}

export interface AboutDto {
  name: string;
  version: string;
}

/** `app_open_reader_window` bookkeeping (SPEC-v0.0.6 §3.2). */
export interface OpenedReaderDto {
  opened: "created" | "focused";
}

/** One page's geometry in points (72/inch) — the reader sizes each page at
 *  its exact aspect ratio (SPEC-v0.0.6 §3.3). */
export interface PdfPageDto {
  width_pt: number;
  height_pt: number;
}

/** `app_pdf_document` payload (SPEC-v0.0.6 §3.2). */
export interface PdfDocDto {
  page_count: number;
  pages: PdfPageDto[];
}

/** One outline entry of the reader table of contents (FR-18, SPEC-v0.0.7
 *  §3.5): `page` is 1-based, `depth` 1 is the top level — the
 *  `app_pdf_outline` payload. */
export interface OutlineEntry {
  title: string;
  page: number;
  depth: number;
}

/** One spine item as `app_epub_manifest` reports it (SPEC-v0.0.9 §3.9
 *  consequence 8, DR-301 item 2): `index` is 1-based reading order, `entry`
 *  the package path, `size` the uncompressed entry bytes the position model
 *  uses (0 for an entry missing from the archive). */
export interface EpubSpineItem {
  index: number;
  entry: string;
  media_type: string;
  size: number;
}

/** `app_epub_manifest` payload (SPEC-v0.0.9 §3.9): the reading facts of an
 *  EPUB. `toc` reuses the `OutlineEntry` shape with `page` carrying the
 *  1-based spine index; `base_url` is the content-protocol prefix the
 *  reader window builds chapter URLs from; `language`/`direction` are the
 *  package's declarations — null means the app default (LTR). */
export interface EpubManifestDto {
  spine: EpubSpineItem[];
  toc: OutlineEntry[];
  base_url: string;
  language: string | null;
  direction: string | null;
}

/** FR-30 (SPEC-v0.0.12 §3.3): one selectable text run in **top-origin
 *  page-point coordinates** — x/y is the run rect's top-left; the TextLayer
 *  scales them with the page's exact display geometry, never measuring. */
export interface TextRunDto {
  text: string;
  x: number;
  y: number;
  w: number;
  h: number;
}

/** `app_pdf_text` payload (SPEC-v0.0.12 §3.3): the runs, the 4 000-run cap
 *  flag, and the page size in points. An empty `runs` list means the page
 *  carries no text — the reader shows no layer. */
export interface PageTextDto {
  runs: TextRunDto[];
  truncated: boolean;
  page_width_pt: number;
  page_height_pt: number;
}

/** `app_cover_candidates` payload item (FR-31, AC-178): one EPUB manifest
 *  image the cover pipeline can decode — the pair is sent back verbatim as
 *  the `epub-image` choice. */
export interface CoverCandidateDto {
  entry: string;
  media_type: string;
}

/** The cover-choice value grammar (SPEC-v0.0.12 §3.8): a 0-based PDF page,
 *  or an EPUB manifest entry with its declared media type. Serialized to a
 *  string for `app_cover_choice`. */
export type CoverChoiceValue =
  | { kind: "page"; page: number }
  | { kind: "epub-image"; entry: string; media: string };

/** The reader-position value grammar (SPEC-v0.0.12 §3.2, OQ-69/OQ-72 A):
 *  a PDF carries a 1-based page plus a 0–1 offset within it; an EPUB a
 *  1-based spine index plus a 0–1 offset within the chapter. Written by the
 *  reader store, validated at the Rust storage boundary. */
export type ReaderPositionValue = { p: number; o: number } | { c: number; o: number };

export interface AppErrorPayload {
  kind: string;
  message: string;
  document_id: number | null;
  /** Only ever present on `password-required`: the stored password the
   *  reader unlock form pre-fills (FR-20, DR-266). */
  password?: string | null;
}

export type SortKey = "title" | "date_added" | "size";
export type SortDir = "asc" | "desc";
/** Secondary key (FR-13): the primary keys plus "none" = unset. */
export type SortSecondary = "none" | SortKey;
export type ViewMode = "list" | "grid";
/** Grid column count override (UI-08): "auto" = width-computed. */
export type GridOverride = "auto" | number;
/** Color ramp (UI-17): "system" follows the OS, the other two override it. */
export type Theme = "system" | "light" | "dark";

/** Current query state (spec §5.3.4 rule 1). `tag_ids` is the active tag
 *  set (FR-02 as modified, DR-82; renamed by UI-42): normalized ascending,
 *  deduplicated, and a document matches when it carries at least one of
 *  them. `reading_only` is the Reading view predicate (FR-19). */
export interface FilterState {
  q: string;
  tag_ids: number[];
  favorites_only: boolean;
  missing_only: boolean;
  reading_only: boolean;
  sort: SortKey;
  dir: SortDir;
  sort_secondary: SortSecondary;
}

/** True when any filter is active (drives empty-state variants). */
export function hasActiveFilter(f: FilterState): boolean {
  return (
    f.q !== "" || f.tag_ids.length > 0 || f.favorites_only || f.missing_only || f.reading_only
  );
}

/** Discriminates a rejected `invoke()` value (R4.6 turns these into toasts). */
export function asAppError(e: unknown): AppErrorPayload {
  if (typeof e === "object" && e !== null && typeof (e as AppErrorPayload).kind === "string") {
    return e as AppErrorPayload;
  }
  return { kind: "internal", message: String(e), document_id: null };
}

/** The two document kinds the app opens. The dispatch is the file's
 *  extension — the same rule the backend uses (import accepts only
 *  `.pdf`/`.epub`; the cover worker dispatches on the extension,
 *  `library/service.rs:404`). The reader window learns its kind from the
 *  route; the main window derives it from the row's filename (FR-31, §3.8). */
export type DocumentKind = "pdf" | "epub";

export function documentKind(filename: string): DocumentKind {
  return filename.toLowerCase().endsWith(".epub") ? "epub" : "pdf";
}
