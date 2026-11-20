<script lang="ts">
  // Toolbar (UI-01 as modified by UI-07, UI-09, UI-13, UI-14, UI-17, UI-18):
  // the single top row — Browse, the search field (which carries the import
  // control), spacer, the Select-mode toggle, View, Settings, window controls.
  // Six controls, all icon-only with tooltips, all at `--control-height`, one
  // gap value, no separators or grouping boxes.
  //
  // The strip is the window drag region in `deep` mode (D2, §3.1): Tauri's
  // `isDragRegion` walks the mousedown path, so the strip's padding, gaps and
  // spacer drag and double-click maximize — and every control marks itself
  // "false", which is what keeps a click in a control's own padding from
  // dragging the window.
  import BrowseMenu from "./BrowseMenu.svelte";
  import SearchBox from "./SearchBox.svelte";
  import ViewMenu from "./ViewMenu.svelte";
  import SettingsMenu from "./SettingsMenu.svelte";
  import WindowControls from "./WindowControls.svelte";
  import Tooltip from "./Tooltip.svelte";
  import Icon from "./Icon.svelte";
  import { layout } from "$lib/state/layout.svelte";
  import { library } from "$lib/state/library.svelte";
  import { currentUiPlatform, windowControlsLayout } from "$lib/platform";
  import { dragStrip, usesOwnDragStrip } from "$lib/dragstrip";
  import { t } from "$lib/i18n";

  // UI-07 MODIFIED (SPEC-v0.0.12 §3.7): on macOS the window-controls cluster
  // is the toolbar's FIRST cell (the traffic-light corner; the existing 12 px
  // toolbar padding is its inset) and the app menu (UI-12) keeps its released
  // right end — the adjacency fact stands only on Windows/Linux. The
  // Windows/Linux mount is byte-for-byte the released one.
  const controlsLayout = windowControlsLayout(currentUiPlatform());
  // D35 repair (SPEC-v0.0.13 §3.6): on macOS the strip renders no drag-region
  // attribute and `use:dragStrip` owns it — press 1 drags, press 2 of a
  // double-click maximizes, no coordinate-exactness requirement. The
  // Windows/Linux mount keeps the released attribute and the built-in
  // behavior byte-for-byte.
  const ownDragStrip = usesOwnDragStrip();
</script>

<header class="toolbar" data-tauri-drag-region={ownDragStrip ? undefined : "deep"} use:dragStrip>
  {#if controlsLayout.side === "left"}
    <WindowControls />
  {/if}
  <BrowseMenu />
  <SearchBox />
  <span class="toolbar-spacer"></span>
  <!-- Select mode (UI-18): the one control whose state must be visible on the
       control itself — pressed state, never text. Session state only: it is
       never written to the settings store and is off at every launch. -->
  <Tooltip text={t("toolbar.select")}>
    {#snippet control({ props })}
      <button
        {...props}
        class="control icon-button select-toggle"
        type="button"
        aria-pressed={layout.selectMode}
        aria-label={t("toolbar.select")}
        data-tauri-drag-region="false"
        onclick={() => {
          // UI-18 MODIFIED (SPEC-v0.0.14 §3.5): turning the control off exits
          // select mode — the selection is cleared with it, the BulkBar goes,
          // the rows return to browse behavior.
          if (layout.selectMode) library.clearSelection();
          layout.toggleSelectMode();
        }}
      >
        <Icon name="select" />
      </button>
    {/snippet}
  </Tooltip>
  <ViewMenu />
  <SettingsMenu />
  {#if controlsLayout.side === "right"}
    <WindowControls />
  {/if}
</header>

<style>
  .toolbar {
    grid-area: toolbar;
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: var(--toolbar-height);
    padding: 0 12px;
    background: var(--surface-0);
    border-bottom: 1px solid var(--border-hairline);
    position: sticky;
    top: 0;
    z-index: 10;
  }

  .toolbar-spacer {
    flex: 1 1 0;
    min-width: 0;
  }
</style>
