// The EPUB reader store's math (SPEC-v0.0.9 §3.13, DR-288/DR-303): the
// position model (cumulative uncompressed entry sizes → chapter byte starts,
// the percent ↔ chapter resolution in both directions), the bounded mount
// window (§3.12 rule 3 — viewport ± 1 chapter, pads that reproduce the full
// height), the height bookkeeping (estimate until first mount, measured kept,
// deltas against the height the layout was actually using), link routing to
// spine chapters through the sanitizer's rewritten protocol URLs, the ToC
// label and current-entry derivations, and the text-size ladder walk. These
// are the numbers AC-132/AC-135 measure in the real app's reader lanes; the
// math itself is proven here.

import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Mock } from "vitest";

const { appEpubManifest, appSetSetting, appGetSettings, libraryGetDocument } = vi.hoisted(() => ({
  appEpubManifest: vi.fn<(id: number) => Promise<unknown>>(),
  appSetSetting: vi.fn(() => Promise.resolve()),
  appGetSettings: vi.fn(() => Promise.resolve({})),
  libraryGetDocument: vi.fn<(id: number) => Promise<unknown>>(),
}));

vi.mock("../../api", () => ({
  appEpubManifest,
  appSetSetting,
  appGetSettings,
  libraryGetDocument,
  appOpenDocument: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ onCloseRequested: vi.fn(() => Promise.resolve(() => {})) }),
}));

import {
  CHAPTER_OVERSCAN,
  CHUNKED_MOUNT_BYTES,
  DEFAULT_TEXT_SIZE,
  EpubReaderStore,
  MIN_ESTIMATE_PX,
  TEXT_SIZE_STEPS,
  chapterForPercent,
  chapterStarts,
  currentTocIndexFor,
  estimateHeight,
  percentAt,
  resolveChapterHref,
  textSizeStep,
  tocLabelFor,
} from "./epubReader.svelte";
import { PAGE_GAP } from "./reader.svelte";
import type { EpubManifestDto, OutlineEntry } from "../types";
import { settings } from "./settings.svelte";

const BASE = "http://pdfkeeper-epub.localhost/42/";

/** A three-chapter book: 1000 / 2000 / 3000 uncompressed bytes, total 6000.
 *  Chapter byte starts are 0 / 1000 / 3000 — 16.667 % and 50 % boundaries. */
const SPINE = [
  { index: 1, entry: "OEBPS/ch1.xhtml", media_type: "application/xhtml+xml", size: 1000 },
  { index: 2, entry: "OEBPS/ch 2.xhtml", media_type: "application/xhtml+xml", size: 2000 },
  { index: 3, entry: "OEBPS/ch3.xhtml", media_type: "application/xhtml+xml", size: 3000 },
];

const TOC: OutlineEntry[] = [
  { title: "Part One", page: 1, depth: 1 },
  { title: "Chapter One", page: 1, depth: 2 },
  { title: "Chapter Two", page: 2, depth: 2 },
  { title: "Part Two", page: 3, depth: 1 },
];

function manifest(overrides: Partial<EpubManifestDto> = {}): EpubManifestDto {
  return {
    spine: SPINE,
    toc: TOC,
    base_url: BASE,
    language: null,
    direction: null,
    ...overrides,
  };
}

/** A store whose manifest is installed directly — the pure derivations are
 *  what's under test; `load()` itself is exercised in the fetch tests. */
function storeWith(m: EpubManifestDto): EpubReaderStore {
  const store = new EpubReaderStore(42);
  store.manifest = m;
  store.heights = m.spine.map(() => 0);
  store.outline = m.toc;
  return store;
}

describe("text-size ladder (§3.13, the DR-221 pattern)", () => {
  it("walks the six fixed steps in both directions", () => {
    expect(TEXT_SIZE_STEPS).toEqual([14, 16, 18, 20, 24, 28]);
    expect(DEFAULT_TEXT_SIZE).toBe(18);
    expect(textSizeStep(18, 1)).toBe(20);
    expect(textSizeStep(20, 1)).toBe(24);
    expect(textSizeStep(24, -1)).toBe(20);
    expect(textSizeStep(20, -1)).toBe(18);
  });

  it("clamps at the ladder ends and resets an unknown value to the default", () => {
    expect(textSizeStep(28, 1)).toBe(28);
    expect(textSizeStep(14, -1)).toBe(14);
    expect(textSizeStep(19, 1)).toBe(DEFAULT_TEXT_SIZE);
  });
});

