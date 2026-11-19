# Book Keeper

A cross-platform desktop app for managing a PDF and EPUB library. Files stay where they are — the app keeps a library index (SQLite) with metadata, tags, favorites, a reading collection, cover thumbnails and full-content search; documents are read in the app's own reader or opened with the OS viewer, never imported into a private store.

Current release: **v0.0.11** (see `docs/specs/INDEX.md` for the release registry; the product line is 0.x until the user declares 1.0.0).

## Features (shipped)

- Import PDFs and EPUBs by file or folder picker, or by drag-and-drop — files are referenced in place, never moved or copied; large imports run in the background while the library stays usable
- Tags (many-to-many), favorites, a **Reading** collection for works in progress, cover thumbnails, list and grid views with configurable columns; the library view is virtualized for large collections
- Search across metadata (title / author / notes / filename) **and document content** (FTS5 full-text over extracted PDF and EPUB text), with per-field scope controls
- Metadata extraction from PDF info dictionaries and EPUB metadata, editing with instant reindex; rename a document's file from inside the app; open the folder containing it
- Built-in readers, each document in its own window: PDFs with page navigation, view modes, contents and zoom; EPUBs as reflowed text with contents, position and text-size controls; password-protected PDFs supported; opening with the OS application remains available
- Missing-file detection, relinking and rescan; delete from the library or with the file
- Selection and bulk actions across the library
- The app's own top bar *is* the title bar: drag to move, minimize / maximize / close buttons, tooltips, styled scrollbars — no system chrome
- Light/dark appearance with the app's own palette; interface language setting (English today); settings window with data location and a **Log** section (the app writes its own log file and can open it)
- Self-updating: installed apps find and install newer published versions (signed update artifacts)
- Status bar with honest counts, toasts with actions, keyboard shortcuts throughout

All features are verified by running the app on Windows. v0.0.11 additionally ships installer packages for macOS and Linux; their runtime behavior on real hardware is still being verified — see **Known issues** below.

## Stack

Tauri 2 + Rust core (`rusqlite` bundled SQLite + FTS5, `lopdf` PDF metadata, `zip` + `quick-xml` EPUB parsing, `pdfium-render` page rendering for covers and the PDF reader, `image` PNG cover cache, `tauri-plugin-updater` self-updates) · Svelte 5 + Vite frontend, headless `bits-ui` primitives styled with the project's own design tokens. Every dependency entered through an approved Decision Log entry in the release specs.

## Repository map

| Path | Role |
|---|---|
| `docs/specs/` | Release specs + status registry (`INDEX.md` is the entry point; specs are the requirements source of truth) |
| `docs/specs/SPEC-ALL-TESTS.md` | Test inventory: every test case in the tree, per layer |
| `docs/release/features/` | End-user guide, one page per capability |
| `docs/release/technical-specs/` | Developer reference for released features (design, commands, invariants, test mapping, change history) |
| `docs/release/notes/` | Release notes, one page per shipped version (plain language; written at Ship, never edited afterwards) |
| `docs/ROADMAP.md` | Proposed direction: release line, known-issue ledger, deferred candidates |
| `src-tauri/` | Rust core: `db` → `library` (service + query) → `pdfmeta` / `epubmeta` / `epubsrv` → `pdf` (pdfium page rendering for the readers) → `covers` → `applog`, Tauri commands in `commands.rs` |
| `ui/` | Svelte frontend: `src/lib/state/` stores, `src/components/` |
| `scripts/` | Project scripts: fixture generator, pdfium fetcher, UI verification runner (`ui-verify.mjs` + flow suites) |
| `.github/workflows/` | CI (`ci.yml`) and the tag-triggered release pipeline (`release.yml`) |
| `RULES.md` / `AGENTS.md` | Coding rules / process rules — binding for any contributor |

## Build & run

Prerequisites: Rust toolchain (rustup; MSVC build tools on Windows), Node.js with pnpm, and the Pdfium dynamic library.

```sh
node scripts/fetch-pdfium.mjs      # downloads pdfium into src-tauri/binaries/ (covers + PDF reader)
cd ui && pnpm install
pnpm run tauri:dev                 # dev app with hot reload, in an isolated data directory (see below)
pnpm run tauri:build               # release build
```

Library data lives in the OS app-data directory of `com.bookkeeper.app`: `library.db` plus the `covers/` thumbnail cache, the reader cache and `logs/app.log`. Without `pdfium.*` in `src-tauri/binaries/` the app runs fully, just without cover thumbnails and the built-in PDF reader.

**Development data is separate from the installed app** (dev tooling, not a shipped feature): `pnpm run tauri:dev` goes through `scripts/dev-app.mjs`, which points the app at `<app data directory>/parallel/dev-data` unless `BOOKKEEPER_DATA_DIR` is already set. The mechanism is the existing test-only override `BOOKKEEPER_DATA_DIR` (`src-tauri/src/data_dir.rs`, DR-162 — the same one the UI verification lanes use to isolate their libraries), and the dev directory deliberately lives inside the app data directory under the `parallel/` namespace: the asset protocol scope in `tauri.conf.json` is compile-time config covering `$APPDATA/covers`, `$APPDATA/reader` and `$APPDATA/parallel/**` — a data directory outside it would write fine but could not display cover thumbnails (the launcher warns when an explicit `BOOKKEEPER_DATA_DIR` points outside). Production builds and the release pipeline are untouched by any of this.

