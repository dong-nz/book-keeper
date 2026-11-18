<script lang="ts">
  // The reader's page layout (UI-30 continuous stack; UI-34 adds the
  // single-page and facing-pair layouts — the store's `viewMode` is the
  // single source, and every mode keeps the mount rule: only viewport-near
  // pages are requested, with per-page geometry from the store's exact math
  // (never measured)). Continuous: the released spacer + translateY pattern
  // with the standard 12 px gap and the standard scrollbar (base.css global
  // rules). Facing: rows of consecutive pairs, row geometry from the pair's
  // taller page. Single (revised by DR-204): exactly one mounted page in a
  // scrolling container — centered when it fits, top-aligned when taller —
  // with edge navigation: continued wheel at a content edge advances one
  // page per full page pitch. All modes center the mounted unit
  // horizontally (DR-205): auto margins center it below fit width and
  // collapse to zero above it, so an over-wide page stays fully reachable.
  import { CONTENT_GUTTER, OVERSCAN, window as mountWindow } from "$lib/virtual";
  import { edgeStep, type ReaderStore } from "$lib/state/reader.svelte";
  import { t } from "$lib/i18n";

  // DR-341 (the real close of DR-334): the overlay `<img>` is permanently
  // present (DR-318), and after the release step (DR-323) its `full` is
  // null — with the src attribute simply absent, a sized image element with
  // no src still paints Chromium's broken-image placeholder at opacity 1
  // (`data-loaded` stays set after the fade). A decodable src never renders
  // the glyph, and the attribute-only patch rule (DR-318) is untouched. The
  // placeholder is a 1×1 transparent GIF served from the app's own origin
  // (`ui/public/`, CSP `img-src 'self'`) — the app's image surfaces all
  // come from controlled origins (the EPUB sanitizer rewrites author
  // data:-URIs through the content protocol), so `data:` stays out of the
  // CSP rather than widening it for one internal detail.
  const TRANSPARENT_PIXEL = "/transparent-1px.gif";

  let {
    store,
  }: {
    store: ReaderStore;
  } = $props();

  let scroller: HTMLDivElement | undefined = $state();

  const mode = $derived(store.viewMode);
  const heights = $derived(store.pageHeights());
  const pageCount = $derived(store.doc?.page_count ?? 0);
  // DR-155: the mount window and the page label derive from the store's
  // own scroll position — one source, one flush, never read inconsistent.
  // Facing mode windows over rows (UI-34); continuous over page heights;
  // horizontal (v0.0.10, DR-290) over the row's width-based table
  // (`rowXOffsets`) on the x-axis — `window()` is axis-agnostic (position,
  // viewport extent, offsets), and its `padTop`/`totalHeight` carry the
  // row's left pad and total width.
  const win = $derived(
    mode === "facing"
      ? mountWindow(store.scrollTop, store.viewportHeight, store.rowOffsets())
      : mode === "horizontal"
        ? mountWindow(store.scrollLeft, store.viewportWidth, store.rowXOffsets())
        : mountWindow(store.scrollTop, store.viewportHeight, store.pageOffsets()),
  );
  // Single mode mounts exactly the current page (UI-34: index-driven).
  const singlePage = $derived(store.currentPage);
  const singleHeight = $derived(heights[singlePage - 1] ?? 0);
  // DR-154: until the first measurement there is no geometry — a zero-height
  // stack makes `pageAt` report the last page and a width-0 request waste a
  // raster. The page rendering and the page requests wait for the measured
  // width; the window shows its loading surface until then, and the store's
  // page position stays at page 1, where the scroller actually is.
  const measured = $derived(store.fitWidth > 0);

  // The mount rule (UI-30/AC-65): images are requested for the mounted
  // window only, never for the whole document — at 1 000 pages this stays
  // bounded by the viewport; in single mode the mount is one page (UI-34).
  // `requestPage` no-ops for pages already fetched at the current width, so
  // scrolling and re-renders re-request nothing.
  // The order (NFR-08 / DR-150): the current page is requested first, and
  // the rest of the window follows only after it has arrived — the page the
  // user asked for leads its queue, and a raster at a zoom the user has
  // left behind is never requested.
  $effect(() => {
    if (!measured) return;
    store.requestPage(store.currentPage);
    const currentWidth = store.pageWidth(store.currentPage);
    const image = store.images.get(store.currentPage);
    if (!(image && image.width === currentWidth)) return;
    if (mode === "single") {
      // Prefetch window (UI-34/AC-79; DR-219's neighbours widened to the
      // ±OVERSCAN window by DR-224): once the current page's image arrives,
      // the current page ± 6 are requested at the same width — the same
      // bounded lookahead the continuous mount window carries (one shared
      // rule, `OVERSCAN`). Queued movement-direction-first: the side the
      // reader is moving leads the other side. The mount rule is untouched:
      // the mount stays one page; the window only serves images into the
      // store's cache, thirteen requests per state whatever the document.
      const center = store.currentPage;
      const ahead = store.lastNavDirection >= 0 ? 1 : -1;
      for (const side of [ahead, -ahead]) {
        for (let step = 1; step <= OVERSCAN; step++) {
          const page = center + side * step;
          if (page >= 1 && page <= pageCount) store.requestPage(page);
        }
      }
      return;
    }
    if (mode === "facing") {
      for (let row = win.start; row < win.end; row++) {
        for (const page of [2 * row + 1, 2 * row + 2]) {
          if (page !== store.currentPage && page <= pageCount) store.requestPage(page);
        }
      }
      return;
    }
    for (let index = win.start; index < win.end; index++) {
      if (index + 1 !== store.currentPage) store.requestPage(index + 1);
    }
  });

  // Navigation plumbing (UI-30): the store computes targets, the scroller
  // performs the motion — same callback pattern as `library.onQueryReset`.
  // The notch callback (UI-39/DR-228) is the single-mode keyboard wheel:
  // the view applies the DR-227 edge rule, the store routes the modes.
  // v0.0.10 (DR-290): in horizontal mode the same offsets and deltas are
  // applied on the x-axis — the store's math is axis-agnostic, the view
  // owns the axis. The effect re-runs on a mode switch (it reads `mode`
  // and the rebound `scroller`), so the registrations always name the
  // live element.
  $effect(() => {
    const horizontal = mode === "horizontal";
    store.onScrollTo = (offsetPx) =>
      scroller?.scrollTo(horizontal ? { left: offsetPx } : { top: offsetPx });
    store.onScrollBy = (deltaPx) =>
      scroller?.scrollBy(horizontal ? { left: deltaPx } : { top: deltaPx });
    store.onNotch = (deltaPx) => notchScroll(deltaPx);
    return () => {
      store.onScrollTo = null;
      store.onScrollBy = null;
      store.onNotch = null;
    };
  });

  // Mode-switch handoff (UI-34: "changing mode keeps `currentPage`"): the
  // view performs the jump once the new layout is mounted — at the moment
  // of the switch the old scroller element is gone. Effects run after the
  // DOM update, so the new scroller and its `onScrollTo` registration (the
  // effect above, declared first, runs first) are in place when this one
  // reads the nonce.
  let revealedNonce = 0;
  $effect(() => {
    const nonce = store.revealNonce;
    if (nonce === 0 || nonce === revealedNonce) return;
    revealedNonce = nonce;
    store.goToPage(store.pendingReveal);
  });

  // Edge navigation (single mode, DR-204; immediate rule per DR-227): a
  // wheel event whose direction points past a reached content edge advances
  // exactly one page through the same `goToPage` path the controls use —
  // no accumulation, and one event never advances more than one page. The
  // landing rule is unchanged: a forward step starts the next page at its
  // top, a backward step lands at the previous page's bottom edge. The
  // keyboard notch handler (UI-39) runs the same rule through `edgeStep`.
  function stepFromEdge(step: number): void {
    store.goToPage(singlePage + step);
    requestAnimationFrame(() => {
      if (!scroller) return;
      scroller.scrollTo({ top: step > 0 ? 0 : scroller.scrollHeight });
    });
  }

  function edgeNavigate(e: WheelEvent) {
    const el = scroller;
    if (!el || mode !== "single" || !measured) return;
    const step = edgeStep(e.deltaY, el.scrollTop, el.clientHeight, el.scrollHeight);
    if (step !== 0) stepFromEdge(step);
  }

  // The keyboard notch (UI-39/DR-228): the single-mode counterpart of one
  // wheel event — at a reached edge the same page step, otherwise the
  // container scrolls by the notch.
  function notchScroll(deltaPx: number): void {
    const el = scroller;
    if (!el || mode !== "single" || !measured) return;
    const step = edgeStep(deltaPx, el.scrollTop, el.clientHeight, el.scrollHeight);
    if (step !== 0) {
      stepFromEdge(step);
      return;
    }
    el.scrollBy({ top: deltaPx });
  }

  // Continuous horizontal (UI-34 MODIFIED, v0.0.10 DR-290): the wheel's
  // counterpart of the row — UI-39 names the arrow keys as "the faithful
  // counterpart of the wheel, which scrolls the row", so the wheel owns the
  // row: a vertical delta scrolls the row horizontally. A real horizontal
  // delta (trackpad pan, Shift+wheel) is left to the browser's native
  // horizontal scroll.
  function rowWheel(e: WheelEvent) {
    const el = scroller;
    if (!el || mode !== "horizontal") return;
    if (e.deltaX !== 0) return;
    e.preventDefault();
    el.scrollBy({ left: e.deltaY });
  }

  function measure() {
    if (!scroller) return;
    // DR-155: the position belongs to the store — the label and the mount
    // window both derive from it, in the same flush. v0.0.10 (DR-290):
    // both axes are written; each mode's derivation reads its own axis.
    store.scrollTop = scroller.scrollTop;
    store.scrollLeft = scroller.scrollLeft;
    store.viewportHeight = scroller.clientHeight;
    store.viewportWidth = scroller.clientWidth;
    // Fit-width is derived from the content box (AC-66): the window minus
    // the two standard gutters (UI-19), exactly as DocumentArea measures it.
    // The fit base is the same in horizontal mode (UI-34 MODIFIED: fit the
    // current page to the viewport; the row scrolls past it).
    store.fitWidth = Math.max(0, scroller.clientWidth - 2 * CONTENT_GUTTER);
  }

  $effect(() => {
    if (!scroller) return;
    // DR-158: the callback only reads; the state write goes to one
    // requestAnimationFrame per frame. Writing $state inside the observer's
    // delivery step re-renders the page stack in the same step, the
    // old-width pages transiently overflow the new scroller box, the
    // resulting scrollbar change re-triggers the observer — and Chromium ends
    // the round with "ResizeObserver loop completed with undelivered
    // notifications". Letting the delivery round finish first applies the
    // new geometry once, cleanly, at most one frame later.
    let raf = 0;
    const observer = new ResizeObserver(() => {
      if (raf) return;
      raf = requestAnimationFrame(() => {
        raf = 0;
        if (!scroller) return;
        // Fit-width re-derives on every resize (UI-30/AC-66): the geometry is
        // computed from the new width, the mounted pages re-request at it.
        store.fitWidth = Math.max(0, scroller.clientWidth - 2 * CONTENT_GUTTER);
        // The viewport extent is measured here too — `observe` fires once
        // immediately, so the first PgUp/PgDn already knows its travel
        // distance (a scroll-only measurement would make it a no-op), on
        // either axis (v0.0.10 horizontal mode steps by the width).
        store.viewportHeight = scroller.clientHeight;
        store.viewportWidth = scroller.clientWidth;
      });
    });
    observer.observe(scroller);
    return () => {
      observer.disconnect();
      if (raf) cancelAnimationFrame(raf);
    };
  });