describe("chapter heights: estimate until measured, then kept (DR-303 item 3)", () => {
  it("estimates from the uncompressed entry size with a floor", () => {
    expect(estimateHeight(0)).toBe(MIN_ESTIMATE_PX);
    expect(estimateHeight(10_000)).toBe(300); // 0.03 px per byte
    expect(estimateHeight(1_000)).toBe(MIN_ESTIMATE_PX); // 30 px < 100 floor
  });

  it("records a measured height once and returns deltas against what the layout used", () => {
    const store = storeWith(manifest());
    // First measurement corrects against the ESTIMATE the layout was using,
    // not against zero — the compensation the view applies must match the
    // offset change it follows.
    const estimate = store.chapterHeight(0);
    expect(store.recordHeight(0, 800)).toBe(800 - estimate);
    expect(store.chapterHeight(0)).toBe(800);
    // A later change (lazy image) reports only its own delta.
    expect(store.recordHeight(0, 900)).toBe(100);
    // Sub-pixel churn and zero-height reads change nothing.
    expect(store.recordHeight(0, 900.5)).toBe(0);
    expect(store.recordHeight(0, 0)).toBe(0);
    expect(store.chapterHeight(0)).toBe(900);
  });

  it("names the chunking threshold the mount rule uses", () => {
    const store = storeWith(manifest());
    const big = manifest();
    big.spine = [...SPINE, { index: 4, entry: "OEBPS/huge.xhtml", media_type: "application/xhtml+xml", size: CHUNKED_MOUNT_BYTES + 1 }];
    const withBig = storeWith(big);
    expect(withBig.isChunked(3)).toBe(true);
    expect(store.isChunked(2)).toBe(false); // 3000 bytes — mounted whole
  });
});

describe("position model (§3.13: cumulative uncompressed entry sizes)", () => {
  it("computes chapter byte starts and the book total", () => {
    const { starts, total } = chapterStarts(SPINE);
    expect(starts).toEqual([0, 1000, 3000]);
    expect(total).toBe(6000);
  });

  it("resolves a percent to the chapter containing it, clamped", () => {
    const { starts, total } = chapterStarts(SPINE);
    expect(chapterForPercent(starts, total, 0)).toBe(0);
    expect(chapterForPercent(starts, total, 16)).toBe(0); // just before the boundary
    expect(chapterForPercent(starts, total, 16.6667)).toBe(1); // exactly at it
    expect(chapterForPercent(starts, total, 50)).toBe(2); // chapter 3 starts at 50 %
    expect(chapterForPercent(starts, total, 99.9)).toBe(2);
    expect(chapterForPercent(starts, total, 120)).toBe(2); // clamped
    expect(chapterForPercent(starts, total, -5)).toBe(0);
    // A book with no declared sizes resolves to the first chapter.
    expect(chapterForPercent([0, 0, 0], 0, 75)).toBe(0);
    expect(chapterForPercent([], 0, 50)).toBe(0);
  });

  it("maps the scroll fraction inside a chapter to the book percent", () => {
    const { starts, total } = chapterStarts(SPINE);
    const sizes = SPINE.map((item) => item.size);
    // Chapter 2 (starts at 1000 bytes = 16.667 %) spans to 50 %.
    expect(percentAt(starts, sizes, total, 1, 0)).toBeCloseTo(100 / 6, 6);
    expect(percentAt(starts, sizes, total, 1, 0.5)).toBeCloseTo(33.3333, 3);
    expect(percentAt(starts, sizes, total, 1, 1)).toBeCloseTo(50, 6);
    // Fractions clamp; the book end never reads over 100.
    expect(percentAt(starts, sizes, total, 2, 1)).toBe(100);
    expect(percentAt(starts, sizes, total, 2, 2)).toBe(100);
    expect(percentAt(starts, sizes, total, 0, -1)).toBe(0);
    expect(percentAt(starts, sizes, 0, 0, 0.5)).toBe(0);
  });
});

