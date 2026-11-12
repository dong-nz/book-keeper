// DR-211 (AC-95): library-facing strings speak of documents; strings about
// the physical file keep "file". The audit's two sets are asserted exactly
// — the document-phrased values as changed, the physical-file values as
// unchanged — plus the invariant that no other string names a library entry
// after a file: every catalog value containing the word "file" must be one
// of the audited keys (the physical set, or the two possessive rows that
// speak of a document's own file).
// The physical set as pinned here is the v0.0.9 state: UI-44 MODIFIED
// (OQ-55 closed → A) and DR-304 returned the import labels, the delete
// copy, the drop title, the empty-state hint and the relink refusal to
// type-neutral wording — the audit's rule stands unchanged; the pinned
// values follow the approved copy.

import { describe, expect, it } from "vitest";
import en from "./en";

// Values the audit re-phrased to the document term (DR-211 list, exact).
const DOCUMENT_PHASED: Record<string, string> = {
  "browse.missing": "Missing documents",
  "browse.rescan": "Rescan documents",
  "status.missingFiles.one": "{count} document missing",
  "status.missingFiles.other": "{count} documents missing",
  "empty.missing.title": "No missing documents",
  "empty.missing.body": "Every document's file was found.",
  "relink.title": "Relink missing document",
  "toast.relinked": "Document relinked",
  "toast.rescanFound.one": "{count} missing document found",
  "toast.rescanFound.other": "{count} missing documents found",
  "toast.rescanRestored.one": "{count} document found again",
  "toast.rescanRestored.other": "{count} documents found again",
  "toast.importSkipped.one": "{count} document skipped (already in library)",
  "toast.importSkipped.other": "{count} documents skipped (already in library)",
  "error.missingFile": "The document's file is missing: {path}",
  "toolbar.sort.size": "Size",
};

// Values whose subject is the physical file — the type-neutral wording the
// library ships since v0.0.9 (UI-44 MODIFIED option A, DR-304). The import
// labels rejoin this set with the option-A revert: "Import files" is the
// general wording, true for every managed type.
const PHYSICAL_FILE: Record<string, string> = {
  "toolbar.import": "Import files",
  "toolbar.importFiles": "Import files",
  "empty.library.action": "Import files",
  "empty.tag.action": "Import files",
  "drop.title": "Drop files to add them",
  "toast.imported.one": "Imported {count} file",
  "toast.imported.other": "Imported {count} files",
  "relink.chooseNew": "Choose new file",
  "relink.body": "The document is in the library, but its file was not found.",
  "relink.errorMissing": "The file you chose is not a readable PDF or EPUB.",
  "delete.withFiles": "Delete with files",
  "delete.fileNote": "This also deletes the files themselves.",
  "delete.body": "The files stay on disk unless you choose to delete them too.",
  "details.pathLabel": "File",
  "empty.library.body": "Import PDFs or EPUBs to get started — files stay where they are.",
  "job.rescan": "Checking files…",
  "error.openFailed": "The operating system could not open the file.",
  "error.io": "A file operation failed: {path}",
  "toast.deletedFiles.one": "Deleted {count} document and its file",
  "toast.deletedFiles.other": "Deleted {count} documents and their files",
  // FR-21 (v0.0.8): the rename dialog speaks of the physical file throughout
  // — the subject IS the file; these join the physical set (DR-268).
  "renameDialog.title": "Rename file",
  "renameDialog.nameLabel": "File name",
  "renameDialog.errorInvalid": "This name can't be used as a file name",
  "renameDialog.errorConflict": "A file with this name already exists in that folder",
};

describe("document terminology (DR-211, AC-95)", () => {
  it("the library-facing set carries the document phrasing exactly", () => {
    for (const [key, value] of Object.entries(DOCUMENT_PHASED)) {
      expect(en[key as keyof typeof en], key).toBe(value);
    }
  });

  it("the physical-file set is unchanged", () => {
    for (const [key, value] of Object.entries(PHYSICAL_FILE)) {
      expect(en[key as keyof typeof en], key).toBe(value);
    }
  });

  it("no string outside the audited set names a library entry after a file", () => {
    // The only values allowed to contain the word "file" are the physical
    // set and the two document-possessive rows ("its file", "the
    // document's file") from the re-phrased set.
    const allowed = new Set([...Object.keys(PHYSICAL_FILE), "empty.missing.body", "error.missingFile"]);
    const offenders = Object.entries(en)
      .filter(
        ([key, value]) =>
          !allowed.has(key) && typeof value === "string" && /\bfiles?\b/i.test(value),
      )
      .map(([key]) => key);
    expect(offenders, `values naming a library entry after a file: ${offenders.join(", ")}`).toEqual([]);
  });
});
