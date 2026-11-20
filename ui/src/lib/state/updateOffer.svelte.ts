// The update offer (UI-56, SPEC-v0.0.13 §3.2): window-local state armed by
// `updates.ts` when a check finds an update; `UpdateModal.svelte` renders
// from it in both window roots (the store is per-window like every other
// window state — a check in one window arms only that window). Clearing it
// is the session deferral: nothing is persisted; only the modal's
// *Skip this version* action writes, through the settings store.

class UpdateOfferStore {
  offer = $state<{ newVersion: string; notes: string } | null>(null);

  arm(newVersion: string, notes: string): void {
    this.offer = { newVersion, notes };
  }

  clear(): void {
    this.offer = null;
  }
}

export const updateOffer = new UpdateOfferStore();
