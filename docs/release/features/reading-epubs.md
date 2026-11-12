# Reading books (EPUBs)

> **Status: ✅ Shipped in v0.0.9** — verified by running the app.

PDF Keeper now reads **EPUB books inside itself**, in their own window, the way it reads PDFs ([reading-pdfs.md](reading-pdfs.md)). A book is one continuous, flowing text — it adapts to the window's width and to the text size you choose, and you read it by scrolling, the way you'd read it on any e-reader.

## Opening a book

The same as for any document: double-click it, press `Enter` on a focused row, or choose **Open**. A book opens in its own reading window; **Open in external viewer** hands it to your computer's other book application instead. A book the app can't read (a damaged file, or one that uses book protection the app doesn't support) says so plainly in the window.

## Reading

- The text flows continuously from chapter to chapter; scroll with the wheel or by dragging.
- The book's own fonts, styles, and illustrations appear as the book intends.
- **Selecting text works** — you can select a passage and copy it, as in any reading application.
- Links inside the book jump to their target in the same window; a link that points outside the book opens in your browser.
- Keys: `↑` `↓` scroll a notch, `PageUp` `PageDown` a screen, `Home` `End` jump to the start or end of the book, `Esc` closes the contents panel first if it's open, otherwise the window. While you're typing in a field, the keys belong to that field.

## Contents, position, and text size

The strip at the bottom of the window holds everything for working with a book:

- **Contents** — the book's own table of contents, when it has one; click any entry to jump there. A book without its own contents gets a list of its chapters instead.
- **Where you are** — the strip shows your position as a percentage of the book, and a **Go to position** field lets you jump to any point you type in.
- **Text size** — smaller and larger buttons step the text size; the size you pick is remembered for every book you read.

## What reading a book does not include yet

- Books that use protection (DRM) can't be read inside the app — the window says so.
- The app does not remember where you stopped reading a book (the same gap exists for PDFs).
- There are no bookmarks, highlights, or notes inside a book, and no page numbers — a book has no fixed pages, so the position is a percentage.
- The app does not search inside a book's text (search covers the library's own fields — see [searching.md](searching.md)).
- Reading never changes your book file.

## Speed

Books open quickly. Measured on the test machine: a book with a 1 MB chapter shows its first readable text about 0.3 s after opening, and scrolling a long book keeps up with the wheel.
