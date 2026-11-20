// The reader's core invariants (SPEC-v0.0.6 §3.3, view-mode semantics per
// SPEC-v0.0.7 §3.6 including the DR-224 navigation direction, the continuous
// horizontal row per SPEC-v0.0.10 §3.2, the zoom re-anchor per §3.16 and the
// ToC tree per §3.5 including the DR-223 toolbar toggle): page geometry is
// exact math from the document's own point sizes, the zoom model anchors to
// the page's actual size and walks only the fixed steps, image requests stay
// inside the command's accepted range, a typed page failure becomes the
// window's error state, the view modes keep one state source and the current
// page across a mode switch, the ToC tree hides whole subtrees under the
// collapse set and its toolbar toggle alternates, and navigation records the
// direction the last movement took. These are the numbers AC-65/72/78/79/80/
// 96 and AC-143…146 measure in the real app; the math itself is proven here.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const {
  appPdfPage,
  appPdfOutline,
  appPdfDocument,
  libraryGetDocument,
  appSetSetting,
  appReaderPositionGet,
  appReaderPositionSet,
  appPdfText,
} = vi.hoisted(() => ({
  appPdfPage: vi.fn<(id: number, page: number, width: number) => Promise<string>>(),
  appPdfOutline: vi.fn<(id: number) => Promise<unknown>>(),
  appPdfDocument: vi.fn<(id: number) => Promise<unknown>>(),
  libraryGetDocument: vi.fn<(id: number) => Promise<unknown>>(),
  appSetSetting: vi.fn<(key: string, value: string) => Promise<void>>(() => Promise.resolve()),
  appReaderPositionGet: vi.fn<(id: number) => Promise<string | null>>(() => Promise.resolve(null)),
  appReaderPositionSet: vi.fn<(id: number, value: string) => Promise<void>>(() => Promise.resolve()),
  appPdfText: vi.fn<(id: number, page: number) => Promise<unknown>>(() =>
    Promise.resolve({ runs: [], truncated: false, page_width_pt: 612, page_height_pt: 792 }),
  ),
}));

vi.mock("../../api", () => ({
  libraryGetDocument,
  appPdfDocument,
  appPdfPage,
  appPdfOutline,
  appOpenDocument: vi.fn(),
  appSetSetting,
  appReaderPositionGet,
  appReaderPositionSet,
  appPdfText,
}));
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (path: string) => `asset:///${path}`,
}));

import {
  PAGE_GAP,
  ReaderStore,
  REQUEST_WIDTH_MAX,
  REQUEST_WIDTH_MIN,
  SCROLL_NOTCH,
  ZOOM_STEPS,
  actualSizeWidth,
  edgeStep,
  encodeReaderPosition,
  imageRequestWidth,
  parseReaderPosition,
  POSITION_WRITE_MS,
  stepZoom,
  textPositions,
} from "./reader.svelte";
import { settings } from "./settings.svelte";
import type { OutlineEntry, PageTextDto, PdfDocDto } from "../types";

/** Two letter-size pages (612×792 pt) and one landscape page (792×612 pt)
 *  — the real fixture geometry (the multi-* fixtures are letter MediaBox). */
const doc: PdfDocDto = {
  page_count: 3,
  pages: [
    { width_pt: 612, height_pt: 792 },
    { width_pt: 612, height_pt: 792 },
    { width_pt: 792, height_pt: 612 },
  ],
};

function storeAt(fitWidth: number): ReaderStore {
  const store = new ReaderStore(1);
  store.doc = doc;
  store.status = "ready";
  store.fitWidth = fitWidth;
  return store;
}

describe("page geometry (exact, never measured)", () => {
  it("fit-width renders every page at the content width and its own aspect ratio", () => {
    const store = storeAt(816); // 612 pt × 4/3 — the fit base here equals actual size
    expect(store.zoom).toEqual({ mode: "fit-width" }); // the default is the fit mode (AC-96)
    expect(store.pageWidth(1)).toBeCloseTo(816, 6);
    const heights = store.pageHeights();
    expect(heights[0]).toBeCloseTo((816 * 792) / 612, 6);
    expect(heights[2]).toBeCloseTo((816 * 612) / 792, 6);
  });

  it("a fixed percent renders the page's actual size × percent — per page, in every mode (AC-96)", () => {
    const store = storeAt(816);
    for (const percent of ZOOM_STEPS) {
      store.setZoomPercent(percent);
      expect(store.pageWidth(1)).toBeCloseTo(816 * (percent / 100), 6); // 612 pt × 4/3 = 816
      expect(store.pageWidth(3)).toBeCloseTo(1056 * (percent / 100), 6); // 792 pt × 4/3 = 1 056
    }
  });

  it("Actual Size renders each page at its true dimensions (AC-96)", () => {
    const store = storeAt(816);
    store.setZoomMode("actual");
    expect(store.pageWidth(1)).toBeCloseTo(actualSizeWidth(612), 6);
    expect(store.pageWidth(3)).toBeCloseTo(actualSizeWidth(792), 6);
    // The aspect is the document's own, so the height is the true height too.
    expect(store.pageHeights()[2]).toBeCloseTo(actualSizeWidth(792) * (612 / 792), 6);
  });

  it("Fit Page fits the whole page into the viewport, capped at the fit base (AC-96)", () => {
    const store = storeAt(816);
    store.viewportHeight = 600;
    store.setZoomMode("fit-page");
    // Height-bound: the page renders exactly the viewport height.
    expect(store.pageWidth(1)).toBeCloseTo((600 * 612) / 792, 6);
    expect(store.pageHeights()[0]).toBeCloseTo(600, 6);
    expect(store.pageWidth(3)).toBeCloseTo((600 * 792) / 612, 6); // 776.5 — under the 816 base
    // A viewport taller than every page: width-bound at the fit base.
    store.viewportHeight = 1200;
    expect(store.pageWidth(1)).toBeCloseTo(816, 6);
  });

  it("resizing re-derives fit-width — the fit mode follows it, never an absolute width", () => {
    const store = storeAt(816);
    const at816 = store.pageHeights()[0];
    store.fitWidth = 1200; // the window was widened
    expect(store.pageWidth(1)).toBeCloseTo(1200, 6);
    expect(store.pageHeights()[0]).toBeCloseTo((1200 * 792) / 612, 6);
    expect(store.pageHeights()[0]).toBeGreaterThan(at816);
  });

  it("the stack offsets carry the gap and the total height, and pages resolve at their tops", () => {
    const store = storeAt(816);
    const heights = store.pageHeights();
    const offsets = store.pageOffsets();
    expect(offsets[0]).toBe(0);
    expect(offsets[1]).toBeCloseTo(heights[0] + PAGE_GAP, 6);
    expect(offsets[3]).toBeCloseTo(heights[0] + heights[1] + heights[2] + 2 * PAGE_GAP, 6);
    expect(offsets.length).toBe(doc.page_count + 1);
    // 1-based page at a scroll position: the page whose top the position touches.
    expect(store.pageAt(offsets, 0)).toBe(1);
    expect(store.pageAt(offsets, offsets[1])).toBe(2);
    expect(store.pageAt(offsets, offsets[3] - 1)).toBe(3);
  });
});

