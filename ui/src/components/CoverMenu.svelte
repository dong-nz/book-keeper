<script lang="ts">
  // Cover menu (FR-31, UI-55, SPEC-v0.0.12 §3.8): the reader chrome strip's
// dedicated cover control — the release's one new control surface, mounted in
// the strip's right-end cluster on every platform (the app menu and the
// reader settings control keep their edge per §3.7; on macOS the window
// controls move per UI-07 MODIFIED and this cluster stays with the settings
// control at the strip's right end). Items per kind: PDF — "Set this page as
// cover" (the page currently shown — valid by construction, no picker) and
// "Automatic cover"; EPUB — "Choose cover" (the candidate grid, the same
// dialog the library picker uses) and "Automatic cover".
// The commands return while the render runs (AC-180); the library card
// follows through the released cover-batch completion re-query (DR-231), so
// this menu owns no cross-window state.
import { DropdownMenu } from "bits-ui";
import * as api from "../api";
import Icon from "./Icon.svelte";
import OverlayAnchor from "./OverlayAnchor.svelte";
import Tooltip from "./Tooltip.svelte";
import CoverPickerDialog from "./CoverPickerDialog.svelte";
import { feedback } from "$lib/state/feedback.svelte";
import { asAppError } from "$lib/types";
import { t } from "$lib/i18n";

let {
  docId,
  kind,
  currentPage,
}: {
  docId: number;
  kind: "pdf" | "epub";
  /** PDF only: the page currently shown (1-based, the store's own). */
  currentPage?: () => number;
} = $props();

let open = $state(false);
let pickerOpen = $state(false);

// A rejected choice (a typed error at the command boundary) is the released
// toast path; the stored choice is unchanged by it (R4.6).
function reportFailure(error: unknown): void {
  feedback.reportError(asAppError(error));
}

function setThisPage(): void {
  const page = currentPage?.();
  if (page === undefined) return;
  void api.appCoverChoice(docId, { kind: "page", page: page - 1 }).catch(reportFailure);
}

function setAutomatic(): void {
  void api.appCoverClear(docId).catch(reportFailure);
}
</script>

<DropdownMenu.Root bind:open>
  <Tooltip text={t("reader.cover.menu")}>
    {#snippet control({ props })}
      <DropdownMenu.Trigger
        {...props}
        class="control icon-button cover-trigger"
        aria-label={t("reader.cover.menu")}
        data-tauri-drag-region="false"
      >
        <Icon name="image" />
      </DropdownMenu.Trigger>
    {/snippet}
  </Tooltip>

  <DropdownMenu.Content class="menu cover-menu">
    <OverlayAnchor />
    {#if kind === "pdf"}
      <DropdownMenu.Item class="menu-item" onSelect={setThisPage}>
        <span class="item-label">
          <Icon name="image" />
          <span>{t("reader.cover.setPage")}</span>
        </span>
      </DropdownMenu.Item>
    {:else}
      <DropdownMenu.Item class="menu-item" onSelect={() => (pickerOpen = true)}>
        <span class="item-label">
          <Icon name="grid" />
          <span>{t("reader.cover.choose")}</span>
        </span>
      </DropdownMenu.Item>
    {/if}
    <DropdownMenu.Item class="menu-item" onSelect={setAutomatic}>
      <span class="item-label">
        <Icon name="relink" />
        <span>{t("reader.cover.automatic")}</span>
      </span>
    </DropdownMenu.Item>
  </DropdownMenu.Content>
</DropdownMenu.Root>

{#if pickerOpen}
  <CoverPickerDialog docId={docId} kind="epub" ondone={() => (pickerOpen = false)} />
{/if}

