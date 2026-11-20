<script lang="ts">
  // FR-30 (UI-54, SPEC-v0.0.12 §3.3): the selectable text layer over a
  // rendered page. The runs arrive in top-origin page-point coordinates
  // (the Rust extraction's flip); the mapping to CSS px is pure arithmetic
  // on the store's exact geometry — scale = display width ÷ page width in
  // points — re-derived on every zoom or mode change, never measured.
  // The layer is invisible until used: transparent text over the raster;
  // the selection paints with the theme's selection token (--selection-tint,
  // UI-38/DR-220). Copy is the webview's own path (Ctrl+C / Ctrl+Insert —
  // no clipboard code, no command, §3.3).
  import type { PageTextDto } from "$lib/types";
  import { textPositions } from "$lib/state/reader.svelte";

  let {
    text,
    displayWidth,
  }: {
    text: PageTextDto;
    displayWidth: number;
  } = $props();

  const positions = $derived(textPositions(text, displayWidth));
</script>

<div class="text-layer" aria-hidden="true">
  {#each positions as run, index (index)}
    <span
      style="left: {run.x}px; top: {run.y}px; width: {run.w}px; height: {run.h}px; font-size: {run.h}px;"
    >{run.text}</span>
  {/each}
</div>

<style>
  .text-layer {
    position: absolute;
    inset: 0;
    overflow: hidden;
    /* The layer never takes a page's interactions: the container passes
       through, only the spans are hit targets — and the reader has no
       drag handlers on the page containers, so a drag is selection and
       nothing else (the §3.3 collision check, re-verified in the lane). */
    pointer-events: none;
  }

  .text-layer span {
    position: absolute;
    color: transparent;
    user-select: text;
    pointer-events: auto;
    white-space: pre;
    line-height: 1;
  }

  .text-layer span::selection {
    background: var(--selection-tint);
  }
</style>
