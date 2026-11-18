# Packaging & CI — technical reference

**Since v0.0.11.** Requirements: NFR-10 (CI verification pipeline), NFR-11 (release packaging and publishing pipeline); AC-157…AC-160. NFR-04 (cross-platform installers, agreed since v0.0.1) is **realized** by this document — the DR-92 "Windows-only builds" limitation ends. Cross-cutting infrastructure: there is no user-guide page for CI; install behavior lives in [getting-started](../features/getting-started.md). Design authority: SPEC-v0.0.11.md §3.4/§3.5 (frozen); this document describes the pipelines **as released and proven by their runs** — the six pre-flight runs (DR-382…DR-388) corrected the design's environment assumptions at several points, and what follows is the corrected shape.

## CI pipeline — `.github/workflows/ci.yml`

**Triggers:** `push` and `pull_request`, both filtered to `branches: ["master"]` — tag pushes belong to `release.yml`; an unfiltered trigger would run CI on a pre-flight tag (DR-382). Concurrency group per ref with `cancel-in-progress`.

**`verify` job — three-OS matrix** (`windows-latest`, `macos-latest`, `ubuntu-24.04`), `fail-fast: false`: every layer reports its own verdict (AC-157). Step order is RULES workflow 3's order. **The UI lane runner is not a CI step (DR-390, the user's decision):** the real-app layer stays the local Phase-A/C entry point (`scripts/ui-verify.mjs`, unchanged); the Windows-only lane job, its `tauri-driver` install and its report-upload step were removed from the workflow before the first green run.

1. `actions/checkout@v7`; wall-clock start (AC-158).
2. Linux: Tauri system packages (`libwebkit2gtk-4.1-dev` et al.) — build dependencies for the Rust layer only.
3. `pnpm/action-setup@v6` (pnpm 12.4.1) + `actions/setup-node@v7` (Node 22, pnpm cache against `ui/pnpm-lock.yaml`); `dtolnay/rust-toolchain@stable` + clippy; `Swatinem/rust-cache@v2`.
4. `node scripts/fetch-pdfium.mjs` — the sidecar is fetched, never committed (DR-50c).
5. `pnpm install --frozen-lockfile`.
6. **The layers, in order:** `cargo test` → `cargo clippy -- -D warnings` → `pnpm test` (vitest) → `pnpm run check` (svelte-check). Any red layer fails the pipeline.
7. Job wall-clock summary (every job, `if: always()`).