describe("navigation", () => {
  it("jumps land one pixel inside the target page's stack offset", () => {
    const store = storeAt(816);
    const delivered: number[] = [];
    store.onScrollTo = (offset) => delivered.push(offset);
    store.goToPage(2);
    // +1 px inside the page (DR-148): sub-pixel scroll snapping must not
    // land a fraction short of the page top.
    expect(delivered).toEqual([store.pageOffsets()[1] + 1]);
  });

  it("targets outside the document clamp to its first and last page", () => {
    const store = storeAt(816);
    const delivered: number[] = [];
    store.onScrollTo = (offset) => delivered.push(offset);
    store.goToPage(0);
    store.goToPage(-5);
    store.goToPage(doc.page_count + 10);
    const offsets = store.pageOffsets();
    expect(delivered).toEqual([1, 1, offsets[doc.page_count - 1] + 1]);
  });

  it("a page jump with no scroller attached does not fail", () => {
    const store = storeAt(816);
    expect(() => store.goToPage(2)).not.toThrow();
  });
});

describe("view modes (UI-34)", () => {
  it("continuous is the default and keeps the released geometry untouched", () => {
    const store = storeAt(816);
    expect(store.viewMode).toBe("continuous");
    expect(store.pageWidth(1)).toBeCloseTo(816, 6); // the released fit-width base
  });

  it("facing fit-width is half a pair row minus the gap; percents anchor to actual size in every mode", () => {
    const store = storeAt(816);
    store.setViewMode("facing");
    expect(store.pageWidth(1)).toBeCloseTo((816 - PAGE_GAP) / 2, 6);
    store.setZoomPercent(150);
    // AC-96: the percent anchor is the page's actual size in every mode —
    // an over-wide page is centered and stays fully reachable (DR-205).
    expect(store.pageWidth(1)).toBeCloseTo(816 * 1.5, 6);
  });

  it("a row is the taller page of its pair; an unpaired last page is a row of one", () => {
    const store = storeAt(816);
    const heights = store.pageHeights();
    const rows = store.rowHeights();
    expect(rows.length).toBe(2); // (1|2), then 3 alone — no special case
    expect(rows[0]).toBeCloseTo(Math.max(heights[0], heights[1]), 6);
    expect(rows[1]).toBeCloseTo(heights[2], 6);
    const offsets = store.rowOffsets();
    expect(offsets[1]).toBeCloseTo(rows[0] + PAGE_GAP, 6); // row step = max(pair) + gap
    expect(offsets.length).toBe(3);
  });

  it("facing jumps and steps land on the row containing the page; the field names its first page", () => {
    const store = storeAt(816);
    store.setViewMode("facing");
    const delivered: number[] = [];
    store.onScrollTo = (offset) => delivered.push(offset);
    const rows = store.rowOffsets();
    store.goToPage(2); // a row's second page: the jump targets the row (UI-34)
    expect(delivered[0]).toBeCloseTo(rows[0] + 1, 6);
    store.scrollTop = delivered[0];
    expect(store.currentPage).toBe(1); // the field shows the row's first page
    store.stepPage(1); // prev/next move one row
    expect(delivered[1]).toBeCloseTo(rows[1] + 1, 6);
    store.scrollTop = delivered[1];
    expect(store.currentPage).toBe(3);
    store.stepPage(1); // past the last row: clamped, the position stands
    expect(delivered[2]).toBeCloseTo(rows[1] + 1, 6);
  });

  it("the field follows the viewport center: a row crossing it owns the page number", () => {
    const store = storeAt(816);
    store.setViewMode("facing");
    const rows = store.rowOffsets();
    store.viewportHeight = 400;
    store.scrollTop = rows[1] - 200; // the viewport center lands on the second row's top
    expect(store.currentPage).toBe(3);
  });

  it("horizontal fit-width fits the current page to the viewport — the released fit base, not half a row", () => {
    const store = storeAt(816);
    store.setViewMode("horizontal");
    // AC-146: the row does not halve the fit base (the facing rule is a
    // pair-row rule); the current page fits the viewport width.
    expect(store.pageWidth(1)).toBeCloseTo(816, 6);
  });

  it("the row's offsets are the pages' display widths plus the gap — per-page for mixed sizes", () => {
    const store = storeAt(816);
    store.setViewMode("horizontal");
    store.setZoomPercent(100); // actual sizes: the landscape page is wider than the portrait ones
    const offsets = store.rowXOffsets();
    const widths = [1, 2, 3].map((page) => store.pageWidth(page));
    expect(widths[2]).toBeGreaterThan(widths[0]);
    expect(offsets[1]).toBeCloseTo(widths[0] + PAGE_GAP, 6); // the x-axis counterpart of the row step
    expect(offsets[2]).toBeCloseTo(widths[0] + widths[1] + 2 * PAGE_GAP, 6);
  });

  it("the field follows the viewport's horizontal center; jumps and steps move one page along the row", () => {
    const store = storeAt(816);
    store.setViewMode("horizontal");
    store.viewportWidth = 1000;
    const offsets = store.rowXOffsets();
    store.scrollLeft = offsets[1] - 500; // the center lands on the second page's left edge
    expect(store.currentPage).toBe(2);
    const delivered: number[] = [];
    store.onScrollTo = (offset) => delivered.push(offset);
    store.goToPage(3);
    expect(delivered[0]).toBeCloseTo(offsets[2] + 1, 6); // the +1 px rule unchanged (DR-148)
    store.scrollLeft = delivered[0];
    expect(store.currentPage).toBe(3);
    store.stepPage(-1); // prev/next move one page along the row, not a row of two (UI-39)
    expect(delivered[1]).toBeCloseTo(offsets[1] + 1, 6);
    store.scrollLeft = delivered[1];
    expect(store.currentPage).toBe(2);
    store.stepPage(1);
    store.stepPage(1); // past the last page: clamped, the position stands
    expect(delivered[3]).toBeCloseTo(offsets[2] + 1, 6);
  });

  it("PgUp/PgDn travel one viewport along the row (UI-30 on the x-axis)", () => {
    const store = storeAt(816);
    store.setViewMode("horizontal");
    store.viewportWidth = 1000;
    const scrolled: number[] = [];
    store.onScrollBy = (delta) => scrolled.push(delta);
    store.scrollViewport(1);
    store.scrollViewport(-2);
    expect(scrolled).toEqual([1000, -2000]);
  });

  it("in horizontal a zoom change anchors to the page at the row's left edge; at the row's end it names the last page", () => {
    const store = storeAt(816);
    store.setViewMode("horizontal");
    store.viewportWidth = 1000;
    const offsets = store.rowXOffsets();
    store.scrollLeft = offsets[1] + 5; // page 2 at the viewport's left edge
    store.setZoomPercent(150);
    expect(store.pendingReveal).toBe(2); // the left-edge anchor (D23 unchanged, x-axis)
    const wide = store.rowXOffsets(); // the maximum belongs to the geometry the user is looking at
    store.scrollLeft = wide[wide.length - 1] - 1000; // the row's maximum scroll
    store.setZoomPercent(100);
    expect(store.pendingReveal).toBe(3); // the right-edge anchor in the clamped case (AC-119's rule)
  });

  it("single mounts the index instead of scrolling; its navigation moves pages", () => {
    const store = storeAt(816);
    const delivered: number[] = [];
    store.onScrollTo = (offset) => delivered.push(offset);
    store.setViewMode("single");
    store.goToPage(3);
    expect(store.singleIndex).toBe(3);
    expect(store.currentPage).toBe(3);
    expect(delivered).toEqual([]); // no scroll request exists in single mode
    store.stepPage(-1);
    expect(store.currentPage).toBe(2);
    store.scrollViewport(1); // PgDn is the next page (UI-34)
    expect(store.currentPage).toBe(3);
    store.scrollViewport(1);
    expect(store.currentPage).toBe(3); // clamped at the last page
    expect(delivered).toEqual([]);
  });

  it("switching mode keeps the current page: single mounts it, scroll modes record it for the view", () => {
    const store = storeAt(816);
    store.scrollTop = store.pageOffsets()[1] + 1; // scrolled to page 2 (DR-148's target)
    expect(store.currentPage).toBe(2);
    store.setViewMode("single"); // entering single mounts the kept page
    expect(store.singleIndex).toBe(2);
    expect(store.currentPage).toBe(2);
    store.setViewMode("facing"); // a scroll-driven mode records it (the view jumps once mounted)
    expect(store.pendingReveal).toBe(2);
    expect(store.revealNonce).toBe(1);
  });

  it("through the row and back: continuous → horizontal → continuous keeps page 2 in view (AC-144)", () => {
    const store = storeAt(816);
    store.scrollTop = store.pageOffsets()[1] + 1; // scrolled to page 2 in continuous
    expect(store.currentPage).toBe(2);
    store.setViewMode("horizontal"); // the row records it the same way (v0.0.10)
    expect(store.pendingReveal).toBe(2);
    const delivered: number[] = [];
    store.onScrollTo = (offset) => delivered.push(offset);
    store.viewportWidth = 1000;
    store.goToPage(store.pendingReveal); // the view's jump, once the row is mounted
    store.scrollLeft = delivered[0];
    expect(store.currentPage).toBe(2); // the row shows the kept page
    store.setViewMode("continuous"); // and back: the kept page is recorded again
    expect(store.pendingReveal).toBe(2);
  });
});