describe("mount window (§3.12 rule 3: viewport ± 1 chapter, honest pads)", () => {
  it("mounts the touched chapters plus one on each side and pads the rest", () => {
    const store = storeWith(manifest());
    // Measure every chapter so the expectations are exact.
    store.recordHeight(0, 800);
    store.recordHeight(1, 1600);
    store.recordHeight(2, 2400);
    store.viewportHeight = 900;
    const offsets = store.chapterOffsets();
    // `stackOffsets` boundaries: content start + gap before each following
    // chapter; the final entry is the total content height (no trailing gap).
    expect(offsets).toEqual([0, 800 + PAGE_GAP, 2400 + 2 * PAGE_GAP, 4800 + 3 * PAGE_GAP - 12]);

    store.scrollTop = 1000; // inside chapter 2 (index 1)
    const range = store.mountRange();
    expect(CHAPTER_OVERSCAN).toBe(1);
    expect(range.start).toBe(0);
    expect(range.end).toBe(3); // touched {1} + overscan {0, 2}
    expect(range.padTop).toBe(0);
    expect(range.padBottom).toBe(0);

    store.scrollTop = 3000; // inside chapter 3 (the last)
    const last = store.mountRange();
    expect(last.start).toBe(1); // {2} + overscan {1}; nothing before that exists to drop
    expect(last.end).toBe(3); // clamped to the spine
    expect(last.padTop).toBe(offsets[1]);
    expect(last.padBottom).toBe(0);

    // At the book start the viewport (900) already reaches past chapter 1's
    // boundary (812) into chapter 2 — touched {0,1}, overscan covers the
    // last one, so everything mounts and no pads are needed.
    store.scrollTop = 0;
    const first = store.mountRange();
    expect(first.start).toBe(0);
    expect(first.end).toBe(3);
    expect(first.padTop + first.padBottom).toBe(0);

    // A four-chapter book to make an unmounted tail exist: the pads plus the
    // mounted span reproduce the full content height (NFR-01).
    const four = manifest();
    four.spine = [
      ...SPINE,
      { index: 4, entry: "OEBPS/ch4.xhtml", media_type: "application/xhtml+xml", size: 4000 },
    ];
    const store4 = storeWith(four);
    store4.recordHeight(0, 800);
    store4.recordHeight(1, 1600);
    store4.recordHeight(2, 2400);
    store4.recordHeight(3, 3200);
    store4.viewportHeight = 100;
    store4.scrollTop = 2000; // inside chapter 2 only
    const mid = store4.mountRange();
    expect(mid.start).toBe(0);
    expect(mid.end).toBe(3); // touched {1} + overscan {0, 2}
    expect(mid.padTop).toBe(0);
    expect(mid.padBottom).toBe(3200); // chapter 4's measured height, nothing else unmounted
  });

  it("derives the current chapter from the store's own scroll source (DR-155)", () => {
    const store = storeWith(manifest());
    store.recordHeight(0, 800);
    store.recordHeight(1, 1600);
    store.recordHeight(2, 2400);
    store.viewportHeight = 900;
    store.scrollTop = 0;
    expect(store.currentChapter).toBe(1);
    store.scrollTop = 815; // past chapter 1's 800 px + the 12 px gap
    expect(store.currentChapter).toBe(2);
    store.scrollTop = 2424; // exactly chapter 3's start
    expect(store.currentChapter).toBe(3);
    // The readout follows the same source: chapter 3 start = 50 %.
    expect(store.bookPercent).toBeCloseTo(50, 6);
    store.scrollTop = 0;
    expect(store.bookPercent).toBeCloseTo(0, 6);
  });

  it("reads 100% once the viewport bottom reaches the end of the last chapter (DR-306 end-of-book rule)", () => {
    const store = storeWith(manifest());
    store.recordHeight(0, 800);
    store.recordHeight(1, 1600);
    store.recordHeight(2, 2400);
    store.viewportHeight = 900;
    // Chapter 3 spans 2424..4824 px of stack and the 50..100 % slice.
    store.scrollTop = 3923; // viewport bottom 4823 — one pixel short of the end
    expect(store.bookPercent).toBeCloseTo(50 + ((3923 - 2424) / 2400) * 50, 6);
    store.scrollTop = 3924; // viewport bottom exactly at the end of the book
    expect(store.bookPercent).toBe(100);
    // The physical flow bottom (past the last chapter's range through the
    // stack's bottom pad) reads 100 too — the clamp holds for any landing.
    store.scrollTop = 4200;
    expect(store.bookPercent).toBe(100);
    // Earlier chapters are untouched: at chapter 2 with the viewport bottom
    // past chapter 3's start the readout still interpolates chapter 2's slice.
    store.scrollTop = 1600;
    expect(store.bookPercent).toBeCloseTo((1000 / 6000) * 100 + ((1600 - 812) / 1600) * (2000 / 6000) * 100, 6);
  });
});

