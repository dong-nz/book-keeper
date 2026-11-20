# Tags

> **Status: ✅ Shipped in v0.0.1 (as *Categories*), renamed to **Tags** in v0.0.8, updated in v0.0.12 and v0.0.14** — verified by running the app. The feature is the same one that shipped as Categories; since v0.0.8 the app calls these groups **tags** everywhere — in the Browse menu, the tags window, document actions, tooltips, messages, and this guide.

## Managing tags
- Open **Browse ▸ Manage tags**: it lists every tag with its live count, has an always-available *New tag* field, and per-row **Rename** / **Delete** actions. Tag names are unique (case-insensitive); a duplicate name is refused with a clear message.
- Deleting a tag removes it from all documents — it never deletes the documents themselves.

## Assigning documents
- A document can belong to **any number of tags** (or none).
- Single document: use the tag picker in its Details dialog (chips you can remove, a searchable list, new tags created as you type — see the v0.0.14 amendment).
- Many at once: select rows (checkboxes) → bulk bar → **Tags** → add and/or remove in one action.
- Importing while one or more tags are active assigns all of them ([importing-files.md](importing-files.md)).

## Browsing
**Browse ▸ Tags** lists your tags with live counts; its items are checkable and **multi-select** — filtering by several tags shows the documents in *any* of them (a document in two of them is listed once). The tag filter combines with search, Favorites, Reading and Missing. The Browse menu shows which tags you're currently filtering by, and since v0.0.14 a strip above the document list repeats every active filter as removable chips ([browsing-and-filtering.md](browsing-and-filtering.md)).

## History (how this page got here)

> **Amendment (2026-11-03):** the rename/delete dialogs were broken before this fix — opening them from the sidebar did nothing at all. Both then worked, including in the collapsed icon rail.

> **Amendment (2026-11-04, v0.0.2):** the sidebar — its category list, the new-category field and the collapsed icon rail — was removed. Everything moved into the **Categories** dialog (opened from **Browse ▸ Manage categories…**) and filtering moved to **Browse ▸ Categories** as a multi-select. Importing while several categories are active has assigned all of them since then.

> **Amendment (2026-11-04, v0.0.3):** documents stopped showing their category names in the list and the grid — rows and cards show document content only. Group membership is seen and changed in the document's editor and in the bulk action, and filtering shows the active selection in the Browse menu.

> **Amendment (2026-11-05, v0.0.4):** in the categories window, each count is quiet text in line with the rest of the row — no filled pill behind the number.

> **Amendment (2026-11-10, v0.0.8):** the application now calls these groups **tags** in every surface: *Browse ▸ Tags*, *Browse ▸ Manage tags*, the Tags window, the bulk **Tags** action, the Details field, all tooltips and messages. Nothing about the feature changed — same groups, same counts, same multi-select filtering, same import assignment; only the name. Your existing groups and assignments carried over unchanged when you updated the app.

> **Amendment (2026-11-19, v0.0.12): the tags window gained a toolbar and *Delete all tags*** (verified by running the app).
>
> - **The tags window now has a bar across its top** — the title, the **Delete all tags** action, and the close button — with the tag list below it, like the app's other working dialogs. The list itself, the *New tag* field, and the per-row Rename / Delete actions work exactly as before.
> - **Delete all tags** clears every tag in one action. A confirmation dialog asks first and says plainly that your documents are kept — nothing is deleted but the tags themselves. After it completes, a message reports how many tags were removed. If you were filtering by tags when you did it, that filter is cleared and your documents show unfiltered. The action is disabled while there are no tags. Like a single tag delete, there is no undo.
>
> Everything else on this page is unchanged — assigning tags, multi-select filtering, live counts, import assignment.

> **Amendment (2026-11-20, v0.0.14): the tag picker in Details, a quicker tags window, counts that stay fresh** (verified by running the app).
>
> - **The tags section of the Details dialog is now a picker.** Your current tags show as **chips**, each with an × that removes it right away — the checkbox list is gone. Below the chips an *Add tag* button opens a searchable list of all your tags: tags the document already has are listed too, marked with a check; typing narrows the list and highlights the matching part of each name; clicking a row assigns or un-assigns that tag immediately. Arrow keys move a highlighted row through the list while you keep typing — Enter then assigns or un-assigns that row, just like clicking it. If nothing matches what you typed, a **Create "…"** row appears at the top of the list: pressing Enter or clicking it creates the tag and assigns it. Esc closes the tag list and leaves the Details dialog open. Nothing is stored until you save the dialog — and everything you changed is saved together.
> - **Your edits are safe when a new tag is created on the spot.** A title you typed, notes you drafted, tags you picked — creating a new tag in the picker no longer resets them. (Before this update, starting a new tag from Details could silently discard changes you hadn't saved yet.)
> - **The tags window opens with the cursor already in the *New tag* field** — you can start typing a name straight away.
> - **Tag counts stay current.** After deleting a document — one or many — the counts in *Browse ▸ Tags* and in the tags window immediately match reality; before this update they could keep counting a deleted document until something else refreshed the list.
>
> Everything else on this page is unchanged — multi-select filtering, import assignment, the tags window's rename / delete / *Delete all tags*.
