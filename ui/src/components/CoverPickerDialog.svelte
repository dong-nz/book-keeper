<script lang="ts">
  // Cover picker (FR-31, UI-55, SPEC-v0.0.12 §3.8): the dialog behind the
// library context menu's "Update cover" (the document's kind decides the
// section). PDF: the page-thumbnail strip, filled lazily as it scrolls —
// windows of pages requested through `app_pdf_thumbs` (batched renders at
// picker width through the service's interactive tier; the app stays
// responsive while they run, AC-181). EPUB: the candidate grid from
// `app_cover_candidates`, rendered through the released `bookkeeper-epub:`
// protocol — the same base URL the reader's book images use (the manifest
// command carries it, platform-correct, §3.10).
// A pick sets the choice and closes the dialog; the library card follows
// through the released cover-batch completion re-query (DR-231). "Automatic
// cover" clears the choice — the released first-page/declared-cover path
// under the untagged key (AC-177).
import { convertFileSrc } from "@tauri-apps/api/core";
import * as api from "../api";
import Modal from "./Modal.svelte";
import { feedback } from "$lib/state/feedback.svelte";
import { asAppError, type CoverCandidateDto, type DocumentKind } from "$lib/types";
import { t } from "$lib/i18n";

// The requested window stays inside the backend's batch cap (MAX_THUMB_BATCH
// = 32, `pdf/mod.rs`); 12 at picker width is a responsive first page of the
// strip.
const THUMB_WINDOW = 12;

let { docId, kind, ondone }: { docId: number; kind: DocumentKind; ondone: () => void } = $props();

let open = $state(true);
$effect(() => {
  if (!open) ondone();
});

// PDF state: the document manifest gives the strip's extent; thumbnails
// arrive per window. EPUB state: manifest base + candidate list, once.
let pageCount = $state(0);
let thumbs = $state(new Map<number, string>()); // page → img src
let nextFrom = $state(1);
let loadingWindow = false; // not $state: the guard, not the render
let candidates = $state<CoverCandidateDto[] | null>(null); // null = not fetched yet
let baseUrl = $state("");
let sentinel = $state<HTMLElement | null>(null);

function reportFailure(error: unknown): void {
  feedback.reportError(asAppError(error));
}

$effect(() => {
  if (kind === "epub") {
    // The base URL is the manifest's own (the platform-correct form the
    // reader uses — no client-side platform guess).
    Promise.all([api.appEpubManifest(docId), api.appCoverCandidates(docId)])
      .then(([manifest, list]) => {
        baseUrl = manifest.base_url;
        candidates = list;
      })
      .catch(reportFailure);
  } else {
    // The manifest command carries the row's stored password (FR-20), so a
    // protected document answers its page count like the reader does.
    void api
      .appPdfDocument(docId)
      .then((doc) => (pageCount = doc.page_count))
      .catch(reportFailure);
  }
});

function loadNextWindow(): void {
  if (loadingWindow || nextFrom > pageCount) return;
  const from = nextFrom;
  const to = Math.min(from + THUMB_WINDOW - 1, pageCount);
  loadingWindow = true;
  void api
    .appPdfThumbs(docId, from, to)
    .then((paths) => {
      const next = new Map(thumbs);
      paths.forEach((path, index) => next.set(from + index, convertFileSrc(path)));
      thumbs = next;
      nextFrom = to + 1;
    })
    .catch(reportFailure)
    .finally(() => {
      loadingWindow = false;
    });
}

// Lazy windows (AC-181): the strip requests its next window when the
// sentinel scrolls into view — the load follows the user, never ahead.
$effect(() => {
  if (kind !== "pdf" || !sentinel || nextFrom > pageCount) return;
  const observer = new IntersectionObserver((entries) => {
    if (entries.some((entry) => entry.isIntersecting)) loadNextWindow();
  });
  observer.observe(sentinel);
  return () => observer.disconnect();
});

