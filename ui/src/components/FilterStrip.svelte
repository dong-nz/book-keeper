<script lang="ts">
  // Active-filter strip (UI-09 MODIFIED, SPEC-v0.0.14 §3.6): mounted in the
  // document area above the scroller, rendered only while a browse filter is
  // active — a mode filter or tag filters, or both. One chip per active
  // element, each × removing only that element; *Clear all* resets the browse
  // filter; search is never shown and never touched. The strip is a
  // display-and-removal surface, never a chooser — the Browse menu stays the
  // control. Chips beyond the available width collapse into a "+N" menu
  // listing the hidden filters with their removal controls; the decision is
  // measurement-driven (inlineCount, DR-434) from a hidden measure row that
  // renders every chip once per set change.
  import { DropdownMenu } from "bits-ui";
  import { library } from "$lib/state/library.svelte";
  import { t } from "$lib/i18n";
  import Icon from "./Icon.svelte";
  import {
    activeFilterChips,
    clearAllPatch,
    hasBrowseFilter,
    inlineCount,
    removeChipPatch,
    type FilterChip,
  } from "$lib/filterstrip";

  // Matches the CSS gap of the strip row; the measure row uses the same chrome.
  const GAP = 8;

  const MODE_LABEL: Record<"favorites" | "missing" | "reading", Parameters<typeof t>[0]> = {
    favorites: "browse.favorites",
    missing: "browse.missing",
    reading: "browse.reading",
  };

  const allChips = $derived(activeFilterChips(library.filter));
  // A tag id with no catalog entry (deleted between renders) carries no label
  // and renders no chip.
  const chips = $derived(
    allChips.filter(
      (chip) => chip.kind === "mode" || library.tags.some((tag) => tag.id === chip.id),
    ),
  );

  function chipKey(chip: FilterChip): string {
    return chip.kind === "mode" ? `m:${chip.mode}` : `t:${chip.id}`;
  }

  function chipLabel(chip: FilterChip): string {
    if (chip.kind === "mode") return t(MODE_LABEL[chip.mode]);
    return library.tags.find((tag) => tag.id === chip.id)?.name ?? "";
  }

  let rowEl = $state<HTMLSpanElement | null>(null);
  let measureEl = $state<HTMLSpanElement | null>(null);
  let chipWidths: number[] = $state([]);
  let controlWidth = 0;
  let clearWidth = 0;
  let inline = $state(Number.MAX_SAFE_INTEGER);

  function measure() {
    if (!measureEl) return;
    chipWidths = Array.from(measureEl.querySelectorAll<HTMLElement>(".strip-chip")).map(
      (el) => el.offsetWidth,
    );
    controlWidth = measureEl.querySelector<HTMLElement>(".strip-more")?.offsetWidth ?? 0;
    clearWidth = measureEl.querySelector<HTMLElement>(".strip-clear")?.offsetWidth ?? 0;
  }

  function recompute() {
    if (!rowEl) return;
    const available = rowEl.clientWidth - clearWidth - GAP;
    inline = inlineCount(chipWidths, available, controlWidth, GAP);
  }

  // Two effects, one direction of dependency each (the cycle this split
  // avoids): the measure effect writes the widths and never reads them; the
  // layout effect reads them and writes `inline`, and never reads it. A
  // single effect both reading and writing `chipWidths` re-triggers itself —
  // measure() builds a fresh array on every pass (effect_update_depth_exceeded).
  $effect(() => {
    chips;
    measure();
  });

  $effect(() => {
    chipWidths;
    recompute();
  });

  $effect(() => {
    const el = rowEl;
    if (!el) return;
    const observer = new ResizeObserver(() => recompute());
    observer.observe(el);
    return () => observer.disconnect();
  });

  function removeChip(chip: FilterChip) {
    library.setFilter(removeChipPatch(library.filter, chip));
  }

  const overflow = $derived(Math.max(0, chips.length - inline));
</script>

{#if hasBrowseFilter(library.filter)}
  <div class="filter-strip">
    <span class="strip-row" bind:this={rowEl}>
      {#each chips.slice(0, inline) as chip (chipKey(chip))}
        <span class="strip-chip">
          <span class="strip-chip-name">{chipLabel(chip)}</span>
          <button
            class="strip-chip-x"
            type="button"
            aria-label={t("filterStrip.remove", { name: chipLabel(chip) })}
            onclick={() => removeChip(chip)}
          >
            <Icon name="close" />
          </button>
        </span>
      {/each}
      {#if overflow > 0}
        <DropdownMenu.Root>
          <DropdownMenu.Trigger
            class="control strip-more"
            type="button"
            aria-label={t("filterStrip.more", { count: overflow })}
          >
            +{overflow}
          </DropdownMenu.Trigger>
          <DropdownMenu.Content class="menu strip-more-menu">
            {#each chips.slice(inline) as chip (chipKey(chip))}
              <div class="menu-item strip-more-row">
                <span class="item-label">{chipLabel(chip)}</span>
                <button
                  class="strip-chip-x"
                  type="button"
                  aria-label={t("filterStrip.remove", { name: chipLabel(chip) })}
                  onclick={() => removeChip(chip)}
                >
                  <Icon name="close" />
                </button>
              </div>
            {/each}
          </DropdownMenu.Content>
        </DropdownMenu.Root>
      {/if}
      <button class="control strip-clear" type="button" onclick={() => library.setFilter(clearAllPatch())}>
        {t("filterStrip.clearAll")}
      </button>
    </span>
    <!-- The measure row mirrors the strip's chrome for every chip and both
         controls, off-layout; it is what makes the overflow decision
         deterministic (no clipped measurement in the visible row). -->
    <span class="strip-measure" aria-hidden="true" bind:this={measureEl}>
      {#each chips as chip (chipKey(chip))}
        <span class="strip-chip">
          <span class="strip-chip-name">{chipLabel(chip)}</span>
          <button class="strip-chip-x" type="button" tabindex="-1"><Icon name="close" /></button>
        </span>
      {/each}
      <span class="control strip-more">+00</span>
      <span class="control strip-clear">{t("filterStrip.clearAll")}</span>
    </span>
  </div>
{/if}

<style>
  .filter-strip {
    position: relative;
    padding: 6px 12px 0;
  }

  .strip-row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .strip-chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    max-width: 100%;
    padding: 2px 4px 2px 8px;
    border: 1px solid var(--border-hairline);
    border-radius: var(--radius-control);
    background: var(--surface-2);
    font-size: var(--text-12);
    white-space: nowrap;
  }

  .strip-chip-name {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .strip-chip-x {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 2px;
    border: none;
    border-radius: var(--radius-control);
    background: none;
    color: var(--text-tertiary);
    cursor: pointer;
  }

  .strip-chip-x:hover {
    color: var(--text-primary);
  }

  .strip-clear {
    margin-left: auto;
    flex: none;
  }

  .strip-more {
    flex: none;
  }

  .strip-measure {
    position: absolute;
    top: 0;
    left: 0;
    visibility: hidden;
    pointer-events: none;
    white-space: nowrap;
    display: flex;
    gap: 8px;
  }

  .strip-more-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .strip-more-row .item-label {
    flex: 1;
    min-width: 0;
  }
</style>
