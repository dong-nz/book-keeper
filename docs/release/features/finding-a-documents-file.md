# Finding a document's file on disk

> **Status: ✅ Shipped in v0.0.8, updated in v0.0.14** — verified by running the app (the item in the context menu and in Details, the file manager opening with the file selected, the item absent for a missing document).

Your files stay where they are — the app only keeps track of them. When you need to see the actual file, **Open containing folder** in a document's context menu or in its Details dialog: your system's file manager opens that document's folder with the file selected.

The action appears only for documents whose file is present. A missing document offers **Relink** instead ([missing-files-relinking.md](missing-files-relinking.md)).

> **Amendment (2026-11-20, v0.0.14): in the Details dialog the action is now a folder icon button** (verified by running the app). The document's path is shown as plain text with a small folder button at its end — clicking it does exactly what the old text button did. Hovering names the action; a missing document shows neither the button nor the tooltip. The context-menu item is unchanged.
