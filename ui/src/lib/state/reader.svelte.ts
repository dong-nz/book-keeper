// The reader window store (SPEC-v0.0.6 §3.3). Per-window by construction:
// each reader webview is its own JS context and runs exactly one instance —
// no UI state crosses windows, and nothing here touches the DOM: geometry is
// exact (computed from the document's point sizes), never measured.

import { convertFileSrc } from "@tauri-apps/api/core";
import * as api from "../../api";
import { asAppError } from "../types";
import type { AppErrorPayload, OutlineEntry, PageTextDto, PdfDocDto } from "../types";
import { indexAt, stackOffsets } from "$lib/virtual";
import { debounce } from "../debounce";
import { settings } from "$lib/state/settings.svelte";

/** UI-30 fixed zoom steps (the ladder unchanged by DR-221): percentages
 *  relative to the page's actual size - 100 % is Actual size, the page's
 *  true dimensions (SPEC-v0.0.7 §3.16). */
export const ZOOM_STEPS = [50, 75, 100, 125, 150, 200, 300] as const;

/** The zoom state (UI-30 as re-anchored by DR-221): a fit mode - fit-width
 *  (the released default), fit-page (the whole page visible in the viewport),
 *  actual (100 % of the page's true size) - or one of the fixed percentages,
 *  relative to the page's actual size. */
export type ReaderZoomMode = "fit-width" | "fit-page" | "actual";
export type ReaderZoom = { mode: ReaderZoomMode } | { mode: "percent"; percent: number };

/** A page's CSS width at its actual size: points are 72/inch, CSS pixels
 *  96/inch - the released rendering math, now the percentage anchor
 *  (100 % = this width, AC-96). */
export function actualSizeWidth(widthPt: number): number {
  return (widthPt * 4) / 3;
}

/** What percentage of the page's actual size a display width represents -
 *  the value the − / + walk continues from when it leaves a fit mode
 *  (UI-30/DR-221). */
export function effectivePercent(displayWidth: number, widthPt: number): number {
  return (displayWidth / actualSizeWidth(widthPt)) * 100;
}
/** The page stack's gap (UI-30): the same 12 px the document views use
 *  (`--document-gap` / `CARD_GAP`), restated as a constant the same way. */
export const PAGE_GAP = 12;

/** One wheel notch (UI-39/DR-228): the keyboard counterpart of a wheel
 *  event — the distance a single notch scrolls (§3.6). */
export const SCROLL_NOTCH = 100;

/** The edge test of the DR-227 rule (UI-34/UI-39): the page step a delta
 *  produces at a reached content edge — +1 past the bottom edge, −1 past
 *  the top edge, 0 when the edge is not reached in the delta's direction.
 *  The 2 px margins are the container's rounding tolerance the wheel
 *  handler used; the helper is the single rule both callers share. */
export function edgeStep(
  delta: number,
  scrollTop: number,
  clientHeight: number,
  scrollHeight: number,
): number {
  const atBottom = scrollTop + clientHeight >= scrollHeight - 2;
  const atTop = scrollTop <= 2;
  if (delta > 0 && atBottom) return 1;
  if (delta < 0 && atTop) return -1;
  return 0;
}
/** `app_pdf_page`'s accepted request range (the command validates the same;
 *  mirroring it here keeps a rounding edge from producing a rejected call). */
export const REQUEST_WIDTH_MIN = 100;
export const REQUEST_WIDTH_MAX = 4000;
/** Image resolution cap: devicePixelRatio at most 2× (spec §3.3). */
export const MAX_PIXEL_RATIO = 2;
/** Fast-render cap (DR-151, preview width per DR-153): the preview stage
 *  requests `min(display width in CSS px, this cap)` — a raster at or below
 *  it displays in ≈ 150–200 ms on the reference machine. A larger device
 *  target is requested in two stages — preview first, full target replacing
 *  it in place; at devicePixelRatio 1 below the cap the preview equals the
 *  target and the request stays single-stage. */
export const PREVIEW_WIDTH_MAX = 1400;

/** DR-323: how long the preview stage stays under the loaded cross-fade
 *  overlay before `previewShown` releases it (the store's release timer).
 *  The overlay's own fade is 160 ms (ReaderView's transition); this window
 *  clears the preview well before a mounted page can be scrolled through,
 *  so the doubled-raster cost is bounded to a page-load tail and never a
 *  steady state. */
const PREVIEW_RELEASE_MS = 600;

export type ReaderStatus = "loading" | "ready" | "error";

// ------------------------------------------------- position memory (FR-28)

/** A decoded reading position: 1-based page + 0–1 fraction within it
 *  (SPEC-v0.0.12 §3.2 value grammar, OQ-72 A). */
export interface ReaderPosition {
  page: number;
  fraction: number;
}

/** The 0–1 clamp every position computation shares. */
export function clampFraction(value: number): number {
  return Number.isFinite(value) ? Math.min(1, Math.max(0, value)) : 0;
}

/** The position anchor key per reader kind (FR-28, §3.2): a PDF stores `p`
 *  (1-based page), an EPUB stores `c` (1-based spine index). */
export type ReaderPositionKey = "p" | "c";

/** Read a stored position value (both kinds' grammars — `{p,o}` for a PDF,
 *  `{c,o}` for an EPUB; exactly one anchor, the same grammar the Rust
 *  validator `positions::validate` enforces). A malformed value, two
 *  anchors, a non-integer page below 1 or an offset outside 0–1 reads as no
 *  position at all - the released default takes over. A page beyond the
 *  document's current extent is CLAMPED, not rejected (FR-28: a relink to
 *  a shorter file resumes at the last page). Pure - the unit tests named in
 *  §3.10 drive it directly. */
