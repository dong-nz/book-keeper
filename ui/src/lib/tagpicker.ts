// Tag picker pure logic (UI-02 MODIFIED, SPEC-v0.0.14 §3.2): the decisions
// behind the Details tags picker — chip derivation, list filtering, the
// membership toggle, and the inline-create decision — so the units drive them
// directly (the dragstrip.ts precedent, DR-424).

export interface TagLike {
  id: number;
  name: string;
}

/** Chips render in catalog order (the store keeps tags name-sorted). */
export function chipsFor<T extends TagLike>(ids: number[], tags: T[]): T[] {
  return tags.filter((tag) => ids.includes(tag.id));
}

/** Case-insensitive substring narrowing; an empty query lists everything. */
export function filterTags<T extends TagLike>(tags: T[], query: string): T[] {
  const needle = query.trim().toLowerCase();
  if (needle === "") return [...tags];
  return tags.filter((tag) => tag.name.toLowerCase().includes(needle));
}

/** Membership toggle, kept normalized ascending (the released set rule). */
export function toggleTagId(ids: number[], id: number): number[] {
  return ids.includes(id) ? ids.filter((x) => x !== id) : [...ids, id].sort((a, b) => a - b);
}

/** What Enter in the filter field means: nothing (empty), joining an exact
 *  (case-insensitive) match already in the catalog, or creating a new tag
 *  with the typed name (the released create path owns the duplicate check). */
export function inlineTagAction(
  query: string,
  tags: TagLike[],
): { kind: "none" } | { kind: "join"; id: number } | { kind: "create"; name: string } {
  const name = query.trim();
  if (name === "") return { kind: "none" };
  const exact = tags.find((tag) => tag.name.toLowerCase() === name.toLowerCase());
  if (exact) return { kind: "join", id: exact.id };
  return { kind: "create", name };
}

/** DR-437 (§3.2): the name split for the match marking — the first
 *  case-insensitive occurrence of the trimmed query is the marked segment;
 *  no query or no occurrence means one unmarked segment. */
export function matchSegments(
  name: string,
  query: string,
): { text: string; match: boolean }[] {
  const needle = query.trim().toLowerCase();
  if (needle === "") return [{ text: name, match: false }];
  const at = name.toLowerCase().indexOf(needle);
  if (at < 0) return [{ text: name, match: false }];
  return [
    { text: name.slice(0, at), match: false },
    { text: name.slice(at, at + needle.length), match: true },
    { text: name.slice(at + needle.length), match: false },
  ].filter((segment) => segment.text !== "");
}

/** DR-437 (AC-205): cursor movement through the filtered list — wrapping
 *  (the released menu convention); -1 is "no cursor", and from no cursor
 *  ↓ enters at the first row, ↑ at the last. An empty list has no cursor. */
export function moveCursorIndex(count: number, current: number, delta: number): number {
  if (count <= 0) return -1;
  if (current < 0) return delta < 0 ? count - 1 : 0;
  return (current + delta + count) % count;
}
