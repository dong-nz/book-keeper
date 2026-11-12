<script lang="ts">
  // The EPUB reader control strip (SPEC-v0.0.9 §3.13, UI-48): exactly three
  // controls in this order — the table-of-contents control (present only
  // when the package has a ToC, first when present, the released DR-207
  // rule), the position control (the readout replacing the page field,
  // derived from the store's single scroll source per DR-155; clicking it
  // opens the percent-jump field with the released page-field editing rule),
  // and the text-size control (the zoom control's − / + pattern, DR-221,
  // naming the current step in the trigger). No view-mode control, no zoom
  // control, no page navigation (§3.13 named exclusions). Values flow
  // through the reader store and the settings store only.
  import Icon from "./Icon.svelte";
  import TocPanel from "./TocPanel.svelte";
  import Tooltip from "./Tooltip.svelte";
  import type { EpubReaderStore } from "$lib/state/epubReader.svelte";
  import { settings } from "$lib/state/settings.svelte";
  import { t } from "$lib/i18n";

  let { store }: { store: EpubReaderStore } = $props();

  // DR-155: the readout reads the store's single scroll-position source —
  // the same one the mount window derives from, never a relayed copy.
  const percent = $derived(Math.round(store.bookPercent));
  const chapter = $derived(store.currentChapter);
  const chapterLabel = $derived(
    store.chapterLabel(chapter) ?? t("reader.epub.chapter", { number: chapter }),
  );
  const readout = $derived(t("reader.epub.positionReadout", { percent, chapter: chapterLabel }));
  const textSize = $derived(settings.epubTextSize);

  // The jump field (the released page-field editing rule, §3.13): the
  // readout is the control; clicking it hands the field to the user, and
  // Enter/blur resolves the percent and returns to the readout.
  let editing = $state(false);
  let field = $state("");

  function startEdit(): void {
    field = String(percent);
    editing = true;
  }

  function commitField(): void {
    editing = false;
    const value = Number.parseFloat(field);
    if (Number.isFinite(value)) store.jumpToPercent(value);
  }

  function focusField(node: HTMLInputElement): void {
    node.focus();
    node.select();
  }
</script>

<div class="reader-controls">
  <!-- The table-of-contents control (UI-33 rules, §3.13): rendered only
       when the package carries a ToC — a book without one offers no control.
       The panel is the released one; the EPUB strip additionally passes the
       current-entry resolver for the §3.13 highlight. -->
  {#if store.outline.length > 0}
    <TocPanel store={store} current={() => store.currentTocIndex()} />
  {/if}

  <div class="group">
    {#if editing}
      <input
        class="control position-field"
        type="text"
        inputmode="numeric"
        data-tauri-drag-region="false"
        aria-label={t("reader.epub.positionJump")}
        value={field}
        use:focusField
        oninput={(event) => {
          field = event.currentTarget.value;
        }}
        onkeydown={(event) => {
          if (event.key === "Enter") {
            event.preventDefault();
            commitField();
          }
        }}
        onblur={commitField}
      />
      <span class="position-unit">%</span>
    {:else}
      <Tooltip text={t("reader.epub.positionJump")}>
        {#snippet control({ props })}
          <button
            {...props}
            type="button"
            class="control position-readout"
            data-tauri-drag-region="false"
            aria-label={t("reader.epub.position", { percent, chapter: chapterLabel })}
            onclick={startEdit}
          >
            {readout}
          </button>
        {/snippet}
      </Tooltip>
    {/if}
  </div>

  <span class="spacer"></span>

  <div class="group">
    <!-- The text-size control (§3.13): the zoom control's pattern (DR-221)
         — − / + step the ladder, the current step named in the middle. The
         change applies live (a CSS custom property; reflow is the
         browser's) and persists globally through the settings store. -->
    <Tooltip text={t("reader.epub.textSizeDecrease")}>
      {#snippet control({ props })}
        <button
          {...props}
          type="button"
          class="control icon-button"
          data-tauri-drag-region="false"
          aria-label={t("reader.epub.textSizeDecrease")}
          onclick={() => store.changeTextSize(-1)}
        >
          <Icon name="minus" />
        </button>
      {/snippet}
    </Tooltip>
    <span class="text-size-value" aria-live="polite" aria-label={t("reader.epub.textSize", { size: textSize })}>
      {textSize} px
    </span>
    <Tooltip text={t("reader.epub.textSizeIncrease")}>
      {#snippet control({ props })}
        <button
          {...props}
          type="button"
          class="control icon-button"
          data-tauri-drag-region="false"
          aria-label={t("reader.epub.textSizeIncrease")}
          onclick={() => store.changeTextSize(1)}
        >
          <Icon name="plus" />
        </button>
      {/snippet}
    </Tooltip>
  </div>
</div>

<style>
  /* The strip frame is the released one (the same grammar as
     `ReaderControls.svelte`): the window's bottom edge row. */
  .reader-controls {
    display: flex;
    align-items: center;
    gap: 6px;
    height: var(--toolbar-height);
    padding: 0 var(--content-gutter);
    border-bottom: 1px solid var(--border-hairline);
    background: var(--surface-0);
  }

  .group {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .spacer {
    flex: 1;
  }

  /* The position readout (UI-13: the control names itself through the
     tooltip and the aria-label too). Tabular figures, like the page field. */
  .position-readout {
    font-variant-numeric: tabular-nums;
    max-width: 40ch;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .position-field {
    width: 56px;
    text-align: center;
    font-variant-numeric: tabular-nums;
  }

  .position-unit {
    color: var(--text-secondary);
  }

  .text-size-value {
    font-variant-numeric: tabular-nums;
    min-width: 48px;
    text-align: center;
  }
</style>