export function parseReaderPosition(raw: string | null, extent: number): ReaderPosition | null {
  if (raw === null || extent < 1) return null;
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return null;
  }
  if (typeof value !== "object" || value === null) return null;
  const record = value as Record<string, unknown>;
  const page = record.p;
  const chapter = record.c;
  if ((page !== undefined) === (chapter !== undefined)) return null; // exactly one anchor
  const anchor = page ?? chapter;
  const offset = record.o;
  if (typeof anchor !== "number" || !Number.isInteger(anchor) || anchor < 1) return null;
  if (typeof offset !== "number" || offset < 0 || offset > 1) return null;
  return { page: Math.min(anchor, extent), fraction: offset };
}

/** Encode the current position for `app_reader_position_set` (§3.2): the
 *  exact grammar with the kind's anchor key, offsets rounded to three
 *  decimals - a fraction beyond sub-pixel precision carries no reading
 *  meaning. */
export function encodeReaderPosition(
  page: number,
  fraction: number,
  key: ReaderPositionKey,
): string {
  return JSON.stringify({ [key]: page, o: Math.round(clampFraction(fraction) * 1000) / 1000 });
}

/** FR-28 write cadence: the debounced window (§3.2 "debounced, 1 s"),
 *  exported so the units can name it. */
export const POSITION_WRITE_MS = 1000;

// ------------------------------------------------ text layer mapping (FR-30)

/** One positioned run in CSS px (UI-54). */
export interface PositionedTextRun {
  x: number;
  y: number;
  w: number;
  h: number;
  text: string;
}

/** FR-30 (UI-54, §3.3): the page-pt → CSS-px mapping. The scale is the
 *  same ratio the page box itself is sized at (`pageWidth` is
 *  `width_pt × scale`), so the layer sits on the raster pixel for pixel;
 *  top-origin coordinates need no flip here (the Rust side already
 *  delivers them top-origin). Pure arithmetic on the store's exact
 *  geometry — re-derived on zoom and mode change, never measured. The
 *  §3.10 frontend units drive it directly. */
export function textPositions(dto: PageTextDto, displayWidth: number): PositionedTextRun[] {
  const scale = dto.page_width_pt > 0 ? displayWidth / dto.page_width_pt : 0;
  return dto.runs.map((run) => ({
    x: run.x * scale,
    y: run.y * scale,
    w: run.w * scale,
    h: run.h * scale,
    text: run.text,
  }));
}

/** View modes (UI-34, DR-180; UI-34 MODIFIED in v0.0.10 by DR-290 adds
 *  `horizontal` — pages in one left-to-right row, the continuous model on
 *  the horizontal axis, never the default): the store's `viewMode` is the
 *  single source for the layout, the strip control's active state, and the
 *  navigation semantics (DR-156's one-state-source discipline). `continuous`
 *  is the released default; pairs are consecutive in reading order (1|2, 3|4,
 *  …); the row is consecutive in reading order on the x-axis.
 *  Named apart from the document-area `ViewMode` in `lib/types.ts` — list
 *  versus grid is a different axis. */
export type ReaderViewMode = "single" | "continuous" | "facing" | "horizontal";

export interface PageImage {
  /** The CSS width this image was fetched at — a zoom change makes the
   *  entry stale and the view re-requests. */
  width: number;
  src: string;
  /** The full-resolution stage (DR-151/DR-153; the seam fix DR-316):
   *  null until the full raster's request resolves, and null whenever the
   *  two stages coincide (single-stage request). The view overlays it on
   *  the preview and cross-fades. DR-323: once the cross-fade has run,
   *  `previewShown` promotes the full stage to `src` and clears this field
   *  — the mounted page keeps one decoded raster, the v0.0.8 memory
   *  profile the AC-73/AC-75 budgets were calibrated against. */
  full: string | null;
}

/** The image request width for a CSS width: device pixels, capped 2×,
 *  clamped into the command's accepted range. Pure, like the rest of this
 *  module's math. */
export function imageRequestWidth(cssWidth: number, devicePixelRatio: number): number {
  const ratio = Math.min(Math.max(devicePixelRatio || 1, 1), MAX_PIXEL_RATIO);
  return Math.min(REQUEST_WIDTH_MAX, Math.max(REQUEST_WIDTH_MIN, Math.round(cssWidth * ratio)));
}

/** The next step of the − / + walk (UI-30 as re-anchored by DR-221): from a
 *  percentage - possibly a fit mode's effective one - the nearest ladder
 *  value in the delta direction (the snap-in AC-96 names); clamped at the
 *  ends, never a value outside the fixed list. */
export function stepZoom(current: number, delta: number): number {
  if (delta > 0) {
    for (const step of ZOOM_STEPS) if (step > current) return step;
    return ZOOM_STEPS[ZOOM_STEPS.length - 1];
  }
  if (delta < 0) {
    for (let i = ZOOM_STEPS.length - 1; i >= 0; i--) {
      if (ZOOM_STEPS[i] < current) return ZOOM_STEPS[i];
    }
    return ZOOM_STEPS[0];
  }
  // delta 0: the nearest ladder value to the current percentage.
  let nearest: number = ZOOM_STEPS[0];
  for (const step of ZOOM_STEPS) {
    if (Math.abs(step - current) < Math.abs(nearest - current)) nearest = step;
  }
  return nearest;
}

// ------------------------------------------------------------------ ToC tree

/** The flat depth-encoding's parent test (UI-33/§3.17): an entry is a parent
 *  exactly when the next entry is deeper. Pure — the PDF reader store and the
 *  EPUB reader store both call it through their own methods (DR-303 item 7),
 *  so the tree rule has exactly one implementation. */
export function tocParentsOf(outline: OutlineEntry[]): number[] {
  const parents: number[] = [];
  for (let i = 0; i < outline.length - 1; i++) {
    if (outline[i + 1].depth > outline[i].depth) parents.push(i);
  }
  return parents;
}

