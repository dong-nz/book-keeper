<script lang="ts">
  // The reader window surface (SPEC-v0.0.6 §3.3 UI-30, layout as modified
  // by SPEC-v0.0.7 §3.3): reached only through the `?reader=<docId>` route
  // (main.ts), it carries no library surface — no toolbar, no menus, no
  // status bar. The window chrome (drag region + document name + the
  // released WindowControls, DR-140) sits at the top edge, the page stack
  // fills the middle, and the reader control strip renders at the window's
  // bottom edge below the stack. The chrome and control strips take the
  // app's own chrome height — no new metric, the same token the toolbar
  // uses. The control strip renders only in the `ready` state; the error
  // state renders no controls.
  import { onMount, onDestroy } from "svelte";
  import { Tooltip as TooltipKit } from "bits-ui"; // the app's own `Tooltip` component is the window's wrapper
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import WindowControls from "./components/WindowControls.svelte";
  import { currentUiPlatform, windowControlsLayout } from "$lib/platform";
  import SettingsMenu from "./components/SettingsMenu.svelte";
  import CoverMenu from "./components/CoverMenu.svelte";
  import ReaderControls from "./components/ReaderControls.svelte";
  import ReaderView from "./components/ReaderView.svelte";
  import EpubReaderControls from "./components/EpubReaderControls.svelte";
  import EpubReaderView from "./components/EpubReaderView.svelte";
  import TocDocked from "./components/TocDocked.svelte";
  import ToastStack from "./components/ToastStack.svelte";
  import UpdateModal from "./components/UpdateModal.svelte";
  import { appinfo } from "$lib/state/appinfo.svelte";
  import { dragStrip, usesOwnDragStrip } from "$lib/dragstrip";
  import Icon from "./components/Icon.svelte";
  import Tooltip from "./components/Tooltip.svelte";
  import { ReaderStore, SCROLL_NOTCH } from "$lib/state/reader.svelte";
  import { EpubReaderStore } from "$lib/state/epubReader.svelte";
  import { settings } from "$lib/state/settings.svelte";
  import { feedback } from "$lib/state/feedback.svelte";
  import { applyTheme, onSystemThemeChange } from "$lib/theme";
  import { isTextTarget } from "$lib/focus";
  import { guardContextMenu } from "$lib/contextmenu";
  import { t } from "$lib/i18n";
  import * as api from "./api";
  import { asAppError } from "$lib/types";

  // SPEC-v0.0.9 §3.5 / DR-303 item 1: the window kind arrives with the route
  // (the command decided it on the resolved path before the window existed).
  // Exactly one store is constructed — no PDF reader machinery ever runs for
  // an EPUB, none for a PDF (AC-125) — and it is the window's only state
  // owner. One store per reader window: `docId` is the route parameter this
  // window was created with and never changes for its lifetime, so capturing
  // it at construction is the intent (the warning is about reactive re-reads).
  let { docId, kind }: { docId: number; kind: "pdf" | "epub" } = $props();
  // svelte-ignore state_referenced_locally
  const pdfStore = kind === "epub" ? null : new ReaderStore(docId);
  // svelte-ignore state_referenced_locally
  const epubStore = kind === "epub" ? new EpubReaderStore(docId) : null;
  const store = $derived(epubStore ?? pdfStore);
  const win = getCurrentWindow();
  // §3.8 (UI-49, revised by DR-337): the pinned panel is part of the window
  // layout — the grid becomes two-column while a ready window's store carries
  // the pin AND the docked panel is open. Only a `ready` window can show it
  // (the pin belongs to the reading surface; an error or loading state
  // renders one column as before).
  const dockedOpen = $derived(
    (pdfStore !== null && pdfStore.status === "ready" && pdfStore.tocPinned && pdfStore.tocDockedOpen) ||
      (epubStore !== null && epubStore.status === "ready" && epubStore.tocPinned && epubStore.tocDockedOpen),
  );

  // Theme through the released path (DR-101): the stored choice wins,
  // "system" follows the OS — the same ramps, including the dark ramp, in
  // every window. settings.load() is the existing read; the reader window
  // never writes settings.
  $effect(() => {
    const choice = settings.theme;
    applyTheme(choice);
    if (choice !== "system") return;
    return onSystemThemeChange(() => applyTheme("system"));
  });

  onMount(() => {
    // DR-230/AC-99: the reader window subscribes to the settings broadcast
    // like every other window — a theme change made elsewhere re-applies
    // here live. The subscription comes before the read, App.svelte's
    // startup order unchanged.
    void (async () => {
      await settings.initEvents();
      await settings.load();
      // UI-57 (SPEC-v0.0.13 §3.4): the reader window's settings menu can run
      // a manual update check (UI-50, DR-344), and the update modal shows the
      // current version — the same one fetch as the main window, per window.
      appinfo.ensure();
      await (epubStore ?? pdfStore)?.load();
    })();
  });

  // The reader window's own key map (UI-30). Esc closes this window —
  // with the ToC panel open it closes the panel first and the window
  // stays open (UI-33, the released overlay-priority guard of §5.2). The
  // guard decides from the app's own overlay registry — an open overlay is
  // one whose `OverlayAnchor` is mounted — and it runs in the CAPTURE
  // phase, before any overlay kit sees the key: a just-closed menu's
  // document-level Escape consumer (bits-ui's escape-layer) unregisters
  // only asynchronously, and in the bubble phase its late `preventDefault`
  // let it swallow the window's own Esc — the sequence Esc-overlay,
  // Esc-within-a-few-ms Esc-window lost the window (DR-340). The behavior
  // contract is unchanged: with an overlay mounted the key belongs to the
  // overlay's own consumer (which runs after this listener), with none the
  // window closes. The navigation keys stay in the bubble listener: the
  // page field's typing and the tree rows' Arrow handling keep the
  // released order.
  // The EPUB map (§3.13, UI-39 semantics with page → flow): ArrowUp/Down
  // one notch, PgUp/PgDn one viewport, Home/End book start/end — and
  // nothing else: no page arrows, no zoom keys (the named exclusions).
  $effect(() => {
    const onEscape = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      if (feedback.overlayCount > 0) return;
      event.preventDefault();
      void win.close();
    };
    window.addEventListener("keydown", onEscape, true);
    return () => window.removeEventListener("keydown", onEscape, true);
  });
  function onKeydown(event: KeyboardEvent) {
    if (isTextTarget(event.target)) return;
    const reader = epubStore ?? pdfStore;
    if (!reader) return;
    if (event.key === "PageDown") {
      event.preventDefault();
      reader.scrollViewport(1);
    } else if (event.key === "PageUp") {
      event.preventDefault();
      reader.scrollViewport(-1);
    } else if (event.key === "Home") {
      event.preventDefault();
      reader.goToPage(1);
    } else if (event.key === "End") {
      event.preventDefault();
      // Book end (UI-39/§3.13): the last page (PDF) / the bottom of the
      // flow (EPUB — `scrollToEnd`, not the last chapter's start).
      if (epubStore !== null) {
        epubStore.scrollToEnd();
      } else {
        reader.goToPage(pdfStore?.doc?.page_count ?? 1);
      }
    } else if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
      // Page navigation (UI-39/DR-228): exactly the strip's prev/next — the
      // same `stepPage` path. With an overlay open the arrows belong to it
      // (the ToC panel's entry keys, every menu's own keys). The EPUB
      // surface has no page navigation (§3.13): the keys do nothing.
      if (epubStore !== null || feedback.overlayCount > 0) return;
      event.preventDefault();
      pdfStore?.stepPage(event.key === "ArrowRight" ? 1 : -1);
    } else if (event.key === "ArrowUp" || event.key === "ArrowDown") {
      // The wheel's keyboard counterpart (UI-39/DR-228): one notch. The
      // store routes the mode — stack scroll in continuous/facing, the
      // view's edge rule in single (DR-227); the EPUB store scrolls the
      // flow.
      if (feedback.overlayCount > 0) return;
      event.preventDefault();
      reader.scrollNotch(event.key === "ArrowDown" ? SCROLL_NOTCH : -SCROLL_NOTCH);
    } else if (
      event.ctrlKey &&
      !event.altKey &&
      !event.metaKey &&
      (event.key === "=" || event.key === "+" || event.key === "-")
    ) {
      // Zoom keys (UI-30/DR-221, AC-96): Ctrl + / Ctrl − walk the ladder —
      // the same `changeZoom` path the − / + buttons use. `=` is the
      // unshifted pair of `+` on every layout; the numpad sends it directly.
      // The EPUB surface has no zoom control (§3.13): the keys do nothing.
      if (epubStore !== null) return;
      event.preventDefault();
      pdfStore?.changeZoom(event.key === "-" ? -1 : 1);
    } else if (
      event.ctrlKey &&
      !event.altKey &&
      !event.metaKey &&
      (event.key === "0" || event.key === "1" || event.key === "2")
    ) {
      // Ctrl 0 / 1 / 2 = Fit width / Fit page / Actual Size (AC-96).
      if (epubStore !== null) return;
      event.preventDefault();
      pdfStore?.setZoomMode(
        event.key === "0" ? "fit-width" : event.key === "1" ? "fit-page" : "actual",
      );
    }
  }

  // The error state's action (AC-69): the external path, its own command —
  // the same call the library window's "Open in external viewer" makes, and
  // the action FR-23/AC-133 keeps for an unrenderable EPUB (AC-134).
  function openExternally(): void {
    void api.appOpenDocument(docId).catch((error: unknown) => {
      feedback.reportError(asAppError(error));
    });
  }

  // The unlock form of a protected document (FR-20, AC-104/AC-105; the
  // mechanism is DR-266 a): the typed password is always written through
  // `library_set_password` before the render retry — the service reads the
  // password only from the row. A successful unlock with *Remember this
  // password* checked keeps it; an unchecked one is temporary and this
  // window clears it again on its close paths (the close control, Esc and
  // the OS close all reach the same intercepted close request).
  const needsPassword = $derived(store?.error?.kind === "password-required");
  let passwordField = $state("");
  let remember = $state(false);
  let unlockError = $state<string | null>(null);
  let unlocking = $state(false);
  // True while this window has written a password the user did not ask to keep.
  let tempStored = false;

  // The payload carries the stored password for the prefill (AC-105: a
  // remembered password reaches the next open; a stored one that no longer
  // fits is what the prefill and *Remove password* are for).
  $effect(() => {
    if (store?.error?.kind === "password-required") {
      passwordField = store.error.password ?? "";
    }
  });

  // The prompt's field is the window's first interaction: focus it when the
  // form mounts (the action form of it — svelte-check flags the attribute).
  function focusField(node: HTMLInputElement): void {
    node.focus();
  }

  async function submitUnlock(): Promise<void> {
    if (unlocking || passwordField === "") return;
    unlocking = true;
    unlockError = null;
    try {
      await api.librarySetPassword(docId, passwordField);
      tempStored = !remember;
      await (epubStore ?? pdfStore)?.load();
      if (store?.error?.kind === "password-required") {
        // A wrong password keeps the form with an inline error (AC-104);
        // the window stays usable — the external action stands below.
        unlockError = t("reader.password.wrong");
      }
    } catch (error) {
      unlockError = asAppError(error).message;
    } finally {
      unlocking = false;
    }
  }

  async function removeStoredPassword(): Promise<void> {
    await api.librarySetPassword(docId, "");
    tempStored = false;
    passwordField = "";
    remember = false;
    unlockError = null;
    // Still protected: the prompt re-appears with an empty field (AC-105).
    await (epubStore ?? pdfStore)?.load();
  }

  let unlistenClose: (() => void) | undefined;
  // FR-28 (DR-406 3): the intercepted close carries the awaited position
  // flush; `closeFlushed` keeps the second (real) close request unhandled,
  // so the flush runs exactly once per window. A `pagehide` write covers an
  // abrupt exit, where no close request runs.
  let closeFlushed = false;
  onMount(() => {
    window.addEventListener("pagehide", () => {
      void (epubStore ?? pdfStore)?.flushPosition();
    });
    void getCurrentWindow()
      .onCloseRequested((event) => {
        if (closeFlushed) return;
        const reader = epubStore ?? pdfStore;
        if (!tempStored && !reader) return;
        event.preventDefault();
        closeFlushed = true;
        const flush = reader?.flushPosition() ?? Promise.resolve();
        const password = tempStored
          ? api.librarySetPassword(docId, "").catch(() => {
              /* the row keeps the temporary value; the next open presents it */
            })
          : Promise.resolve();
        void Promise.all([flush, password]).finally(() => {
          tempStored = false;
          void getCurrentWindow().close();
        });
      })
      .then((un) => {
        unlistenClose = un;
      })
      .catch(() => {
        /* no window handle to hold — the row keeps the temporary value */
      });
  });
  onDestroy(() => {
    unlistenClose?.();
  });

  // UI-07 MODIFIED (SPEC-v0.0.12 §3.7): on macOS the window-controls cluster
  // is the chrome strip's FIRST cell (the traffic-light corner; the existing
  // chrome padding is its inset) for both reader kinds; the reader settings
  // control (UI-50) keeps its released right end — its adjacency to the
  // controls is a Windows/Linux fact. The Windows/Linux mount is unchanged.
  const controlsLayout = windowControlsLayout(currentUiPlatform());
  // D35 repair (SPEC-v0.0.13 §3.6): on macOS the chrome strip renders no
  // drag-region attribute and `use:dragStrip` owns it; the Windows/Linux
  // mount keeps the released attribute byte-for-byte.
  const ownDragStrip = usesOwnDragStrip();
