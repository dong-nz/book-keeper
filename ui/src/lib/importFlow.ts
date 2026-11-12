// Import entry points shared by the search field's import control (UI-14), the
// empty-state action and the Ctrl+I shortcut, native dialogs only (DR-44); all
// import side effects live in the library store.
// UI-46/AC-126 (SPEC-v0.0.9 §3.4): the picker filter accepts both managed
// types together; the filter's display name stays type-neutral like every
// other import label (UI-44: no control names one type).
import { open } from "@tauri-apps/plugin-dialog";
import { library } from "$lib/state/library.svelte";

const DOCUMENT_FILTERS = [{ name: "Documents", extensions: ["pdf", "epub"] }];

export async function pickFilesAndImport(): Promise<void> {
  const selected = await open({ multiple: true, filters: DOCUMENT_FILTERS });
  if (selected === null) return;
  await library.importPaths(Array.isArray(selected) ? selected : [selected]);
}

export async function pickFolderAndImport(): Promise<void> {
  const dir = await open({ directory: true });
  if (typeof dir !== "string") return;
  await library.importFolder(dir);
}
