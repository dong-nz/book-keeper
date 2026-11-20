# Reading PDFs

> **Status: ✅ Shipped in v0.0.6, updated in v0.0.7, v0.0.8, v0.0.9, v0.0.10 and v0.0.12** — verified by running the app.

Book Keeper now opens documents **inside itself**. Double-click a document (or press `Enter` on a focused row, or choose **Open** in its right-click menu) and the document opens in its own reading window — no other application needs to start.

## The reading window

Each document you open gets its own window, named after the document. It opens at a comfortable default size and you can resize it, minimize it, or move it like any other window. Opening the same document again brings its existing window to the front instead of opening a second copy.

The reading window holds nothing but the document: no top bar, no menus, no status bar. Your library window stays fully usable while you read — you can keep browsing, searching, or organizing with the document open.

Across the top of the window is a slim control strip:

- **Page navigation** — previous / next buttons, and a `current / total` page field: type a page number there and press Enter to jump straight to it.
- **Zoom** — *Fit width* (the default: the page fills the window's width), fixed levels **50 / 75 / 100 / 125 / 150 / 200 / 300 %**, and **− / +** buttons that step between them.

The keyboard works too: `PgUp` / `PgDn` move a page, `Home` and `End` jump to the first and last page.

Pages form one continuous vertical column — scroll to read. When you change zoom, the page you were looking at stays where it was.

Closing the reading window (its window button, or `Esc`) leaves your library exactly as it was; the document's temporary preview data is cleaned up, and other open reading windows are untouched.

## Opening with another application

Opening a document with your system's PDF application is still there — it just moved to the menu: right-click a document and choose **Open in external viewer**. The first menu item, **Open**, is the built-in reader.

## When a document can't be read

- If the file is missing, nothing opens silently: a message appears offering **Relink** ([missing-files-relinking.md](missing-files-relinking.md)).
- If the file exists but can't be displayed as a PDF, the reading window says so and offers **Open in external viewer** anyway, so another application can try.

> **What reading does not include yet.** You can view, zoom and navigate — but not select or search text inside a document, and the reader does not remember where you stopped: every document opens at the first page. There are no tabs inside one window: each document gets its own window. These are candidates for later releases.

> **Verified behavior.** Opening a document shows its first page within well under a second, and page jumps and zoom changes are shown within a few tenths of a second — including in documents with 1 000 pages. Everything above was tested by running the application.

> **Amendment (2026-11-09, v0.0.7): the reading window was reworked.** All of this was verified by running the app; everything not mentioned below works exactly as described above.
>
> - **The control strip moved to the bottom of the window.** The top of the window now shows only the document's name — in full, shortened with "…" only when it genuinely doesn't fit. When a document can't be displayed as a PDF, the reading window shows its message and no control strip at all.
> - **View modes.** The strip carries a new view control with three modes: **Single page** — one page at a time, centered in the window; scrolling past the top or bottom edge turns exactly one page; **Continuous** — the vertical column described above (this is the default); **Facing** — two consecutive pages side by side, moving pair by pair. The choice applies to that reading window only and is not remembered: every newly opened window starts in Continuous.
> - **Pages are centered horizontally** in all three modes.
> - **Zoom now works against the page's real size.** The zoom control is one select offering **Fit width** (still the default), **Fit page**, **Actual size**, and fixed levels **50 / 75 / 100 / 125 / 150 / 200 / 300 %** — those percentages now mean the page's own size, so *Actual size* really is 100 %. `Ctrl` + `+` / `Ctrl` + `−` step through the levels; `Ctrl+0` / `Ctrl+1` / `Ctrl+2` jump to Fit width / Fit page / Actual size. Changing zoom keeps the page you were looking at in place, as before.
> - **Table of contents.** If a PDF has an outline (bookmarks), the strip shows a **Table of contents** control — the first control in the strip. It opens a panel: a fixed bar across its top carries one expand/collapse-all toggle (an icon button; hovering it names the action it's about to perform), and the contents scroll independently below it — the bar stays visible while you scroll. The panel opens collapsed to the outline's top level; expand it section by section with the little arrows, or all at once with the toggle. Clicking an entry jumps to that place in the document. `Esc` closes the panel first, and only then the window. Documents without an outline show no such control.
> - **More keys.** In a reading window: `←` / `→` navigate exactly like the strip's buttons (in Facing mode they move by a pair); `↑` / `↓` scroll like one notch of the wheel — at an edge in Single-page mode, one press turns exactly one page. `PgUp` / `PgDn`, `Home` / `End`, the page field and `Esc` are unchanged. While you're typing in the page field, or while the contents panel or a menu is open, these keys belong to that thing and don't move the document.
> - **A theme change reaches open reading windows.** Pick Light, Dark or Auto in the Settings menu and already-open reading windows change with it, immediately ([settings-and-appearance.md](settings-and-appearance.md)).
> - **Pages arrive faster.** The image-writing step for reading pages was retuned — measured on the test machine, the work the app spends on a large page nearly halved (332 ms → 207 ms for a full-size page), and the whole cold-scroll path now runs under a budget the verification suite enforces: no more than 0.35 s of app work per page served when scrolling a 192-page document from its very first (cold) page; a warm, already-drawn scroll does no serving work at all. Nothing about the documents changes — only how quickly they arrive.
>
> The "What reading does not include yet" note above still stands: text selection and search inside a document, and remembering where you stopped, are not in this version either.

> **Amendment (2026-11-10, v0.0.8): two changes to the reading window** (verified by running the app).
>
> - **A protected document opens with a password prompt, not an error.** When a PDF needs a password to open, the reading window shows a prompt where you type it; with the right password the document behaves like any other. A wrong password keeps the prompt open with a clear message ([password-protected-pdfs.md](password-protected-pdfs.md)).
> - **Changing zoom near the end of a document no longer moves the page.** Earlier, at the last page, switching from a *Fit* mode to another zoom level and back could leave the page sitting away from the window's bottom edge — the view jumped when it shouldn't have. Now the page you were looking at stays anchored through any zoom change, including at the document's end and in every view mode. Measured in a 192-page document: fit → 50 % → fit leaves the last page exactly where it was.
>
> Everything else about reading — view modes, contents, navigation, keys, the strip — is unchanged.

> **Amendment (2026-11-12, v0.0.9): opening a document no longer flashes** (verified in the running app). Earlier, a reading window showed an empty moment for a beat before the first page appeared — a visible flash. The first page now arrives with a short, smooth cross-fade: the document appears as one continuous motion, and the same fade covers switching a document's view mode. Opening speed is unchanged — measured on the test machine, the first page of a 192-page document still appears in about 0.37 s from the double-click. Everything else about reading is unchanged. Books (EPUBs) open in their own window, described in [reading-epubs.md](reading-epubs.md).

> **Amendment (2026-11-17, v0.0.10): a fourth view mode, a pinnable contents panel, and a settings menu in the reading window** (all verified by running the app).
>
> - **Continuous horizontal.** The view control now offers a fourth mode: **Continuous horizontal** — pages side by side in one long row, like a film strip, and you read it by scrolling sideways (with the wheel or by dragging). It suits wide, landscape-shaped documents and comparing pages next to each other. Its keys run along the row: `←` / `→` move to the previous / next page, `PageUp` / `PageDown` travel one window-width along the row, `↑` / `↓` scroll the row sideways by one notch, `Home` / `End` jump to the first / last page, and the page field jumps to the page you type. *Fit width* fits the page you're currently looking at. Switching into or out of the mode keeps the page you were reading exactly where it was. Like the other modes, the choice applies to that window only and is not remembered — a new window still starts in Continuous. Pages arrive at the same measured speed as in the other modes.
> - **You can pin the contents panel.** The contents panel's top bar now carries a **pin** control at its right end (its icon shows which state you're in). Clicking it docks the panel as a permanent side panel at the left edge of the reading window: the pages re-arrange into the remaining space, *Fit width* follows the narrower area, and clicking a contents entry no longer closes the panel — you can keep jumping through a book while its contents stay open. `Esc` still closes the panel first, and a pinned panel comes back at the same docked spot when you show it again; the pin lasts as long as that reading window is open (a new window starts unpinned). Unpinning through the pin control restores the old temporary behavior.
> - **The reading window has a settings control.** A settings button now sits at the right end of the top bar, just left of the window buttons. Its menu is the same Settings menu you know from the library window: choose the appearance (Light / Dark / Auto — the change applies everywhere, live), open the **Settings** window inside the reading window, or **Check for updates** ([settings-and-appearance.md](settings-and-appearance.md), [keeping-the-app-updated.md](keeping-the-app-updated.md)).
> - **A small drawing fix while pages arrive.** Pages fade in as before (v0.0.9); during the fade no placeholder icon ever shows — earlier a small "broken image" icon could flash.
>
> Everything else about reading — the other modes, navigation, zoom, page speed, opening with another application — is unchanged.

> **Amendment (2026-11-19, v0.0.12): the reader remembers you, text becomes selectable, and you can choose the document's cover** (verified by running the app).
>
> - **The reader remembers where you stopped.** Open a document again and it comes back to the page you were last on — including how far down that page you had scrolled, in whichever view mode you were using, for every mode. The memory is per document and survives app restarts. Deleting a document forgets its place; relinking or renaming a file keeps it. If the file has since been replaced by a shorter one, the reader opens at the nearest page that exists — no error, no blank window. While you read, the place is saved quietly in the background; if the app is closed abruptly, at most the last second of reading is lost. There is no "continue reading" prompt or list — it just opens where you left off.
> - **Your view mode is remembered too.** The mode you last chose (Continuous, Single page, Facing, Continuous horizontal) is the mode new reading windows open in — across documents and across restarts. Choosing a mode per window works exactly as before; only the starting point changed. Until you choose a mode for the first time, the default is still Continuous. (This replaces the v0.0.10 note that modes are never remembered.)
> - **Select and copy text.** Drag across a page and its text highlights, like anywhere else — the selectable text sits invisibly over the rendered page at the real text positions. Copy with `Ctrl+C` (or `Ctrl+Insert`); the selection is shown in the app's own selection color. It works in every view mode and after zooming. Pages that are just scanned images have no text to select — they look and behave exactly as before. Selecting does not interfere with scrolling, navigation, or the controls.
> - **Choose which page is the document's cover.** The top bar now carries a **cover** control (at the right end, left of the settings button). In a PDF it offers **"Set this page as cover"** — the page you're currently looking at becomes the document's cover in the library — and **"Automatic cover"**, which reverts to the first page. The choice is kept; it survives restarts. You can also choose from the library itself — see [covers.md](covers.md).
> - **On macOS**, the reading window's own window controls now sit at the **left** end of the top bar (close first), like a Mac window; the cover, settings and page controls keep their positions at the right end. Windows is unchanged. (Design verified by automated checks; the look on real Mac hardware is confirmed with the published release — see [the-app-window.md](the-app-window.md).)
>
> The "What reading does not include yet" note above no longer applies to two of its items: text selection and remembering where you stopped are now in. Still not included: searching inside a document, selecting whole pages at once, saved selections or highlights, and tabs inside one window.
