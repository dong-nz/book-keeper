<script lang="ts">
  // Relink dialog (UI-05, FR-15): opened from a row action or from the
  // missing-file toast action (feedback.relinkTargetId is the single entry
  // point). UI-46/AC-128 (SPEC-v0.0.9 §3.4): the picker presents the file
  // type matching the document's own extension; a mismatch chosen anyway
  // (native dialogs allow "All files") still shows the typed inline error,
  // never a toast.
  import { open } from "@tauri-apps/plugin-dialog";
  import * as api from "../api";
  import { library } from "$lib/state/library.svelte";
  import { feedback } from "$lib/state/feedback.svelte";
  import { asAppError, type DocumentDto } from "$lib/types";
  import { t } from "$lib/i18n";
  import Modal from "./Modal.svelte";

  // The presented filter follows the document's extension — the same type
  // derivation the service applies (`is_document_path`), mirrored for the
  // picker only; the service stays the authority on acceptance.
  function documentFilters(filename: string): { name: string; extensions: string[] }[] {
    const dot = filename.lastIndexOf(".");
    const extension = dot === -1 ? "" : filename.slice(dot + 1).toLowerCase();
    return extension === "epub"
      ? [{ name: "EPUB", extensions: ["epub"] }]
      : [{ name: "PDF", extensions: ["pdf"] }];
  }

  const targetId = $derived(feedback.relinkTargetId);

  let dialogOpen = $state(false);
  let doc = $state<DocumentDto | null>(null);
  let error = $state<string | null>(null);
  let busy = $state(false);

  $effect(() => {
    if (targetId === null) {
      dialogOpen = false;
      doc = null;
      return;
    }
    dialogOpen = true;
    error = null;
    doc = null;
    void (async () => {
      try {
        doc = await api.libraryGetDocument(targetId);
      } catch (error_) {
        feedback.reportError(asAppError(error_));
        feedback.closeRelink();
      }
    })();
  });

  $effect(() => {
    if (!dialogOpen && targetId !== null) feedback.closeRelink();
  });

  async function chooseFile() {
    if (!targetId) return;
    const selected = await open({ filters: documentFilters(doc?.filename ?? "") });
    if (typeof selected !== "string") return;
    busy = true;
    const failure = await library.relink(targetId, selected);
    busy = false;
    if (failure === null) {
      dialogOpen = false; // the store effect closes the dialog via closeRelink
    } else {
      error = failure.kind === "unsupported_type" ? t("relink.errorMissing") : null;
    }
  }
</script>

{#if targetId !== null}
  <Modal title={t("relink.title")} width={420} bind:open={dialogOpen}>
    <div class="dialog-body relink-body">
      <p>{t("relink.body")}</p>
      {#if doc}
        <p class="path-line">
          <span class="hint">{t("relink.storedPath")}</span>
          <code>{doc.path}</code>
        </p>
      {/if}
      {#if error}
        <p class="inline-error" role="alert">{error}</p>
      {/if}
      <div class="dialog-actions">
        <button class="control control-primary" disabled={busy || doc === null} onclick={() => void chooseFile()}>
          {t("relink.chooseNew")}
        </button>
      </div>
    </div>
  </Modal>
{/if}

<style>
  .relink-body p {
    font-size: var(--text-13);
    color: var(--text-secondary);
    margin: 0 0 10px;
  }

  .path-line {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .path-line code {
    font-family: var(--font-mono);
    font-size: var(--text-12);
    color: var(--text-tertiary);
    user-select: all;
    overflow-wrap: anywhere;
  }

  .hint {
    font-size: var(--text-11);
    color: var(--text-tertiary);
  }
</style>
