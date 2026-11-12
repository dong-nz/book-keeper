# Importing files

> **Status: ✅ Shipped in v0.0.1, updated in v0.0.2, v0.0.3, v0.0.4, v0.0.7, v0.0.8 and v0.0.9** — verified by running the app. *This page was named "Importing PDFs" until v0.0.9, when the app began accepting EPUB books as well; the name followed what the app does.*

## Three ways to add documents
1. **File picker** — Import button (or Ctrl+I), select one or many files.
2. **Drag and drop** — drop files onto the app window.
3. **Import folder…** — pick a directory; every supported document inside (PDFs and EPUB books) is added in one action. Nothing is watched continuously — it is a single bulk action.

The app accepts **PDF and EPUB** files. Any other file type is skipped, with a notice in the results.

## Auto-assignment
If a tag is active while you import, the imported documents are assigned to it automatically. With nothing active, nothing is assigned — organize later via the editor or bulk actions.

## Duplicates
A file whose exact path is already in the library is skipped, with a notice in the results. The app does not look for content-identical copies at other paths.

## What happens after import
- Title and author are read from the file's own properties when available — the PDF's internal properties, or the book's own metadata for an EPUB; otherwise the filename is used (see [metadata-editing.md](metadata-editing.md)).
- Cover thumbnails are generated in the background — the app stays usable while it works.
- A toast reports how many were imported/skipped; the status bar shows library totals.

## Limits (v1)
No full-text content indexing (neither inside PDFs nor inside books); importing does not move, copy, or rename your files.

> **Amendment (2026-11-04, v0.0.2):** auto-assignment now follows the filters you actually have active, and the sidebar it referred to is gone. Import while **one or more** categories are checked (Browse ▸ Categories — they appear as chips in the toolbar) and every imported document is added to **each of those categories** in the same single action. With no category active, nothing is assigned, exactly as before. Duplicate handling, progress reporting and the rest of the import flow are unchanged.

> **Amendment (2026-11-04, v0.0.3):** the **Import button is no longer in the toolbar**. The import control is now the small **＋** at the right end of the search box: clicking it gives the same two choices as before — **Import files…** and **Import folder…** — and both open the same pickers (files with the PDF filter, or a whole folder). While you have text in the search box, that same spot holds the clear button instead, so the ＋ reappears when you clear it. `Ctrl+I` (Cmd+I) and drag-and-drop onto the window work exactly as before, and everything described above about what an import does — auto-assignment to every active category, duplicates skipped with a notice, titles and covers filled in in the background — is unchanged.

> **Amendment (2026-11-05, v0.0.4):** one correction, verified by running the app. The two import menu items are now named **Import files** and **Import folder**, without the dots (see the label rule in [browsing-and-filtering.md](browsing-and-filtering.md)). What they open is the same as before. The progress labels shown *during* an import keep their dots — "Importing…", "Generating covers…" describe work under way, and that's exactly what they're for. Everything else on this page is unchanged.

> **Amendment (2026-11-09, v0.0.7): importing no longer blocks the application** (verified by running the app: a 300-file import while searching and browsing, and the 10 000-file run).
>
> - **The app answers while an import works.** Earlier versions held the library busy for the whole duration of an import: search, browsing, category selection and settings stopped responding until it finished. Now an import works in small batches that hold the library only for a moment each, and everything else — searching, browsing, starring, opening a document, changing a setting — keeps answering normally while the import runs.
> - **Progress is now visible for imports of any size.** Earlier, an import of up to 500 files reported only its final result — the progress bar in the bottom bar never appeared for ordinary imports. Now it moves from the start of the import to its completion, whatever the size.
> - The results toast, duplicate skipping, auto-assignment to active categories, background cover generation and everything else on this page are unchanged.

> **Amendment (2026-11-10, v0.0.8):** two changes, both verified by running the app.
>
> - **The import actions are named plainly.** The two menu items are now **Import PDFs** and **Import folder** — the same pickers as before, just named for what they do instead of the word "files". The empty library's buttons carry the same names, and the small label on the import control reads "Import PDFs". `Ctrl+I` opens the file picker, **`Ctrl+O` the folder picker** ([keyboard-shortcuts.md](keyboard-shortcuts.md)).
> - **Progress counts move in smaller steps.** During an import the progress bar and its count advance every 5 files instead of every 50 — on a large import the numbers now visibly walk, and the "Importing…" label updates as they do.
> - Auto-assignment now applies to **tags**: import while one or more tags are active and every imported document joins all of them ([tags.md](tags.md)). Everything else on this page is unchanged.

> **Amendment (2026-11-12, v0.0.9): the app now also manages EPUB books** (verified by running the app) — and with it the import wording stops naming one file type.
>
> - **Both file types are accepted everywhere you add documents.** The file picker shows PDFs and EPUBs; **Import folder** adds both kinds from the directory you pick; drag-and-drop accepts both. Other file types are still skipped with a notice.
> - **The import actions are named "Import files" again.** The menu items, the empty library's buttons, and the import control's label all read **Import files** / **Import folder**, and the completion toast reports "Imported N file(s)" — no control names one file type while the app accepts both.
> - **Books are treated exactly like PDFs everywhere else on this page**: auto-assignment to active tags, duplicate skipping, background title/author reading and cover generation, progress reporting. A book's title and author come from the book's own properties when it has them; its cover comes from the book's own cover image ([covers.md](covers.md)).
> - Books sit in the list next to PDFs with no special mark — you tell them apart by the extension in the filename. Reading a book inside the app is described in [reading-epubs.md](reading-epubs.md).