describe("zoom walk (UI-30 as re-anchored by DR-221)", () => {
  it("− / + move through the fixed steps and clamp at the ends", () => {
    expect(stepZoom(100, -1)).toBe(75);
    expect(stepZoom(75, -1)).toBe(50);
    expect(stepZoom(50, -1)).toBe(50); // the floor stays 50 %
    expect(stepZoom(200, 1)).toBe(300);
    expect(stepZoom(300, 1)).toBe(300); // the ceiling stays 300 %
  });

  it("a non-ladder value snaps to the nearest step in the pressed direction (AC-96)", () => {
    expect(stepZoom(137, 1)).toBe(150);
    expect(stepZoom(137, -1)).toBe(125);
    expect(stepZoom(112, 1)).toBe(125);
    expect(stepZoom(112, -1)).toBe(100);
  });

  it("from a fit mode the walk continues from the mode's effective percent of the current page (AC-96)", () => {
    // Fit width at 1 200 CSS px on a 612 pt page: effective 147.06 %.
    const store = storeAt(1200);
    store.changeZoom(1);
    expect(store.zoom).toEqual({ mode: "percent", percent: 150 });
    store.setZoomMode("fit-width");
    store.changeZoom(-1);
    expect(store.zoom).toEqual({ mode: "percent", percent: 125 });
    // Fit width on the landscape page (792 pt, actual 1 056) at the same
    // window: effective 113.6 % — + lands on 125, − on 100.
    store.setZoomMode("fit-width");
    store.scrollTop = store.pageOffsets()[2] + 1; // now on page 3
    store.changeZoom(1);
    expect(store.zoom).toEqual({ mode: "percent", percent: 125 });
    store.setZoomMode("fit-width");
    store.changeZoom(-1);
    expect(store.zoom).toEqual({ mode: "percent", percent: 100 });
  });

  it("the store's changeZoom only ever produces listed steps", () => {
    const store = storeAt(816);
    for (let i = 0; i < 10; i++) store.changeZoom(1);
    expect(store.zoom).toEqual({ mode: "percent", percent: 300 });
    store.setZoomMode("fit-width");
    expect(store.zoom).toEqual({ mode: "fit-width" });
  });

  it("the zoom select's items set exactly the modes and the ladder (AC-96)", () => {
    const store = storeAt(816);
    for (const mode of ["fit-page", "actual", "fit-width"] as const) {
      store.setZoomMode(mode);
      expect(store.zoom).toEqual({ mode });
    }
    for (const percent of ZOOM_STEPS) {
      store.setZoomPercent(percent);
      expect(store.zoom).toEqual({ mode: "percent", percent });
    }
  });
});

describe("the zoom anchor handoff (D23 / AC-119)", () => {
  it("a zoom change records the current page for the reveal after the layout change", () => {
    const store = storeAt(816);
    store.viewportHeight = 600;
    store.scrollTop = store.pageOffsets()[1]; // the viewport top edge is on page 2
    store.setZoomPercent(150);
    expect(store.pendingReveal).toBe(2);
    expect(store.revealNonce).toBe(1);
    // The reveal landed: the view is at page 2's top edge in the new geometry.
    store.scrollTop = store.pageOffsets()[1];
    store.setZoomMode("fit-page");
    expect(store.pendingReveal).toBe(2);
    expect(store.revealNonce).toBe(2);
  });

  it("a change that cannot change the geometry records no reveal", () => {
    const store = storeAt(816);
    store.setZoomMode("fit-width"); // the mode it is already in
    expect(store.revealNonce).toBe(0);
    store.setZoomPercent(50);
    store.changeZoom(-1); // clamped at the floor: no layout change
    expect(store.zoom).toEqual({ mode: "percent", percent: 50 });
    expect(store.revealNonce).toBe(1);
  });

  it("single mode records no reveal — its page is index-driven", () => {
    const store = storeAt(816);
    store.setViewMode("single");
    store.setZoomPercent(150);
    expect(store.revealNonce).toBe(0);
  });

  it("a bottom-clamped view anchors to the viewport's bottom page, not the top edge's partial one", () => {
    // The AC-119 walk's middle state: fit → zoom out clamps the scroll to the
    // maximum; the top edge then shows a partial page while the user's anchor
    // is the last page. The next geometry change must re-anchor from that page.
    const store = storeAt(816);
    store.viewportHeight = 1500;
    const offsets = store.pageOffsets();
    store.scrollTop = offsets[offsets.length - 1] - 1500; // exactly at the maximum
    expect(store.currentPage).toBe(2); // the top edge's page — not the anchor
    store.setZoomPercent(75);
    expect(store.pendingReveal).toBe(3); // the last page: the bottom edge's page
    expect(store.revealNonce).toBe(1);
  });
});

