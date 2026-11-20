// Update discovery and flow (FR-25 / UI-52 as MODIFIED by SPEC-v0.0.13
// §3.2–§3.3, DR-422). The plugin's typed client stays the command surface
// (the DR-332 pattern): fetch, version comparison, signature verification,
// download and the installer launch all run in the plugin's Rust side.
// This module surfaces the result: a found update arms the update modal
// (UI-56) — the availability toast is retired; a startup arm honours the
// version-scoped Skip (FR-25, §3.2); a completed install on macOS/Linux
// surfaces the completion and relaunches the app (§3.3, OQ-75 closed A).
// The up-to-date and failure toasts and the job progress bar are unchanged
// (UI-16 / UI-12). No business logic lives here; none is allowed to (R1.2's
// shape holds on the frontend side too).

import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { feedback } from "$lib/state/feedback.svelte";
import { settings } from "$lib/state/settings.svelte";
import { updateOffer } from "$lib/state/updateOffer.svelte";
import { t } from "./i18n";

let checking = false;
let armed: { version: string; update: Update } | null = null;

export async function checkForUpdates(manual: boolean): Promise<void> {
  if (checking) return; // the startup call and the menu item share one path
  checking = true;
  try {
    const update = await check();
    if (update) armOffer(update, manual);
    else if (manual) feedback.notify("updates.uptodate");
  } catch {
    // The plugin's own error text is technical transport language; the typed
    // surface is the app's message (AC-151). The raw text is not surfaced.
    if (manual)
      feedback.reportError({
        kind: "internal",
        message: t("updates.checkFailed"),
        document_id: null,
      });
  } finally {
    checking = false;
  }
}

function armOffer(update: Update, manual: boolean): void {
  // A startup arm honours the Skip (AC-184/AC-186): an update equal to the
  // skipped version asks nothing at startup; a manual check always re-presents
  // (FR-25, AC-187). Plain equality is enough — the plugin only ever reports
  // strictly newer versions (spec fact F13).
  if (!manual && update.version === settings.updateSkippedVersion) {
    void update.close();
    return;
  }
  // The same update already on screen is not re-armed; a superseded or
  // re-presented resource is closed — the client's own contract (DR-332).
  if (armed && armed.version === update.version && updateOffer.offer) {
    void update.close();
    return;
  }
  if (armed) void armed.update.close();
  armed = { version: update.version, update };
  updateOffer.arm(update.version, update.body?.trim() ?? "");
}

/** The modal's Install action: the released download-and-install flow,
 *  progress through the job bar (UI-12). The dialog closes on the click —
 *  the progress bar is the surface while the install runs (§3.3). */
export function installOfferedUpdate(): void {
  if (!armed) return;
  updateOffer.clear();
  void installUpdate(armed.update);
}

async function installUpdate(update: Update): Promise<void> {
  let total = 0;
  let received = 0;
  let known = false; // no contentLength → nothing determinate to render (DR-332 item 4)
  try {
    await update.downloadAndInstall((event) => {
      if (event.event === "Started") {
        total = event.data.contentLength ?? 0;
        known = total > 0;
        received = 0;
        if (known) feedback.jobProgress({ kind: "update", processed: 0, total });
      } else if (event.event === "Progress") {
        received += event.data.chunkLength;
        if (known) feedback.jobProgress({ kind: "update", processed: Math.min(received, total), total });
      }
    });
    // The completion path (SPEC-v0.0.13 §3.3, OQ-75 closed A): reached on
    // macOS/Linux only — Windows exits inside the plugin's installer launch
    // (spec fact F2). Clear the bar, surface the completion, then after a
    // short visible beat relaunch so the new version runs (FR-25, AC-189).
    feedback.jobProgress({ kind: "update", processed: 1, total: 1 }); // clears the bar
    updateOffer.clear();
    armed = null;
    void update.close();
    feedback.notify("updates.installed");
    await new Promise((resolve) => setTimeout(resolve, 1500));
    await relaunch();
  } catch {
    feedback.jobProgress({ kind: "update", processed: 1, total: 1 }); // clears the bar
    feedback.reportError({
      kind: "internal",
      message: t("updates.installFailed"),
      document_id: null,
    });
    armed = null;
    updateOffer.clear();
    void update.close();
  }
}

/** The modal's *Skip this version* action (FR-25, §3.2): persist the skip
 *  through the settings store (written immediately — the skip must survive a
 *  restart made right after it, AC-186) and close the offer. */
export function skipOfferedUpdate(): void {
  if (!armed) return;
  settings.skipUpdateVersion(armed.version);
  updateOffer.clear();
  void armed.update.close();
  armed = null;
}
