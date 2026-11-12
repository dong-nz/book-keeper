# Renaming a document's file

> **Status: ✅ Shipped in v0.0.8, updated in v0.0.9** — verified by running the app (a real file renamed on disk, the record following it, tags/notes/favorite/Reading surviving, refused names changing nothing, a file-derived title re-derived).

Files you imported often carry the name a scanner or a download gave them. You can fix that from inside the app: **Rename** in a document's context menu or in its Details dialog.

- The app renames the actual file, in the same folder, and updates its library record to match. The file stays where it is — only its name changes.
- Everything you attached to the document survives: tags, favorite state, notes, Reading membership, a remembered password.
- A name already used by another library document, or by an existing file in that folder, is refused with a clear message — and nothing changes: not the file, not the record.
- Names the file system doesn't allow (empty, with slashes or reserved characters, ending in a dot or a space) are refused before anything happens.
- If the title shown in the app came from the filename, it updates with the new name. A title you edited yourself stays yours.
- The document's cover follows the new name automatically.

> **Amendment (2026-11-12, v0.0.9): renaming between the two supported types is now allowed** (verified in the running app). A document's name may change its extension between `.pdf` and `.epub` — the app accepts such a rename, renames the real file, and re-reads the document as the new type: a file-derived title and the cover follow the new file. A rename to any *other* extension (`.txt`, `.jpg`, …) is refused with a clear message, and nothing changes — not the file, not the record. Books that already have their own internal title keep it through a rename, like PDFs with internal titles do. Everything else on this page is unchanged.
