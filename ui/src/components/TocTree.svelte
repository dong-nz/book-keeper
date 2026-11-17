<script lang="ts">
  // The shared tree region of the ToC panel (FR-24 / UI-49, SPEC-v0.0.10
  // §3.8): the visible-entry loop and nothing else — one implementation used
  // by both panel presentations. The row wrapper is supplied by the caller as
  // a snippet (a `DropdownMenu.Item` in the overlay, a focusable layout row
  // in the docked panel); the row content is shared through
  // `TocEntryContent`. The released scroll rule is unchanged (DR-232): the
  // tree region carries the scroll alone, the panel box never scrolls, and
  // `min-height: 0` bounds the region by the flex column, not by a second
  // magic height.
  import type { Snippet } from "svelte";
  import type { OutlineEntry } from "$lib/types";
  import { type TocStore } from "$lib/state/reader.svelte";

  let {
    store,
    current,
    row,
  }: {
    store: TocStore;
    current?: () => number;
    row: Snippet<[{ entry: OutlineEntry; index: number; isCurrent: boolean }]>;
  } = $props();
</script>

<div class="toc-tree">
  {#each store.tocVisible() as item (item.index)}
    {@render row({ entry: item.entry, index: item.index, isCurrent: current?.() === item.index })}
  {/each}
</div>

<style>
  .toc-tree {
    min-height: 0;
    overflow-y: auto;
  }
</style>
