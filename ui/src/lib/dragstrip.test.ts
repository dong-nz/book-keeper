// D35 repair (SPEC-v0.0.13 §3.6): the macOS strip's press decision —
// press 1 drags, press 2 of a double-click maximizes, nothing else acts;
// every other platform defers to the built-in attribute path. The mount
// wiring (`dragStrip`) is exercised on the real app (Phase B); this is the
// decision the §3.10 units name.

import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: vi.fn() }));

import { handleStripPress } from "./dragstrip";

function fakeWin() {
  return {
    startDragging: vi.fn(() => Promise.resolve()),
    toggleMaximize: vi.fn(() => Promise.resolve()),
  };
}

describe("handleStripPress (SPEC-v0.0.13 §3.6)", () => {
  it("Windows/Linux: nothing — the released attribute path owns the strip", () => {
    const win = fakeWin();
    handleStripPress(1, 0, win, "other");
    handleStripPress(2, 0, win, "other");
    expect(win.startDragging).not.toHaveBeenCalled();
    expect(win.toggleMaximize).not.toHaveBeenCalled();
  });

  it("macOS: the first press drags, the second press of a double-click maximizes", () => {
    const win = fakeWin();
    handleStripPress(1, 0, win, "macos");
    expect(win.startDragging).toHaveBeenCalledTimes(1);
    handleStripPress(2, 0, win, "macos");
    expect(win.toggleMaximize).toHaveBeenCalledTimes(1);
    expect(win.startDragging).toHaveBeenCalledTimes(1); // the second press does not drag again
  });

  it("only the left button acts", () => {
    const win = fakeWin();
    handleStripPress(1, 2, win, "macos"); // right click
    handleStripPress(2, 1, win, "macos"); // middle click double
    expect(win.startDragging).not.toHaveBeenCalled();
    expect(win.toggleMaximize).not.toHaveBeenCalled();
  });
});