describe("the ToC tree (UI-33 as revised by DR-218)", () => {
  // The flat depth encoding the command delivers (§3.5): a parent is exactly
  // an entry followed by a deeper one.
  const outline: OutlineEntry[] = [
    { title: "Chapter 1", page: 1, depth: 1 },
    { title: "1.1 Section", page: 2, depth: 2 },
    { title: "1.1.1 Subsection", page: 3, depth: 3 },
    { title: "Chapter 2", page: 5, depth: 1 },
    { title: "2.1 Section", page: 6, depth: 2 },
  ];

  function treeStore(): ReaderStore {
    const store = storeAt(816);
    store.outline = outline;
    store.tocCollapsed = new Set(store.tocParents()); // the load-time initialization
    return store;
  }

  it("parents are exactly the entries followed by a deeper entry", () => {
    const store = storeAt(816);
    store.outline = outline;
    expect(store.tocParents()).toEqual([0, 1, 3]); // 1.1 is a parent of 1.1.1
    expect(store.isTocParent(0)).toBe(true);
    expect(store.isTocParent(1)).toBe(true);
    expect(store.isTocParent(2)).toBe(false);
    expect(store.isTocParent(4)).toBe(false); // the last entry has no child
  });

  it("the tree initializes collapsed to the first level: only top-level entries are visible (AC-78)", () => {
    const store = treeStore();
    expect([...store.tocCollapsed]).toEqual([0, 1, 3]);
    expect(store.tocVisible().map((v) => v.entry.title)).toEqual(["Chapter 1", "Chapter 2"]);
  });

  it("the outline load performs that initialization through the command path (AC-78)", async () => {
    libraryGetDocument.mockResolvedValue({ id: 1, filename: "t.pdf" });
    appPdfDocument.mockResolvedValue(doc);
    appPdfOutline.mockResolvedValue(outline);
    const store = new ReaderStore(1);
    await store.load();
    expect(store.status).toBe("ready");
    expect(store.tocVisible().map((v) => v.index)).toEqual([0, 3]);
  });

  it("expanding a parent reveals its whole subtree up to the next entry of its own level (AC-78)", () => {
    const store = treeStore();
    store.toggleTocEntry(0); // Chapter 1 opens: its direct children show, 1.1's own subtree is still collapsed
    expect(store.tocVisible().map((v) => v.index)).toEqual([0, 1, 3]);
    store.toggleTocEntry(1); // the nested subtree follows its parent's state, not the ancestor's
    expect(store.tocVisible().map((v) => v.index)).toEqual([0, 1, 2, 3]);
    store.toggleTocEntry(3); // Chapter 2 opens too
    expect(store.tocVisible().map((v) => v.index)).toEqual([0, 1, 2, 3, 4]);
    store.toggleTocEntry(0); // Chapter 1 collapses again — its whole subtree hides with it; Chapter 2, opened earlier, stays open
    expect(store.tocVisible().map((v) => v.index)).toEqual([0, 3, 4]);
  });

  it("the toolbar toggle alternates modes and drives the whole tree (AC-78, DR-223)", () => {
    const store = treeStore();
    expect(store.tocExpandMode).toBe(true); // the tree opens collapsed: the first action is expand-all
    store.tocToggleAll();
    expect(store.tocCollapsed.size).toBe(0);
    expect(store.tocVisible().map((v) => v.index)).toEqual([0, 1, 2, 3, 4]);
    expect(store.tocExpandMode).toBe(false); // the control now names collapse-all
    store.tocToggleAll();
    expect([...store.tocCollapsed]).toEqual([0, 1, 3]);
    expect(store.tocVisible().map((v) => v.index)).toEqual([0, 3]);
    expect(store.tocExpandMode).toBe(true); // and flips back
  });

  it("per-entry toggles never change the toggle's mode; its action always applies to the whole tree (AC-78, DR-223)", () => {
    const store = treeStore();
    store.toggleTocEntry(0); // a manual partial expansion…
    expect(store.tocExpandMode).toBe(true); // …leaves the mode untouched
    store.tocToggleAll(); // still expand-all — applied to the whole tree
    expect(store.tocVisible().map((v) => v.index)).toEqual([0, 1, 2, 3, 4]);
    expect(store.tocExpandMode).toBe(false);
    store.toggleTocEntry(3); // a manual collapse…
    expect(store.tocExpandMode).toBe(false); // …again leaves the mode untouched
    store.tocToggleAll(); // still collapse-all
    expect(store.tocVisible().map((v) => v.index)).toEqual([0, 3]);
  });

  it("toggling a leaf changes nothing", () => {
    const store = treeStore();
    store.toggleTocEntry(2); // 1.1.1 has no child
    expect(store.tocVisible().map((v) => v.index)).toEqual([0, 3]);
  });

  it("a document without an outline has no tree and no hidden entries", () => {
    const store = storeAt(816);
    expect(store.tocParents()).toEqual([]);
    expect(store.tocVisible()).toEqual([]);
  });
});

describe("the ToC pin (FR-24 / UI-49, SPEC-v0.0.10 §3.8, revised by DR-337)", () => {
  it("is session state of the window store: unpinned by default, closed through the reset", () => {
    const store = storeAt(816);
    expect(store.tocPinned).toBe(false); // every new reader window starts unpinned (OQ-60 → A)
    expect(store.tocDockedOpen).toBe(false); // the docked panel is shown only while pinned (DR-337)
    store.setTocPinned(true);
    expect(store.tocPinned).toBe(true);
    expect(store.tocDockedOpen).toBe(true); // pinning writes visibility with the mode
    // The single un-pin path (DR-337): the pin control writes false here,
    // and the panel closes with the mode.
    store.setTocPinned(false);
    expect(store.tocPinned).toBe(false);
    expect(store.tocDockedOpen).toBe(false);
    // Per window, not inherited: a second store — a new reader window — is
    // unaffected by the first one's pin.
    expect(new ReaderStore(1).tocPinned).toBe(false);
    expect(new ReaderStore(1).tocDockedOpen).toBe(false);
  });

  it("separates visibility from the pin: the strip control and Esc hide and re-show the panel keeping the pinned position", () => {
    const store = storeAt(816);
    store.setTocPinned(true);
    // Hide through the visibility path — the pin is untouched.
    store.setTocDockedOpen(false);
    expect(store.tocDockedOpen).toBe(false);
    expect(store.tocPinned).toBe(true);
    // Re-show — the panel comes back in the pinned state (the same docked
    // position; the layout is derived from the same two booleans).
    store.setTocDockedOpen(true);
    expect(store.tocDockedOpen).toBe(true);
    expect(store.tocPinned).toBe(true);
    // Visibility writes never change the pin mode, in either direction.
    store.setTocDockedOpen(false);
    store.setTocDockedOpen(false); // idempotent — hiding an already hidden panel
    expect(store.tocPinned).toBe(true);
    expect(store.tocDockedOpen).toBe(false);
  });
});