**`summary` job** (`needs: verify`, `if: always()`): reads the run's own job records through `gh api …/jobs` piped through `scripts/ci-summary.py` — per-job wall times plus the run's combined span, published to the step summary regardless of verdict (AC-158, D29's data source).

## Release pipeline — `.github/workflows/release.yml`

**Trigger:** `push` tags `v*`. **Version guard** (first step): the tag's version — everything before the first `-` — must equal `tauri.conf.json`'s version; a `-`-carrying tag is a pre-flight (allowed against the matching base version, e.g. `v0.0.10-preflight` proves the pipeline of a 0.0.10 tree without consuming a version).

**`build` job — matrix `include`** (one entry per OS, `fail-fast: false`): per-entry `fetch-pdfium` flags (`--universal` on macOS per DR-359 — the sidecar must match the universal bundle), `extra-rust-targets` (`x86_64-apple-darwin` on macOS), and `bundle-dir` — **macOS uploads from `src-tauri/target/universal-apple-darwin/release/bundle/`**: a `--target` build writes under the target name (DR-385). Pdfium fetch passes `secrets.GITHUB_TOKEN` (DR-384: the runners' shared unauthenticated IPs hit GitHub's 60/h limit).

**Build invocation** (the CLI from the repo root, like the harness): `tauri build --config '{"bundle":{"createUpdaterArtifacts":true,"targets":["nsis","msi","app","dmg","appimage","deb"]}}'` plus `--target universal-apple-darwin` on macOS. The config override is per-build only — `tauri.conf.json` stays as released (DR-358 b). The target union **includes `app`**: the macOS updater artifact (`Book.Keeper.app.tar.gz` + `.sig`) exists only because the app target is built (DR-385 — the shipped installers-only list has no darwin updater artifact). Signing: `TAURI_SIGNING_PRIVATE_KEY` from repository secrets (the keypair of DR-377, pubkey pinned in `tauri.conf.json`, id `E90D6DD0F1A54E35`); no password variable needed (verified with the CLI, DR-377).

**`publish` job:** downloads the three bundle artifacts into one directory, then:

1. **Flatten** (DR-387): `find` filtered to the artifact extensions (`exe msi dmg AppImage deb app.tar.gz sig`) into `tmp/release/flat/`, names normalized **spaces → dots** — GitHub's asset API rewrites spaces itself, so the flat names, the served asset names and the manifest URLs must agree on the dot form. The raw bundle tree (AppImage staging, deb intermediates — 269 files for 11 real artifacts in the fifth run) is never published.
2. **Manifest**: `node scripts/publish-release.mjs --dist tmp/release/flat --version <tag version, suffix stripped> --tag <tag> --repo <repository> --notes docs/release/notes/v<version>.md --out tmp/release/latest.json` — see below.
3. **Publish**: `softprops/action-gh-release@v3` with `files` = `latest.json` **first**, then `tmp/release/flat/*` (manifest first so a burst limit can never strand it); `draft: true` when the tag contains a `-` — a pre-flight release is invisible to `releases/latest`, which serves published releases only. That is what makes a pipeline pre-flight safe.

## The manifest — `scripts/publish-release.mjs`

- **Platform map** (the updater plugin's exact `{os}-{arch}` lookup, no universal fallback — DR-358 a): `windows-x86_64` → the NSIS setup exe (`*_x64-setup.exe`); `darwin-aarch64` **and** `darwin-x86_64` → the one universal artifact (`Book.Keeper.app.tar.gz` — no version, no arch in its name); `linux-x86_64` → the AppImage. The `.msi`, `.deb` and `.dmg` ship as installers; they are not updater entries.
- **Signatures:** each entry's `signature` field is the matching `.sig` file's text verbatim; a needed artifact with no `.sig` sibling → exit 1.
- **Version guard per entry:** versioned names must carry the manifest version (the darwin artifact carries none — checked by shape).
- **`notes`:** the first section body of the release notes page (`docs/release/notes/v<version>.md`) — the text the released app shows in the update notification.
- **URLs:** the tag-form download path (`releases/download/v<version>/<dot-name>`) — valid once the release is published.

## Product identity (FR-27, the rename)

`productName: "Book Keeper"`, identifier `com.bookkeeper.app` (data directory platform-resolved under that identifier — **no migration from the old `com.pdfkeeper.app` path**: the user's decision DR-381, no installed user base to migrate). The binary is `bookkeeper.exe` with `binaries/pdfium.dll` beside it; artifact names carry the product name (`Book.Keeper_0.0.11_x64-setup.exe`, `Book.Keeper.app.tar.gz`, … — served in the dot form). The updater endpoint is `https://github.com/dong-nz/book-keeper/releases/latest/download/latest.json` (repository renamed same-day, DR-375).

## Verified artifact shape (the sixth pre-flight run, DR-388)

12 assets: `latest.json` + `Book.Keeper_0.0.10_x64-setup.exe`+`.sig`, `…_x64_en-US.msi`+`.sig`, `Book.Keeper_0.0.10_universal.dmg`, `Book.Keeper.app.tar.gz`+`.sig`, `Book.Keeper_0.0.10_amd64.AppImage`+`.sig`, `Book.Keeper_0.0.10_amd64.deb`+`.sig`; manifest with four platform entries whose URLs match the served asset names verbatim. Full pipeline ~24.5 min.

## Test mapping (observation-type ACs — the pipeline runs are the tests)

- **AC-157:** a green CI run on a real push **and** a deliberately failing one (the red exercise: a throwaway failing Rust test on `master`, proving a red test layer turns the matrix red with `fail-fast: false`); run records cited in the Build ledger.
- **AC-158:** the `summary` job's per-job + combined wall report on those runs.
- **AC-159:** the release page's artifact list — met by the pre-flight (run 44250608428) and reproduced by every real tag.
- **AC-160:** the published manifest resolved by a running app — the manifest half via the published release; the app half is the user's installed-app observation (the fixture path `flowAppUpdate` remains the in-app behavior proof, unchanged since v0.0.10).
- **Local guards proven during build:** YAML validation of both workflow files; the version guard's four tag cases; the assembler dry-run against the *real* run-4/run-5 artifacts (4 entries, real signatures) with the negative case red; the tar invocation under GNU tar 1.35; `fetch-pdfium.mjs --universal`'s darwin-only guard.

## Invariants

- CI runs the same commands as the local Phase-C, minus the real-app layer, which stays local (DR-390) — CI defines no second verification language (RULES R6).
- A pre-flight never consumes a version and never reaches users (draft + invisible to `releases/latest`).
- The Release carries only the filtered artifacts; the manifest is always uploaded first.
- `tauri.conf.json` stays as released; every packaging behavior lives in the workflow's per-build override.

## Change history

- **2026-11-18 (v0.0.11):** First shipped. Built through six pre-flight runs, each defect recorded in SPEC-v0.0.11 (DR-383 tar drive-colon parsing; DR-383 `universal-apple-darwin` target name; DR-384 unauthenticated Pdfium-fetch rate limit; DR-385 per-OS `bundle-dir` + the missing `app` target; DR-386 the assembler's invented artifact names; DR-387 raw-tree publishing + GitHub's space→dot name rewriting) and proven green end to end by run 44250608428. The CI pipeline's own first observations (red exercise + green runs on `master`) are recorded in the Build ledger of SPEC-v0.0.11. The lane runner was removed from the CI workflow before the first green run — the user's decision, DR-390; the first Linux CI run then caught a latent cross-platform defect in the rename-rejection test (accepted case-only rename on case-sensitive filesystems), fixed test-side per DR-391.
