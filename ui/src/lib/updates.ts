// Update discovery and flow (FR-25 / UI-52, SPEC-v0.0.10 §3.11, DR-332). The
// plugin's typed client is the command surface — the pattern the opener plugin
// established (UI-40): the fetch, version comparison, signature verification,
// download and installer launch all run in the plugin's Rust side. This module
// only surfaces the results, and only through released affordances: the
// availability toast with its Install action, the up-to-date and failure
// toasts (UI-16 rules), and the job-progress bar for the download (UI-12).
// No business logic lives here; none is allowed to (R1.2's shape holds on the
// frontend side too).

import { check, type Update } from "@tauri-apps/plugin-updater";
import { feedback } from "$lib/state/feedback.svelte";
import { t } from "./i18n";

let checking = false;
let armed: { version: string; toastId: number; update: Update } | null = null;

/** Run the update check. `manual` is the Settings-menu path, which reports
 *  every outcome (FR-25); the startup call is silent about its own failure
 *  and about a current app (AC-153) but arms the availability affordance
 *  when an update is found (UI-52). */
export async function checkForUpdates(manual: boolean): Promise<void> {
  if (checking) return; // the startup call and the menu item share one path
  checking = true;
  try {
    const update = await check();
    if (update) armAvailability(update);
    else if (manual) feedback.notify("updates.uptodate");
  } catch {
    // The plugin's own error text is technical transport language; the typed
    // surface is the app's message (AC-151). The raw text is not shown.
    if (manual) feedback.reportError({ kind: "internal", message: t("updates.checkFailed"), document_id: null });
  } finally {
    checking = false;
  }
}

function armAvailability(update: Update): void {
  // The same update already on screen is not re-pushed; after its dismissal
  // (or when a newer version arrives) the affordance re-arms, and the
  // replaced `Update` resource is closed — the client's own contract.
  if (armed && armed.version === update.version && feedback.toasts.some((toast) => toast.id === armed!.toastId)) {
    return;
  }
  if (armed) void armed.update.close();
  const notes = update.body?.trim();
  const toastId = feedback.notifyWithAction(
    notes ? "updates.availableNotes" : "updates.available",
    notes ? { version: update.version, notes } : { version: update.version },
    { labelKey: "updates.install", run: () => void installUpdate(update) },
  );
  armed = { version: update.version, toastId, update };
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
        if (known) feedback.jobProgress({ kind: "update", processed: 0, total });
      } else if (event.event === "Progress") {
        received += event.data.chunkLength;
        if (known) feedback.jobProgress({ kind: "update", processed: Math.min(received, total), total });
      }
    });
    // On success the plugin has launched the platform installer and the app
    // exits (updater.rs `std::process::exit(0)`, DR-329 item 4) — there is
    // deliberately no completion or exit handling here.
  } catch {
    feedback.jobProgress({ kind: "update", processed: 1, total: 1 }); // clears the bar
    feedback.reportError({ kind: "internal", message: t("updates.installFailed"), document_id: null });
    armed = null;
    void update.close();
  }
}