describe("the navigation direction (UI-34 as extended by DR-224)", () => {
  it("it follows the delta of each navigation and defaults to forward", () => {
    const store = storeAt(816);
    store.setViewMode("single");
    expect(store.lastNavDirection).toBe(1); // forward is the initial default
    store.goToPage(3);
    expect(store.lastNavDirection).toBe(1);
    store.stepPage(-1); // 3 → 2
    expect(store.lastNavDirection).toBe(-1);
    store.goToPage(2); // a same-page jump keeps the previous direction
    expect(store.lastNavDirection).toBe(-1);
    store.goToPage(1); // backward to the first page
    expect(store.lastNavDirection).toBe(-1);
    store.goToPage(3); // forward again
    expect(store.lastNavDirection).toBe(1);
  });

  it("clamped navigation records the direction of the movement actually taken", () => {
    const store = storeAt(816); // the fixture document has 3 pages
    store.setViewMode("single");
    store.goToPage(99); // clamped to 3: forward from 1
    expect(store.currentPage).toBe(3);
    expect(store.lastNavDirection).toBe(1);
    store.goToPage(-5); // clamped to 1: backward from 3
    expect(store.currentPage).toBe(1);
    expect(store.lastNavDirection).toBe(-1);
  });
});

describe("the edge rule and the keyboard notch (UI-34/UI-39 as re-derived by DR-227/DR-228)", () => {
  it("a delta at a reached edge steps exactly one page — any magnitude (DR-227)", () => {
    // A tall page: 1 556 px of content in a 660 px viewport.
    expect(edgeStep(4, 896, 660, 1556)).toBe(1); // a trackpad micro-event at the bottom edge steps
    expect(edgeStep(-4, 0, 660, 1556)).toBe(-1); // and at the top edge
  });

  it("the edge test carries the 2 px rounding margin (DR-227)", () => {
    expect(edgeStep(100, 894, 660, 1556)).toBe(1); // 894 + 660 = 1 554 ≥ 1 556 − 2
    expect(edgeStep(-100, 2, 660, 1556)).toBe(-1);
    expect(edgeStep(100, 893, 660, 1556)).toBe(0); // 1 553 is past the margin — still mid-page
  });

  it("mid-page deltas never step; the direction decides which edge counts (DR-227)", () => {
    expect(edgeStep(100, 400, 660, 1556)).toBe(0); // neither edge
    expect(edgeStep(100, 0, 660, 1556)).toBe(0); // at the top edge, scrolling forward
    expect(edgeStep(-100, 896, 660, 1556)).toBe(0); // at the bottom edge, scrolling back
  });

  it("a page fully inside the viewport steps in the delta's direction (DR-227)", () => {
    expect(edgeStep(100, 0, 1556, 1556)).toBe(1); // both edges reached at once
    expect(edgeStep(-100, 0, 1556, 1556)).toBe(-1);
  });

  it("the notch dispatch is mode-aware: single hands the edge rule to the view, the scroll modes scroll (AC-98)", () => {
    const store = storeAt(816);
    const scrolled: number[] = [];
    const notched: number[] = [];
    store.onScrollBy = (delta) => scrolled.push(delta);
    store.onNotch = (delta) => notched.push(delta);
    store.setViewMode("continuous");
    store.scrollNotch(SCROLL_NOTCH);
    store.scrollNotch(-SCROLL_NOTCH);
    store.setViewMode("facing");
    store.scrollNotch(SCROLL_NOTCH);
    store.setViewMode("horizontal"); // the row scrolls through the same callback (UI-39 MODIFIED)
    store.scrollNotch(SCROLL_NOTCH);
    store.setViewMode("single");
    store.scrollNotch(-SCROLL_NOTCH);
    expect(scrolled).toEqual([SCROLL_NOTCH, -SCROLL_NOTCH, SCROLL_NOTCH, SCROLL_NOTCH]);
    expect(notched).toEqual([-SCROLL_NOTCH]);
  });
});

describe("image request width", () => {
  it("requests device pixels at most 2× the CSS width", () => {
    expect(imageRequestWidth(800, 1)).toBe(800);
    expect(imageRequestWidth(800, 1.5)).toBe(1200);
    expect(imageRequestWidth(800, 3)).toBe(1600); // capped at 2×
  });

  it("stays inside the command's accepted range", () => {
    expect(imageRequestWidth(10, 1)).toBe(REQUEST_WIDTH_MIN);
    expect(imageRequestWidth(3000, 2)).toBe(REQUEST_WIDTH_MAX); // 6000 → 4000
    expect(imageRequestWidth(0, 1)).toBe(REQUEST_WIDTH_MIN);
  });
});

