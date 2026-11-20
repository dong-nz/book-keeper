<script lang="ts">
  // Update modal (UI-56, SPEC-v0.0.13 §3.2, DR-422): the released dialog
  // component. The version line shows the current version and the new one
  // ("X → Y"); the manifest's release notes render as plain text (OQ-74
  // closed A — `publish-release.mjs` publishes plain-text notes and the
  // plugin forwards them as `body`); exactly two actions: *Install* (the
  // released download-and-install flow, progress through the job bar, §3.3)
  // and *Skip this version* (persisted through the settings store, FR-25).
  // Escape, the X or the overlay defers for this session only — nothing is
  // persisted (AC-185). Mounted in both window roots; the offer store is
  // per-window, so a check arms only its own window.
  import Modal from "./Modal.svelte";
  import { t } from "$lib/i18n";
  import { appinfo } from "$lib/state/appinfo.svelte";
  import { updateOffer } from "$lib/state/updateOffer.svelte";
  import { installOfferedUpdate, skipOfferedUpdate } from "$lib/updates";

  let open = $state(false);

  // The offer drives the dialog: an armed check opens it; an action or a
  // dismissal closes it, and a dismissal clears the offer — deferral for
  // this session is the absence of a decision, not a decision (AC-185).
  $effect(() => {
    if (updateOffer.offer) open = true;
    else open = false;
  });
  $effect(() => {
    if (!open && updateOffer.offer) updateOffer.clear();
  });
</script>

<Modal title={t("updates.modal.title")} width={480} bind:open>
  {#if updateOffer.offer}
    <div class="dialog-body">
      <p class="versions tabular">
        {#if appinfo.version}
          <span class="current">{appinfo.version}</span>
          <span class="arrow" aria-hidden="true">→</span>
        {/if}
        <span class="new">{updateOffer.offer.newVersion}</span>
      </p>
      {#if updateOffer.offer.notes}
        <p class="notes">{updateOffer.offer.notes}</p>
      {/if}
      <div class="dialog-actions">
        <button class="control" type="button" onclick={installOfferedUpdate}>
          {t("updates.install")}
        </button>
        <button class="control" type="button" onclick={skipOfferedUpdate}>
          {t("updates.skip")}
        </button>
      </div>
    </div>
  {/if}
</Modal>

<style>
  .dialog-body {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding-top: 4px;
  }

  .versions {
    font-size: var(--text-14);
    color: var(--text-primary);
  }

  .versions .arrow {
    margin: 0 6px;
    color: var(--text-tertiary);
  }

  .versions .new {
    font-weight: 600;
  }

  /* The manifest notes are plain text; their line structure is the notes'
     own layout, preserved (UI-56: "notes text", no rich text). */
  .notes {
    font-size: var(--text-13);
    color: var(--text-secondary);
    white-space: pre-line;
  }

  .dialog-actions {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
  }
</style>
