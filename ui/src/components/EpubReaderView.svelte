<script lang="ts">
  // The EPUB reading surface (SPEC-v0.0.9 §3.13, UI-48, DR-303): one view
  // mode — continuous vertical flow in spine order. The mount window reuses
  // the released `$lib/virtual` windowing mechanism with the §3.12 rule-3
  // bound (viewport ± 1 chapter); measured chapter heights are kept across
  // recycling, an unmeasured chapter shows only its header line at the edge
  // of the mount window. Chapter documents arrive through the content
  // protocol (the store fetches them) and are mounted in a Shadow DOM per
  // chapter — author styles and element ids are scoped inside it, the app's
  // theme ramp and the text-size custom property reach the content through
  // inheritance. The reading column: centered, 68ch measure, UI-19 gutters.
  import { onMount } from "svelte";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { CONTENT_GUTTER } from "$lib/virtual";
  import {
    CHUNK_NODES,
    type EpubReaderStore,
  } from "$lib/state/epubReader.svelte";
  import { settings } from "$lib/state/settings.svelte";
  import { feedback } from "$lib/state/feedback.svelte";
  import { t } from "$lib/i18n";
  import { asAppError } from "$lib/types";

  let { store }: { store: EpubReaderStore } = $props();

  let scroller = $state<HTMLDivElement>();
  const range = $derived(store.mountRange());
  const spine = $derived(store.manifest?.spine ?? []);
  const mounted = $derived.by(() => {
    const indexes: number[] = [];
    for (let i = range.start; i < Math.min(range.end, spine.length); i++) indexes.push(i);
    return indexes;
  });
  // The manifest's declared direction, narrowed to what the `dir` attribute
  // accepts; a chapter's own `<html dir>` overrides it inside the action.
  const direction = $derived.by(() => {
    const declared = store.manifest?.direction;
    return declared === "ltr" || declared === "rtl" || declared === "auto" ? declared : undefined;
  });
  const textSize = $derived(settings.epubTextSize);

  // Imperative chapter machinery (DR-303 items 2/3/4): the shadow hosts,
  // their parsed node lists, and how many nodes are mounted. Plain Maps —
  // they describe DOM state, not UI state; the reactive sources stay the
  // store's (text, heights, scroll position).
  const hosts = new Map<number, ChapterHost>();
  const parsedCache = new Map<number, ParsedChapter>();
  // Chapters a link jump forced to mount completely (the fragment target
  // must exist before the view can scroll to it).
  const fullMount = new Set<number>();
  let pendingFragment: { chapter: number; fragment: string } | null = null;

  interface ParsedChapter {
    styles: string[];
    links: string[];
    dir: string | null;
    nodes: Node[];
  }

  interface ChapterHost {
    section: HTMLElement;
    body: HTMLElement;
    root: ShadowRoot;
    nodes: Node[];
    mountedCount: number;
  }

  // Base CSS injected into every chapter's shadow root BEFORE the author's,
  // so the book's own stylesheet wins (the reading typography is the app's
  // — §3.13 — and the author overrides it the way EPUB says it may). Custom
  // properties inherit across the shadow boundary, so `--epub-text-size` and
  // the theme ramp reach the content.
  const CHAPTER_BASE_CSS = `
    :host, .flow-root { color: inherit; }
    /* A formatting root: the book content's own margins stay inside the
       measured box, so the stack's gaps are exactly the model's PAGE_GAP
       and the DOM geometry and the position model agree on every edge. */
    .flow-root { display: flow-root; }
    body { margin: 0; }
    p, li, div, span, td, th { overflow-wrap: break-word; }
    img, svg, video { max-width: 100%; height: auto; }
    img { display: block; margin: 0.6em auto; }
    a { color: var(--accent); }
    blockquote { margin: 0.6em 0 0.6em 1.2em; }
    hr { border: none; border-top: 1px solid var(--border-hairline); margin: 1.2em 0; }
    pre, code { white-space: pre-wrap; overflow-wrap: break-word; }
    table { border-collapse: collapse; max-width: 100%; }
  `;

  function labelFor(index0: number): string | null {
    return store.chapterLabel(index0 + 1);
  }

  function parseChapter(index0: number): ParsedChapter | null {
    const cached = parsedCache.get(index0);
    if (cached) return cached;
    const text = store.chapterText.get(index0 + 1);
    if (text === undefined) return null;
    const doc = new DOMParser().parseFromString(text, "text/html");
    // Stylesheet links are delivery plumbing wherever the package put them
    // (real-world chapters use the head; some bodies carry them too): all
    // are collected for the scoped injection and the font-face hoist, and
    // top-level body ones are dropped from the content nodes so each link
    // exists exactly once.
    const parsed: ParsedChapter = {
      styles: Array.from(doc.head.querySelectorAll("style")).map((s) => s.textContent ?? ""),
      links: Array.from(
        doc.querySelectorAll('head link[rel="stylesheet"], body link[rel="stylesheet"]'),
      )
        .map((l) => l.getAttribute("href"))
        .filter((href): href is string => href !== null),
      dir: doc.documentElement.getAttribute("dir"),
      nodes: Array.from(doc.body.childNodes).filter(
        (node) =>
          !(
            node instanceof HTMLLinkElement &&
            (node.getAttribute("rel") ?? "").toLowerCase() === "stylesheet"
          ),
      ),
    };
    parsedCache.set(index0, parsed);
    // The header-line fallback needs the chapter's own first heading before
    // the header renders (§3.13: ToC label, else first heading, else none).
    const heading = doc.body.querySelector("h1, h2, h3, h4, h5, h6");
    const headingText = heading?.textContent?.trim();
    if (headingText && !store.chapterHeadings.has(index0 + 1)) {
      const next = new Map(store.chapterHeadings);
      next.set(index0 + 1, headingText);
      store.chapterHeadings = next;
    }
    return parsed;
  }

  // The chapter action (the Svelte action contract: node + parameter, an
  // optional teardown): register the section (measured) and its body's
  // shadow root (mounted), build content, and tear the registration down on
  // unmount (recycling).
  function chapterSection(node: HTMLElement, index0: number) {
    const body = node.querySelector(".chapter-body") as HTMLElement;
    const root = body.attachShadow({ mode: "open" });
    const host: ChapterHost = { section: node, body, root, nodes: [], mountedCount: 0 };
    hosts.set(index0, host);
    buildChapter(index0, host);
    return {
      destroy: () => {
        hosts.delete(index0);
      },
    };
  }

  // DR-307: `@font-face` rules are document-scoped by the CSS spec — a face
  // declared inside a chapter's shadow tree is never registered (measured on
  // this engine: no FontFace, no request, from `<style>` and adopted sheets
  // alike). The faces the package's CSS declares are hoisted into one
  // document-level style in this reader window; the rest of the author CSS
  // stays scoped to the chapter (document-level selectors would leak into the
  // window chrome). Deduplicated per window: one stylesheet is typically
  // linked from every chapter.
  const hoistedFaces = new Set<string>();
  const fetchedStylesheets = new Set<string>();
  let fontStyle: HTMLStyleElement | null = null;

  function hoistFaces(css: string): void {
    const faces = css.match(/@font-face\s*\{[^{}]*\}/g);
    if (!faces) return;
    const base = store.manifest?.base_url ?? "";
    for (const face of faces) {
      // DR-308: only faces the delivery mechanism can resolve are hoisted.
      // A face whose src names a remote host — or any reference the delivery
      // rewrote to nothing — stays in the chapter's scoped CSS, where this
      // engine ignores it (measured, DR-307). The reading surface never
      // addresses a remote host, even when the author applies such a font to
      // the body text (AC-137: the url() to a remote host produces no
      // request).
      const urls = Array.from(face.matchAll(/url\(([^)]*)\)/g), (m) =>
        m[1].trim().replace(/^['"](.*)['"]$/, "$1"),
      );
      if (!base || urls.some((url) => !url.startsWith(base))) continue;
      const key = face.replace(/\s+/g, " ").trim();
      if (hoistedFaces.has(key)) continue;
      hoistedFaces.add(key);
      if (!fontStyle) {
        fontStyle = document.createElement("style");
        fontStyle.setAttribute("data-epub-fonts", "");
        document.head.append(fontStyle);
      }
      fontStyle.append(face);
    }
  }

  // A linked stylesheet keeps its `<link>` (its selectors apply scoped); its
  // text is fetched once per window for the face extraction. Best-effort: a
  // failed fetch leaves the link's own delivery untouched. The served CSS
  // carries protocol URLs (DR-307), so a hoisted face resolves.
  function hoistLinkedFaces(links: string[]): void {
    for (const href of links) {
      if (fetchedStylesheets.has(href)) continue;
      fetchedStylesheets.add(href);
      void fetch(href)
        .then((response) => (response.ok ? response.text() : ""))
        .then((css) => hoistFaces(css))
        .catch(() => undefined);
    }
  }

  function buildChapter(index0: number, host: ChapterHost): void {
    const parsed = parseChapter(index0);
    // Idempotent: a chapter whose text arrives after its section mounted is
    // built once, when the text lands — the rebuild replaces the shell.
    const style = document.createElement("style");
    style.textContent = CHAPTER_BASE_CSS;
    host.root.replaceChildren(style);
    host.nodes = [];
    host.mountedCount = 0;
    if (parsed === null) return; // text still in flight or failed — the
    // store's fetch effect re-runs `buildChapter` when it arrives.
    if (parsed.dir) host.section.setAttribute("dir", parsed.dir);
    for (const css of parsed.styles) {
      const s = document.createElement("style");
      s.textContent = css;
      host.root.append(s);
      hoistFaces(css);
    }
    for (const href of parsed.links) {
      const link = document.createElement("link");
      link.rel = "stylesheet";
      link.href = href;
      host.root.append(link);
    }
    hoistLinkedFaces(parsed.links);
    const flowRoot = document.createElement("div");
    flowRoot.className = "flow-root";
    host.root.append(flowRoot);
    host.nodes = parsed.nodes;
    appendChunk(index0, host);
  }

  function appendChunk(index0: number, host: ChapterHost): void {
    const flowRoot = host.root.querySelector(".flow-root");
    if (!flowRoot) return;
    const chunked = store.isChunked(index0) && !fullMount.has(index0);
    const limit = chunked
      ? Math.min(host.mountedCount + CHUNK_NODES, host.nodes.length)
      : host.nodes.length;
    const slice = host.nodes.slice(host.mountedCount, limit);
    flowRoot.append(...slice);
    host.mountedCount = limit;
    if (host.mountedCount >= host.nodes.length) {
      resolvePendingFragment(index0);
    } else if (chunked) {
      scheduleChunkTopUp();
    }
  }

  // Chunk top-up (§3.12 rule 3): when the mounted slice of a huge chapter
  // approaches the viewport edge, the next slice goes in. The check runs on
  // the reactive scroll position — the same single source everything else
  // derives from (DR-155).
  let topUpFrame = 0;
  function scheduleChunkTopUp(): void {
    if (topUpFrame) return;
    topUpFrame = requestAnimationFrame(() => {
      topUpFrame = 0;
      if (!scroller) return;
      const scrollerRect = scroller.getBoundingClientRect();
      for (const [index0, host] of hosts) {
        if (!store.isChunked(index0) || host.mountedCount >= host.nodes.length) continue;
        // The section's bottom edge sits this far below the viewport top;
        // top up when it nears 1.5 viewports (the released chunk rule).
        const bottom = host.section.getBoundingClientRect().bottom;
        if (bottom - scrollerRect.top < 1.5 * store.viewportHeight) {
          appendChunk(index0, host);
        }
      }
    });
  }

  // A measured height change above the reader's place moves the content
  // below it down, not the reader (lazy images must not shift what is on
  // screen, DR-303 item 3). Native anchoring is off on the scroller so this
  // compensation is the only one.
  let heightObserver: ResizeObserver | undefined;
  function measureChapter(index0: number): void {
    const host = hosts.get(index0);
    if (!host || !scroller) return;
    const delta = store.recordHeight(index0, host.section.offsetHeight);
    if (delta !== 0 && index0 < store.currentChapter - 1) {
      programmaticScroll = true;
      scroller.scrollTop += delta;
    }
    if (delta !== 0 && stickToEnd) {
      // The stack grew while `End` owns the position: re-apply the landing.
      programmaticScroll = true;
      scroller.scrollTo({ top: scroller.scrollHeight });
    }
  }

  // The reading column's top padding (the `.reading` rule below) is chrome,
  // not content: the store's position model describes the content stack, so
  // the boundary compensates — reads subtract the pad, jumps add it back.
  const STACK_TOP_PAD = 24; // must equal `.reading`'s padding-top

  // DR-306 companion: the `End` landing sticks to the flow bottom while the
  // content settles — the last chapters mount as the bottom approaches, and
  // their measured heights grow the stack past the first landing. Any other
  // navigation or a user scroll cancels the stick.
  let stickToEnd = false;
  let programmaticScroll = false;

  function measure(): void {
    if (!scroller) return;
    // DR-155: the position belongs to the store — the readout, the mount
    // window, and the panel highlight all derive from it, in one flush.
    store.scrollTop = scroller.scrollTop - STACK_TOP_PAD;
    store.viewportHeight = scroller.clientHeight;
    if (stickToEnd) scroller.scrollTo({ top: scroller.scrollHeight });
  }

  function onScroll(): void {
    if (!programmaticScroll) stickToEnd = false;
    programmaticScroll = false;
    measure();
    scheduleChunkTopUp();
  }

  // Fetch what the mount window needs (the released mount rule: only the
  // window, never the book); a chapter whose text arrives late (the first
  // paint's chapter 1 is already in hand — §3.13) builds when it lands.
  $effect(() => {
    for (const index0 of mounted) {
      const index1 = index0 + 1;
      if (!store.chapterText.has(index1) && !store.chapterErrors.has(index1)) {
        void store.requestChapter(index1);
      } else if (hosts.has(index0) && store.chapterText.has(index1)) {
        const host = hosts.get(index0)!;
        if (host.mountedCount === 0 && host.nodes.length === 0) buildChapter(index0, host);
      }
    }
  });

  // Measurement plumbing (the released pattern, DR-158): one rAF per frame;
  // the observers only read, the state writes go through the store.
  $effect(() => {
    if (!scroller) return;
    let raf = 0;
    const viewportObserver = new ResizeObserver(() => {
      if (raf) return;
      raf = requestAnimationFrame(() => {
        raf = 0;
        measure();
      });
    });
    viewportObserver.observe(scroller);
    heightObserver = new ResizeObserver(() => {
      if (raf) return;
      raf = requestAnimationFrame(() => {
        raf = 0;
        for (const index0 of [...hosts.keys()]) measureChapter(index0);
      });
    });
    for (const index0 of mounted) {
      const host = hosts.get(index0);
      if (host) heightObserver.observe(host.section);
    }
    measure();
    for (const index0 of mounted) measureChapter(index0);
    return () => {
      viewportObserver.disconnect();
      heightObserver?.disconnect();
      if (raf) cancelAnimationFrame(raf);
    };
  });

  // The store's jump lands through the view's scroller (the released
  // callback pattern).
  $effect(() => {
    store.onScrollTo = (offsetPx) => {
      if (!scroller) return;
      // The store's `scrollToEnd` sentinel (DR-306): stick to the bottom.
      stickToEnd = offsetPx === Number.MAX_SAFE_INTEGER;
      programmaticScroll = true;
      if (stickToEnd) scroller.scrollTo({ top: scroller.scrollHeight });
      else scroller.scrollTo({ top: offsetPx + STACK_TOP_PAD });
    };
    store.onScrollBy = (deltaPx) => {
      stickToEnd = false;
      programmaticScroll = true;
      scroller?.scrollBy({ top: deltaPx });
    };
    return () => {
      store.onScrollTo = null;
      store.onScrollBy = null;
    };
  });

  // Internal links and the ToC's fragment jumps (§3.13): the target chapter
  // mounts first (fully, so the id exists), then the view scrolls to it.
  function jumpToTarget(chapter: number | null, fragment: string): void {
    const index0 = chapter ?? store.currentChapter - 1;
    pendingFragment = fragment ? { chapter: index0, fragment } : null;
    if (fragment) fullMount.add(index0);
    store.goToPage(index0 + 1);
    resolvePendingFragment(index0);
  }

  function resolvePendingFragment(index0: number): void {
    if (!pendingFragment || pendingFragment.chapter !== index0) return;
    const host = hosts.get(index0);
    if (!host || host.mountedCount < host.nodes.length) return;
    const target = host.root.getElementById(pendingFragment.fragment);
    if (!target) {
      pendingFragment = null;
      return; // the id is not in the chapter — the chapter top stands
    }
    programmaticScroll = true;
    target.scrollIntoView({ block: "start" });
    pendingFragment = null;
  }

  function onFlowClick(event: MouseEvent): void {
    // Chapter content lives in a shadow root — the event target crosses the
    // boundary as the host, so the anchor is read from the composed path
    // (`target.closest` would stop at the host and miss every book link).
    const anchor = event
      .composedPath()
      .find((node): node is HTMLAnchorElement => node instanceof HTMLAnchorElement);
    if (!anchor) return;
    const href = anchor.getAttribute("href");
    if (!href) return;
    const target = store.resolveHref(href);
    if (target !== null) {
      event.preventDefault();
      jumpToTarget(target.chapter, target.fragment);
      return;
    }
    // Not internal (DR-303 item 6): a scheme-qualified href is the OS
    // default browser's business; anything else gets no navigation.
    event.preventDefault();
    if (/^[a-z][a-z0-9+.-]*:/i.test(href)) {
      void openUrl(href).catch((error: unknown) => {
        feedback.reportError(asAppError(error));
      });
    }
  }

  onMount(() => {
    measure();
  });
</script>

  <!-- The delegated link activation (§3.13): the click target is always a
       real `<a>` inside a chapter — the container only routes it; keyboard
       reading is the window's own key map, not this element's (the released
       delegation pattern of `DocumentArea`). -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="flow"
    bind:this={scroller}
    onscroll={onScroll}
    onclick={onFlowClick}
    role="document"
    aria-label={store.documentName}
  >
  <div
    class="reading"
    style="--epub-text-size: {textSize}px"
    dir={direction ?? undefined}
  >
    {#if range.padTop > 0}
      <div class="pad" style="height: {range.padTop}px">
        {#if range.start > 0 && labelFor(range.start - 1)}
          <p class="edge-label">{labelFor(range.start - 1)}</p>
        {/if}
      </div>
    {/if}
    {#each mounted as index0 (index0)}
      <section class="chapter" use:chapterSection={index0}>
        {#if labelFor(index0)}
          <p class="chapter-label">{labelFor(index0)}</p>
        {/if}
        <div class="chapter-body"></div>
        {#if store.chapterErrors.has(index0 + 1)}
          <p class="chapter-error" role="alert">{t("reader.epub.chapterError")}</p>
        {/if}
      </section>
    {/each}
    {#if range.padBottom > 0}
      <div class="pad" style="height: {range.padBottom}px">
        {#if range.end < spine.length && labelFor(range.end)}
          <p class="edge-label">{labelFor(range.end)}</p>
        {/if}
      </div>
    {/if}
  </div>
</div>

<style>
  /* The scroller fills the middle row of the reader grid (the released
     window layout); `overflow-anchor: none` keeps Chromium's native scroll
     anchoring out of the way — the view's own compensation is the only one
     (§3.12 rule 3, DR-303 item 3). */
  .flow {
    min-height: 0;
    overflow-y: auto;
    overflow-anchor: none;
    background: var(--surface-0);
    color: var(--text-primary);
  }

  /* The reading column: centered, max measure 68ch, UI-19 gutters
     (§3.13). A dark reading mode is the theme, not a per-window setting —
     everything here reads the active ramp. */
  .reading {
    max-width: 68ch;
    margin: 0 auto;
    /* The top padding is the stack's top chrome; the script's
       `STACK_TOP_PAD` compensates it against the store's model. */
    padding: 24px var(--content-gutter) 48px;
    font-family: "Iowan Old Style", "Palatino Linotype", Palatino, Georgia, serif;
    font-size: var(--epub-text-size, 18px);
    line-height: 1.6;
    text-align: start;
  }

  .chapter {
    /* The gap between chapters is the released stack gap constant
       (`PAGE_GAP`), the same 12 px the window's other stacks use. */
    margin-bottom: 12px;
  }

  /* The chapter header line (§3.13): content only — the ToC label or the
     chapter's own first heading, in the UI's voice, not the book's. */
  .chapter-label,
  .edge-label {
    font-family: var(--font-ui);
    font-size: var(--text-13);
    color: var(--text-secondary);
    margin: 0 0 8px;
  }

  .edge-label {
    padding-top: 24px;
  }

  .chapter-error {
    font-family: var(--font-ui);
    font-size: var(--text-13);
    color: var(--error);
    margin: 0;
  }
</style>