describe("page serving through the store", () => {
  beforeEach(() => {
    appPdfPage.mockReset();
    appPdfPage.mockImplementation((_id, page, width) =>
      Promise.resolve(`C:/appdata/reader/1/${page}-${width}.png`),
    );
  });

  it("serves a mounted page once per width: scrolling does not re-request it", async () => {
    const store = storeAt(816);
    store.requestPage(2);
    store.requestPage(2);
    await vi.waitFor(() => expect(store.images.has(2)).toBe(true));
    expect(appPdfPage).toHaveBeenCalledTimes(1);
    expect(appPdfPage).toHaveBeenCalledWith(1, 2, 816);
    const image = store.images.get(2)!;
    expect(image.width).toBe(816);
    expect(image.src).toContain("asset:///");
    store.requestPage(2); // already fetched at this width
    expect(appPdfPage).toHaveBeenCalledTimes(1);
  });

  it("a zoom change re-requests at the new width (NFR-08's re-show path)", async () => {
    const store = storeAt(816);
    store.requestPage(1);
    await vi.waitFor(() => expect(store.images.has(1)).toBe(true));
    store.changeZoom(-1); // 100 → 75 %
    store.requestPage(1);
    await vi.waitFor(() => expect(appPdfPage).toHaveBeenCalledTimes(2));
    expect(appPdfPage.mock.calls[1]).toEqual([1, 1, Math.round(816 * 0.75)]);
    expect(store.images.get(1)!.width).toBeCloseTo(612, 6);
  });

  it("a target above the fast-render cap previews first; the view's previewShown promotes the full stage after the release window (DR-151 as re-derived by DR-316/DR-323)", async () => {
    const store = storeAt(816);
    store.setZoomPercent(200); // a 1 632 px target, above the 1 400 px cap
    const full = Promise.withResolvers<string>();
    appPdfPage.mockImplementation((_id, page, width) =>
      width === 1632 ? full.promise : Promise.resolve(`C:/appdata/reader/1/${page}-${width}.png`), // full stage gated
    );
    store.requestPage(1);
    await vi.waitFor(() => expect(store.images.has(1)).toBe(true));
    expect(appPdfPage.mock.calls[0]).toEqual([1, 1, 1400]); // the preview leads
    expect(store.images.get(1)!.src).toContain("1-1400.png"); // and displays while full is pending
    await vi.waitFor(() => expect(appPdfPage.mock.calls.length).toBe(2));
    expect(appPdfPage.mock.calls[1]).toEqual([1, 1, 1632]); // the full target follows
    full.resolve("C:/appdata/reader/1/1-1632.png");
    // DR-316: the full stage lands as the overlay source, not as a replacement.
    await vi.waitFor(() => expect(store.images.get(1)!.full).toContain("1-1632.png"));
    store.previewShown(1, 1632); // the view's call from the overlay's load event (DR-323)
    await vi.waitFor(() => expect(store.images.get(1)!.src).toContain("1-1632.png"), { timeout: 2000 });
    expect(store.images.get(1)!.full).toBeNull(); // the mounted page carries one raster again
    expect(store.images.get(1)!.width).toBeCloseTo(1632, 6); // the entry stays at the CSS width
  });

  it("a request before the first fit-width measurement stays inside the command's range (DR-153)", async () => {
    // Window-start state: fitWidth is not derived yet, so the CSS width is
    // 0 — every request width must still land in [100, 4000].
    const store = storeAt(0);
    store.requestPage(1);
    await vi.waitFor(() => expect(appPdfPage).toHaveBeenCalledTimes(1));
    expect(appPdfPage.mock.calls[0]).toEqual([1, 1, 100]); // clamped, never 0
    expect(store.status).toBe("ready"); // a startup request must not error the window
  });

  it("at devicePixelRatio above 1 the preview uses the CSS display width, not the cap (DR-153)", async () => {
    // The reference machine's fit-width case: 967 CSS px at dpr 1.75 — the
    // device target (1 692) exceeds the cap, but the preview stage renders
    // at the display width; the cap only bounds display widths above it.
    const ratio = globalThis.devicePixelRatio;
    Object.defineProperty(globalThis, "devicePixelRatio", { value: 1.75, configurable: true });
    try {
      const store = storeAt(967);
      const full = Promise.withResolvers<string>();
      appPdfPage.mockImplementation((_id, page, width) =>
        width === 1692 ? full.promise : Promise.resolve(`C:/appdata/reader/1/${page}-${width}.png`), // full stage gated
      );
      store.requestPage(1);
      await vi.waitFor(() => expect(store.images.has(1)).toBe(true));
      expect(appPdfPage.mock.calls[0]).toEqual([1, 1, 967]); // the preview leads at display width
      expect(store.images.get(1)!.src).toContain("1-967.png"); // and displays while full is pending
      await vi.waitFor(() => expect(appPdfPage.mock.calls.length).toBe(2));
      expect(appPdfPage.mock.calls[1]).toEqual([1, 1, 1692]); // the full device target follows
      full.resolve("C:/appdata/reader/1/1-1692.png");
      await vi.waitFor(() => expect(store.images.get(1)!.full).toContain("1-1692.png")); // the overlay source (DR-316)
      store.previewShown(1, 967); // the view's call, at the page's CSS width (DR-323)
      await vi.waitFor(() => expect(store.images.get(1)!.src).toContain("1-1692.png"), { timeout: 2000 });
      expect(store.images.get(1)!.full).toBeNull(); // one raster again
      expect(store.images.get(1)!.width).toBeCloseTo(967, 6); // the entry stays at the CSS width
    } finally {
      Object.defineProperty(globalThis, "devicePixelRatio", { value: ratio, configurable: true });
    }
  });

  it("a target at or below the fast-render cap is a single request (DR-151)", async () => {
    const store = storeAt(816);
    store.changeZoom(+1); // 100 → 125 %: 1 020 px, below the cap
    store.requestPage(1);
    await vi.waitFor(() => expect(store.images.has(1)).toBe(true));
    store.requestPage(1); // already fetched at this width
    await Promise.resolve();
    expect(appPdfPage).toHaveBeenCalledTimes(1);
    expect(appPdfPage).toHaveBeenCalledWith(1, 1, 1020);
  });

  it("a typed page failure becomes the window's error state (§3.2)", async () => {
    const store = storeAt(816);
    appPdfPage.mockImplementation(() =>
      Promise.reject({ kind: "pdf_unopenable", message: "Cannot Read PDF", document_id: null }),
    );
    store.requestPage(1);
    await vi.waitFor(() => expect(store.status).toBe("error"));
    expect(store.error?.kind).toBe("pdf_unopenable");
  });
});

describe("the scroll position is a single source (DR-155)", () => {
  it("currentPage follows the store's scrollTop through the exact offsets", () => {
    const store = storeAt(816);
    const offsets = store.pageOffsets();
    expect(store.currentPage).toBe(1);
    store.scrollTop = offsets[1] + 1; // one pixel inside page 2 — the goToPage target (DR-148)
    expect(store.currentPage).toBe(2);
    store.scrollTop = offsets[2] + 1;
    expect(store.currentPage).toBe(3);
    store.scrollTop = 0;
    expect(store.currentPage).toBe(1);
  });

  it("an unmeasured window claims page 1, the position the scroller is at (DR-154)", () => {
    const cold = storeAt(0);
    cold.scrollTop = 999_999;
    expect(cold.currentPage).toBe(1);
  });

  it("goToPage and currentPage agree: the delivered target names the page it claims", () => {
    const store = storeAt(816);
    let delivered = -1;
    store.onScrollTo = (offset) => {
      delivered = offset;
    };
    store.goToPage(3);
    expect(delivered).toBe(store.pageOffsets()[2] + 1);
    store.scrollTop = delivered;
    expect(store.currentPage).toBe(3);
  });
});

// ---------------------------------------------------------------------------
// Position memory (FR-28, SPEC-v0.0.12 §3.2): the value grammar, the
// restore/clamp math per mode (DR-406 1/2/5), the debounced write discipline
// and the close flush (AC-167/AC-168). View-mode memory (FR-29/AC-169) and
// the text-layer mapping (FR-30/UI-54, §3.3) are the other §3.10 units.
// ---------------------------------------------------------------------------