export function isTocParentOf(outline: OutlineEntry[], index: number): boolean {
  const entry = outline[index];
  const next = outline[index + 1];
  return entry !== undefined && next !== undefined && next.depth > entry.depth;
}

/** The entries the panel renders, in order, under the current collapse
 *  state: an entry is hidden when ANY ancestor is in the hidden set (the
 *  subtree rule - collapsing a parent hides everything below it until the
 *  next entry at its own level or shallower; a nested parent's own state
 *  never overrides a collapsed ancestor above it). */
export function tocVisibleOf(
  outline: OutlineEntry[],
  collapsed: ReadonlySet<number>,
): { entry: OutlineEntry; index: number }[] {
  const visible: { entry: OutlineEntry; index: number }[] = [];
  const ancestors: { depth: number; collapsed: boolean }[] = [];
  for (let i = 0; i < outline.length; i++) {
    const entry = outline[i];
    while (ancestors.length > 0 && ancestors[ancestors.length - 1].depth >= entry.depth) {
      ancestors.pop();
    }
    if (!ancestors.some((ancestor) => ancestor.collapsed)) {
      visible.push({ entry, index: i });
    }
    ancestors.push({ depth: entry.depth, collapsed: collapsed.has(i) });
  }
  return visible;
}

/** The structural shape `TocPanel` reads (DR-303 item 7): both reader stores
 *  satisfy it. `outline` is the flat depth-encoded tree; `goToPage` names the
 *  target an entry carries — a 1-based page in the PDF reader, a 1-based spine
 *  index in the EPUB reader (the manifest's ToC encoding, §3.9 consequence 8). */
export interface TocStore {
  outline: OutlineEntry[];
  tocCollapsed: ReadonlySet<number>;
  tocExpandMode: boolean;
  /** Whether the panel is pinned as a docked layout panel (FR-24/UI-49,
   *  SPEC-v0.0.10 §3.8): session state of the window's store, default false
   *  (OQ-60 closed → A, DR-327). */
  tocPinned: boolean;
  /** Whether the docked panel is shown (DR-337): the visibility state kept
   *  separate from the pin — the ToC control and `Esc` toggle it, the pin
   *  control owns the mode. Meaningful only while pinned; default false. */
  tocDockedOpen: boolean;
  tocVisible(): { entry: OutlineEntry; index: number }[];
  isTocParent(index: number): boolean;
  toggleTocEntry(index: number): void;
  tocToggleAll(): void;
  /** The single pin path (§3.8, revised by DR-337): the panel's pin control
   *  reaches it alone; it writes visibility with the mode (pin → shown,
   *  un-pin → closed). */
  setTocPinned(pinned: boolean): void;
  /** The visibility path (DR-337): the strip control in the pinned state
   *  and the docked panel's `Esc` write it; it never changes the pin. */
  setTocDockedOpen(open: boolean): void;
  goToPage(page: number): void;
}

export class ReaderStore {
  status = $state<ReaderStatus>("loading");
  error = $state<AppErrorPayload | null>(null);
  doc = $state<PdfDocDto | null>(null);
  /** The strip shows the document name (UI-30); the row read supplies it —
   *  `app_pdf_document` carries geometry only (§3.2), and the row is the
   *  existing whitelisted source of the filename. */
  documentName = $state("");
  zoom = $state<ReaderZoom>({ mode: "fit-width" });
  /** The selected view mode (UI-34): the single source for the layout in
   *  `ReaderView`, the strip control's active state, and the navigation
   *  semantics. Default `continuous` — the released behavior. */
  viewMode = $state<ReaderViewMode>("continuous");
  /** The document outline (FR-18): empty for documents without one — the
   *  strip then renders no ToC control (UI-33). Loaded with the geometry. */
  outline = $state<OutlineEntry[]>([]);
  /** The ToC collapse state (UI-33 as revised by DR-218): the outline indexes
   *  whose subtree is hidden. Initialized when the outline loads: every parent
   *  entry - the panel opens collapsed to the first level (the user's choice,
   *  over the agent's all-expanded recommendation). Session-scoped: this store
   *  lives exactly as long as the reader window (no persistence, OQ-45 closed
   *  as no-persistence). */
  tocCollapsed = $state<ReadonlySet<number>>(new Set());
  /** The toolbar toggle's mode (UI-33 as revised by DR-223): true = the
   *  control's next activation is expand-all, false = collapse-all.
   *  Flip-flop, per the user's decision: it alternates on every activation
   *  and is never touched by per-entry toggles. Initialized with the set —
   *  the tree opens collapsed, so the first action is expand-all. Same
   *  session scope as the set. */
  tocExpandMode = $state(true);
  /** Whether the ToC panel is pinned as a docked layout panel at the window's
   *  left edge (FR-24/UI-49, SPEC-v0.0.10 §3.8). The docked layout must be
   *  known to the view to re-lay out the content column, so the store — the
   *  window's single source of view state (DR-155) — carries it. Default
   *  `false`; per-window session scope, not inherited by new windows
   *  (OQ-60 closed → A, DR-327); no settings field. */
  tocPinned = $state(false);
  /** Whether the docked panel is shown (DR-337): visibility is separate from
   *  the pin — the strip control and `Esc` toggle this, the pin control owns
   *  the mode. Session scope like the pin; default `false`. */
  tocDockedOpen = $state(false);
  /** Single-page mode's mounted page (UI-34): in that mode the current
   *  page is index-driven, not scroll-driven; `goToPage` writes it. */
  singleIndex = $state(1);
  /** The direction of the last navigation (UI-34 as extended by DR-224):
   *  the sign of the last page delta; forward is the initial default. The
   *  single-mode prefetch window queues movement-direction-first from it. */
  lastNavDirection = $state(1);
  /** Mode-switch handoff (UI-34: "changing mode keeps `currentPage`"):
   *  switching to a scroll-driven mode records the page to land on and a
   *  nonce; `ReaderView` performs the jump once the new layout is mounted
   *  (the old scroller element is gone at the moment of the switch). */
  pendingReveal = 1;
  revealNonce = $state(0);
  /** Fit-width content width: the reader content box minus the two gutters.
   *  Set by the view on mount and on every resize — fit-width re-derives
   *  with the width (UI-30/AC-66). */
  fitWidth = $state(0);
  /** Fetched page images, keyed by 1-based page. Only viewport-near pages
   *  are ever requested (UI-30's mount rule, enforced by the view). */
  images = $state(new Map<number, PageImage>());
  /** FR-30 (UI-54, §3.3): each page's extracted text runs, keyed by 1-based
   *  page — extracted once per page per window session, coordinates stay
   *  page points; the display scaling is `TextLayer`'s arithmetic. An entry
   *  with an empty `runs` list is the no-layer case (a scan), cached like
   *  any other so the request never repeats. */
  texts = $state(new Map<number, PageTextDto>());

