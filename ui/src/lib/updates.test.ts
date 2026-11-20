// FR-25 / UI-56 (SPEC-v0.0.13 §3.2–§3.3, AC-184…AC-188): the update flow
// surfaces a found update as the modal offer, honours the version-scoped
// Skip at startup, re-presents on a manual check, and relaunches after a
// completed install. The plugin client, the feedback surface and the settings
// store are mocked — the flow's own decisions are what this file tests.

import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  check: vi.fn<() => Promise<unknown>>(() => Promise.resolve(null)),
  relaunch: vi.fn(() => Promise.resolve()),
  notify: vi.fn(),
  reportError: vi.fn(),
  jobProgress: vi.fn(),
  settings: {
    updateSkippedVersion: "",
    skipUpdateVersion: vi.fn((version: string) => {
      mocks.settings.updateSkippedVersion = version;
    }),
  },
}));

vi.mock("@tauri-apps/plugin-updater", () => ({ check: mocks.check }));
vi.mock("@tauri-apps/plugin-process", () => ({ relaunch: mocks.relaunch }));
vi.mock("./state/feedback.svelte", () => ({
  feedback: {
    notify: mocks.notify,
    reportError: mocks.reportError,
    jobProgress: mocks.jobProgress,
  },
}));
// The settings mirror only as the Skip's reader/writer; its own behavior is
// tested in `state/settings.test.ts`.
vi.mock("./state/settings.svelte", () => ({ settings: mocks.settings }));

function fakeUpdate(version: string, body?: string) {
  return {
    version,
    body,
    close: vi.fn(() => Promise.resolve()),
    downloadAndInstall: vi.fn(() => Promise.resolve()),
  };
}

/** Each test runs against a fresh module instance: `checking` and `armed`
 *  are module state, and the offer store is per-window state — a test must
 *  not inherit another test's armed update. The hoisted mocks persist. */
async function flow() {
  vi.resetModules();
  mocks.check.mockReset().mockResolvedValue(null);
  mocks.relaunch.mockReset();
  mocks.notify.mockReset();
  mocks.reportError.mockReset();
  mocks.jobProgress.mockReset();
  mocks.settings.skipUpdateVersion.mockClear(); // keeps its base implementation
  const updates = await import("./updates");
  const { updateOffer } = await import("./state/updateOffer.svelte");
  return { updates, updateOffer };
}

describe("update flow (SPEC-v0.0.13 §3.2, FR-25)", () => {
  beforeEach(() => {
    mocks.settings.updateSkippedVersion = "";
  });

  it("a startup find arms the modal offer with the version and the notes", async () => {
    const { updates, updateOffer } = await flow();
    const update = fakeUpdate("0.9.9", "  Fixed the reader.  ");
    mocks.check.mockResolvedValue(update);
    await updates.checkForUpdates(false);
    expect(updateOffer.offer).toEqual({ newVersion: "0.9.9", notes: "Fixed the reader." });
  });

  it("a startup offer equal to the skipped version asks nothing", async () => {
    const { updates, updateOffer } = await flow();
    mocks.settings.updateSkippedVersion = "0.9.9";
    const update = fakeUpdate("0.9.9");
    mocks.check.mockResolvedValue(update);
    await updates.checkForUpdates(false); // AC-184: silent, and the resource closes
    expect(updateOffer.offer).toBeNull();
    expect(update.close).toHaveBeenCalledTimes(1);
  });

  it("a manual check re-presents the skipped version (AC-187)", async () => {
    const { updates, updateOffer } = await flow();
    mocks.settings.updateSkippedVersion = "0.9.9";
    mocks.check.mockResolvedValue(fakeUpdate("0.9.9"));
    await updates.checkForUpdates(true);
    expect(updateOffer.offer?.newVersion).toBe("0.9.9");
  });

  it("the Skip action persists the version and closes the offer", async () => {
    const { updates, updateOffer } = await flow();
    const update = fakeUpdate("0.9.9");
    mocks.check.mockResolvedValue(update);
    await updates.checkForUpdates(true);
    updates.skipOfferedUpdate();
    expect(mocks.settings.skipUpdateVersion).toHaveBeenCalledWith("0.9.9");
    expect(updateOffer.offer).toBeNull();
    expect(update.close).toHaveBeenCalledTimes(1);
  });

  it("the same update already on screen is not armed twice", async () => {
    const { updates, updateOffer } = await flow();
    const first = fakeUpdate("0.9.9");
    mocks.check.mockResolvedValue(first);
    await updates.checkForUpdates(true);
    const second = fakeUpdate("0.9.9");
    mocks.check.mockResolvedValue(second);
    await updates.checkForUpdates(true);
    expect(second.close).toHaveBeenCalledTimes(1);
    expect(first.close).not.toHaveBeenCalled();
    expect(updateOffer.offer?.newVersion).toBe("0.9.9");
  });

  it("Install closes the dialog, then the completion surfaces and relaunches (AC-189)", async () => {
    vi.useFakeTimers();
    try {
      const { updates, updateOffer } = await flow();
      const update = fakeUpdate("0.9.9");
      mocks.check.mockResolvedValue(update);
      await updates.checkForUpdates(true);
      updates.installOfferedUpdate();
      expect(updateOffer.offer).toBeNull(); // the job bar is the surface while it runs
      await vi.advanceTimersByTimeAsync(1500);
      expect(mocks.jobProgress).toHaveBeenCalledWith({ kind: "update", processed: 1, total: 1 });
      expect(mocks.notify).toHaveBeenCalledWith("updates.installed");
      expect(mocks.relaunch).toHaveBeenCalledTimes(1);
    } finally {
      vi.useRealTimers();
    }
  });

  it("a failed manual check surfaces the app's message, not the plugin's (AC-151)", async () => {
    const { updates } = await flow();
    mocks.check.mockRejectedValue(new Error("plugin transport detail"));
    await updates.checkForUpdates(true);
    expect(mocks.reportError).toHaveBeenCalledTimes(1);
    expect(mocks.reportError.mock.calls[0][0]).toMatchObject({
      kind: "internal",
      message: "Couldn't check for updates.",
    });
  });

  it("a manual check with nothing new says up to date; a startup check says nothing", async () => {
    const { updates } = await flow();
    await updates.checkForUpdates(false);
    expect(mocks.notify).not.toHaveBeenCalled();
    await updates.checkForUpdates(true);
    expect(mocks.notify).toHaveBeenCalledWith("updates.uptodate");
  });
});
