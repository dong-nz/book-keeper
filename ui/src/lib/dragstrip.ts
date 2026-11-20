// The chrome strips' drag handling on the macOS mount (D35 repair,
// SPEC-v0.0.13 §3.6). Tauri's built-in `drag.js` is driven by the
// `data-tauri-drag-region` attribute; its macOS branch maximizes only when
// the second press and its release land on exactly the same coordinates —
// real pointing devices drift, so the built-in path almost never fires on
// macOS (the ledger's root cause). On macOS the mounts render no attribute
// and this action owns the strip: the first press starts the native drag
// (the same call the built-in makes), the second press of a double-click
// toggles maximize. On Windows/Linux the action is inert and the attribute
// with the built-in behavior is the released path byte-for-byte.

import { getCurrentWindow } from "@tauri-apps/api/window";
import { currentUiPlatform } from "./platform";

/** Whether the mount must render its own drag handling instead of the
 *  attribute (macOS only, §3.6). */
export function usesOwnDragStrip(): boolean {
  return currentUiPlatform() === "macos";
}

/** The press decision, pure and testable (§3.10): the mouse-event facts in,
 *  the window calls out. Only left-button presses act; a double-click's
 *  second press maximizes, its first press drags — the released Windows
 *  sequence restated for the platform that never reaches it. */
export function handleStripPress(
  detail: number,
  button: number,
  win: { startDragging: () => Promise<void>; toggleMaximize: () => Promise<void> },
  platform: "macos" | "other",
): void {
  if (platform !== "macos") return;
  if (button !== 0) return;
  if (detail === 2) void win.toggleMaximize();
  else if (detail === 1) void win.startDragging();
}

/** The mount action. Inert on every platform but macOS (§3.6). */
export function dragStrip(node: HTMLElement) {
  if (!usesOwnDragStrip()) return;
  const onMousedown = (event: MouseEvent) => {
    // Controls inside the strip keep their released "false" overrides — the
    // action honours them exactly as the built-in's deep mode did.
    if ((event.target as HTMLElement).closest('[data-tauri-drag-region="false"]')) return;
    handleStripPress(event.detail, event.button, getCurrentWindow(), currentUiPlatform());
  };
  node.addEventListener("mousedown", onMousedown);
  return { destroy: () => node.removeEventListener("mousedown", onMousedown) };
}
