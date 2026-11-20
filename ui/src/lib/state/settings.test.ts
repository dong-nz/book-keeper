// AC-09 (settings round-trip): a query change must store every setting it
// changes. One change touches four keys in the same tick, and `debounce` is
// trailing-edge by design (it keeps the arguments of the last call), so the
// store has to batch the pending keys rather than debounce one call per key —
// otherwise three of the four settings are silently dropped.

import { beforeEach, afterEach, describe, expect, it, vi } from "vitest";
import type { FilterState } from "../types";

const { appSetSetting, appGetSettings, logLevelSet } = vi.hoisted(() => ({
  appSetSetting: vi.fn(() => Promise.resolve()),
  appGetSettings: vi.fn(() => Promise.resolve({})),
  logLevelSet: vi.fn(() => Promise.resolve()),
}));

vi.mock("../../api", () => ({ appSetSetting, appGetSettings, logLevelSet }));

import { settings, decodeFilter } from "./settings.svelte";

const filter: FilterState = {
  q: "doc",
  tag_ids: [3, 1],
  favorites_only: false,
  missing_only: false,
  reading_only: false,
  sort: "size",
  dir: "desc",
  sort_secondary: "title",
};

/** What the store actually asked the backend to store, as `key=value` facts. */
function written(): string[] {
  return appSetSetting.mock.calls.map(([key, value]) => `${key}=${value}`).sort();
}

describe("settings store", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    appSetSetting.mockClear();
    logLevelSet.mockClear();
  });
  afterEach(() => vi.useRealTimers());

  it("stores every key a query change modifies", () => {
    settings.rememberQuery(filter);
    expect(appSetSetting).not.toHaveBeenCalled(); // writes are debounced (R4.3)

    vi.advanceTimersByTime(300);
    expect(written()).toEqual(
      ["sort=size", "dir=desc", "sort_secondary=title", "last_filter=all&tags=3,1"].sort(),
    );
  });

  it("coalesces a burst into one write per key, holding the last value", () => {
    settings.rememberQuery({ ...filter, sort: "title", dir: "asc" });
    settings.rememberQuery({ ...filter, sort: "date_added", favorites_only: true });

    vi.advanceTimersByTime(300);
    const writes = written();
    const keys = writes.map((w) => w.split("=")[0]);
    expect(new Set(keys).size).toBe(keys.length); // no key written twice
    expect(writes).toEqual(
      [
        "sort=date_added",
        "dir=desc",
        "sort_secondary=title",
        "last_filter=favorites&tags=3,1",
      ].sort(),
    );
  });

  it("keeps a view change and a column override written together", () => {
    settings.setView("grid");
    settings.setGridColumns(4);

    vi.advanceTimersByTime(300);
    expect(written()).toEqual(["view=grid", "grid_columns=4"].sort());
  });

  // AC-113 (UI-42 + FR-19): the grammar's write form is `mode&tags=…` with
  // the fourth mode; the two legacy read forms survive unchanged.
  it("writes the reading mode with the current tags token", () => {
    settings.rememberQuery({ ...filter, favorites_only: false, reading_only: true });
    vi.advanceTimersByTime(300);
    expect(written()).toContain("last_filter=reading&tags=3,1");
  });

  // UI-53 (FR-26, AC-156): the Log section's selector rides the dedicated
  // `log_level_set` command — immediate, outside the debounced batch — so
  // the level takes effect without a restart; the settings DTO's key is the
  // restore path and a corrupt row reads as the released default.
  it("writes the log level immediately, outside the settings batch", () => {
    settings.setLogLevel("info");
    expect(settings.logLevel).toBe("info");
    expect(logLevelSet).toHaveBeenCalledTimes(1);
    expect(logLevelSet).toHaveBeenCalledWith("info");
    expect(appSetSetting).not.toHaveBeenCalled();
    vi.advanceTimersByTime(300);
    expect(written()).not.toContain("log_level=info");
  });

  it("restores the stored log level and reads a corrupt row as Error", async () => {
    appGetSettings.mockResolvedValueOnce({ log_level: "info" });
    await settings.load();
    expect(settings.logLevel).toBe("info");

    appGetSettings.mockResolvedValueOnce({ log_level: "verbose" });
    await settings.load();
    expect(settings.logLevel).toBe("error");

    appGetSettings.mockResolvedValueOnce({});
    await settings.load();
    expect(settings.logLevel).toBe("error");
  });

  it("restores the skipped update version and reads corrupt rows as nothing skipped", async () => {
    appGetSettings.mockResolvedValueOnce({ update_skipped_version: "0.9.9" });
    await settings.load();
    expect(settings.updateSkippedVersion).toBe("0.9.9");

    appGetSettings.mockResolvedValueOnce({ update_skipped_version: "0.9.9-beta; keep" });
    await settings.load();
    expect(settings.updateSkippedVersion).toBe("");

    appGetSettings.mockResolvedValueOnce({});
    await settings.load();
    expect(settings.updateSkippedVersion).toBe("");
  });

  it("writes the skipped version immediately, outside the settings batch", () => {
    // AC-186 (SPEC-v0.0.13 §3.2): a restart made right after the Skip has
    // to read it — there is no debounce window to wait for.
    settings.skipUpdateVersion("0.9.9");
    expect(appSetSetting).toHaveBeenCalledWith("update_skipped_version", "0.9.9");
    expect(settings.updateSkippedVersion).toBe("0.9.9");
  });
});

describe("last_filter decoding (AC-113)", () => {
  it("decodes the current form with all four modes", () => {
    expect(decodeFilter("all")).toEqual({ mode: "all", tagIds: [] });
    expect(decodeFilter("reading")).toEqual({ mode: "reading", tagIds: [] });
    expect(decodeFilter("reading&tags=7,2,2")).toEqual({ mode: "reading", tagIds: [2, 7] });
    expect(decodeFilter("favorites&tags=1")).toEqual({ mode: "favorites", tagIds: [1] });
  });

  it("decodes a v0.0.7 value and a v0.0.1 value unchanged", () => {
    expect(decodeFilter("favorites&cats=3,1")).toEqual({ mode: "favorites", tagIds: [1, 3] });
    expect(decodeFilter("missing&cats=5")).toEqual({ mode: "missing", tagIds: [5] });
    expect(decodeFilter("cat:4")).toEqual({ mode: "all", tagIds: [4] });
  });
});
