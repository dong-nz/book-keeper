// UI-02 MODIFIED (SPEC-v0.0.14 §3.2): the tag picker's decisions — chip
// derivation, list filtering, the membership toggle, the inline-create call.
// The units drive the pure module directly (the dragstrip.ts precedent).

import { describe, expect, it } from "vitest";

import {
  chipsFor,
  filterTags,
  inlineTagAction,
  matchSegments,
  moveCursorIndex,
  toggleTagId,
} from "./tagpicker";

const TAGS = [
  { id: 3, name: "Contracts" },
  { id: 1, name: "Invoices" },
  { id: 2, name: "standards" },
];

describe("chipsFor (UI-02: chips render in catalog order)", () => {
  it("keeps the catalog's own order, not the membership order", () => {
    expect(chipsFor([2, 1], TAGS).map((t) => t.id)).toEqual([1, 2]);
  });
  it("drops ids that are no longer in the catalog", () => {
    expect(chipsFor([1, 9], TAGS).map((t) => t.id)).toEqual([1]);
  });
  it("an empty membership renders no chips", () => {
    expect(chipsFor([], TAGS)).toEqual([]);
  });
});

describe("filterTags (UI-02: the filter field narrows the list as typed)", () => {
  it("an empty or whitespace query lists everything", () => {
    expect(filterTags(TAGS, "").map((t) => t.id)).toEqual([3, 1, 2]);
    expect(filterTags(TAGS, "  ").length).toBe(3);
  });
  it("case-insensitive substring narrowing", () => {
    expect(filterTags(TAGS, "CON").map((t) => t.id)).toEqual([3]);
    expect(filterTags(TAGS, "invo").map((t) => t.id)).toEqual([1]);
    expect(filterTags(TAGS, "STAND").map((t) => t.id)).toEqual([2]);
  });
  it("a query nothing matches lists nothing", () => {
    expect(filterTags(TAGS, "zzz")).toEqual([]);
  });
});

describe("toggleTagId (UI-02: clicking a list entry adds or removes it)", () => {
  it("adds an id and keeps the set normalized ascending", () => {
    expect(toggleTagId([2], 1)).toEqual([1, 2]);
    expect(toggleTagId([5], 10)).toEqual([5, 10]);
  });
  it("removes an id that is already a member", () => {
    expect(toggleTagId([1, 2, 3], 2)).toEqual([1, 3]);
  });
});

describe("inlineTagAction (UI-02: Enter joins an exact match or creates)", () => {
  it("empty or whitespace does nothing", () => {
    expect(inlineTagAction("", TAGS)).toEqual({ kind: "none" });
    expect(inlineTagAction("   ", TAGS)).toEqual({ kind: "none" });
  });
  it("a case-insensitive exact match joins the existing tag", () => {
    expect(inlineTagAction("inVOICES", TAGS)).toEqual({ kind: "join", id: 1 });
    expect(inlineTagAction("Standards ", TAGS)).toEqual({ kind: "join", id: 2 });
  });
  it("a non-exact name creates the typed name (trimmed)", () => {
    expect(inlineTagAction("  Receipts  ", TAGS)).toEqual({ kind: "create", name: "Receipts" });
    // a substring match is NOT an exact match — a partial filter still creates
    expect(inlineTagAction("Contract", TAGS)).toEqual({ kind: "create", name: "Contract" });
  });
});

describe("matchSegments (UI-02 DR-437: the filter marks the matched substring)", () => {
  it("an empty or whitespace query marks nothing", () => {
    expect(matchSegments("Invoices", "")).toEqual([{ text: "Invoices", match: false }]);
    expect(matchSegments("Invoices", "  ").length).toBe(1);
  });
  it("the first case-insensitive occurrence is the marked segment", () => {
    // the segments keep the NAME's own case — the query only locates the cut
    expect(matchSegments("Contracts", "CON")).toEqual([
      { text: "Con", match: true },
      { text: "tracts", match: false },
    ]); // a name containing the query twice marks only the first occurrence
    expect(matchSegments("re-re", "re")).toEqual([
      { text: "re", match: true },
      { text: "-re", match: false },
    ]);
  });
  it("a mid-name match splits into before / marked / after", () => {
    expect(matchSegments("design-review", "sign")).toEqual([
      { text: "de", match: false },
      { text: "sign", match: true },
      { text: "-review", match: false },
    ]);
  });
  it("a query nothing matches leaves the name unmarked", () => {
    expect(matchSegments("Invoices", "zzz")).toEqual([{ text: "Invoices", match: false }]);
  });
});

describe("moveCursorIndex (AC-205: ↑/↓ move the cursor row, wrapping)", () => {
  it("an empty list has no cursor", () => {
    expect(moveCursorIndex(0, -1, 1)).toBe(-1);
  });
  it("from no cursor, ↓ enters at the first row and ↑ at the last", () => {
    expect(moveCursorIndex(3, -1, 1)).toBe(0);
    expect(moveCursorIndex(3, -1, -1)).toBe(2);
  });
  it("steps move and wrap in both directions", () => {
    expect(moveCursorIndex(3, 0, 1)).toBe(1);
    expect(moveCursorIndex(3, 1, -1)).toBe(0);
    expect(moveCursorIndex(3, 2, 1)).toBe(0);
    expect(moveCursorIndex(3, 0, -1)).toBe(2);
  });
});
