// Platform detection and the window-controls layout decision (UI-07 MODIFIED,
// SPEC-v0.0.12 §3.7). The app draws its own title bar on every platform
// (UI-07 released); on macOS the cluster moves to the traffic-light corner,
// close first. The decision is a pure function the mounts consume — the
// Windows/Linux answer is the released shape byte-for-byte. No new
// dependency, no OS API: the webview's own user-agent string is the input,
// evaluated once per window at startup and memoized.

/** The two shapes the app knows. Everything not macOS keeps the released
 *  right-end layout — Linux included (the user's decision: right-end is the
 *  Linux default; per-user DE button layouts are not observable by the app). */
export type UiPlatform = "macos" | "other";

/** UA-based detection (§3.7): the macOS marks in any standard UA string.
 *  Exported pure with the UA injected — the §3.10 units drive it directly. */
export function uiPlatform(userAgent: string): UiPlatform {
  return /Macintosh|Mac OS X/.test(userAgent) ? "macos" : "other";
}

let cachedPlatform: UiPlatform | undefined;

/** The window's own platform, evaluated once at app start (§3.7). */
export function currentUiPlatform(): UiPlatform {
  cachedPlatform ??= uiPlatform(navigator.userAgent);
  return cachedPlatform;
}

/** One of the three app-drawn controls (the same set the capabilities file
 *  grants; the commands and geometry are unchanged — only the order). */
export type WindowControlAction = "close" | "minimize" | "maximize";

export interface WindowControlsLayout {
  side: "left" | "right";
  order: WindowControlAction[];
}

/** The single layout source both mounts consume (§3.7): macOS → the cluster
 *  at the left end, close–minimize–maximize (the traffic-light order);
 *  Windows/Linux → the released right end, minimize–maximize–close. */
export function windowControlsLayout(platform: UiPlatform): WindowControlsLayout {
  return platform === "macos"
    ? { side: "left", order: ["close", "minimize", "maximize"] }
    : { side: "right", order: ["minimize", "maximize", "close"] };
}