  readonly docId: number;
  /** Set by ReaderView: jump the scroll to an absolute stack offset. */
  onScrollTo: ((offsetPx: number) => void) | null = null;
  /** Set by ReaderView: shift the scroll by a pixel delta. */
  onScrollBy: ((deltaPx: number) => void) | null = null;
  /** Set by ReaderView (single mode): scroll one wheel notch — the view
   *  applies the DR-227 edge rule through `edgeStep` (UI-39/DR-228). */
  onNotch: ((deltaPx: number) => void) | null = null;
  /** Set by ReaderView on every measure: the scroller's viewport height —
   *  the distance one `PgUp`/`PgDn` travels (UI-30). */
  viewportHeight = $state(0);
  /** UI-34 MODIFIED (v0.0.10, DR-290): the horizontal mode's counterparts —
   *  the row's scroll position and viewport width, written by the view on
   *  every measure. The same DR-155 rule on the other axis: the page label
   *  and the mount window both derive from these, in one flush. */
  scrollLeft = $state(0);
  viewportWidth = $state(0);
  /** The scroller's position, written by ReaderView on every measure
   *  (DR-155): the single source both the page label and the mount window
   *  derive from, so they update in one flush and can never be read
   *  inconsistent. */
  scrollTop = $state(0);
  /** The page the reader is on (UI-30's page field): in continuous mode
   *  derived from the store's own scroll position — the one source the
   *  control strip's label and the view's mount window both read (DR-155);
   *  in single mode it is the mounted index (UI-34: index-driven); in
   *  facing mode it is the first page of the row whose span contains the
   *  viewport center (UI-34). Before the first measurement the geometry is
   *  degenerate and `pageAt` over a zero-height stack would name the last
   *  page, so the scroll-driven position stays page 1, where the scroller
   *  actually is (DR-154). */
  currentPage = $derived.by(() => {
    if (this.viewMode === "single") return this.singleIndex;
    if (this.fitWidth <= 0) return 1;
    if (this.viewMode === "facing") {
      const rows = this.rowOffsets();
      if (rows.length < 2) return 1;
      const center = this.scrollTop + this.viewportHeight / 2;
      return Math.min(2 * indexAt(rows, center) + 1, this.doc?.page_count ?? 1);
    }
    if (this.viewMode === "horizontal") {
      // UI-34 MODIFIED (v0.0.10): the page containing the viewport's
      // horizontal center — `pageAt` over the row's width-based offset
      // table, evaluated against the row position (DR-155 unchanged).
      return this.pageAt(this.rowXOffsets(), this.scrollLeft + this.viewportWidth / 2);
    }
    return this.pageAt(this.pageOffsets(), this.scrollTop);
  });
  /** Requests in flight, keyed by page with the width they were asked at:
   *  scrolling back and forth over a page must not fire the same request
   *  twice before the first one lands. */
  #pending = new Map<number, number>();
  #pendingText = new Set<number>();

  /** FR-28 (DR-406 1): the position read at load, applied once the view
   *  has real geometry (`applyRestoredPosition`). While it is pending the
   *  view requests and paints no page — the restored page is the first
   *  page painted, so the "no visible stop at page 1" rule holds by
   *  construction. Reactive so the view's request effect re-runs the
   *  moment the restore lands, even when the restore is a no-op scroll. */
  #restore: ReaderPosition | null = null;
  restorePending = $state(false);
  /** FR-28 (AC-168): the debounced position write (1 s, §3.2) — one write
   *  per settle, last value wins; `flushPosition` cancels it on close. */
  #writePosition = debounce(() => this.#writePositionNow(), POSITION_WRITE_MS);

  constructor(docId: number) {
    this.docId = docId;
  }

  /** `PgUp`/`PgDn` (UI-30; UI-34 semantics per mode): continuous scrolls
   *  one viewport through the view's scroller; single moves one page;
   *  facing moves one row (two pages); horizontal (v0.0.10, DR-290) moves
   *  one viewport along the row — the released viewport-step rule on the
   *  horizontal axis (UI-34 MODIFIED). */
  scrollViewport(pages: number): void {
    if (this.viewMode === "single") {
      this.goToPage(this.currentPage + pages);
      return;
    }
    if (this.viewMode === "facing") {
      this.goToPage(this.currentPage + 2 * pages);
      return;
    }
    if (this.viewMode === "horizontal") {
      this.onScrollBy?.(pages * this.viewportWidth);
      return;
    }
    this.onScrollBy?.(pages * this.viewportHeight);
  }

  /** Prev/next (UI-30; UI-34 semantics): one page, except facing mode,
   *  where the field always names a row's first page and the step is one
   *  row. The strip's buttons and nothing else call this. */
  stepPage(delta: number): void {
    this.goToPage(this.currentPage + (this.viewMode === "facing" ? 2 * delta : delta));
  }

