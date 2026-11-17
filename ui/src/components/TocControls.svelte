<script lang="ts">
  // The ToC panel's toolbar controls (FR-24 / UI-49, SPEC-v0.0.10 §3.8): the
  // released expand/collapse flip-flop (UI-33 as revised by DR-223/DR-244)
  // plus the new pin toggle — one implementation rendered by both panel
  // presentations: inside the overlay's `DropdownMenu.Item` toolbar row
  // (`TocPanel`) and inside the docked panel's plain toolbar row
  // (`TocDocked`). Both controls keep the strip's icon-control grammar
  // (`control icon-button`, icon-only, the app's tooltip).
  // The pin is the released toggle affordance (UI-49): a static accessible
  // name, the pressed state carrying the state — `aria-pressed` reflects the
  // store's pin, exactly like the mode buttons. Its click writes through the
  // store's single path: in the overlay panel that pins the panel; in the
  // docked panel it un-pins and closes it (UI-49: "activating it again
  // returns to the released overlay behavior — the panel closes"). The
  // preventDefault/stopPropagation pair is the released chevron pattern: the
  // click must not activate the row carrying the controls.
  import Icon from "./Icon.svelte";
  import Tooltip from "./Tooltip.svelte";
  import { type TocStore } from "$lib/state/reader.svelte";
  import { t } from "$lib/i18n";

  let { store }: { store: TocStore } = $props();
</script>

<Tooltip text={t(store.tocExpandMode ? "reader.toc.expandAll" : "reader.toc.collapseAll")}>
  {#snippet control({ props })}
    <!-- DR-244: icon-only, the strip's own icon control — the mode-following
         text names the control through the tooltip and the aria-label. -->
    <button
      type="button"
      class="control icon-button"
      data-tauri-drag-region="false"
      {...props}
      aria-label={t(store.tocExpandMode ? "reader.toc.expandAll" : "reader.toc.collapseAll")}
      onclick={(event) => {
        event.preventDefault();
        event.stopPropagation();
        store.tocToggleAll();
      }}
    >
      <Icon name={store.tocExpandMode ? "expand-all" : "collapse-all"} />
    </button>
  {/snippet}
</Tooltip>
<span class="toc-pin">
  <Tooltip text={t("reader.toc.pin")}>
    {#snippet control({ props })}
      <button
        type="button"
        class="control icon-button"
        data-tauri-drag-region="false"
        {...props}
        aria-label={t("reader.toc.pin")}
        aria-pressed={store.tocPinned}
        onclick={(event) => {
          event.preventDefault();
          event.stopPropagation();
          store.setTocPinned(!store.tocPinned);
        }}
      >
        <Icon name={store.tocPinned ? "pin" : "unpin"} />
      </button>
    {/snippet}
  </Tooltip>
</span>

<style>
  /* UI-49: the pin sits at the toolbar row's right end in both states. */
  .toc-pin {
    margin-left: auto;
    display: inline-flex;
  }
</style>
