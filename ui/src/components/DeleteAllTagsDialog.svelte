<script lang="ts">
  // Delete-all-tags confirmation (FR-32): the DeleteTagDialog pattern — local
  // `open` bound into Modal (DR-63). The body states the FR-02 fact: the
  // tags go, the documents stay.
  import Modal from "./Modal.svelte";
  import { t } from "$lib/i18n";

  let { ondone }: { ondone: (confirmed: boolean) => void } = $props();

  let open = $state(true);
  let confirmed = false;

  $effect(() => {
    if (!open) ondone(confirmed);
  });
</script>

<Modal title={t("tagsDialog.deleteAllTitle")} width={420} bind:open>
  <p class="dialog-body">{t("tagsDialog.deleteAllBody")}</p>
  <div class="dialog-actions">
    <button class="control" type="button" onclick={() => (open = false)}>{t("details.discard")}</button>
    <button
      class="control control-danger"
      type="button"
      onclick={() => {
        confirmed = true;
        open = false;
      }}
    >
      {t("tagsDialog.deleteAllAction")}
    </button>
  </div>
</Modal>