function pickPage(page: number): void {
  void api
    .appCoverChoice(docId, { kind: "page", page: page - 1 })
    .then(() => (open = false))
    .catch(reportFailure);
}

function pickImage(candidate: CoverCandidateDto): void {
  void api
    .appCoverChoice(docId, { kind: "epub-image", entry: candidate.entry, media: candidate.media_type })
    .then(() => (open = false))
    .catch(reportFailure);
}

function setAutomatic(): void {
  void api
    .appCoverClear(docId)
    .then(() => (open = false))
    .catch(reportFailure);
}

// The §3.2 namespace rule percent-decodes the request path, so each segment
// goes encoded — the same shape the reader's rewritten book images carry.
function entryUrl(entry: string): string {
  return `${baseUrl}${entry.split("/").map(encodeURIComponent).join("/")}`;
}
</script>

<Modal title={t("coverPicker.title")} width="fit-content" bind:open>
  {#snippet actions()}
    <button class="control" onclick={setAutomatic}>{t("coverPicker.automatic")}</button>
  {/snippet}
  <div class="picker">
    {#if kind === "pdf"}
      <div class="strip">
        {#each [...thumbs.entries()] as [page, src] (page)}
          <button class="tile" aria-label={t("reader.pageAlt", { page })} onclick={() => pickPage(page)}>
            <img src={src} alt="" />
            <span class="tile-number">{page}</span>
          </button>
        {/each}
        {#if nextFrom <= pageCount}
          <div class="sentinel" bind:this={sentinel} aria-hidden="true"></div>
        {/if}
      </div>
    {:else if candidates === null}
      <!-- The honest skeleton while the candidates load (UI-29): a neutral
           ground, no fake content. -->
      <div class="strip">
        {#each Array(6) as _, i (i)}
          <div class="tile tile-loading"></div>
        {/each}
      </div>
    {:else if candidates.length === 0}
      <p class="empty">{t("coverPicker.noImages")}</p>
    {:else}
      <div class="grid">
        {#each candidates as candidate (candidate.entry)}
          <button class="tile" onclick={() => pickImage(candidate)}>
            <img src={entryUrl(candidate.entry)} alt="" />
          </button>
        {/each}
      </div>
    {/if}
  </div>
</Modal>

<style>
  /* The picker's fill is a fixed four-column grid (DR-410): the dialog is
     sized to it (`width="fit-content"`), so the thumbnail rows fill the
     dialog exactly — no dead space beside the columns, on either fill. */
  .picker {
    display: flex;
    flex-direction: column;
    gap: 12px;
    width: calc(4 * 132px + 3 * 8px);
  }

  /* The strip and the grid are the same tile language (the picker is one
     control with two fills): fixed tiles in fixed columns, contain-fit
     images. */
  .strip,
  .grid {
    display: grid;
    grid-template-columns: repeat(4, 132px);
    gap: 8px;
  }

  .tile {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    width: 132px;
    padding: 6px;
    background: var(--surface-1);
    border: 1px solid var(--border-hairline);
    border-radius: var(--radius-control);
    transition:
      background var(--duration-hover) var(--ease),
      border-color var(--duration-hover) var(--ease);
  }

  .tile img {
    width: 120px;
    height: 160px;
    object-fit: contain;
    background: var(--surface-0);
  }

  /* The theme's hover language (DR-408): the menu-hover tint on the tile,
     the accent border, the page number in the accent — the released tokens,
     both ramps, no new color. */
  .tile:hover {
    border-color: var(--accent);
    background: var(--accent-quiet);
  }

  .tile-number {
    font-size: var(--text-11);
    color: var(--text-secondary);
  }

  .tile:hover .tile-number {
    color: var(--accent);
  }

  .tile-loading {
    height: 188px;
    animation: none;
  }

  .sentinel {
    grid-column: 1 / -1;
    width: 100%;
    height: 24px;
  }

  .empty {
    margin: 0;
    padding: 12px 0;
    font-size: var(--text-13);
    color: var(--text-secondary);
  }
</style>
