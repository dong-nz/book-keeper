# Cover thumbnails

> **Status: ✅ Shipped in v0.0.1, updated in v0.0.3, v0.0.4, v0.0.5, v0.0.7, v0.0.8, v0.0.9 and v0.0.12** — verified by running the app.

Every document shows a cover: a thumbnail of its first page, in list rows and grid cards.

- Covers are generated when you import, in a background queue — the app stays responsive while working.
- They are regenerated when Rescan detects the file changed, or when a cover is missing.
- If a page can't be rendered, a generic PDF symbol is shown instead; the import still succeeds.
- Covers are stored in the app's own data folder and cleaned up automatically when you remove a document from the library.

Covers are display-only: opening a document still uses your system's PDF viewer ([opening-and-deleting.md](opening-and-deleting.md)).

> **Amendment (2026-11-04, v0.0.3):** covers got bigger along with the rows and cards. A list row's cover is now 45×64 (it grew with the taller 64 px row rather than keeping its old 34×48 size), and a grid card's cover scales with its card — cards are as wide as your window and column count allow, up to 320 px, so the cover grows with them ([sorting-and-views.md](sorting-and-views.md)). Nothing else changed: covers are still made in the background when you import, still regenerated when a file changes or a cover is missing, still replaced by a generic PDF symbol when a page can't be rendered, and still cleaned up when you remove the document.

> **Amendment (2026-11-05, v0.0.4):** two corrections, verified by running the app.
>
> - **Covers are sharper.** They were always stored at a fixed modest resolution, so a large card cover (or a cover on a high-resolution display) was an enlarged, soft image. Covers are now produced at a higher resolution — display sizes are unchanged, and covers stored by earlier versions are quietly replaced in the background, with nothing for you to do.
> - **Card covers stay inside their card.** With wide cards (a low column count in a wide window) the cover image could run past the card's right border. The cover now fills exactly the space the card gives it.
>
> Everything else on this page is unchanged — background generation, regeneration on change, the fallback symbol, and cleanup on removal.

> **Amendment (2026-11-06, v0.0.5): covers are generated much faster in large libraries.** Thumbnail making now works on several fronts at once instead of strictly one at a time. Measured on the test machine: building every thumbnail from scratch for a 10 000-document library took a little over six minutes in v0.0.4; the same rebuild now takes a little over two — around 79 thumbnails a second instead of 27. The more processor cores your machine has, the more you gain; on a two-core machine the gain is small. While it works, everything else about a large import is exactly as before: the app stays usable, the progress in the bottom bar moves the same way, and covers simply arrive sooner. Nothing else on this page changed — background generation, regeneration on change, the fallback symbol, and cleanup on removal all behave as described.

> **Amendment (2026-11-09, v0.0.7): covers now appear while you watch** (verified by running the app).
>
> - **Covers arrive in the open list as they are made.** While the background queue works, each finished thumbnail appears in the list or grid you're looking at — no restart, no reload, no switching to the grid and back. A large import fills in its covers in front of you; the same applies to covers rebuilt after a Rescan or a cache cleanup.
>
> Generation speed itself is unchanged in this version (the speed-up in v0.0.7 went to reading pages, not to cover making — see [reading-pdfs.md](reading-pdfs.md)). Nothing else on this page changed — background generation, regeneration on change, the fallback symbol, and cleanup on removal all behave as described.

> **Amendment (2026-11-10, v0.0.8): two cover behaviors** (verified by running the app).
>
> - **A password-protected document shows the neutral placeholder cover** until you unlock it once with *Remember this password* ticked — after that its real first-page cover is made and appears like any other ([password-protected-pdfs.md](password-protected-pdfs.md)).
> - **Renaming a document's file carries its cover along.** After a rename through the app, the cover in the list follows the renamed file automatically — nothing for you to do ([renaming-a-documents-file.md](renaming-a-documents-file.md)).
>
> Generation, speed, regeneration on change, the fallback symbol, and cleanup on removal are unchanged.

> **Amendment (2026-11-12, v0.0.9): books (EPUBs) get covers too** (verified in the running app).
>
> - **A book's cover is the cover image the book itself declares.** It is fitted into the cover space whole — never cropped — so nothing printed on a book's cover is cut away; the space around it shows the card's background color.
> - **A book that declares no usable cover image gets the same generic placeholder** every unrenderable document gets. The import still succeeds, and everything else about the book works normally.
> - Background generation, the appearance of covers in the open list, regeneration on change, and cleanup on removal work for books exactly as described above.

> **Amendment (2026-11-19, v0.0.12): you can choose which page or image represents a document** (verified by running the app).
>
> - **"Update cover…" in the document's right-click menu** opens a cover chooser. For a PDF it shows a strip of the document's page thumbnails — they appear as you scroll, so even a long document opens the chooser instantly; click any page and that page becomes the document's cover. For a book it shows a grid of the images inside the book; pick one and it becomes the cover. A book with no pictures inside offers nothing to pick and keeps its current cover.
> - **The reader can do the same from where you're reading:** in a PDF, the cover control in the top bar offers **"Set this page as cover"** for the page you're looking at; in a book, **"Choose cover…"** opens the same image grid ([reading-pdfs.md](reading-pdfs.md), [reading-epubs.md](reading-epubs.md)).
> - **"Automatic cover"** — in the chooser and in the reader — reverts to the old behavior: the first page for a PDF, the book's own declared cover for a book.
> - **Your choice is kept** — it survives restarts, and the new cover appears in the list and grid like any other, arriving through the same background queue: the app stays responsive while it's made. If a chosen page can't be rendered, the document shows the generic placeholder instead — nothing else about the document changes.
>
> Everything else on this page is unchanged — background generation, regeneration on change, the fallback symbol, and cleanup on removal. One note: after a Rescan finds a file changed, a document you gave a manual cover keeps that choice; documents without one behave exactly as before.
