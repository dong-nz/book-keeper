<script lang="ts">
  // The docked ToC panel presentation (FR-24 / UI-49, SPEC-v0.0.10 §3.8):
  // the same panel content — the shared toolbar controls (`TocControls`), the
  // shared tree (`TocTree`), the shared row content (`TocEntryContent`) —
  // rendered as window layout, not as an `OverlayAnchor` popup: the reader
  // window's host places this element in the page area's left column, from
  // below the chrome strip to the control strip.
  // §3.8 extends the released `Esc` priority rule (M7) to the docked state:
  // the panel registers with the window's guard (the released
  // `OverlayAnchor` mechanism — mount is the open signal), so the window's
  // own `Esc` listener defers, and the panel's own `Esc` handler hides it
  // first, keeping the pinned position (DR-337 — the un-pin path is the pin
  // control in the panel's own toolbar).
  // The rows here are not menu items — they are focusable layout rows with
  // the same keys: Enter/Space selects (navigates, and the panel stays open
  // — the close-on-select rule belongs to the unpinned state only, §3.8),
  // ArrowRight/ArrowLeft toggle a focused parent's subtree. Entry clicks
  // navigate through the store's `goToPage`, the same path as everywhere.
  import OverlayAnchor from "./OverlayAnchor.svelte";
  import TocControls from "./TocControls.svelte";
  import TocEntryContent from "./TocEntryContent.svelte";
  import TocTree from "./TocTree.svelte";
  import { type TocStore } from "$lib/state/reader.svelte";
  import { t } from "$lib/i18n";

  // Same shared shape as the overlay presentation (DR-303 item 7): `current`
  // is the optional current-entry resolver the EPUB host passes (§3.13).
  let {
    store,
    current,
  }: {
    store: TocStore;
    current?: () => number;
  } = $props();

  function onKeydown(event: KeyboardEvent): void {
    // The panel's `Esc`: hides the docked panel first, the window stays
    // (UI-48/AC-148 "Esc closes the pinned panel first"). DR-337: `Esc`
    // keeps the pinned position — the ToC control re-shows the panel docked;
    // the un-pin path is the panel's own pin control. The window's own
    // listener already deferred on the registered overlay count.
    if (event.key !== "Escape" || event.defaultPrevented) return;
    event.preventDefault();
    store.setTocDockedOpen(false);
  }
</script>

<svelte:window onkeydown={onKeydown} />

<aside class="menu toc-panel toc-docked" aria-label={t("reader.toc")}>
  <OverlayAnchor />
  <!-- The toolbar row: the same chrome recipe (`.toc-panel .toc-toolbar`,
       base.css) overridden flush for the docked state (DR-335), and the same
       shared controls as the overlay variant — the expand/collapse flip-flop
       and the pin toggle at the row's right end, here pressed: its click
       un-pins and closes the panel (the single un-pin path, DR-337). -->
  <div class="toc-toolbar">
    <TocControls {store} />
  </div>
  <TocTree {store} {current}>
    {#snippet row({ entry, index, isCurrent })}
      <div
        class="menu-item toc-entry"
        role="button"
        tabindex="0"
        data-current={isCurrent || undefined}
        onclick={() => store.goToPage(entry.page)}
        onkeydown={(event) => {
          // Only the row itself is activated by its keys; the chevron button
          // keeps its native activation (the released pattern).
          if (event.target !== event.currentTarget) return;
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            store.goToPage(entry.page);
          } else if (store.isTocParent(index) && (event.key === "ArrowRight" || event.key === "ArrowLeft")) {
            event.preventDefault();
            store.toggleTocEntry(index);
          }
        }}
      >
        <TocEntryContent {store} {entry} {index} current={isCurrent} />
      </div>
    {/snippet}
  </TocTree>
</aside>

<style>
  /* The docked panel is layout: it fills the host's left column — the grid
     row between the chrome strip and the control strip is its height — so the
     overlay's viewport cap (DR-207) does not apply, and the menu's overlay
     chrome (radius, shadow, floating edge) is dropped at the window's edge. */
  .toc-docked {
    max-height: none;
    height: 100%;
    border-radius: 0;
    border-left: none;
    box-shadow: none;
    /* DR-343 (Phase-B feedback item 3): the panel is layout, and a grid
       item honors the z-index the `.menu` overlay recipe brings — 50 put it
       above the settings modal (40/41). The docked panel paints above the
       page surface (its column's edge against the pages' shadows) and below
       every floating overlay: modals 40/41, menus 50, tooltips 60. */
    z-index: 10;
  }

  /* DR-335 (Phase-B feedback item 2): the docked toolbar renders flush —
     the rounded top corners belong to the floating overlay recipe (DR-226)
     and read as corner radius on a panel that sits against the window edge;
     the row is also shorter than the overlay's (vertical padding 8 → 4 px).
     The controls and the hairline are unchanged. */
  .toc-docked .toc-toolbar {
    border-radius: 0;
    padding-block: 4px;
  }

  /* The layout rows keep the menu row's hover grammar (`.menu-item`,
     base.css); keyboard focus shows the app's focus ring. */
  .toc-entry:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
</style>
