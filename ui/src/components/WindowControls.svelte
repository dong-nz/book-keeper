<script lang="ts">
  // Window controls (UI-07): the OS title bar is removed (`decorations: false`),
  // so minimize / maximize-restore / close are the app's own buttons at the
  // toolbar's right end. The capabilities file grants these three window
  // commands plus `start-dragging`, which Tauri's own drag-region script calls
  // on the strip (R1.5, spec §3.1).
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import Icon from "./Icon.svelte";
  import Tooltip from "./Tooltip.svelte";
  import { currentUiPlatform, windowControlsLayout } from "$lib/platform";
  import { t } from "$lib/i18n";

  const win = getCurrentWindow();
  let maximized = $state(false);

  // UI-07 MODIFIED (SPEC-v0.0.12 §3.7): the three buttons are unchanged —
  // same commands, same tooltips/aria labels, same 44 px geometry (DR-315) —
  // but their ORDER comes from the layout decision: macOS renders
  // close–minimize–maximize (the traffic-light order), Windows/Linux the
  // released minimize–maximize–close. The mount side (left/right) is the
  // hosts' concern (`windowControlsLayout().side`).
  const layout = windowControlsLayout(currentUiPlatform());
  const controls = $derived(
    layout.order.map((action) =>
      action === "minimize"
        ? {
            action,
            tooltip: t("window.minimize"),
            label: t("window.minimize"),
            icon: "minus" as const,
            extraClass: "",
            run: () => void win.minimize(),
          }
        : action === "maximize"
          ? {
              action,
              tooltip: maximized ? t("window.restore") : t("window.maximize"),
              label: maximized ? t("window.restore") : t("window.maximize"),
              icon: maximized ? ("restore" as const) : ("maximize" as const),
              extraClass: "",
              run: () => void win.toggleMaximize(),
            }
          : {
              action,
              tooltip: t("window.close"),
              label: t("window.close"),
              icon: "close" as const,
              extraClass: " window-close",
              run: () => void win.close(),
            },
    ),
  );

  // The maximize button mirrors the window state: while maximized it is a
  // Restore button (spec §3.1). `isMaximized` is part of the granted
  // `core:window:default` set; the resize listener only re-reads it.
  $effect(() => {
    let active = true;
    void win
      .isMaximized()
      .then((value) => {
        if (active) maximized = value;
      })
      .catch(() => {});
    const unlisten = win.onResized(async () => {
      try {
        const value = await win.isMaximized();
        if (active) maximized = value;
      } catch {
        /* the glyph keeps its last known state */
      }
    });
    return () => {
      active = false;
      void unlisten.then((fn) => fn());
    };
  });
</script>

{#each controls as item (item.action)}
  <Tooltip text={item.tooltip}>
    {#snippet control({ props })}
      <button
        {...props}
        class="window-button{item.extraClass}"
        data-maximized={item.action === "maximize" && maximized ? "" : undefined}
        aria-label={item.label}
        data-tauri-drag-region="false"
        onclick={item.run}
      >
        <Icon name={item.icon} />
      </button>
    {/snippet}
  </Tooltip>
{/each}

<style>
  .window-button {
    display: grid;
    place-items: center;
    width: 44px;
    /* DR-315: the controls never share the flex shrink with a long title —
       the title (min-width: 0 + ellipsis) absorbs all of it. */
    flex-shrink: 0;
    height: var(--toolbar-height);
    color: var(--text-secondary);
    border-radius: 0;
  }

  .window-button:hover {
    background: var(--surface-2);
    color: var(--text-primary);
  }

  .window-close:hover {
    background: var(--danger);
    color: var(--text-on-accent);
  }
</style>