## CI and releases

- **CI** (`.github/workflows/ci.yml`): every push to `master` runs the Rust test suite + clippy and the frontend unit tests + type checks on Windows, macOS and Linux (`fail-fast: false`). The real-app UI lanes are not part of CI (they need a desktop session); they run locally.
- **Releases** (`.github/workflows/release.yml`): pushing a `v*` tag builds the packaged app for all three platforms (macOS as a universal binary), produces the minisign-signed updater artifacts and the `latest.json` manifest, and publishes the GitHub Release with the release-notes page. Installed apps pick the update up from there.
- The Windows installers carry no purchased store certificate yet — Windows shows an "Unknown publisher" notice on first run; macOS shows the usual "downloaded from the Internet" notice once. Both are packaging facts, described in the user guide.

## Production builds by platform

`pnpm run tauri:build` (run from `ui/`) compiles in release mode and bundles installers for the platform it runs on, under `src-tauri/target/release/bundle/`. `scripts/fetch-pdfium.mjs` downloads the Pdfium library of the current platform (`--universal` for a universal macOS build), and the bundle config (`resources: ["binaries/*pdfium*"]`) ships it in `binaries/` inside the package.

### Windows (x64)

- Prerequisites: rustup + Microsoft C++ Build Tools ("Desktop development with C++"), WebView2 runtime (preinstalled on Windows 10/11), and the VBSCRIPT optional feature for MSI builds; Node LTS + pnpm.
- Artifacts: `bundle/nsis/Book Keeper_<ver>_x64-setup.exe` (NSIS) and `bundle/msi/Book Keeper_<ver>_x64_en-US.msi`; the installer places `bookkeeper.exe` with `binaries/pdfium.dll` beside it.

### Linux (x64)

- Prerequisites: rustup, Node LTS + pnpm, plus system packages — on Debian/Ubuntu: `libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev` (Arch/Fedora/… variants in the [Tauri prerequisites docs](https://v2.tauri.app/start/prerequisites/)).
- Artifacts: `.deb` (package name `book-keeper`) and `.AppImage`. Build on the same distro family you ship to (glibc compatibility); the AppImage is the portable artifact.
- `fetch-pdfium` downloads `libpdfium.so`.

### macOS (Apple Silicon / Intel)

- Prerequisites: rustup, Xcode Command Line Tools (`xcode-select --install`), Node LTS + pnpm.
- Artifacts: `.app` + `.dmg`; the release pipeline builds `universal-apple-darwin` (one artifact for both architectures). Cross-compile locally with `pnpm run tauri:build --target aarch64-apple-darwin` (or `x86_64-apple-darwin`) — pnpm forwards flags to `tauri` directly; do not use `--` as a separator (it forwards the rest to cargo).
- `fetch-pdfium` downloads `libpdfium.dylib` (`--universal` builds a two-architecture library).

## Known issues

The factual ledger is `docs/ROADMAP.md` §4; each release's notes list what applies to it. The two that affect the **shipped v0.0.11 packages** today, both scheduled for repair in v0.0.12:

- **D34** — on the packaged macOS and Linux apps, PDFs/EPUBs cannot be read and covers do not render: the bundled Pdfium is placed where the app's search path does not reach (the macOS `.app` and the `.deb` use the OS-standard resource locations; Windows and AppImage coincidentally match the search path). Development builds are unaffected.
- **D33** — on macOS the app's data folder (`~/Library/Application Support/com.bookkeeper.app`) cannot be opened in Finder: the identifier ends in `.app`, so macOS treats the folder as an application bundle. The folder's contents are fine; v0.0.12 changes the identifier to `com.bookkeeper` (fresh start, no data migration).

## Tests & verification

```sh
cd src-tauri && cargo test && cargo clippy --all-targets -- -D warnings
cd ui && pnpm test && pnpm run check         # vitest + svelte-check
node scripts/ui-verify.mjs                   # full UI verification against the real built app
```

`ui-verify.mjs` is the UI verification entry point: it runs **every UI test case as its own lane** — each with its own app data directory and driver ports — in barrier-separated phases (build first; the timing-sensitive flows one at a time; the 10 000-document import measured while nothing else runs; the rest in parallel), with the combined run asserted against a 300 s budget. Exit 0 only if all lanes pass. Add `--report` to save a final summary (verdict, tallies, the per-lane table, failed check names) as `tmp/test-reports/verify-<timestamp>.md`, or `--report <path>` to choose the file.

Debugging aids when a lane fails: `SMOKE_FLOW=flowX node scripts/ui-smoke.mjs` reproduces a single flow standalone; `SMOKE_SET=<area>` runs one feature area; `node scripts/ui-10k.mjs` runs the 10 000-document suite alone; `node scripts/ui-verify.mjs --coarse` is the documented two-suite fallback. The full test inventory is `docs/specs/SPEC-ALL-TESTS.md`.

The UI harness drives the built app through WebDriver (`tauri-driver` + a version-matched `msedgedriver` on PATH); fixtures are generated on demand by `scripts/make-fixtures.mjs` (never committed). Verification policy: RULES R6 — unit tests plus a real-app verification run are both required before a release ships.

## Contributing

Read [`AGENTS.md`](AGENTS.md) first: the workflow is spec-driven — requirements live in `docs/specs/`, code follows [`RULES.md`](RULES.md), and implementation only starts from an APPROVED spec with an explicit user go.
