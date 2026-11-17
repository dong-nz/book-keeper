<script lang="ts">
  // The shared entry-row content of the ToC panel (FR-24 / UI-49,
  // SPEC-v0.0.10 §3.8): the indent cell, the parent's toggle chevron, the
  // title and the right-aligned target page — the released row structure and
  // classes unchanged (DR-218), rendered inside both row wrappers: the
  // overlay's `DropdownMenu.Item` (`TocPanel`) and the docked panel's
  // focusable layout row (`TocDocked`). The wrapper differs per presentation;
  // the row content has one implementation (§3.8: "the same component, the
  // same tree"). The current-entry highlight is passed in as a flag — the
  // resolver stays with the caller (§3.13); the visual rule is the released
  // one (the accent on the title).
  import Icon from "./Icon.svelte";
  import type { OutlineEntry } from "$lib/types";
  import { type TocStore } from "$lib/state/reader.svelte";
  import { t } from "$lib/i18n";

  let {
    store,
    entry,
    index,
    current = false,
  }: {
    store: TocStore;
    entry: OutlineEntry;
    index: number;
    current?: boolean;
  } = $props();

  const isParent = $derived(store.isTocParent(index));
  const expanded = $derived(isParent && !store.tocCollapsed.has(index));
</script>

<span class="toc-indent" style="width: {(entry.depth - 1) * 12}px"></span>
{#if isParent}
  <button
    type="button"
    class="toc-toggle"
    data-tauri-drag-region="false"
    aria-expanded={expanded}
    aria-label={t(expanded ? "reader.toc.collapseEntry" : "reader.toc.expandEntry")}
    onclick={(event) => {
      // The chevron toggles without jumping: the click must not reach the
      // row's activation handler — the released pattern, in both wrappers.
      event.preventDefault();
      event.stopPropagation();
      store.toggleTocEntry(index);
    }}
  >
    <Icon name={expanded ? "chevron-down" : "chevron-right"} />
  </button>
{/if}
<span class="toc-title" class:current>{entry.title}</span>
<span class="toc-page">{entry.page}</span>

<style>
  /* The tree indent (UI-33): the released (depth − 1) × 12 px, a real indent
     cell so the chevron sits at the entry's own level. */
  .toc-indent {
    flex: none;
  }

  /* The parent chevron: a quiet inline control inside the row, the same
     icon-button language as the strip, without the button chrome. */
  .toc-toggle {
    flex: none;
    display: inline-flex;
    align-items: center;
    padding: 0;
    border: none;
    background: none;
    color: var(--text-secondary);
    cursor: pointer;
  }

  .toc-toggle:hover {
    color: var(--text-primary);
  }

  .toc-title {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* The current-entry highlight (§3.13, EPUB reader only): the entry the
     reader's scroll source currently sits on, named the way the strip's
     active states are named (the accent, no extra decoration) — the released
     rule, now carried by the title itself so both wrappers share it. */
  .toc-title.current {
    color: var(--accent);
  }

  /* The page number sits at the item's trailing end (UI-33: right-aligned). */
  .toc-page {
    margin-left: auto;
    color: var(--text-secondary);
    font-variant-numeric: tabular-nums;
  }
</style>
