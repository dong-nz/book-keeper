// The app's own version identity (UI-57, SPEC-v0.0.13 §3.4): `app_about` is
// fetched once per window at startup and never re-fetched; the status-bar
// label and the update modal render from this live state. A failed command
// leaves the label absent — UI-57 renders exactly what the app itself
// reports, nothing else. The store is per-window like every other window
// state; both window roots call `ensure()` in their init sequence.

import * as api from "../../api";

class AppInfoStore {
  version = $state<string | null>(null);
  #started = false;

  /** Start the single fetch. Idempotent: the second call does nothing. */
  ensure(): void {
    if (this.#started) return;
    this.#started = true;
    api
      .appAbout()
      .then((about) => {
        this.version = about.version;
      })
      .catch(() => {
        /* no label rather than a wrong label (UI-57) */
      });
  }
}

export const appinfo = new AppInfoStore();