describe("link routing (§3.13, the sanitizer's rewritten form, DR-302)", () => {
  it("routes a bare fragment to the current chapter", () => {
    expect(resolveChapterHref("#sec-3", BASE, SPINE)).toEqual({ chapter: null, fragment: "sec-3" });
  });

  it("resolves protocol URLs — percent-encoded or not — to spine chapters", () => {
    expect(resolveChapterHref(`${BASE}OEBPS/ch1.xhtml`, BASE, SPINE)).toEqual({
      chapter: 0,
      fragment: "",
    });
    // The sanitizer's rewritten form percent-encodes the entry path.
    expect(resolveChapterHref(`${BASE}OEBPS/ch%202.xhtml#fig`, BASE, SPINE)).toEqual({
      chapter: 1,
      fragment: "fig",
    });
    expect(resolveChapterHref(`${BASE}OEBPS/ch3.xhtml`, BASE, SPINE)).toEqual({
      chapter: 2,
      fragment: "",
    });
  });

  it("refuses anything that is not an in-book chapter", () => {
    expect(resolveChapterHref("https://example.com/a.html", BASE, SPINE)).toBeNull();
    expect(resolveChapterHref("OEBPS/ch2.xhtml", BASE, SPINE)).toBeNull(); // relative leftover
    expect(resolveChapterHref(`${BASE}OEBPS/other.xhtml`, BASE, SPINE)).toBeNull();
    expect(resolveChapterHref(`${BASE}OEBPS/ch%zz.xhtml`, BASE, SPINE)).toBeNull(); // broken encoding
  });
});

describe("ToC derivations (§3.13)", () => {
  it("labels a chapter with the first entry targeting it, else nothing", () => {
    expect(tocLabelFor(TOC, 0)).toBe("Part One"); // first, not "Chapter One"
    expect(tocLabelFor(TOC, 1)).toBe("Chapter Two");
    expect(tocLabelFor(TOC, 2)).toBe("Part Two");
    expect(tocLabelFor(TOC, 3)).toBeNull();
    expect(tocLabelFor([], 0)).toBeNull();
  });

  it("highlights the last entry (document order) the scroll has reached", () => {
    expect(currentTocIndexFor(TOC, 1)).toBe(1); // both Part One and Chapter One sit on chapter 1
    expect(currentTocIndexFor(TOC, 2)).toBe(2);
    expect(currentTocIndexFor(TOC, 3)).toBe(3);
    expect(currentTocIndexFor(TOC, 0)).toBe(-1); // before any entry
    expect(currentTocIndexFor([], 1)).toBe(-1);
  });

  it("prefers the ToC label, falls back to the chapter's own heading, else none", () => {
    const store = storeWith(manifest());
    expect(store.chapterLabel(1)).toBe("Part One");
    const headings = new Map(store.chapterHeadings);
    headings.set(4, "An Untitled Chapter's Own Heading");
    store.chapterHeadings = headings;
    expect(store.chapterLabel(4)).toBe("An Untitled Chapter's Own Heading");
    expect(store.chapterLabel(5)).toBeNull();
  });
});

