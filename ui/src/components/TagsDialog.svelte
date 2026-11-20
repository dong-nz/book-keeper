<script lang="ts">
  // Tags manager (UI-10 as modified by DR-414): the tag CRUD as a toolbar
  // dialog (the cover picker's shape, DR-408/DR-410) — title, *Delete all
  // tags* (FR-32) and the close control on the bar, the content in the
  // variant's scroll area, whose released geometry (DR-411/DR-412) carries
  // the scrollbar shape the user asked for (DR-413). Dialog mount rule per
  // DR-63: this component owns `open` and reports the close through `ondone`.
  import Modal from "./Modal.svelte";
  import TagList from "./TagList.svelte";
  import NewTagField from "./NewTagField.svelte";
  import RenameTagDialog from "./RenameTagDialog.svelte";
  import DeleteTagDialog from "./DeleteTagDialog.svelte";
  import DeleteAllTagsDialog from "./DeleteAllTagsDialog.svelte";
  import { library } from "$lib/state/library.svelte";
  import { t } from "$lib/i18n";
  import { tagActions, type TagUiHost } from "$lib/actions";
  import type { TagDto } from "$lib/types";

  let { ondone }: { ondone: () => void } = $props();

  let open = $state(true);
  let renaming = $state<TagDto | null>(null);
  let deleting = $state<TagDto | null>(null);
  let deletingAll = $state(false);

  const host: TagUiHost = {
    openRename: (tag) => (renaming = tag),
    openDelete: (tag) => (deleting = tag),
  };
  const rowActions = tagActions(host);

  $effect(() => {
    if (!open) ondone();
  });
</script>

<Modal title={t("tagsDialog.title")} width={420} bind:open>
  {#snippet actions()}
    <button
      class="control"
      type="button"
      disabled={library.tags.length === 0}
      onclick={() => (deletingAll = true)}
    >
      {t("tagsDialog.deleteAll")}
    </button>
  {/snippet}
  <div class="tags-dialog">
    <NewTagField />
    {#if library.tags.length === 0}
      <p class="empty">{t("tagsDialog.empty")}</p>
    {:else}
      <TagList actions={rowActions} />
    {/if}
  </div>

  {#if renaming}
    <RenameTagDialog tag={renaming} ondone={() => (renaming = null)} />
  {/if}

  {#if deleting}
    {@const target = deleting}
    <DeleteTagDialog
      name={target.name}
      ondone={(confirmed) => {
        if (confirmed) void library.deleteTag(target.id);
        deleting = null;
      }}
    />
  {/if}

  {#if deletingAll}
    <DeleteAllTagsDialog
      ondone={(confirmed) => {
        if (confirmed) void library.deleteAllTags();
        deletingAll = false;
      }}
    />
  {/if}
</Modal>

<style>
  /* The list panel is plain content: the toolbar variant's scroll area owns
     the scrolling and the scrollbar shape (DR-411/DR-412), so the panel's
     own cap and the DR-413 bleed rules are gone (DR-414). */
  .tags-dialog {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .empty {
    margin: 8px 0;
    font-size: var(--text-13);
    color: var(--text-tertiary);
  }
</style>