</script>

{#snippet pageBox(page: number, height: number)}
  {@const width = store.pageWidth(page)}
  {@const image = store.images.get(page)}
  <div class="page" style="width: {width}px; height: {height}px">
    {#if image && image.width === width}
      <img src={image.src} alt={t("reader.pageAlt", { page })} style="width: {width}px; height: {height}px" />
      <!-- DR-316: the full-resolution stage overlays the preview and fades
           in at its own decode completion — the resolution swap reads as
           sharpening, not a flash. Decorative: the base image carries the
           page's accessible name. DR-318: the overlay element is ALWAYS
           present (its src is the transparent placeholder until the full
           stage appears — DR-341) — the branch never changes shape, so an
           entry update is an attribute-only patch: the base image node is
           never rebuilt, and the painted preview is never torn down (a
           rebuilt <img> must decode again — that gap was the surviving
           flash). DR-323: the load event hands the page to the store's
           release timer — after the fade the full stage becomes the base
           src and this overlay's src returns to the placeholder, so the
           coexistence is the fade's own window, not the page's whole
           mounted lifetime. -->
      <img
        class="page-full"
        src={image.full ?? TRANSPARENT_PIXEL}
        alt=""
        aria-hidden="true"
        data-loaded={undefined}
        style="width: {width}px; height: {height}px"
        onload={(e) => {
          // Svelte types the generic `load` event at `EventTarget & Element`
          // (`EventHandler` in svelte/elements omits the element type), so
          // the image is read through the same cast the other components use
          // for their element-specific fields.
          const imageEl = e.currentTarget as HTMLImageElement;
          // Only the full stage's own load drives the fade and the release
          // (DR-316/DR-323). The placeholder (and a stale load event that
          // lands after the src patch) must not set `data-loaded` — an
          // early fade would re-create the flash the seam exists to hide.
          if (image.full === null || imageEl.currentSrc !== image.full) return;
          imageEl.setAttribute("data-loaded", "");
          store.previewShown(page, width);
        }}
      />
    {:else}
      <!-- The honest skeleton (UI-29): a neutral ground while the
           request is in flight — no fake content, no spinner copy. -->
      <div class="page-placeholder" role="img" aria-label={t("reader.pageAlt", { page })}></div>
    {/if}
  </div>
{/snippet}

{#if mode === "single"}
  <!-- Single page (UI-34, revised by DR-204): exactly one mounted page in
       a scrolling container — centered when it fits, top-aligned when
       taller, so a fit-width page is reachable to its bottom edge;
       continued wheel at a content edge navigates (edgeNavigate). The
       content gutters apply the same way; fit-width measures this box. -->
  <div class="reader-single" bind:this={scroller} onwheel={edgeNavigate}>
    {#if store.status === "ready" && measured}
      {@render pageBox(singlePage, singleHeight)}
    {/if}
  </div>
{:else if mode === "horizontal"}
  <!-- Continuous horizontal (UI-34 MODIFIED, v0.0.10 DR-290): one row of
       pages in reading order, free horizontal scroll — the continuous model
       on the x-axis. Same windowing: row geometry IS page geometry (the
       same offset table), `padTop`/`totalHeight` carry the row's left pad
       and total width, same OVERSCAN bounds (§3.3). The row is vertically
       centered with auto margins — DR-205's rule on the other axis:
       centered when it fits, start-aligned when taller, so a page taller
       than the viewport stays fully reachable through vertical scroll. -->
  <div class="reader-row" bind:this={scroller} onscroll={measure} onwheel={rowWheel}>
    {#if store.status === "ready" && measured && win.end > win.start}
      <div class="row-track" style="width: {win.totalHeight}px">
        <div class="row-window" style="transform: translateX({win.padTop}px)">
          {#each Array.from({ length: win.end - win.start }, (_, i) => win.start + i) as index (index)}
            {@render pageBox(index + 1, heights[index])}
          {/each}
        </div>
      </div>
    {/if}
  </div>
{:else}
  <div class="reader-scroll" bind:this={scroller} onscroll={measure}>
    {#if store.status === "ready" && measured && win.end > win.start}
      <div class="stack" style="height: {win.totalHeight}px">
        <div class="window" style="transform: translateY({win.padTop}px)">
          {#if mode === "facing"}
            <!-- Facing (UI-34): rows of consecutive pairs (1|2, 3|4, … —
                 DR-180); the row is the taller page's height, the pair's
                 inner gap is the standard one. An unpaired last page
                 renders alone in its row — no special case. -->
            {#each Array.from({ length: win.end - win.start }, (_, i) => win.start + i) as row (row)}
              <div class="row">
                {#each [2 * row + 1, 2 * row + 2] as page (page)}
                  {#if page <= pageCount}
                    {@render pageBox(page, heights[page - 1])}
                  {/if}
                {/each}
              </div>
            {/each}
          {:else}
            {#each Array.from({ length: win.end - win.start }, (_, i) => win.start + i) as index (index)}
              {@render pageBox(index + 1, heights[index])}
            {/each}
          {/if}
        </div>
      </div>
    {/if}
  </div>
{/if}

<style>
  .reader-scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    /* DR-152: with a windowed mount the browser's default scroll anchoring
       compensates transient layout shifts above the viewport and drags the
       settled position a page pitch off target; the position the app
       measures must be the position it requested. */
    overflow-anchor: none;
    /* UI-19: the standard content inset; the window keeps its inset: 0
       block (padTop/padBottom and the mount bound are unaffected), and the
       pages fill the padded content box — the same rule the list follows. */
    padding: 0 var(--content-gutter);
    box-sizing: border-box;
  }

  .stack {
    position: relative;
  }

  /* Continuous horizontal (UI-34 MODIFIED, v0.0.10, DR-290): the scroller
     owns the x-axis — free horizontal scroll, the same content gutters
     (fit-width measures this box), and the same `overflow-anchor: none`
     (DR-152 on the x-axis: windowed mounting must not let the engine drag
     the measured position). The vertical axis stays reachable: with
     `overflow-x: auto` the visible `overflow-y` computes as `auto`, so a
     page taller than the viewport scrolls vertically — the row's
     counterpart of a zoomed page in single mode. */
  .reader-row {
    flex: 1;
    min-height: 0;
    overflow-x: auto;
    overflow-anchor: none;
    padding: 0 var(--content-gutter);
    box-sizing: border-box;
  }

  /* The row's track: the full row width as an explicit box — the honest
     scrollbar rule `.stack` follows for `totalHeight` (NFR-01) — at least
     the viewport height, so the row's auto margins have a cross size to
     center against; it grows to a taller page, whose top stays reachable
     because the auto margins collapse to zero (DR-205's pattern, other
     axis). */
  .row-track {
    display: flex;
    min-height: 100%;
  }

  /* The row's window: mounted pages in reading order, left to right, the
     standard 12 px gap (the same token/constant lockstep as `.window`'s
     `--card-gap`/`PAGE_GAP`), translated by the left pad — the counterpart
     of `.window`'s translateY pattern. `margin-block: auto` centers the row
     in the viewport when it fits and collapses to zero when it does not;
     the pages carry the same auto margins on the cross axis for centering
     within the row (the counterpart of `.row`'s `margin-inline: auto`). */
  .row-window {
    display: flex;
    gap: var(--card-gap);
    margin-block: auto;
  }

  .reader-row .page {
    margin-block: auto;
  }

  /* Facing mode's pair row (UI-34): two pages, the standard gap, the
     shorter page top-aligned so the row's height is the taller one's.
     DR-205: auto margins center the row below fit width and collapse to
     zero above it (an over-wide row stays fully reachable). */
  .row {
    display: flex;
    align-items: flex-start;
    gap: var(--card-gap);
    margin-inline: auto;
  }

  /* Single page (UI-34, revised by DR-204): a scrolling container —
     vertical always, horizontal when zoomed beyond the width. The page
     carries auto margins on all four sides: centered when it fits,
     top-/start-aligned when larger, and every edge stays reachable
     (`place-content: center` on a grid would clip a taller page's top
     beyond scroll reach — the pattern this replaces). Inside the same
     content gutters; fit-width measures this box. */
  .reader-single {
    min-height: 0;
    overflow: auto;
    display: flex;
    padding: 0 var(--content-gutter);
    box-sizing: border-box;
  }

  .reader-single .page {
    margin: auto;
  }

  .window {
    display: flex;
    flex-direction: column;
    /* The standard 12 px gap (UI-11/UI-30): the CSS reads the token, the
       offsets read `PAGE_GAP` — the same token/constant lockstep rule
       `virtual.ts` states for `--card-gap`/`CARD_GAP`. */
    gap: var(--card-gap);
  }

  .page {
    border: 1px solid var(--border-hairline);
    border-radius: var(--radius-control);
    box-shadow: var(--shadow-overlay);
    overflow: hidden;
    /* DR-316: the positioning base for the full-stage overlay. */
    position: relative;
    /* DR-205: horizontal centering in the continuous stack — auto margins
       center a page below fit width and collapse to zero above it. */
    margin-inline: auto;
  }

  .page img {
    display: block;
  }

  /* DR-316: the full-resolution stage over the preview (the two-stage
     pipeline DR-151/DR-153 is unchanged — only its seam is hidden).
     Invisible until its own load event; then a short fade to 1. The
     top/left origin is the padding box — the same origin the base image
     lays at, so the two rasters coincide pixel for pixel. */
  .page-full {
    position: absolute;
    top: 0;
    left: 0;
    opacity: 0;
    transition: opacity 160ms ease-out;
  }

  .page-full[data-loaded] {
    opacity: 1;
  }

  .page-placeholder {
    width: 100%;
    height: 100%;
    background: var(--surface-1);
  }
</style>