</script>

<svelte:window onkeydown={onKeydown} oncontextmenu={guardContextMenu} />

<!-- One Tooltip.Provider per window (the released §3.6 pattern): each window
     is its own app root, and the reader's control strip uses Tooltips. -->
<TooltipKit.Provider delayDuration={300}>
<div class="reader" class:pinned={dockedOpen}>
  <header class="chrome" data-tauri-drag-region={ownDragStrip ? undefined : "deep"} use:dragStrip>
    {#if controlsLayout.side === "left"}
      <WindowControls />
    {/if}
    <Icon name="book-open" />
    <Tooltip text={store?.documentName ?? ""}>
      {#snippet control({ props })}
        <span class="name" {...props}>{store?.documentName}</span>
      {/snippet}
    </Tooltip>
    <span class="chrome-spacer"></span>
    <!-- UI-50 as revised by DR-338 and DR-344: the main window's settings menu, complete
         (theme cycle, Settings…, Check for updates, About), in the chrome's
         right end immediately before the window controls — next to the minimize
         button, the user's placement — in every reader window, the PDF reader's
         and the EPUB reader's alike (reader surfaces are consistent, DR-344).
         The dialogs and the update check operate in this window through released
         components — the OverlayAnchor guard the menu registers with, this
         window's ToastStack, the plugin's app-level commands (the reader-windows
         capability carries updater:default, DR-344). -->
    <!-- UI-55 (FR-31, §3.8): the cover menu joins the strip's right-end
         cluster on every platform, left of the settings control — the
         release's one new control surface. It acts on the ready surface
         only: the page shown (PDF) or the book's candidates (EPUB). The
         window controls keep their ends (UI-07 MODIFIED moves only them). -->
    {#if pdfStore && pdfStore.status === "ready"}
      <CoverMenu docId={pdfStore.docId} kind="pdf" currentPage={() => pdfStore?.currentPage ?? 1} />
    {:else if epubStore && epubStore.status === "ready"}
      <CoverMenu docId={epubStore.docId} kind="epub" />
    {/if}
    <SettingsMenu />
    {#if controlsLayout.side === "right"}
      <WindowControls />
    {/if}
  </header>

  <!-- Exactly one surface per window (§3.5, AC-125): the kind the route
       carried decides it — the PDF window renders the released surface
       unchanged, the EPUB window the flow surface. The loading state renders
       nothing in both (the released rule). -->
  {#if pdfStore && pdfStore.status === "ready"}
    <!-- Pinned (UI-49/§3.8): the panel is layout — the left column of the
         page area, placed before the content column in the grid. -->
    {#if pdfStore.tocPinned && pdfStore.tocDockedOpen}
      <TocDocked store={pdfStore} />
    {/if}
    <ReaderView store={pdfStore} />
    <ReaderControls store={pdfStore} />
  {:else if epubStore && epubStore.status === "ready"}
    {#if epubStore.tocPinned && epubStore.tocDockedOpen}
      <!-- The same panel in the EPUB window (UI-48 MODIFIED, DR-327); the
           current-entry resolver is the store's own (§3.13), the same one
           the strip passes. -->
      <TocDocked store={epubStore} current={() => epubStore.currentTocIndex()} />
    {/if}
    <EpubReaderView store={epubStore} />
    <EpubReaderControls store={epubStore} />
  {:else if store?.status === "error"}
    {#if needsPassword}
      <!-- The unlock form (FR-20/AC-104): a `password-required` payload
           renders a form, not the error copy — the window stays usable, and
           a successful unlock replaces the form with the page stack in this
           same window (no second window, no app reload). -->
      <div class="unlock-state">
        <p class="error-title">{t("reader.password.title")}</p>
        <form class="unlock-form" onsubmit={(event) => { event.preventDefault(); void submitUnlock(); }}>
          <label class="password-label">
            {t("reader.password.field")}
            <input
              class="field"
              class:field-error={unlockError !== null}
              type="password"
              bind:value={passwordField}
              autocomplete="off"
              use:focusField
            />
          </label>
          {#if unlockError !== null}
            <p class="inline-error" role="alert">{unlockError}</p>
          {/if}
          <label class="remember-row">
            <input type="checkbox" bind:checked={remember} />
            {t("reader.password.remember")}
          </label>
          <div class="unlock-actions">
            <button type="submit" class="control control-primary" disabled={passwordField === "" || unlocking}>
              {t("reader.password.unlock")}
            </button>
            {#if store?.error?.password}
              <button type="button" class="control" onclick={() => void removeStoredPassword()}>
                {t("reader.password.remove")}
              </button>
            {/if}
          </div>
        </form>
        <button type="button" class="control" onclick={openExternally}>
          {t("row.openExternal")}
        </button>
      </div>
    {:else}
      <!-- The error state (UI-31/AC-69): the typed failure rendered in the
           window, with the external-open action FR-17 requires for an
           unrenderable file. -->
      <div class="error-state">
        <Icon name="alert-triangle" />
        <!-- The error state (UI-31/AC-69/AC-133): the typed failure rendered
             in the window, with the external-open action; the copy names the
             kind this window was opened for (§3.13). -->
        <p class="error-title">{epubStore ? t("reader.epub.error") : t("reader.error")}</p>
        <p class="error-detail">{store?.error?.message}</p>
        <button type="button" class="control control-primary" onclick={openExternally}>
          {t("row.openExternal")}
        </button>
      </div>
    {/if}
  {/if}
</div>
</TooltipKit.Provider>

<!-- Toasts only for the error state's action failing (the external open
     rejecting after the window was already open). The mechanism is the
     released one, not a reader variant. -->
<ToastStack />

<!-- UI-56 (SPEC-v0.0.13 §3.2): the update modal in the reader window too —
     the settings menu's manual check arms it here exactly as in the main
     window (the released reader-surface pattern, DR-344). -->
<UpdateModal />

<style>
  .reader {
    display: grid;
    /* DR-214: the column is clamped to the viewport. An implicit auto
       track takes the chrome's intrinsic min-content (the nowrap name +
       the control groups' minimums) as its minimum, which let the strip
       exceed the window instead of ellipsizing the name (DR-206). */
    grid-template-columns: minmax(0, 1fr);
    grid-template-rows: var(--toolbar-height) minmax(0, 1fr) auto;
    height: 100dvh;
    background: var(--surface-0);
    color: var(--text-primary);
    font-family: var(--font-ui);
    font-size: var(--text-13);
  }

  /* Pinned ToC panel (FR-24 / UI-49, SPEC-v0.0.10 §3.8): the page area
     becomes a two-column region — the panel column at the window's left edge
     (the fixed width recorded by DR-331), the content column beside it. The
     chrome and control strips keep spanning the whole window; the panel owns
     the middle row's first column, the view the second, so the content box
     shrinks in the DOM and the released fit-width and mount-window machinery
     measure the smaller box unchanged (the §3.8 "no pipeline change" rule). */
  .reader.pinned {
    grid-template-columns: var(--toc-panel-width) minmax(0, 1fr);
  }

  .reader.pinned .chrome {
    grid-column: 1 / -1;
  }

  .reader.pinned :global(.toc-docked) {
    grid-column: 1;
    grid-row: 2;
  }

  .reader.pinned :global(.reader-scroll),
  .reader.pinned :global(.reader-row),
  .reader.pinned :global(.reader-single),
  .reader.pinned :global(.flow) {
    grid-column: 2;
    grid-row: 2;
  }

  .reader.pinned :global(.reader-controls) {
    grid-column: 1 / -1;
    grid-row: 3;
  }

  .chrome {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 var(--content-gutter);
    border-bottom: 1px solid var(--border-hairline);
  }

  /* Name display (DR-206): no fixed character cap — the name keeps its
     content width while the strip has room (the spacer absorbs the free
     space) and shrinks with an ellipsis only on real overflow; the app
     tooltip carries the full name either way (UI-37/AC-87). */
  .name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-primary);
    font-size: var(--text-14);
  }

  .chrome-spacer {
    flex: 1;
  }

  .error-state {
    display: grid;
    place-content: center;
    justify-items: center;
    gap: 8px;
    color: var(--text-secondary);
  }

  .error-title {
    margin: 0;
    color: var(--text-primary);
    font-size: var(--text-16);
  }

  .error-detail {
    margin: 0 0 8px;
    font-size: var(--text-12);
  }

  /* The unlock form (FR-20): the released form rules — global `.field` /
     `.inline-error`, a 4 px rhythm, the primary action the same control
     grammar the error state already uses. */
  .unlock-state {
    display: grid;
    place-content: center;
    justify-items: center;
    gap: 16px;
    color: var(--text-secondary);
  }

  .unlock-form {
    display: grid;
    gap: 8px;
    width: 320px;
    max-width: calc(100vw - 2 * var(--content-gutter));
  }

  .unlock-form label {
    color: var(--text-secondary);
    font-size: var(--text-12);
  }

  .password-label {
    display: grid;
    gap: 4px;
  }

  .remember-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .unlock-actions {
    display: flex;
    gap: 8px;
    margin-top: 4px;
  }
</style>
