// UI-09 MODIFIED (SPEC-v0.0.14 §3.6): the strip's chip derivation, the
// per-chip removal patches, Clear all, and the width-driven overflow decision.
// The units drive the pure module directly.

import { describe, expect, it } from "vitest";

import {
  activeFilterChips,
  clearAllPatch,
  hasBrowseFilter,
  inlineCount,
  removeChipPatch,
} from "./filterstrip";
import type { FilterChip, FilterLike } from "./filterstrip";

function filter(overrides: Partial<FilterLike> = {}): FilterLike {
  return { favorites_only: false, missing_only: false, reading_only: false, tag_ids: [], ...overrides };
}

describe("activeFilterChips / hasBrowseFilter (UI-09: one chip per active element)", () => {
  it("no filter elements → no chips, the strip does not render", () => {
    expect(activeFilterChips(filter())).toEqual([]);
    expect(hasBrowseFilter(filter())).toBe(false);
  });
  it("a mode filter and tag filters: the mode first, then the tag ids", () => {
    const chips = activeFilterChips(filter({ missing_only: true, tag_ids: [7, 2] }));
    expect(chips).toEqual([
      { kind: "mode", mode: "missing" },
      { kind: "tag", id: 7 },
      { kind: "tag", id: 2 },
    ]);
    expect(hasBrowseFilter(filter({ tag_ids: [7] }))).toBe(true);
  });
  it("several mode filters stack in the Browse menu's own order", () => {
    const chips = activeFilterChips(filter({ reading_only: true, favorites_only: true, missing_only: true }));
    expect(chips).toEqual([
      { kind: "mode", mode: "favorites" },
      { kind: "mode", mode: "missing" },
      { kind: "mode", mode: "reading" },
    ]);
  });
});

describe("removeChipPatch (UI-09: × removes only that element)", () => {
  const both = filter({ favorites_only: true, tag_ids: [1, 2] });
  it("a mode chip clears only that mode; the tags stay", () => {
    expect(removeChipPatch(both, { kind: "mode", mode: "favorites" })).toEqual({
      favorites_only: false,
      missing_only: false,
      reading_only: false,
    });
  });
  it("a tag chip removes only its id; the mode stays", () => {
    expect(removeChipPatch(both, { kind: "tag", id: 1 })).toEqual({ tag_ids: [2] });
  });
});

describe("clearAllPatch (UI-09: Clear all removes every filter element)", () => {
  it("clears modes and tags — and carries no search field at all", () => {
    const patch = clearAllPatch();
    expect(patch).toEqual({ favorites_only: false, missing_only: false, reading_only: false, tag_ids: [] });
    expect("q" in patch).toBe(false);
  });
});

describe("inlineCount (DR-434: the overflow decision is deterministic)", () => {
  it("no chips → nothing to render", () => {
    expect(inlineCount([], 500, 60, 8)).toBe(0);
  });
  it("everything fits inline → all inline, no control", () => {
    expect(inlineCount([100, 120], 238, 60, 8)).toBe(2); // 100+8+120 = 228 ≤ 238
    expect(inlineCount([100, 120], 228, 60, 8)).toBe(2); // exact fit is inline
  });
  it("not everything fits → the control's width is reserved and chips pack in order", () => {
    // available 260: total 100+8+120+8+90 = 326 > 260 → budget 260-60-8 = 192;
    // 100 fits (100), 120 does not (228 > 192) → one inline, rest behind +N.
    expect(inlineCount([100, 120, 90], 260, 60, 8)).toBe(1);
    // same chips, wider room: budget 300-60-8 = 232; 100+8+120 = 228 fits,
    // +90 (326) does not → two inline
    expect(inlineCount([100, 120, 90], 300, 60, 8)).toBe(2);
  });
  it("the decision depends only on the numbers", () => {
    const widths = [40, 55, 70, 45];
    for (const available of [150, 220, 400]) {
      const a = inlineCount(widths, available, 60, 8);
      const b = inlineCount(widths, available, 60, 8);
      expect(a).toBe(b);
      expect(a).toBeLessThanOrEqual(widths.length);
    }
  });
});