describe("store navigation (the released goToPage path, flow version)", () => {
  it("lands at a chapter's start with the released +1 px snap tolerance, clamped", () => {
    const store = storeWith(manifest());
    const landed: number[] = [];
    store.onScrollTo = (offset) => landed.push(offset);
    store.recordHeight(0, 800);
    store.recordHeight(1, 1600);
    store.recordHeight(2, 2400);
    const offsets = store.chapterOffsets();
    store.goToPage(2);
    expect(landed).toEqual([offsets[1] + 1]);
    store.goToPage(0); // clamped to the first chapter
    store.goToPage(99); // clamped to the last
    expect(landed).toEqual([offsets[1] + 1, offsets[0] + 1, offsets[2] + 1]);
    store.goToPage(2.7); // truncation, the released page-field rule
    expect(landed[3]).toBe(offsets[1] + 1);
  });

  it("resolves a percent jump to the start of the chapter containing it", () => {
    const store = storeWith(manifest());
    const landed: number[] = [];
    store.onScrollTo = (offset) => landed.push(offset);
    store.recordHeight(0, 800);
    store.recordHeight(1, 1600);
    store.recordHeight(2, 2400);
    store.jumpToPercent(16.6667); // exactly the chapter-2 boundary
    store.jumpToPercent(75); // inside chapter 3
    store.jumpToPercent(-10); // clamped to the book start
    expect(landed).toEqual([
      store.chapterOffsets()[1] + 1,
      store.chapterOffsets()[2] + 1,
      store.chapterOffsets()[0] + 1,
    ]);
  });

  it("routes viewport and notch scrolls through the view's callbacks", () => {
    const store = storeWith(manifest());
    const scrolled: number[] = [];
    store.onScrollBy = (delta) => scrolled.push(delta);
    store.viewportHeight = 900;
    store.scrollViewport(1);
    store.scrollViewport(-1);
    store.scrollNotch(100);
    expect(scrolled).toEqual([900, -900, 100]);
  });
});

describe("chapter fetch (the content protocol, the released request-dedup rule)", () => {
  let fetchMock: Mock;

  beforeEach(() => {
    fetchMock = vi.fn(() =>
      Promise.resolve({ ok: true, text: () => Promise.resolve("<html><body>ok</body></html>") }),
    );
    vi.stubGlobal("fetch", fetchMock);
  });

  it("builds the chapter URL from the manifest base and the encoded entry", async () => {
    const store = storeWith(manifest());
    await store.requestChapter(2);
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(fetchMock.mock.calls[0][0]).toBe(`${BASE}OEBPS/ch%202.xhtml`);
    expect(store.chapterText.get(2)).toContain("ok");
  });

  it("deduplicates in-flight requests for the same chapter", async () => {
    const store = storeWith(manifest());
    await Promise.all([store.requestChapter(1), store.requestChapter(1), store.requestChapter(1)]);
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it("marks a failed fetch on the chapter, not the window", async () => {
    fetchMock.mockImplementation(() => Promise.resolve({ ok: false, text: () => Promise.resolve("") }));
    const store = storeWith(manifest());
    await store.requestChapter(1);
    expect(store.chapterErrors.has(1)).toBe(true);
    expect(store.chapterText.has(1)).toBe(false);
    expect(store.status).toBe("loading"); // the book never comes down
  });

  it("ignores indexes outside the spine", async () => {
    const store = storeWith(manifest());
    await store.requestChapter(0);
    await store.requestChapter(4);
    expect(fetchMock).not.toHaveBeenCalled();
  });
});

describe("text size flows through the settings store (§3.9 consequence 6)", () => {
  it("the strip's walk persists through the settings store, clamped", () => {
    settings.epubTextSize = 18;
    const store = storeWith(manifest());
    store.changeTextSize(1);
    expect(settings.epubTextSize).toBe(20);
    for (let i = 0; i < 10; i++) store.changeTextSize(1);
    expect(settings.epubTextSize).toBe(28); // the ladder top
    store.changeTextSize(-1);
    expect(settings.epubTextSize).toBe(24);
  });
});