  /** `ArrowDown`/`ArrowUp` (UI-39/DR-228; UI-39 MODIFIED v0.0.10 extends
   *  the mode list: in horizontal the notch scrolls the ROW — the faithful
   *  counterpart of the wheel, which scrolls the row). One wheel notch:
   *  continuous, horizontal and facing scroll through the view's scroll
   *  callback; single delegates to the view's notch handler, which applies
   *  the DR-227 edge rule (page step at a reached edge, container scroll
   *  otherwise). */
  scrollNotch(delta: number): void {
    if (this.viewMode === "single") {
      this.onNotch?.(delta);
      return;
    }
    this.onScrollBy?.(delta);
  }

  /** Outline indexes that have children - the flat depth encoding's parent
   *  test (UI-33/§3.17): an entry is a parent exactly when the next entry
   *  is deeper. */
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

  /** The single toolbar control (UI-33/DR-223): performs the action its
   *  mode names, then flips the mode. The action always applies to the
   *  whole tree; per-entry toggles never change the mode. */
  tocToggleAll(): void {
    if (this.tocExpandMode) this.tocExpandAll();
    else this.tocCollapseAll();
    this.tocExpandMode = !this.tocExpandMode;
  }

  /** The pin's single write path (UI-49/§3.8, revised by DR-337): the
   *  panel's pin control reaches it alone, and it writes the docked panel's
   *  visibility with the mode — pinning shows the panel, un-pinning closes
   *  it and returns to the released overlay behavior. */
  setTocPinned(pinned: boolean): void {
    this.tocPinned = pinned;
    this.tocDockedOpen = pinned;
  }

  /** The visibility path (DR-337): the strip control in the pinned state
   *  and the docked panel's `Esc` write it; it never changes the pin. */
  setTocDockedOpen(open: boolean): void {
    this.tocDockedOpen = open;
  }

  /** Switch view mode keeping the current page (UI-34): single mounts the
   *  kept page directly (index-driven); the scroll-driven modes record it
   *  and `ReaderView` scrolls there once the new layout is mounted — the
   *  old scroller element no longer exists at the moment of the switch. */
  setViewMode(mode: ReaderViewMode): void {
    if (mode === this.viewMode) return;
    const page = this.currentPage;
    this.viewMode = mode;
    // FR-29 (AC-169): the choice stays per-window (the released handoff
    // below is untouched); the remembered value is written through the
    // settings store's debounced batch, exactly as `view`/`theme` are.
    settings.setReaderViewMode(mode);
    if (mode === "single") {
      this.singleIndex = page;
      return;
    }
    this.pendingReveal = page;
    this.revealNonce++;
  }

  /** Load: the row for the strip's name, the geometry for everything else,
   *  and the outline after the geometry (FR-18/UI-33) — an empty list for
   *  documents without one. A typed failure — including a file deleted
   *  after this window opened — becomes the window's error state
   *  (§3.2/§3.3, AC-69). */
  async load(): Promise<void> {
    this.status = "loading";
    this.error = null;
    try {
      const row = await api.libraryGetDocument(this.docId);
      this.documentName = row.filename;
      this.doc = await api.appPdfDocument(this.docId);
      this.outline = await api.appPdfOutline(this.docId);
      // Default collapsed to the first level (UI-33/DR-218): every parent
      // starts in the hidden set; the toolbar toggle starts in expand mode
      // to match (DR-223).
      this.tocCollapsed = new Set(this.tocParents());
      this.tocExpandMode = true;
      // FR-29 (§3.2): the store is constructed from the remembered mode —
      // `settings.load()` completed before this call (ReaderApp's startup
      // order); the released default stands until the user first chooses.
      this.viewMode = settings.readerViewMode;
      // FR-28 (AC-167): the position is read before `ready` so the view
      // applies it on its first real geometry — no page is requested or
      // painted at page 1 first (DR-406 1). A failed read is the released
      // default; a stale page is clamped inside `parseReaderPosition`.
      try {
        const stored = await api.appReaderPositionGet(this.docId);
        this.#restore = parseReaderPosition(stored, this.doc.page_count);
      } catch {
        this.#restore = null;
      }
      this.restorePending = this.#restore !== null;
      this.status = "ready";
    } catch (error) {
      this.error = asAppError(error);
      this.status = "error";
    }
  }

  // ------------------------------------------------- position memory (FR-28)

  /** Apply the restored position (DR-406 1): the view calls this ONCE, in
   *  its first step that carries real geometry — the scroll target is the
   *  anchor table plus `o ×` the unit's computed display height (§3.2);
   *  facing applies the fraction within the pair row (DR-406 2), single
   *  mounts the page directly (index-driven, no scroll). A no-op restore
   *  (page 1, offset 0) still clears the flag — the view's request effect
   *  re-runs on the flip and the first page paints. */
  applyRestoredPosition(): void {
    if (this.status !== "ready" || !this.doc) return; // geometry-free step: nothing to apply against
    const restore = this.#restore;
    this.#restore = null;
    if (restore) {
      if (this.viewMode === "single") {
        this.singleIndex = restore.page;
      } else {
        const facing = this.viewMode === "facing";
        const horizontal = this.viewMode === "horizontal";
        const offsets = facing
          ? this.rowOffsets()
          : horizontal
            ? this.rowXOffsets()
            : this.pageOffsets();
        const index = facing ? Math.floor((restore.page - 1) / 2) : restore.page - 1;
        const unit = facing
          ? (this.rowHeights()[index] ?? 0)
          : horizontal
            ? this.pageWidth(restore.page)
            : (this.pageHeights()[restore.page - 1] ?? 0);
        const target = offsets[index] + 1 + restore.fraction * unit;
        // The store's own scroll source moves with the requested position
        // (DR-155): until the scroll event confirms it, the page label and
        // the mount window already name the RESTORED page — the first
        // requests belong to it, never to page 1.
        if (horizontal) this.scrollLeft = target;
        else this.scrollTop = target;
        this.onScrollTo?.(target);
      }
    }
    this.restorePending = false;
  }

