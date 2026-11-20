// Active-filter strip pure logic (UI-09 MODIFIED, SPEC-v0.0.14 §3.6): the
// derivation of the strip's chips from the filter state, the per-chip removal
// patches, the Clear-all patch, and the width-driven overflow decision — kept
// pure so the units drive them directly.

export interface FilterLike {
  favorites_only: boolean;
  missing_only: boolean;
  reading_only: boolean;
  tag_ids: number[];
}

export type FilterChip =
  | { kind: "mode"; mode: "favorites" | "missing" | "reading" }
  | { kind: "tag"; id: number };

/** One chip per active filter element: the mode first (the Browse menu's
 *  own order), then the tag ids. Search is never a chip (UI-09 MODIFIED). */
export function activeFilterChips(f: FilterLike): FilterChip[] {
  const chips: FilterChip[] = [];
  if (f.favorites_only) chips.push({ kind: "mode", mode: "favorites" });
  if (f.missing_only) chips.push({ kind: "mode", mode: "missing" });
  if (f.reading_only) chips.push({ kind: "mode", mode: "reading" });
  for (const id of f.tag_ids) chips.push({ kind: "tag", id });
  return chips;
}

/** The strip renders exactly when this is true. */
export function hasBrowseFilter(f: FilterLike): boolean {
  return activeFilterChips(f).length > 0;
}

/** The setFilter patch that removes only this element (a mode chip resets
 *  the mode and leaves the tag set; a tag chip removes only its id). */
export function removeChipPatch(f: FilterLike, chip: FilterChip): Partial<FilterLike> {
  if (chip.kind === "mode") {
    return {
      favorites_only: chip.mode === "favorites" ? false : f.favorites_only,
      missing_only: chip.mode === "missing" ? false : f.missing_only,
      reading_only: chip.mode === "reading" ? false : f.reading_only,
    };
  }
  return { tag_ids: f.tag_ids.filter((id) => id !== chip.id) };
}

/** Clear all: no mode, no tags — search untouched (never read, never written). */
export function clearAllPatch(): Partial<FilterLike> {
  return { favorites_only: false, missing_only: false, reading_only: false, tag_ids: [] };
}

/** The overflow decision (DR-434): from the measured chip widths, how many
 *  chips render inline. All fit → all inline, no control. Otherwise the
 *  `+N` control's width is reserved and chips are packed in order into what
 *  remains. Deterministic from the numbers alone — the shape the narrow-width
 *  lane can assert. */
export function inlineCount(
  widths: number[],
  available: number,
  controlWidth: number,
  gap: number,
): number {
  const total = widths.reduce((sum, w) => sum + w, 0) + gap * Math.max(0, widths.length - 1);
  if (widths.length === 0 || total <= available) return widths.length;
  const budget = available - controlWidth - gap;
  let used = 0;
  let count = 0;
  for (const w of widths) {
    const need = w + (count > 0 ? gap : 0);
    if (used + need > budget) break;
    used += need;
    count += 1;
  }
  return count;
}