describe("position grammar (FR-28, §3.2 value grammar)", () => {
  it("reads both kinds' grammars", () => {
    expect(parseReaderPosition('{"p":5,"o":0.4}', 10)).toEqual({ page: 5, fraction: 0.4 });
    expect(parseReaderPosition('{"c":3,"o":0}', 8)).toEqual({ page: 3, fraction: 0 });
  });

  it("clamps a stale anchor to the document's current extent (AC-167: relink to a shorter file)", () => {
    expect(parseReaderPosition('{"p":9,"o":0.5}', 3)).toEqual({ page: 3, fraction: 0.5 });
  });

  it("reads a malformed value as no position at all — the released default takes over", () => {
    expect(parseReaderPosition(null, 3)).toBeNull();
    expect(parseReaderPosition("not json", 3)).toBeNull();
    expect(parseReaderPosition('"page 4"', 3)).toBeNull();
    expect(parseReaderPosition('{"p":1,"o":0.5,"c":2}', 3)).toBeNull(); // two anchors
    expect(parseReaderPosition('{"o":0.5}', 3)).toBeNull(); // no anchor
    expect(parseReaderPosition('{"p":0,"o":0.5}', 3)).toBeNull(); // anchor below 1
    expect(parseReaderPosition('{"p":1.5,"o":0.5}', 3)).toBeNull(); // non-integer anchor
    expect(parseReaderPosition('{"p":2,"o":1.5}', 3)).toBeNull(); // offset outside 0–1
    expect(parseReaderPosition('{"p":2}', 3)).toBeNull(); // no offset
    expect(parseReaderPosition('{"p":2,"o":0.5}', 0)).toBeNull(); // an empty document
  });

  it("encodes the exact grammar with the kind's anchor key and three-decimal offsets", () => {
    expect(JSON.parse(encodeReaderPosition(12, 0.4, "p"))).toEqual({ p: 12, o: 0.4 });
    expect(JSON.parse(encodeReaderPosition(3, 1 / 3, "c"))).toEqual({ c: 3, o: 0.333 });
    expect(encodeReaderPosition(2, -0.2, "p")).toBe('{"p":2,"o":0}'); // clamped before encoding
  });
});

describe("position restore (FR-28/AC-167, DR-406 1/2)", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    appReaderPositionSet.mockClear();
    appReaderPositionGet.mockReset();
    appReaderPositionGet.mockResolvedValue(null);
    settings.setReaderViewMode("continuous");
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  /** A store through the real `load()` path with a stored position present
   *  or absent — the same command mocks the released load test uses. */
  async function loadedReader(position: string | null): Promise<ReaderStore> {
    libraryGetDocument.mockResolvedValue({ id: 1, filename: "t.pdf" });
    appPdfDocument.mockResolvedValue(doc);
    appPdfOutline.mockResolvedValue([]);
    appReaderPositionGet.mockResolvedValue(position);
    const store = new ReaderStore(1);
    await store.load();
    store.fitWidth = 816; // 612 pt × 4/3 — the fit base used across this file
    store.viewportHeight = 600;
    return store;
  }

  it("reads the stored position before `ready` and holds the mount until it lands", async () => {
    const store = await loadedReader('{"p":2,"o":0.5}');
    expect(store.status).toBe("ready");
    expect(store.restorePending).toBe(true);
    // The gate is real: while pending, a scheduled write records nothing.
    store.schedulePositionRecord();
    vi.advanceTimersByTime(POSITION_WRITE_MS);
    expect(appReaderPositionSet).not.toHaveBeenCalled();
  });

  it("a failed position read is the released default: no restore, no error", async () => {
    appReaderPositionGet.mockRejectedValueOnce(new Error("no handle"));
    const store = await loadedReader(null);
    expect(store.status).toBe("ready");
    expect(store.restorePending).toBe(false);
    expect(store.error).toBeNull();
  });

  it("continuous lands at the page top plus o × the page's display height — the first page named is the restored one", async () => {
    const store = await loadedReader('{"p":2,"o":0.5}');
    const lands: number[] = [];
    store.onScrollTo = (px) => lands.push(px);
    store.applyRestoredPosition();
    expect(lands).toHaveLength(1);
    expect(lands[0]).toBeCloseTo(store.pageOffsets()[1] + 1 + 0.5 * store.pageHeights()[1], 6);
    expect(store.restorePending).toBe(false);
    expect(store.currentPage).toBe(2);
  });

  it("facing lands within the pair row (DR-406 2): the row anchor plus o × the row height", async () => {
    settings.setReaderViewMode("facing");
    const store = await loadedReader('{"p":3,"o":0.5}');
    expect(store.viewMode).toBe("facing"); // FR-29: the store starts in the remembered mode
    const lands: number[] = [];
    store.onScrollTo = (px) => lands.push(px);
    store.applyRestoredPosition();
    // Page 3 is the first page of row 1 (3|4).
    expect(lands[0]).toBeCloseTo(store.rowOffsets()[1] + 1 + 0.5 * store.rowHeights()[1], 6);
    expect(store.currentPage).toBe(3);
  });

  it("horizontal lands at the page's left edge plus o × its display width (the x-axis model)", async () => {
    settings.setReaderViewMode("horizontal");
    const store = await loadedReader('{"p":3,"o":0.25}');
    const lands: number[] = [];
    store.onScrollTo = (px) => lands.push(px);
    store.applyRestoredPosition();
    expect(lands[0]).toBeCloseTo(store.rowXOffsets()[2] + 1 + 0.25 * store.pageWidth(3), 6);
    expect(store.scrollLeft).toBeCloseTo(lands[0], 6);
    expect(store.currentPage).toBe(3);
  });

  it("single mounts the restored page directly (index-driven, no scroll)", async () => {
    settings.setReaderViewMode("single");
    const store = await loadedReader('{"p":2,"o":0.7}');
    const lands: number[] = [];
    store.onScrollTo = (px) => lands.push(px);
    store.applyRestoredPosition();
    expect(lands).toEqual([]); // no scroll motion in single mode
    expect(store.singleIndex).toBe(2);
    expect(store.currentPage).toBe(2);
    expect(store.restorePending).toBe(false);
  });

  it("a stale position clamps: the relink-to-shorter case lands on the last page", async () => {
    const store = await loadedReader('{"p":9,"o":0.5}'); // doc has 3 pages
    const lands: number[] = [];
    store.onScrollTo = (px) => lands.push(px);
    store.applyRestoredPosition();
    expect(lands[0]).toBeCloseTo(store.pageOffsets()[2] + 1 + 0.5 * store.pageHeights()[2], 6);
    expect(store.currentPage).toBe(3);
  });

  it("a no-op restore (page 1, offset 0) still clears the gate so the first page paints", async () => {
    const store = await loadedReader('{"p":1,"o":0}');
    store.onScrollTo = () => {};
    store.applyRestoredPosition();
    expect(store.restorePending).toBe(false);
  });

  it("applying a restore twice is inert, and without a stored position it changes nothing", async () => {
    const store = await loadedReader('{"p":2,"o":0.5}');
    store.onScrollTo = () => {};
    store.applyRestoredPosition();
    const lands: number[] = [];
    store.onScrollTo = (px) => lands.push(px);
    store.applyRestoredPosition(); // second call: no second jump
    expect(lands).toEqual([]);
    const plain = await loadedReader(null);
    plain.onScrollTo = () => {};
    plain.applyRestoredPosition(); // nothing was stored: no jump, gate stays closed
    expect(plain.restorePending).toBe(false);
  });
});

