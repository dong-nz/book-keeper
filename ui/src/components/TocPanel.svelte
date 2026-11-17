<script lang="ts">
  // The table-of-contents panel's strip control and its overlay presentation
  // (FR-18 / UI-33 as revised by DR-218, DR-223, DR-226, DR-232, DR-242,
  // DR-244 — SPEC-v0.0.7 §3.5 — and by FR-24 / UI-49, SPEC-v0.0.10 §3.8):
  // the released menu mechanism — a DropdownMenu anchored to the strip's
  // icon-only ToC control (the UI-13 pattern with a tooltip) — rendering a
  // collapsible tree through the shared parts: the toolbar controls
  // (`TocControls`), the tree region (`TocTree`) and the row content
  // (`TocEntryContent`), each with one implementation shared with the docked
  // presentation (`TocDocked`).
  // Unpinned is the released behavior, unchanged: the toolbar row is one menu
  // item (`closeOnSelect` false, and per DR-242 no activation of its own
  // beyond the whole-tree action), the panel opens collapsed to the first
  // level, entries jump through the store's `goToPage` and the selection
  // closes the panel, `OverlayAnchor` registers the open overlay so the
  // window's `Esc` closes the panel before it would close the window (§5.2).
  // Pinned (UI-49): the overlay is suppressed — the strip control stays in
  // place, reflects the open docked panel (`aria-pressed`, the released
  // toggle affordance), and its click closes the panel: "closing a pinned
  // panel by the ToC control resets it to the unpinned state". The docked
  // panel itself is window layout, rendered by the host (§3.8).
  // The toolbar row's Enter/Space keyboard parity (DR-242) is kept for the
  // expand/collapse action; the target guard keeps it from stealing the pin
  // control's own activation.
  import { DropdownMenu } from "bits-ui";
  import Icon from "./Icon.svelte";
  import OverlayAnchor from "./OverlayAnchor.svelte";
  import Tooltip from "./Tooltip.svelte";
  import TocControls from "./TocControls.svelte";
  import TocEntryContent from "./TocEntryContent.svelte";
  import TocTree from "./TocTree.svelte";
  import { type TocStore } from "$lib/state/reader.svelte";
  import { t } from "$lib/i18n";

  // DR-303 item 7: the panel is one component for both readers — it reads the
  // structural `TocStore` shape (the tree math is shared, the `page` field is
  // the target in both readers). `current` is optional: the EPUB strip passes a
  // reader-derived current-entry resolver (§3.13 highlight rule); the released
  // PDF strip does not, and its panel behavior is unchanged.
  let {
    store,
    current,
  }: {
    store: TocStore;
    current?: () => number;
  } = $props();
</script>

{#if store.tocPinned}
  <!-- UI-49 (revised by DR-337): the pinned state's strip control — the same
       control, the same position, the released toggle affordance; its click
       hides the docked panel keeping the pinned position, and clicking again
       shows it at the same docked position. The pressed state reflects the
       panel's visibility; the un-pin path is the panel's own pin control. -->
  <Tooltip text={t("reader.toc")}>
    {#snippet control({ props })}
      <button
        {...props}
        type="button"
        class="control icon-button"
        data-tauri-drag-region="false"
        aria-label={t("reader.toc")}
        aria-pressed={store.tocDockedOpen}
        onclick={() => store.setTocDockedOpen(!store.tocDockedOpen)}
      >
        <Icon name="list" />
      </button>
    {/snippet}
  </Tooltip>
{:else}
  <DropdownMenu.Root>
    <Tooltip text={t("reader.toc")}>
      {#snippet control({ props })}
        <DropdownMenu.Trigger
          {...props}
          class="control icon-button"
          data-tauri-drag-region="false"
          aria-label={t("reader.toc")}
        >
          <Icon name="list" />
        </DropdownMenu.Trigger>
      {/snippet}
    </Tooltip>
    <DropdownMenu.Content class="menu toc-panel">
      <OverlayAnchor />
      <!-- The toolbar (UI-33/DR-218, one row with the controls per DR-223,
           rendered as chrome per DR-226): a menu item so keyboard navigation
           reaches it, `closeOnSelect={false}` so the tree updates under a
           still-open panel; the `.toc-toolbar` / `.control` styling (base.css)
           gives it the strip ground, the bottom hairline and a compact
           content-width button. The controls themselves are shared with the
           docked presentation (`TocControls`): the expand/collapse flip-flop
           and the pin toggle at the row's right end (UI-49). -->
      <DropdownMenu.Item
        class="menu-item toc-toolbar"
        closeOnSelect={false}
        data-tauri-drag-region="false"
        onkeydown={(event) => {
          // DR-242: keyboard parity — Enter/Space on the highlighted row runs
          // the same action the expand control's click runs; the kit's own
          // selection (which now does nothing) must not double-run. UI-49:
          // only when the row itself is the target — the pin control keeps
          // its own activation.
          if (event.target !== event.currentTarget) return;
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            event.stopPropagation();
            store.tocToggleAll();
          }
        }}
      >
        <TocControls {store} />
      </DropdownMenu.Item>
      <!-- DR-232: the tree region — the entry rows and the scroll, nothing
           else. The panel box above it never scrolls. The row wrapper here is
           the menu item (the kit's roving focus works through the menu's
           context); the row content is shared (`TocEntryContent`). Entry
           selection jumps through the store's `goToPage` — the same path as
           the page field (UI-33) — and closes the panel (the unpinned rule). -->
      <TocTree {store} {current}>
        {#snippet row({ entry, index, isCurrent })}
          <DropdownMenu.Item
            class="menu-item toc-entry"
            data-tauri-drag-region="false"
            data-current={isCurrent || undefined}
            onSelect={() => store.goToPage(entry.page)}
            onkeydown={(event) => {
              // ArrowRight / ArrowLeft toggle a focused parent's subtree
              // (UI-33/DR-218); the kit's own handling of those keys (submenu
              // navigation) must not run for a tree row.
              if (store.isTocParent(index) && (event.key === "ArrowRight" || event.key === "ArrowLeft")) {
                event.preventDefault();
                event.stopPropagation();
                store.toggleTocEntry(index);
              }
            }}
          >
            <TocEntryContent {store} {entry} {index} current={isCurrent} />
          </DropdownMenu.Item>
        {/snippet}
      </TocTree>
    </DropdownMenu.Content>
  </DropdownMenu.Root>
{/if}
