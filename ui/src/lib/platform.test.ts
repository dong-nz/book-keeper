// UI-07 MODIFIED (SPEC-v0.0.12 §3.7/§3.10): the platform detection and the
// window-controls layout decision, unit-tested for all three platform answers.
// These units are the Windows-shape guarantee: the harness platform resolves
// "other" and the released right-end layout must come out byte-for-byte.

import { describe, expect, it } from "vitest";

import { uiPlatform, windowControlsLayout } from "./platform";

const MAC_UA =
  "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/129.0.0.0 Safari/537.36";
const MAC_SAFARI_UA =
  "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.4 Safari/605.1.15";
const WINDOWS_UA =
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/129.0.0.0 Safari/537.36";
const LINUX_UA =
  "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/129.0.0.0 Safari/537.36";

describe("platform detection (UI-07 MODIFIED, §3.7)", () => {
  it("recognizes the macOS user-agent strings", () => {
    expect(uiPlatform(MAC_UA)).toBe("macos");
    expect(uiPlatform(MAC_SAFARI_UA)).toBe("macos");
  });

  it("every other platform keeps the released shape", () => {
    expect(uiPlatform(WINDOWS_UA)).toBe("other");
    expect(uiPlatform(LINUX_UA)).toBe("other");
    expect(uiPlatform("")).toBe("other");
  });
});

describe("window-controls layout (UI-07 MODIFIED, the single source)", () => {
  it("macOS: the cluster at the left end, close–minimize–maximize", () => {
    const layout = windowControlsLayout(uiPlatform(MAC_UA));
    expect(layout.side).toBe("left");
    expect(layout.order).toEqual(["close", "minimize", "maximize"]);
  });

  it("Windows: the released right end, minimize–maximize–close — unchanged", () => {
    const layout = windowControlsLayout(uiPlatform(WINDOWS_UA));
    expect(layout.side).toBe("right");
    expect(layout.order).toEqual(["minimize", "maximize", "close"]);
  });

  it("Linux: the released right end likewise (the user's decision: right-end is the Linux default)", () => {
    const layout = windowControlsLayout(uiPlatform(LINUX_UA));
    expect(layout.side).toBe("right");
    expect(layout.order).toEqual(["minimize", "maximize", "close"]);
  });

  it("the order is always the same three controls, in some order", () => {
    for (const platform of ["macos", "other"] as const) {
      const order = windowControlsLayout(platform).order;
      expect([...order].sort()).toEqual(["close", "maximize", "minimize"]);
    }
  });
});