describe("position write discipline (FR-28/AC-168)", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    appReaderPositionSet.mockClear();
    appReaderPositionGet.mockReset();
    appReaderPositionGet.mockResolvedValue(null);
    settings.setReaderViewMode("continuous");
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  async function readyReader(): Promise<ReaderStore> {
    libraryGetDocument.mockResolvedValue({ id: 1, filename: "t.pdf" });
    appPdfDocument.mockResolvedValue(doc);
    appPdfOutline.mockResolvedValue([]);
    const store = new ReaderStore(1);
    await store.load();
    store.fitWidth = 816;
    store.viewportHeight = 600;
    return store;
  }

  it("scroll calls debounce to one write per settle — last value wins", async () => {
    const store = await readyReader();
    const offsets = store.pageOffsets();
    store.scrollTop = offsets[1] + 400;
    store.schedulePositionRecord();
    store.scrollTop = offsets[1] + 500;
    store.schedulePositionRecord();
    store.schedulePositionRecord();
    expect(appReaderPositionSet).not.toHaveBeenCalled(); // no per-scroll writes
    vi.advanceTimersByTime(POSITION_WRITE_MS);
    expect(appReaderPositionSet).toHaveBeenCalledTimes(1);
    expect(appReaderPositionSet.mock.calls[0][0]).toBe(1);
    const written = JSON.parse(appReaderPositionSet.mock.calls[0][1]);
    expect(written.p).toBe(2); // the LAST value, not the first
    expect(written.o).toBeCloseTo(500 / store.pageHeights()[1], 3);
  });

  it("flush cancels the pending debounce and writes the last value once — the close path", async () => {
    const store = await readyReader();
    store.scrollTop = store.pageOffsets()[2] + 100;
    store.schedulePositionRecord();
    await store.flushPosition();
    expect(appReaderPositionSet).toHaveBeenCalledTimes(1);
    expect(JSON.parse(appReaderPositionSet.mock.calls[0][1]).p).toBe(3);
    vi.advanceTimersByTime(POSITION_WRITE_MS);
    expect(appReaderPositionSet).toHaveBeenCalledTimes(1); // cancelled, not doubled
  });

  it("a flush with nothing to record writes nothing", async () => {
    const store = new ReaderStore(1); // never loaded: no position to record
    await store.flushPosition();
    expect(appReaderPositionSet).not.toHaveBeenCalled();
  });

  it("single-mode navigation records without a scroll event (DR-406 3/5)", async () => {
    settings.setReaderViewMode("single");
    const store = await readyReader();
    store.goToPage(2); // the index change IS the position change
    vi.advanceTimersByTime(POSITION_WRITE_MS);
    expect(appReaderPositionSet).toHaveBeenCalledTimes(1);
    expect(JSON.parse(appReaderPositionSet.mock.calls[0][1])).toEqual({ p: 2, o: 0 });
  });

  it("a failed write is silent — the next settle re-tries", async () => {
    const store = await readyReader();
    appReaderPositionSet.mockRejectedValueOnce(new Error("window closing"));
    store.scrollTop = store.pageOffsets()[1] + 300;
    store.schedulePositionRecord();
    vi.advanceTimersByTime(POSITION_WRITE_MS);
    expect(store.status).toBe("ready");
    expect(store.error).toBeNull();
    store.schedulePositionRecord();
    vi.advanceTimersByTime(POSITION_WRITE_MS);
    expect(appReaderPositionSet).toHaveBeenCalledTimes(2); // re-tried, not swallowed forever
  });
});

describe("view-mode memory (FR-29/AC-169)", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    appSetSetting.mockClear();
    appReaderPositionGet.mockReset();
    appReaderPositionGet.mockResolvedValue(null);
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  async function loadedStore(): Promise<ReaderStore> {
    libraryGetDocument.mockResolvedValue({ id: 1, filename: "t.pdf" });
    appPdfDocument.mockResolvedValue(doc);
    appPdfOutline.mockResolvedValue([]);
    const store = new ReaderStore(1);
    await store.load();
    store.fitWidth = 816;
    store.viewportHeight = 600;
    return store;
  }

  it("a new store starts in the remembered mode", async () => {
    settings.setReaderViewMode("facing");
    const store = await loadedStore();
    expect(store.viewMode).toBe("facing");
  });

  it("the released default (continuous) stands until the user first chooses", async () => {
    settings.setReaderViewMode("continuous"); // the store's own initial value
    const store = await loadedStore();
    expect(store.viewMode).toBe("continuous");
  });

  it("choosing a mode stays a per-window action and persists through the settings batch", async () => {
    settings.setReaderViewMode("continuous");
    const store = await loadedStore();
    store.scrollTop = store.pageOffsets()[1] + 1; // the reader is on page 2
    store.setViewMode("single");
    expect(store.viewMode).toBe("single"); // the window switches exactly as today
    expect(store.singleIndex).toBe(2); // carrying the current page (the released rule)
    vi.advanceTimersByTime(300); // the settings store's debounced batch
    expect(appSetSetting.mock.calls).toContainEqual(["view_mode", "single"]);
    const reopened = await loadedStore(); // a NEW window…
    expect(reopened.viewMode).toBe("single"); // …opens in the chosen mode
  });
});

describe("text layer (FR-30/UI-54, §3.3)", () => {
  const dto: PageTextDto = {
    runs: [{ text: "Page 2 of the selection probe", x: 72, y: 100, w: 300, h: 12 }],
    truncated: false,
    page_width_pt: 612,
    page_height_pt: 792,
  };

  it("maps page points to CSS px at the display scale — deterministic arithmetic, re-derived on zoom", () => {
    const at2x = textPositions(dto, 1224); // 612 pt × 2
    expect(at2x[0].x).toBeCloseTo(144, 9);
    expect(at2x[0].y).toBeCloseTo(200, 9);
    expect(at2x[0].w).toBeCloseTo(600, 9);
    expect(at2x[0].h).toBeCloseTo(24, 9);
    expect(at2x[0].text).toBe("Page 2 of the selection probe");
    const atFit = textPositions(dto, 816); // the same run at the fit base
    expect(atFit[0].x).toBeCloseTo(96, 9);
    expect(atFit[0].h).toBeCloseTo(16, 9);
    // A degenerate page width maps to zero, never to Infinity/NaN.
    expect(textPositions({ ...dto, page_width_pt: 0 }, 816)[0].x).toBe(0);
  });

  it("requests each page once per window session; a failed extraction caches the empty layer", async () => {
    const store = storeAt(816);
    appPdfText.mockResolvedValueOnce(dto);
    store.requestText(2);
    await vi.waitFor(() => expect(store.texts.has(2)).toBe(true));
    expect(store.texts.get(2)!.runs[0].text).toContain("selection probe");
    store.requestText(2);
    expect(appPdfText).toHaveBeenCalledTimes(1); // the cache answers, not the command
    appPdfText.mockRejectedValueOnce(new Error("no text layer"));
    store.requestText(3);
    await vi.waitFor(() => expect(store.texts.has(3)).toBe(true));
    expect(store.texts.get(3)!.runs).toEqual([]); // the scan case: a cached empty answer
    store.requestText(3);
    expect(appPdfText).toHaveBeenCalledTimes(2); // never re-asked
    expect(store.error).toBeNull(); // a failed extraction never becomes the window error
  });
});