  /** Schedule the debounced position write (DR-406 3): the view's `measure`
   *  and the store's own navigation calls reach it — the trailing debounce
   *  is the write discipline (AC-168: one write per settle, last value
   *  wins). A pending restore is not a reading position yet: writing it
   *  would re-store what was just read. */
  schedulePositionRecord(): void {
    if (this.status !== "ready" || this.restorePending) return;
    this.#writePosition();
  }

  /** The current position value in the §3.2 grammar, or null when there is
   *  nothing to record. The fractions come from the store's own sources —
   *  the scroll position over the computed offset tables (DR-155's single
   *  source), single mode from the container scroll against the mounted
   *  page's computed height (DR-406 5). No measurement, no new state. */
  positionValue(): string | null {
    const doc = this.doc;
    if (!doc || doc.page_count === 0 || this.status !== "ready") return null;
    const page = this.currentPage;
    let fraction = 0;
    if (this.viewMode === "single") {
      const height = this.pageHeights()[page - 1] ?? 0;
      const max = height - this.viewportHeight;
      fraction = max > 0 ? clampFraction(this.scrollTop / max) : 0;
    } else if (this.viewMode === "facing") {
      const row = Math.floor((page - 1) / 2);
      const height = this.rowHeights()[row] ?? 0;
      fraction = height > 0 ? clampFraction((this.scrollTop - (this.rowOffsets()[row] ?? 0)) / height) : 0;
    } else if (this.viewMode === "horizontal") {
      const width = this.pageWidth(page);
      fraction = width > 0 ? clampFraction((this.scrollLeft - (this.rowXOffsets()[page - 1] ?? 0)) / width) : 0;
    } else {
      const height = this.pageHeights()[page - 1] ?? 0;
      fraction = height > 0 ? clampFraction((this.scrollTop - (this.pageOffsets()[page - 1] ?? 0)) / height) : 0;
    }
    return encodeReaderPosition(page, fraction, "p");
  }

