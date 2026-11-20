// UI-57 (SPEC-v0.0.13 §3.4, AC-190): the version identity is fetched once
// per window and never re-fetched; a failed command leaves the label absent.

import { describe, expect, it, vi } from "vitest";

const { appAbout } = vi.hoisted(() => ({
  appAbout: vi.fn(() => Promise.resolve({ name: "Book Keeper", version: "0.0.13" })),
}));

vi.mock("../../api", () => ({ appAbout }));

import { appinfo } from "./appinfo.svelte";

describe("appinfo store (SPEC-v0.0.13 §3.4)", () => {
  it("ensure() starts one fetch and repeats change nothing", async () => {
    appinfo.ensure();
    appinfo.ensure();
    expect(appAbout).toHaveBeenCalledTimes(1);
    await vi.waitFor(() => expect(appinfo.version).toBe("0.0.13"));
    appinfo.ensure();
    expect(appAbout).toHaveBeenCalledTimes(1); // never re-fetched
  });
});