  #writePositionNow(): void {
    const value = this.positionValue();
    if (value === null) return;
    void api.appReaderPositionSet(this.docId, value).catch(() => {
      /* a lost position is not a user-facing error (the settings pattern);
         the next settle re-tries */
    });
  }

  /** Flush on window close (FR-28/AC-168): cancel the debounce and write
   *  the last value; the caller awaits it before the window closes. */
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

  // ------------------------------------------------------ text layer (FR-30)

  /** Request one page's text (§3.3): lazily, only for pages entering the
   *  render window, once per page per window session — the same request
   *  discipline as page images. A page without embedded text answers an
   *  empty run list and is cached like any other; a failed extraction is
   *  the no-layer case silently (a scan behaves exactly like today; the
   *  window's error state belongs to the page-image path). */
  requestText(page: number): void {
    if (this.status !== "ready") return;
    if (this.texts.has(page) || this.#pendingText.has(page)) return;
    this.#pendingText.add(page);
    api
      .appPdfText(this.docId, page)
      .then((text) => {
        const next = new Map(this.texts);
        next.set(page, text);
        this.texts = next;
      })
      .catch(() => {
        // No layer for this page (see above) - and the empty answer is
        // CACHED: extraction runs once per page per window session (§3.3),
        // a failure included, so the mount effect never re-asks.
        const pagePt = this.doc?.pages[page - 1];
        const next = new Map(this.texts);
        next.set(page, {
          runs: [],
          truncated: false,
          page_width_pt: pagePt?.width_pt ?? 0,
          page_height_pt: pagePt?.height_pt ?? 0,
        });
        this.texts = next;
      })
      .finally(() => this.#pendingText.delete(page));
  }

  /** A page's CSS width at the current zoom (UI-30 as re-anchored by
   *  DR-221/AC-96, §3.16): fit-width is the released fit base; fit-page
   *  fits the whole page into the viewport (the page's own aspect through
   *  the viewport height, capped at the fit base); actual is the page's
   *  true size (`width_pt × 4/3`); a fixed percent is `actual × percent`.
   *  Mixed-size documents keep per-page widths — the stack geometry already
   *  derives each page's height from its own aspect, so only the width
   *  anchor changes. In facing mode the fit base is one half of a pair row
   *  - (content width - 12 px) / 2 (UI-34). */
  pageWidth(page: number): number {
    const doc = this.doc;
    if (!doc || this.fitWidth <= 0) return 0;
    const geometry = doc.pages[page - 1];
    if (!geometry) return 0;
    const fit =
      this.viewMode === "facing"
        ? Math.max(0, (this.fitWidth - PAGE_GAP) / 2)
        : this.fitWidth;
    const zoom = this.zoom;
    if (zoom.mode === "fit-width") return fit;
    const actual = actualSizeWidth(geometry.width_pt);
    if (zoom.mode === "actual") return actual;
    if (zoom.mode === "percent") return (actual * zoom.percent) / 100;
    const byHeight =
      this.viewportHeight > 0 ? (this.viewportHeight * geometry.width_pt) / geometry.height_pt : 0;
    return Math.min(fit, byHeight);
  }

  /** Per-page CSS heights from the exact point geometry (§3.3): the aspect
   *  ratio is the document's own, so the stack's offsets are computable
   *  from data alone — the list's no-measurement rule, applied to pages. */
  pageHeights(): number[] {
    if (!this.doc) return [];
    return this.doc.pages.map(
      (page, index) => (this.pageWidth(index + 1) * page.height_pt) / page.width_pt,
    );
  }

  /** Stack prefix offsets including the gap — the input `window()` expects. */
  pageOffsets(): number[] {
    return stackOffsets(this.pageHeights(), PAGE_GAP);
  }

  /** Continuous horizontal row prefix offsets (UI-34 MODIFIED, v0.0.10,
   *  DR-290): the same `stackOffsets` rule on the x-axis — the pages'
   *  display widths at the current zoom, per-page for mixed-size documents
   *  (the same no-measurement rule `pageHeights` applies to the y-axis).
   *  `window()` is axis-agnostic; this is the table its x-axis use reads. */
  rowXOffsets(): number[] {
    if (!this.doc) return [];
    return stackOffsets(
      this.doc.pages.map((_, index) => this.pageWidth(index + 1)),
      PAGE_GAP,
    );
  }

  /** Facing-mode row heights (UI-34): a row's height is the taller of its
   *  pair; an unpaired last page is a row of one — the layout degrades
   *  without a special case. */
  rowHeights(): number[] {
    const heights = this.pageHeights();
    const rows: number[] = [];
    for (let index = 0; index < heights.length; index += 2) {
      rows.push(Math.max(heights[index], heights[index + 1] ?? heights[index]));
    }
    return rows;
  }

  /** Row prefix offsets — the mount window's input in facing mode; the
   *  row offset step is max(pair heights) + gap (UI-34). */
  rowOffsets(): number[] {
    return stackOffsets(this.rowHeights(), PAGE_GAP);
  }

  /** 1-based page index at a scroll position (the page field's value). */
  pageAt(offsets: number[], scrollTop: number): number {
    if (offsets.length < 2) return 1;
    return indexAt(offsets, scrollTop) + 1;
  }

  /** Jump to a 1-based page: clamped to the document, delivered through the
   *  view's scroller (UI-30: page field, prev/next, `Home`/`End`; the ToC
   *  panel's entries use the same path, UI-33). In single mode the jump
   *  changes the mounted index instead of scrolling (UI-34); in facing mode
   *  it lands on the row containing the page — the row's first page is the
   *  field's value (UI-34). */
  goToPage(page: number): void {
    if (!this.doc || this.doc.page_count === 0) return;
    const clamped = Math.min(Math.max(1, Math.trunc(page)), this.doc.page_count);
    // The movement direction for the single-mode prefetch window (DR-224):
    // the sign of this navigation's delta; a same-page jump keeps the
    // previous direction.
    const direction = Math.sign(clamped - this.currentPage);
    if (direction !== 0) this.lastNavDirection = direction;
    if (this.viewMode === "single") {
      this.singleIndex = clamped;
      // FR-28 (DR-406 3): in single mode a navigation may produce no scroll
      // event at all — the index change IS the position change.
      this.schedulePositionRecord();
      return;
    }
    const facing = this.viewMode === "facing";
    const horizontal = this.viewMode === "horizontal";
    const offsets = facing ? this.rowOffsets() : horizontal ? this.rowXOffsets() : this.pageOffsets();
    const index = facing ? Math.floor((clamped - 1) / 2) : clamped - 1;
    // +1 px (DR-148): the compositor snaps a scroll to device pixels, which
    // can land a fraction short of the page top — `pageAt` would then name
    // the previous page. One pixel inside the target page makes the jump
    // exact for any snap and is invisible to the user.
    this.onScrollTo?.(offsets[index] + 1);
    this.schedulePositionRecord();
  }

  /** D23 (v0.0.8 / AC-119): a zoom change is a layout change, and the
   *  released mode-switch handoff applies to it unchanged — the scroll-driven
   *  modes record the anchor page and `ReaderView` scrolls there once the
   *  new geometry is mounted. Call it BEFORE the zoom state changes: the
   *  geometry and scroll position that name the anchor are the ones the user
   *  is looking at. Normal case: the page at the viewport's top edge — the
   *  same page the page field names. Bottom-clamped case (the AC-119
   *  sequence): when the measured scroll sits at the maximum of the current
   *  geometry, the viewport's bottom edge names the anchor — the page the
   *  user is anchored to (the last page), not the partial page the top edge
   *  happens to show after the clamp. The anchor survives the whole
   *  fit → out → fit walk: the clamped reveal lands at the maximum, the
   *  next geometry change re-anchors from the page, not from the clamped
   *  position. Single mode needs nothing: its page is index-driven and
   *  stays mounted. */
  #keepZoomAnchor(): void {
    if (this.viewMode === "single") return;
    // v0.0.10 (DR-290): the horizontal mode runs the same rule on the
    // x-axis — the anchor is the page at the viewport's LEFT edge, and the
    // clamped case names the page at the RIGHT edge (the row's counterpart
    // of the bottom-clamped sequence).
    const horizontal = this.viewMode === "horizontal";
    const offsets =
      this.viewMode === "facing"
        ? this.rowOffsets()
        : horizontal
          ? this.rowXOffsets()
          : this.pageOffsets();
    const position = horizontal ? this.scrollLeft : this.scrollTop;
    const viewport = horizontal ? this.viewportWidth : this.viewportHeight;
    let page = this.currentPage;
    const maxScroll = Math.max(0, (offsets[offsets.length - 1] ?? 0) - viewport);
    if (offsets.length > 1 && position >= maxScroll - 2) {
      page = this.pageAt(offsets, position + viewport - 1);
    }
    this.pendingReveal = page;
    this.revealNonce++;
  }

  /** − / + between the fixed steps (UI-30 as re-anchored by DR-221). From a
   *  percent state the released walk over the ladder. From a fit mode the
   *  walk continues from that mode's effective percentage of the current
   *  page (AC-96: the nearest step in the pressed direction) - Fit Width on
   *  a wide page can therefore land above or below 100 %. */
  changeZoom(delta: number): void {
    const zoom = this.zoom;
    if (zoom.mode === "percent") {
      const percent = stepZoom(zoom.percent, delta);
      if (percent === zoom.percent) return; // clamped: no layout change, no re-anchor
      this.#keepZoomAnchor();
      this.zoom = { mode: "percent", percent };
      return;
    }
    const doc = this.doc;
    if (!doc || doc.pages.length === 0) return;
    const page = Math.min(Math.max(this.currentPage, 1), doc.pages.length);
    const effective = effectivePercent(this.pageWidth(page), doc.pages[page - 1].width_pt);
    this.#keepZoomAnchor();
    this.zoom = { mode: "percent", percent: stepZoom(effective, delta) };
  }

  /** The zoom select's fit-mode items (UI-30/DR-221): Fit width is the
   *  released default control, Fit page and Actual Size the new ones. */
  setZoomMode(mode: ReaderZoomMode): void {
    if (this.zoom.mode === mode) return; // unchanged geometry: no re-anchor
    this.#keepZoomAnchor();
    this.zoom = { mode };
  }

  /** The zoom select's ladder items - only listed percentages are ever set
   *  (AC-96: the select offers exactly the fixed ladder). */
  setZoomPercent(percent: number): void {
    if (this.zoom.mode === "percent" && this.zoom.percent === percent) return;
    this.#keepZoomAnchor();
    this.zoom = { mode: "percent", percent };
  }

  /** Request one page's image at the current width. A page whose entry was
   *  fetched at this width is not re-requested; a zoom change makes mounted
   *  pages stale and the view calls this again (NFR-08's ≤ 300 ms re-show:
   *  the Rust cache serves re-requests of a known width without rendering).
   *  Two stages above the fast-render cap (DR-151, preview width per
   *  DR-153): the preview at `min(CSS display width, PREVIEW_WIDTH_MAX)`
   *  lands and displays first, the full target then arrives as the
   *  cross-fade overlay (DR-316/DR-318) and is promoted to the base src
   *  when the fade completes (DR-323); when the preview equals the target
   *  this is a single request. */
  requestPage(page: number): void {
    if (this.status !== "ready") return;
    const width = this.pageWidth(page);
    const fetched = this.images.get(page);
    if (fetched && fetched.width === width) return;
    if (this.#pending.get(page) === width) return; // same request already in flight
    const target = imageRequestWidth(width, globalThis.devicePixelRatio ?? 1);
    // The preview is the CSS display width taken through the same range
    // clamp as the target (imageRequestWidth at ratio 1): before the first
    // fit-width measurement `width` is 0, and the raw value would be a
    // rejected request.
    const preview = Math.min(target, imageRequestWidth(width, 1), PREVIEW_WIDTH_MAX);
    this.#pending.set(page, width);
    // A stale stage completing late must not clear a newer request's guard.
    const settlePending = () => {
      if (this.#pending.get(page) === width) this.#pending.delete(page);
    };
    void api
      .appPdfPage(this.docId, page, preview)
      .then((path) => {
        this.#setImage(page, width, path, null);
        if (preview === target) {
          settlePending();
          return undefined;
        }
        // DR-319: superseded mid-flight (the fit changed) — the current-width
        // request re-renders; skip the pointless full render of this chain.
        if (this.pageWidth(page) !== width) return undefined;
        return api.appPdfPage(this.docId, page, target).then((full) => {
          // DR-316: the full stage is published as an overlay source, not
          // as a replacement of the preview src — the view cross-fades it.
          this.#setImage(page, width, path, full);
          settlePending();
        });
      })
      .catch((error: unknown) => {
        settlePending();
        // A failed page request on a typed error means the document is gone
        // or unopenable — the same error state the document load yields
        // (§3.2: a file deleted after the window opened yields the typed
        // error, which the window renders as its error state).
        this.error = asAppError(error);
        this.status = "error";
      });
  }

  /** DR-323: release the preview stage once the cross-fade has completed.
   *  The view calls this from the overlay's load event — the moment its
   *  fade begins. After `PREVIEW_RELEASE_MS` the full stage is promoted to
   *  the entry's `src` and the overlay source cleared: the base image
   *  re-points at the raster the overlay just showed (same URL, already
   *  decoded and cached — an attribute-only patch invisible under the
   *  opaque overlay), and the page's steady state is one decoded raster
   *  again — the v0.0.8 memory profile the AC-73/AC-75 budgets were
   *  calibrated against (the doubled held rasters of DR-318's always-on
   *  coexistence were the regression measured in the v0.0.9 evidence run).
   *  Guarded like `#setImage`: a chain superseded (zoom/fit change) or a
   *  different full stage published since the load event releases nothing
   *  — the captured URL must still be the entry's current full stage. */
  previewShown(page: number, width: number): void {
    const entry = this.images.get(page);
    if (!entry || entry.width !== width || entry.full === null) return;
    const full = entry.full;
    setTimeout(() => {
      const current = this.images.get(page);
      if (!current || current.width !== width || current.full !== full) return;
      const next = new Map(this.images);
      next.set(page, { width, src: full, full: null });
      this.images = next;
    }, PREVIEW_RELEASE_MS);
  }

  #setImage(page: number, width: number, path: string, full: string | null): void {
    // DR-319: a stage whose request was superseded (the fit changed while it
    // was in flight) never publishes — its width would not match what the
    // view mounts, and writing it would flip the mounted page back to the
    // skeleton until the current request lands (the open-time flash).
    if (this.pageWidth(page) !== width) return;
    const next = new Map(this.images);
    next.set(page, {
      width,
      src: convertFileSrc(path),
      full: full === null ? null : convertFileSrc(full),
    });
    this.images = next;
  }
}
