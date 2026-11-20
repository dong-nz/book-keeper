# SPEC-ALL-TESTS.md — Test Inventory (all layers)

**Living document, not a release spec.** It has no lifecycle status and is not a requirements source: requirements and acceptance criteria stay in the release specs (this file never restates them as definitions). It is the factual inventory of every test case the repository implements *today*, per `docs/specs/INDEX.md` → current spec → `AGENTS.md` §2a maintenance rules (DR-160).

Regenerated 2026-11-12 by extracting the sources directly: `#[test]` items under `src-tauri/src/`, `it()`/`test()` items under `ui/src/`, and every `check()`/`skip()` call site in `scripts/ui-smoke.mjs` and `scripts/ui-10k.mjs`. Re-extracted 2026-11-17 for the v0.0.10 test deltas (the `flowReaderHorizontal` flow, the `flowReaderToc` and `flowEpubReader` extensions, the `flowReaderSettings` flow, the store unit additions, the `flowAppUpdate` flow and its fixture-build harness wiring) and verified against the tree by count; re-verified the same day after the Phase-B repairs (the re-pinned `flowReaderToc` 34 / `flowEpubReader` 42 / `flowReaderSettings` 9 call sites, the revised row texts per DR-337/DR-338, the "separates visibility from the pin" store units — all matched, and the reading-pdfs + reading-epubs lanes run passed 233/233); re-extracted the same day after the second Phase-B round (the DR-341 decodable-overlay regression check in `flowReaderOpen`, the DR-342 docked-position checks in `flowReaderToc` and `flowEpubReader`, the DR-343 dialog-over-panel hit test in `flowReaderSettings` — the flows then carried 16 / 35 / 43 / 10 call sites; the lanes runs passed 237/237; the AC-67 delete-cache check gained the same bounded wait its close-path siblings use — a check-side fix, no new call site); the DR-344 extension adds three more `flowReaderSettings` call sites (the EPUB-host control and menu checks, the functional *Check for updates* proof in a reader window) — now 13; the final lanes runs passed 240/240. At the v0.0.10 Ship commit the extraction was re-run against the tree and the §3 counts corrected (the direct-run shape 601 check call sites, 5 conditional skips, 6 `timingRow()` call sites, 61 flows); the fixture endpoint port is the tree's 14299 (DR-331 item 5); re-extracted 2026-11-17 for the v0.0.11 test deltas (the `applog` module's 10 tests, the two settings-store log-level units, the `flowLogs` flow with its 11 check call sites) and verified against the tree by count; re-extracted the same day for the v0.0.11 verification-infrastructure deltas (DR-360–DR-362: the `searchFor`/`clearSelection` reworks, the fixture-appearance waits, the tie-group scoping, the flow-restore undos, the `flowVirtualization` geometry-derived bound, the `ui-10k.mjs` corpus publish/seed — none adds or removes a `check()`/`skip()` call site; the `flowStartup` seeded-corpus branch adds one `check()` and one `skip()` call site). Counts below are call sites in the current working tree; parameterized checks (width matrix, contrast matrix) run once per case, and a lane run additionally asserts its own suite-level checks — core lanes the same three the direct run asserts (AC-56 sleep scan, AC-58 wall time, AC-60 area coverage), docs lanes their own budget check — so a full run asserts more than the number of listed call sites (the per-lane pattern is exact: each lane reports its flow's call sites plus those suite-level checks, the width matrix doubling at its two tested widths). The v0.0.7 full-mode lane run (Phase C, DR-192) will record its lane tallies here when it completes; the previous full-mode reference is the v0.0.6 Ship run (44/44 lanes green, 474/474 check executions, combined wall 130.7 s — inside the NFR-06 300 s budget). Re-extracted 2026-11-18 for the v0.0.11 rename deltas (FR-27: the `relocate` module's 8 tests added; no test gained or lost a `check()`/`skip()` call site anywhere — the rename re-pinned existing strings in place: the About/updates flow assertions, the EPUB protocol base URLs, the harness exe/env/data-dir names); the rename-relevant feature lanes (`settings-and-appearance`, `keeping-up-to-date`, `reading-epubs`) ran 186/186 against the renamed binary; re-extracted the same day after the user's DR-381 (the data-directory migration removed: the `relocate` section and its 8 tests deleted, the count back to 142 in 12 files; no `check()`/`skip()` call site changed — the flowStrings were untouched by the migration's presence; `flowLogs` re-run 19/19 against the reduced binary on a fresh data directory). Re-extracted 2026-11-18 at the v0.0.11 Ship against the whole tree (§2a): the Rust and unit counts confirmed (142 in 12 files; 161 in 18 files), every §1/§2 section header and every §3 flow row matches the tree; the summary rows corrected — they had carried mid-build numbers since the v0.0.10 Ship. Measured against this tree: 630 flow-body `check()` call sites, 612 in the direct-run shape (611 asserted by a direct full run), three suite-level checks — no AC-49 "source scan" exists in the harness — three helper-internal `check()` calls (`fail()`, `timingRow()`, the error-toast guard), six `skip()` call sites, six `timingRow()` call sites; the 10k file's ten and the scale flows' 18 check + 3 `timingRow()` call sites confirmed. The whole delta since the v0.0.10 Ship tree is exactly `flowLogs` +11 and `flowStartup` +1 check +1 skip (DR-362) — every other flow's call sites unchanged. Re-extracted 2026-11-19 for the v0.0.12 test deltas (§8 ledger, SPEC-v0.0.12): Rust +31 — the new `positions` module (4), `db` +1 (the v3 migration), `library/service` +6, `pdf` +12 (bind candidates per D34, `page_text`/`serve_text`, `serve_thumbs`), `epubmeta` +2, `covers` +6 — 173 in 13 files; frontend +38 — `reader.test.ts` +23 (position grammar/restore/write-discipline, view-mode memory, text layer), `epubReader.test.ts` +9 (position memory incl. the DR-407 pre-update height judgment), the new `platform.test.ts` (6) — 199 call sites in 19 files; harness +39 check call sites — the new flows `flowReaderMemory` 13 / `flowReaderText` 6 / `flowCoverChoice` 20 (no new `skip()` or `timingRow()` call sites), plus the four new fixtures (`text-probe.pdf`, `image-only.pdf`, `dense-text.pdf`, `epub-images.epub`); three drifted rows corrected against the tree — the EPUB store's `estimates …` and `labels a chapter with the first entry …` renames and the fast-render-cap preview row re-pinned to its DR-316/DR-323 form. The three new lanes ran green 2026-11-19 — `flowReaderMemory` 17/17, `flowReaderText` 10/10, `flowCoverChoice` 24/24 check executions (13/6/20 call sites; the mode-loop and picker-window sites run per case); the frontend units ran 285/285 at run time. The Rust layer was re-run against this tree 2026-11-19: 172/173 green — the single failure is the wall-time-sensitive `import_of_ten_thousand_fixtures_stays_within_budget` (31.96 s against the 30 s budget under the full suite's parallel load; the isolated re-run passes in 2.16 s — flake class, the D29 measurement family; the layer was green at the commands-stage checkpoint). Re-verified 2026-11-19 after the DR-408 picker-toolbar revision: `flowCoverChoice` now carries 21 call sites (the toolbar-shape check added; the reset-control check re-anchored from the dialog's bottom action row to the toolbar bar, its row text extended to name the toolbar); the flow-body total 670, the direct-run shape 652 (651 asserted by a direct full run); no other flow changed. `flowCoverChoice` re-run isolated after the revision: 25/25 check executions (3 950 ms). A full-mode run (65 lanes, 957/964, wall 420 s) taken during this verification found five released lanes red — triage and lane corrections in SPEC-v0.0.12 DR-409: the AC-80/AC-142 new-window mode-default rows re-pinned to the FR-29 contract (`flowReaderViewModes` 43/43, `flowReaderHorizontal` 31/31 isolated), the AC-149 chrome-order rows extended for the cover control (`flowReaderSettings` 17/17 isolated), `flowEpubReader` 47/47 isolated (its AC-137 image row is load-bound), `flowReaderPerformance` 12/12 isolated after the flow states the D24 row's position (no new call sites there). Re-verified 2026-11-19 after the DR-410 pinned-toolbar + column-fit revision: `flowCoverChoice` now carries 23 call sites (the pinned-bar and column-fit checks added); the flow-body total 672, the direct-run shape 654 (653 asserted by a direct full run); no other flow changed. `flowCoverChoice` re-run isolated: 27/27 check executions (the pinned-bar check drives the scroll itself — the flow's own `scrollIntoView` had nothing to scroll while the first window still fit the clamped dialog); `flowReaderSettings` re-run 17/17 — the no-actions branch of `Modal.svelte` renders exactly as before. Re-verified 2026-11-19 after the DR-411 display-order + full-bleed revision: `flowCoverChoice` now carries 24 call sites (the full-bleed check added; the column-fit probe gained per-condition diagnostics and a subpixel tolerance, the lazy-window checks re-anchored — no call-site change there); the flow-body total 673, the direct-run shape 655 (654 asserted by a direct full run); no other flow changed. The Rust layer carries the same 173 tests with one rename — `epubmeta::tests::cover_candidates_follow_the_books_display_order` replaces `cover_candidates_list_manifest_raster_images_in_manifest_order` (its fixture extended to a two-document spine with display order distinct from the manifest order), and the service unit `cover_candidates_lists_the_epub_manifest_images_and_refuses_a_pdf` re-pinned to the regenerated `epub-images.epub` fixture's display order — 173/173 green, clippy `-D warnings` clean. `flowCoverChoice` re-run isolated after the revision: 28/28 check executions (the first isolated run opened the strip with 12 tiles, an earlier set run with 24 — the bounded-window claim holds for both). Re-verified 2026-11-19 after the DR-412 corner-clearance fix: no call-site change (the full-bleed check re-anchored in place — sides flush, bottom inset measured); `flowCoverChoice` re-run isolated: 28/28 check executions. Re-verified the same day after the DR-413 Tags-manager scrollbar shape: `flowTagCrud` gained one check (the list panel reaches the dialog's side edges, its bottom clear of the rounded corners — 6 → 7 call sites in that flow); the flow-body total 674, the direct-run shape 656 (655 asserted by a direct full run); `flowTagCrud` isolated 14/14, the tags-area set re-run 45/45 green; no other flow changed. Re-verified the same day after the DR-414 toolbar-dialog + delete-all revision: `flowTagCrud` now carries 11 call sites (7 → 11 — the DR-413 panel-geometry check replaced by the toolbar-structure and variant-geometry pair, three delete-all-flow checks added); Rust +1 — `library/service`'s `tags_delete_all_detaches_every_membership_and_keeps_documents` — 174 in 13 files; `flowScrollbars`'s tags-manager surface measurement re-anchored to the shared variant's `.modal-scroll` (no call-site change). The flow-body total 678, the direct-run shape 660 (659 asserted by a direct full run). The tags-area set 49/49 green 2026-11-19 (57–60 s), `flowScrollbars` isolated 14/14 (the tags manager content area measured at the same 10 px gutter and token color as the released surfaces); frontend units 285/285, `svelte-check` 0 errors. Re-verified the same day after the DR-415 top-clearance fix: no call-site change (the `flowTagCrud` variant-geometry check extended in place with the input-top clearance assertion); `flowTagCrud` isolated 18/18 (measured clearance 4 px), `flowCoverChoice` — the variant's other consumer — 28/28, `flowScrollbars` 14/14; the tags-area set ran 45/47 with two Browse-menu open crashes (`flowTagCrud` at a menu item, `flowQuietCounts` at the *Manage tags* target — the D30/D31 flake class; both re-ran isolated green, 18/18 and 8/8), no flow's call sites changed.

|Layer|Runner|Test call sites|
|---|---|---|
|Rust core|`cargo test` (in `src-tauri/`)|174 `#[test]` in 13 files|
|Frontend units|`pnpm -C ui test` (Vitest)|199 test call sites in 19 files (the design-tokens checks iterate the role-pair and token lists: vitest reports **285** test cases at run time - 91 from those 5 call sites; every other file runs exactly its call-site count)|
|UI smoke harness|`node scripts/ui-smoke.mjs`|65 flows (62 run directly - 60 internal + the two exported prefix flows; the three exported scale flows are executed only by the 10k suite and its lanes), 678 check call sites in flow bodies - 660 belong to the direct-run shape; plus 3 suite-level checks (AC-56 sleep scan, AC-58 wall time, AC-60 area coverage) and 3 `check()` calls inside the shared helpers - the `fail()` recorder, `timingRow()`'s budget assertion and the error-toast guard; plus 6 conditional `skip()` call sites and the AC-72 timing rows, asserted through the shared `timingRow()` helper (6 call sites). A direct full run asserts exactly 659 of the direct-run-shape sites - flowStartup's seeded-corpus branch (one check and one skip, DR-362) executes only in a seeded scale lane; under `scripts/ui-verify.mjs` full mode each core lane additionally asserts the three suite-level checks, which is why the full lane run reports more executions - see §3|
|10 000-document suite|`node scripts/ui-10k.mjs`|1 flow (8 check call sites) + 2 suite-level checks (abort guard, NFR-06 budget) = 10 in the file; the suite additionally runs the shared prefix (flowStartup — 3 check and 1 skip call sites since DR-362 — and flowDropAndSkip — 2 check call sites) and the three exported scale flows (18 check and 3 `timingRow()` call sites); the width matrix runs its four checks per tested window width; a seeded scale lane (DR-362) asserts the prefix's checks and, only when the empty-state variant is absent at its seeded startup, its one skip|
|Full-verification orchestrator|`node scripts/ui-verify.mjs`|1 scheduler gate (every lane green + combined wall ≤ 300 s; with `SMOKE_VERIFY_WALL_GATE=0` the wall is measured and reported but does not gate the exit — DR-357, introduced for the CI lane job, which the user removed from CI in the same release, DR-390; the switch stays documented for local measured runs) — implemented as its own exit gate, not a `check()` call site, so it is not in the counts above; §3|

The authoritative **requirement → test** mapping stays in each release spec (acceptance-criteria index + Build ledger). This document is the other direction: every test, and what it asserts. The `covers` columns list only requirement IDs the test source itself names; `—` means the test text carries no ID, not that none exists.

---

## 1. Rust unit tests — `cargo test`

Run: `cargo test` in `src-tauri/` (no UI, no network; per RULES R1.1 every product behavior is testable here). Clippy must stay clean (`cargo clippy -- -D warnings`).

### `src-tauri/src/db/mod.rs` — schema & migration (7)

|Test|Verifies|
|---|---|
|`fresh_database_migrates_to_current_version`|a new database lands on the current `user_version` schema, created clean: the tag tables exist and no category-named table is ever created (DR-273)|
|`migration_is_idempotent_and_keeps_data`|re-running migrations preserves rows (additive migration rule)|
|`foreign_keys_are_enforced`|the `PRAGMA foreign_keys` contract behind tag assignment|
|`v2_upgrade_rebuilds_the_tag_tables_and_preserves_rows`|the AC-113 upgrade half: a pre-upgrade (version-1) database keeps its tag groups, memberships, assignments, settings and filter rows through the rebuild onto the clean tag tables (rows copied, ids preserved, old tables dropped), and gains the `reading` and `password` columns|
|`fts_triggers_keep_index_in_sync_with_every_mutation`|FTS5 index follows insert/update/delete|
|`settings_round_trip_and_overwrite`|settings table write/read/overwrite|
|`v3_migration_adds_cover_choice_to_an_existing_database`|the SPEC-v0.0.12 §3.10 migration half: the additive `cover_choice` column lands on an existing version-2 database with rows kept, and every existing choice is NULL (automatic — the released behavior)|

### `src-tauri/src/data_dir.rs` — data-directory resolution (3, DR-162)

|Test|Verifies|
|---|---|
|`data_dir_override_absolute_is_used`|an absolute `BOOKKEEPER_DATA_DIR` replaces the app resolution (the parallel-suite mechanism)|
|`data_dir_without_override_keeps_the_app_resolution`|unset or empty means the released `app_data_dir()` resolution, unchanged|
|`data_dir_override_must_be_absolute`|a relative override is a typed validation error naming the variable and the rejected value|

### `src-tauri/src/applog/mod.rs` — application logging (10, FR-26, SPEC-v0.0.11 §3.2)

|Test|Verifies|covers|
|---|---|---|
|`error_level_writes_and_info_is_suppressed_by_default`|the default level records Error lines only; an Info line never lands|AC-154|
|`info_level_adds_info_lines_and_keeps_error_lines`|Info level adds Info lines and keeps Error lines; switching back to Error suppresses lifecycle again while failures keep landing|AC-155|
|`line_format_is_timestamp_level_subsystem_message`|the exact four-field line shape `YYYY-MM-DDTHH:MM:SSZ LEVEL subsystem: message`|AC-154|
|`messages_are_collapsed_to_single_lines`|newlines and tabs inside a message are collapsed — one physical line per event|—|
|`cap_rotates_to_a_single_backup_and_keeps_two_files_maximum`|the 2 MiB cap rotates on the next write, moves the full history to `.1`, and a second crossing replaces the backup — two files maximum|AC-155|
|`a_directory_that_cannot_be_written_never_fails_a_product_call`|an unwritable logs directory: init and every write fail at the OS level, nothing panics, nothing escapes; `ensure_file` reports the IO failure, the path is still reported|—|
|`ensure_file_creates_the_log_on_a_clean_install`|the open-log-file action creates the empty file on a clean install|—|
|`level_set_accepts_only_the_two_grammar_values`|`set_level` rejects anything but `error`/`info` as a typed validation error|AC-155|
|`timestamps_match_the_utc_calendar_at_known_epochs`|the civil-time formatter at known epochs, the leap day, and the century rule|—|
|`the_bridge_forwards_plugin_error_records_into_the_log_file`|the DR-352 `log`-facade bridge forwards the updater plugin's Error records; Debug records do not cross|AC-154|

### `src-tauri/src/lib.rs` — startup gate & command contract (4)

|Test|Verifies|covers|
|---|---|---|
|`ui_gate_releases_a_waiter_when_marked`|startup task publishes counts only after the first command proves the UI bridge|FR-15|
|`ui_gate_wait_returns_false_on_timeout`|the gate cannot hang a waiter forever|—|
|`only_command_calls_mark_the_ui_gate`|nothing but a real command invocation opens the gate|—|
|`command_router_matches_the_ui_command_client`|the invoke handler and `ui/src/api.ts` expose exactly the same command list (spec §5.3.3 — the whitelist cannot drift on either side)|—|

### `src-tauri/src/library/mod.rs` — import primitives (4)

|Test|Verifies|
|---|---|
|`normalize_path_is_absolute_and_lexically_clean`|stored paths are absolute, `.`/`..` resolved|
|`normalize_path_keeps_absolute_input_unchanged_apart_from_redundant_parts`|normalization never rewrites a clean absolute path|
|`pdf_detection_is_extension_based_and_case_insensitive`|`.pdf`/`.PDF` accepted, other extensions rejected|
|`fallback_title_strips_the_extension`|filename fallback title drops the extension|

### `src-tauri/src/library/query.rs` — search & ordering (16)

|Test|Verifies|covers|
|---|---|---|
|`compile_match_builds_quoted_prefix_tokens`|each token becomes a quoted FTS prefix term|—|
|`prefix_search_hits_all_four_fields`|title/author/filename/notes all match (FR-03 four-field search)|—|
|`filters_compose_with_and_semantics`|mode + filters combine as AND|—|
|`several_active_tags_select_the_union`|multi-tag filter = union|—|
|`reading_view_selects_exactly_the_members`|the `reading_only` predicate returns exactly the marked documents|FR-19|
|`favorites_are_pinned_under_every_sort_key_and_direction`|favorites lead the list regardless of sort|—|
|`sort_whitelist_maps_known_keys_and_defaults_to_title_asc`|only whitelisted sort keys, default title A→Z|UI-04|
|`secondary_whitelist_maps_keys_and_treats_none_as_unset`|secondary-key whitelist; `none` clears it|FR-13|
|`order_clause_with_no_secondary_is_the_shipped_clause`|no secondary key ⇒ exactly the pre-v0.0.5 ORDER BY|—|
|`secondary_key_resolves_primary_ties_deterministically`|primary → secondary → id total order|—|
|`slices_partition_the_full_ordering_without_gaps_or_overlap`|paged slices reassemble the full ordered set|—|
|`malformed_fts_query_triggers_the_like_fallback`|user text that breaks FTS syntax falls back to LIKE|—|
|`like_fallback_escapes_wildcards_so_tokens_stay_literal`|`%`/`_` stay literal in the fallback|—|
|`ids_query_returns_the_full_matching_set_in_list_order`|the id query (Select-all-matching) matches the visible order|—|
|`tag_names_travel_with_rows_for_badges`|rows carry their tag names for the UI|—|
|`cover_path_is_delivered_only_when_the_cache_file_exists`|an unprepared row carries no cover path; the path appears with the cached file (DR-237)|AC-100|

### `src-tauri/src/library/service.rs` — library operations (39)

|Test|Verifies|covers|
|---|---|---|
|`import_extracts_info_titles_and_falls_back_to_filename`|Info-dictionary title, filename fallback|FR-05|
|`import_accepts_epub_documents_and_uses_the_package_metadata`|the same pipeline takes `.epub`: title/author from the OPF; the malformed fixture is refused with the typed error and nothing is stored; a no-metadata package falls back to the filename title|AC-126, AC-139…AC-141|
|`folder_walk_collects_both_document_types`|the folder walk collects `.pdf` and `.epub` at any depth and nothing else|AC-126|
|`import_skips_duplicates_and_auto_assigns_the_selected_tag`|duplicate skip + assignment on import|FR-01|
|`import_of_ten_thousand_fixtures_stays_within_budget`|10 000-file import under the time budget — the corpus is the AC-131 mix (9 000 PDFs + 1 000 EPUBs), title probes for one document of each type|AC-01, NFR-02, AC-131|
|`search_of_ten_thousand_documents_stays_within_budget`|search latency at 10 000 documents|AC-02, AC-01|
|`tags_membership_bulk_edit_and_delete_cascade`|create/assign/bulk-edit/delete with document detach|AC-03, FR-02|
|`tags_delete_all_detaches_every_membership_and_keeps_documents`|the bulk delete reports the count, removes every tag row, cascades the memberships away, leaves the documents and their counts untouched, and an empty library answers 0|FR-32, AC-182|
|`reading_membership_single_and_bulk_with_view_and_counts`|the Reading toggle per document and across a selection; the Reading view lists exactly the members; the status counts follow every change|AC-121, AC-103, FR-19|
|`reading_view_combines_with_search_favorites_tags_and_missing`|the Reading predicate composes with every other filter under the same AND rule (spec §3.2)|AC-103|
|`opening_relinking_and_rescan_never_change_reading_membership`|the manual-only rule: rescan, relink and an edited record leave Reading membership exactly as it was; the badge count still reports the member|AC-102, FR-19|
|`stored_password_round_trips_and_requeues_the_cover`|store, overwrite, clear a document's password; every non-empty store re-enqueues the cover with the new password; clearing re-renders nothing|AC-105, FR-20|
|`rename_moves_the_file_and_the_record_and_preserves_identity`|file and record move together; tags, favorite, notes, Reading membership and the stored password belong to the row, not the name; relink and missing detection then work against the new path|AC-107, AC-109, FR-21|
|`a_user_title_survives_the_rename_an_auto_title_re_derives`|`title_source = 'user'` is unchanged by a rename; the extracted title follows the new filename through the same gate the released relink uses|AC-109|
|`rename_rejects_invalid_and_colliding_names_and_changes_nothing`|typed rejections (invalid name, collision with an existing document) happen before any file or row moves — nothing moved, nothing enqueued; the case-only rename asserts each filesystem's documented behavior: `name_conflict` on case-insensitive, an ordinary accepted rename on case-sensitive (file follows, row follows, auto title re-derives, exactly one cover job) — ordering made it Linux-red until DR-391|AC-108, FR-21|
|`rename_between_supported_extensions_is_accepted_and_re_derives`|a rename between supported extensions (`.pdf` ↔ `.epub`) is accepted: the file and record move together and the extracted title re-derives through the same gate (FR-21 MODIFIED)|AC-124|
|`rename_to_an_unsupported_extension_is_rejected_and_changes_nothing`|a target extension outside the supported pair is refused with the typed error before anything moves|AC-124|
|`library_capabilities_work_on_an_epub_document`|details/notes, favorite, Reading membership and rename behave on an EPUB record exactly as on a PDF one|AC-124|
|`delete_library_only_keeps_the_file_delete_with_file_removes_it_and_its_cover`|both delete modes, cover cleanup on file delete|AC-04, AC-62, FR-09, FR-11|
|`rescan_marks_missing_and_relink_restores_the_entry`|rescan flags missing, relink restores|AC-05|
|`relink_is_type_matched`|the relink candidate must carry the document's own extension — a PDF row is not relinked to an EPUB file and the other way round|AC-128|
|`rescan_restores_entries_when_files_come_back`|reappearing files clear the flag|—|
|`title_source_transitions_gate_relink_re_derivation`|a manual title survives relink; an auto title re-derives|—|
|`update_rejects_empty_title_at_the_service_boundary`|empty title never reaches the database|—|
|`every_mutation_emits_matching_status_counts`|status events after each mutation match the resulting counts|—|
|`import_progress_events_cover_the_whole_batch`|progress events span the whole import|—|
|`import_pipeline_events_cover_every_chunk_and_fire_outside_the_lock`|the progress bar appears before any file is processed, advances in steps of at most `CHUNK` (twelve full chunks of 5 for 60 fixtures — the bound revised from 25 by DR-279), covers the whole import, and is emitted while the connection is free — never from inside a lock hold (AC-101 pipeline audit)|AC-101, AC-123, FR-11|
|`settings_carry_secondary_sort_and_grid_override`|secondary sort + manual column count persist|FR-13, UI-08|
|`epub_text_size_accepts_only_the_ladder_steps`|the reading text-size setting stores only the ladder's fixed steps|AC-135|
|`theme_defaults_to_system_and_accepts_only_the_three_states`|theme setting values `system/light/dark` only|UI-17|
|`last_filter_grammar_accepts_the_new_form_and_the_legacy_one`|the persisted filter grammar: the modes including `reading` and the `&tags=` list form, and the legacy `&cats=`/`cat:` forms all decode; malformed values are rejected|AC-113|
|`log_level_grammar_and_reopen_survival`|the log level persists through the settings table and survives a reopen; no row is the default `error`, the grammar is exactly the two values, anything else is a typed rejection|FR-26, AC-155|
|`import_assigns_every_active_tag`|import assigns to every active tag|FR-10|
|`cover_choice_set_and_clear_store_the_value_and_enqueue_the_render`|a valid choice is stored verbatim and the enqueued cover job carries the parsed choice; clearing NULLs the column and re-enqueues the automatic cover; both return the updated row|FR-31, AC-177, AC-180|
|`cover_choice_rejects_bad_grammar_and_type_mismatch_before_storage`|malformed JSON, a bad grammar, and a kind that does not match the row's file type are the typed rejection at the service boundary — the stored value never changes|FR-31, AC-179|
|`cover_candidates_lists_the_epub_manifest_images_and_refuses_a_pdf`|the picker grid's list: every manifest raster the pipeline can decode, in the book's display order (DR-411: the fixture's chapter order, the unreferenced declared cover last); a PDF is the typed refusal (its picker is the thumbnail strip)|FR-31, AC-178|
|`view_mode_setting_round_trips_with_the_four_modes`|the released settings pair serves FR-29's key unchanged: the default is `continuous`, the four mode values round-trip, anything else (`bogus`, empty, wrong case, an old name) is the typed validation rejection — no new command exists|FR-29, AC-169|
|`position_rows_do_not_bloat_the_settings_dto`|reader positions are not settings: the settings DTO carries only the released keys (`reader.pos.*` rows are excluded from it); the position pair is the dedicated command path|FR-28|
|`delete_documents_removes_position_memory_relink_keeps_it`|delete removes the deleted ids' position keys in the same transaction (R2); relink keeps the id, so the position memory survives it|FR-28, AC-168, FR-14|

### `src-tauri/src/positions/mod.rs` — reader position storage (4, FR-28, SPEC-v0.0.12 §3.2)

|Test|Verifies|covers|
|---|---|---|
|`position_round_trip_get_set`|a written position is exactly what `get` returns (last value wins); an unwritten one is `None`|AC-167|
|`malformed_values_are_rejected_at_the_boundary`|the §3.2 grammar: malformed JSON, out-of-range bounds, both anchors, a 0 anchor, extra members — none reaches storage|FR-28, AC-167|
|`unknown_id_is_rejected`|`set` and `get` for a document id that does not exist are the typed `DocumentNotFound`|FR-28|
|`cleanup_removes_only_the_named_documents_keys`|the delete-cleanup pass removes exactly the named ids' `reader.pos.<id>` keys; another document's memory is untouched|FR-28, AC-168|

### `src-tauri/src/pdf/mod.rs` — reader engine (23)

|Test|Verifies|covers|
|---|---|---|
|`the_queue_takes_interactive_requests_ahead_of_cover_jobs`|reader render jobs outrank cover jobs in the pdfium queue|—|
|`reader_cache_paths_address_one_directory_per_document`|per-document cache directory layout|—|
|`reader_cache_cleanup_removes_the_document_directory_and_tolerates_absence`|close-time cleanup is complete and idempotent|AC-62, AC-67|
|`document_info_reports_the_fixture_page_count_and_sizes`|page count + page sizes from the document|AC-61|
|`a_protected_document_prompts_and_unlocks_with_the_right_password`|a protected document is `PasswordRequired` on every request path with no or wrong password; the stored password opens it — the fixture is genuinely RC4-encrypted with the user password "sesame"|AC-104, FR-20|
|`page_rendering_honours_the_scale_bounds_and_page_range`|render scale clamped, requested range respected|AC-61, AC-62|
|`rendered_pages_are_distinguishable_by_the_drawn_page_number`|fixtures render distinct pixels per page|AC-61|
|`the_scale_fixture_serves_its_last_page_like_its_first`|the 1 000-page scale fixture reports 1 000 pages and its last page renders|AC-65|
|`serve_page_writes_the_cache_once_and_serves_the_cached_file_after`|cache written once, served on re-request|—|
|`the_chosen_encoder_writes_lossless_pixels_at_the_unchanged_cache_key`|the chosen encoder (DR-196) round-trips through the `image` crate with identical dimensions and identical pixels, at the unchanged cache path|AC-74|
|`the_reader_helpers_report_an_unopenable_document_as_a_typed_error`|non-PDF/corrupt file → typed `AppError`, not a panic|AC-69|
|`bind_candidates_reach_the_packaged_macos_and_deb_layouts`|the pure candidate function (D34 fix) finds the library in the published layouts: the macOS `.app` (`Contents/Resources/binaries` relative to the binary at `Contents/MacOS`) and the deb (`/usr/lib/Book Keeper/binaries` relative to `/usr/bin`) — the injected extra directories are what the packaged search path gains|FR-25, D34, AC-175|
|`bind_candidates_keep_the_released_dev_and_next_to_binary_paths`|the source-tree and Windows/AppImage search order is untouched: exe directory → `binaries/` beside the exe → injected directories → `binaries/` in the working directory; unknown process paths contribute nothing, as today's `current_exe`/`current_dir` failures|D34|
|`page_text_returns_top_origin_runs_for_a_text_bearing_page`|the probe fixture's known sentence (drawn bottom-origin at (72, 700) on a 612 × 792 page) comes back as top-origin page-point runs — the y sits near the page top, the coordinate flip is the whole point of the proof|FR-30, AC-169|
|`a_page_without_text_extracts_nothing_and_is_not_an_error`|the image-only fixture (a real image XObject, no text operators) yields an empty run list — a scan gets no layer, not a failure|FR-30, AC-171|
|`a_dense_page_is_capped_at_four_thousand_runs_with_the_flag_set`|the dense fixture's 4 010 runs serve as 4 000 with the `truncated` flag raised (§3.3 cap)|FR-30|
|`page_text_follows_the_released_password_path`|the protected fixture answers `PasswordRequired` with no password (the reader's existing unlock form applies) and extracts with the stored one — no new prompt|FR-30, FR-20|
|`serve_text_caches_the_extraction_and_serves_the_cached_page_after`|the page cache's serving rule mirrored for text: a miss extracts and writes, a hit serves from the file with no re-extraction (the sentinel method); a cache file that no longer parses counts as a miss|FR-30|
|`serve_text_maps_the_password_state_to_the_released_typed_error`|the command-side classification is the released one (AC-104's mapping): `serve_text` hands the password state through unchanged|FR-30, AC-104|
|`serve_thumbs_renders_the_window_at_picker_width_and_caches_it`|a window of pages comes back as file paths in page order under the reader directory's `picker/` subdirectory (DR-404) at the picker width; a second request is served from the cache (the sentinel method)|FR-31, AC-181|
|`serve_thumbs_rejects_malformed_windows_before_rendering_anything`|a malformed window is refused with the existing validation kind (the `app_pdf_page` bounds pattern) — no rendering work done|FR-31|
|`serve_thumbs_fails_a_window_past_the_document_with_the_released_error`|a page the document cannot answer fails the window with the released typed error (the picker asks only within the manifest it already has; same classification as page serving)|FR-31|
|`serve_thumbs_honours_the_released_password_path`|the batch carries the stored password like every other request: no password is the typed prompt state, the right password renders|FR-31, FR-20|

### `src-tauri/src/pdfmeta/mod.rs` — metadata extraction and outlines (6)

|Test|Verifies|covers|
|---|---|---|
|`info_dictionary_hit`|Title/Author/Subject/Keywords read from the Info dictionary|—|
|`encrypted_malformed_and_info_less_files_yield_nothing`|no metadata invented for unreadable files|FR-05|
|`outline_returns_the_manifest_tree_for_multi_192`|the outline source yields exactly the manifest tree — titles, 1-based pages, depth — derived from `fixtures/multi/manifest.json`, not hard-coded|AC-77|
|`documents_without_an_outline_yield_the_empty_list`|no outline is an empty list, never an error (outline-less multi-tier file, Info-less fixture)|AC-77, UI-33|
|`an_unparseable_file_yields_the_typed_error`|a file that cannot be parsed at all → `PdfUnopenable`, the same classification the reader paths give|AC-77|
|`unresolvable_destinations_are_skipped_while_siblings_remain`|a dangling page reference and a named destination absent from the names tree cost their own entries only|AC-77|

### `src-tauri/src/epubmeta/mod.rs` — EPUB package reading (23)

|Test|Verifies|covers|
|---|---|---|
|`extracts_title_author_and_cover_from_the_opf`|title, author and the cover reference read from the OPF package|AC-139|
|`extracts_metadata_from_a_stored_only_archive`|the same reading works on a stored-only (uncompressed) EPUB|AC-139|
|`no_metadata_yields_no_fields`|a package without metadata yields no fields, nothing invented|AC-140|
|`missing_cover_declaration_yields_no_cover`|no cover property: no cover reference|AC-140|
|`declared_svg_cover_is_a_reference_not_bytes`|an SVG cover stays a reference — no raster bytes exist at import|AC-127|
|`malformed_package_is_a_typed_error`|a damaged OPF is the typed error, never a panic or a silent empty record|AC-141|
|`mimetype_rules_reject_a_zip_that_is_not_an_epub`|the mimetype rules (first entry, uncompressed, exact value) decide what an EPUB is|AC-126|
|`missing_container_or_rootfile_is_a_typed_error`|no container.xml or no rootfile pointer: typed error|AC-141|
|`href_resolution_rules`|relative href resolution — `../`, `./`, percent-encoded segments, archive-root forms|AC-126|
|`cover_href_resolves_through_the_archive_namespace`|the cover href resolves inside the archive's own directory tree|AC-127|
|`cover_pointing_at_a_missing_entry_yields_no_cover`|a cover reference to an absent entry is no cover, not an error|AC-127|
|`oversized_metadata_entry_is_refused`|the entry-size ceiling bounds what metadata reading opens|AC-139|
|`extract_never_opens_oversized_chapter_entries`|metadata extraction never touches the chapter entries it does not need|AC-139|
|`whitespace_only_title_is_no_title`|a blank declared title is no title — the filename fallback applies|AC-140|
|`read_package_returns_the_spine_in_reading_order`|`read_package` yields the spine in the package's reading order|AC-132|
|`spine_sizes_are_uncompressed_and_missing_entries_read_as_zero`|the position model's sizes are uncompressed entry sizes; an absent entry reads as zero, never breaks the model|AC-135|
|`read_package_builds_the_toc_from_the_nav_document`|the ToC tree from the EPUB3 nav document|AC-132|
|`read_package_falls_back_to_the_ncx_navmap`|the NCX navmap yields the same tree shape|AC-132|
|`read_package_derives_a_toc_from_the_spine_when_no_toc_exists`|no ToC in the package: the spine-derived list|AC-132|
|`read_package_reads_declared_language_and_direction`|the package's declared language and writing direction reach the manifest|AC-132|
|`read_package_rejects_a_malformed_package_like_import`|the reader path gives the same typed error the import path gives|AC-133|
|`cover_image_at_decodes_a_declared_raster_entry_and_refuses_the_rest`|§3.8: a chosen entry renders iff the manifest declares it with a raster media type (the released decode rules); everything else — absent, SVG-only, non-raster — is `None`, the released glyph fallback for a stale choice|FR-31, AC-178|
|`cover_candidates_follow_the_books_display_order`|`app_cover_candidates`'s enumeration (DR-411): spine-item order, then the order inside each spine document, first mention wins; declared raster images no spine document references follow in manifest document order; the two raster types only, resolved entry paths (the same container-root convention as the spine), a duplicate declaration listed once|FR-31, AC-178|

### `src-tauri/src/epubsrv/mod.rs` — EPUB content delivery (19)

|Test|Verifies|covers|
|---|---|---|
|`parse_request_accepts_all_url_forms`|the protocol request line parses every form the platform hands over — including the Windows `bookkeeper-epub://localhost/<id>/…` shape (DR-305) and the custom-protocol forms|AC-136|
|`parse_request_normalizes_and_refuses_hostile_paths`|percent-decoding, `..`/`.` normalization, hostile paths refused before any file is opened|AC-136|
|`base_url_follows_the_platform_branch`|the manifest's base URL carries the platform's own protocol form|AC-136|
|`encoding_helpers`|percent-encode/decode round-trips used by the URL rewrite|—|
|`base64_and_data_uri_decoding`|`data:` URI decoding to bytes|AC-137|
|`sanitizer_removes_executable_content`|`<script>`, `on*` handlers, `<iframe>`, `javascript:` hrefs — all removed|AC-137|
|`sanitizer_rewrites_package_references_to_protocol_urls`|src/href/srcset point into the content protocol|AC-136|
|`sanitizer_rewrites_data_uris_to_synthetic_entries`|a `data:` URI becomes a synthetic entry served through the protocol|AC-137|
|`sanitizer_is_best_effort_on_unparseable_documents`|a document the parser cannot read is served sanitized-as-possible, never refused|AC-137|
|`serve_returns_the_spine_chapter_sanitized_as_text_html`|a spine chapter is served as `text/html`, sanitized|AC-136|
|`synthetic_entries_re_derive_after_eviction`|a evicted synthetic entry re-derives from the package, not from disk|AC-137|
|`serve_answers_assets_with_declared_types_and_raw_bytes`|images and fonts arrive with the media type the package declares, bytes unmodified|AC-136|
|`serve_rewrites_css_url_tokens_to_protocol_urls`|`url()` tokens in delivered CSS point into the protocol — no remote host is ever addressed|AC-137|
|`serve_sanitizes_svg_entries`|SVG entries are sanitized on delivery|AC-137|
|`typed_errors_before_and_around_the_file`|typed errors precede any file read; every one answers as protocol 404|AC-133|
|`absent_entries_and_unreadable_packages_are_typed_errors`|a missing entry and an unreadable package are the same classification|AC-133|
|`the_lru_caches_output_and_invalidates_on_mtime`|the serving cache holds output per document, invalidated when the file's mtime moves|AC-138|
|`the_lru_holds_its_caps`|the cache's entry and byte ceilings hold|AC-138|
|`manifest_composes_the_reading_facts_with_the_base_url`|the `app_epub_manifest` payload: spine, sizes, ToC, language/direction, base URL|AC-125, AC-132|

### `src-tauri/src/covers/mod.rs` — cover pipeline (16)

|Test|Verifies|covers|
|---|---|---|
|`cache_key_changes_with_any_part_of_the_document_identity`|path/mtime/size each change the cache key|FR-16, AC-36|
|`enqueue_missing_covers_queues_only_documents_without_a_cache_file`|backfill skips covered documents|—|
|`sweep_orphans_removes_pngs_no_document_owns`|orphaned cache files deleted|—|
|`worker_renders_valid_pdfs_and_leaves_glyph_fallback_for_unrenderable`|real PNGs for valid files, fallback for broken ones|AC-06, AC-36|
|`epub_cover_renders_through_the_pipeline_by_contain_fit`|the declared EPUB cover goes through the real worker and lands under the document's cache key, drawn contain-fit inside the cover box|AC-127|
|`epub_without_a_usable_declared_cover_leaves_the_glyph_fallback`|no usable declared cover (absent, SVG-only, missing entry): the glyph fallback, no broken image|AC-127|
|`a_protected_document_gets_no_cover_until_its_password_is_stored`|the cover of a protected document stays absent until its password is stored through the service (the reader's unlock path); it is then re-enqueued and the real image appears under the unchanged cache key|AC-106, FR-20|
|`a_renamed_document_renders_under_the_new_key_and_the_stale_entry_is_swept`|the real rename path through the real cover pool: the cover follows the new identity (the key composes from the new path) and the stale entry is removed by the released orphan sweep|AC-110, FR-21|
|`cover_pool_size_clamps_the_override_and_bounds_the_default`|pool-size setting clamped to its allowed range|—|
|`worker_pool_renders_every_cover_exactly_once_across_two_workers`|parallel workers: no duplicate, no missed cover|AC-57|
|`no_choice_keys_are_byte_identical_to_the_released_function`|with no choice the cache-key function is byte-identical to the released one (test-pinned literals): nothing is hashed, nothing re-keys, untouched documents never re-render|FR-16, AC-180|
|`cover_choice_grammar_is_validated_at_the_boundary`|kind/file-type match, malformed JSON and malformed page numbers are rejected at the command boundary; an out-of-range page number is grammatical — its fallback is the render's job (DR-404)|FR-31, AC-179|
|`enqueue_missing_covers_reads_the_choice_column_and_checks_the_tagged_key`|the expected key is the choice-tagged one: a document whose chosen cover is missing is re-enqueued even when its superseded automatic file still sits in the cache, and the job carries the parsed choice to the render stage|FR-31, AC-177|
|`sweep_keeps_the_chosen_cover_and_sweeps_the_superseded_automatic_one`|a row's expected key is its choice-tagged one: the chosen cover survives the startup sweep, the superseded automatic file is an orphan|FR-31|
|`a_chosen_page_renders_under_the_tagged_key_and_completes_without_loops`|the render dispatch: the chosen page renders through the pipeline under the choice-tagged key (the pixels are the chosen page's, not page 1's), and afterwards both the startup enqueue and the sweep treat the entry as complete|FR-31, AC-180|
|`an_out_of_range_page_choice_renders_the_automatic_cover_under_its_tagged_key`|DR-404 + §3.10 out-of-range half: the choice stands, page 1 is rendered, the file lands under the tagged key and the entry is complete — no enqueue/sweep loop|FR-31|

---

## 2. Frontend unit tests — Vitest (`pnpm -C ui test`)

### `ui/src/lib/actions.test.ts` — 3 · *the bulk favorite control (UI-41, AC-112, v0.0.8)*
- adds favorites for a non-favorite selection — outline star, Add label
- removes favorites for an all-favorite selection — filled star, Remove label
- the control is re-derived per call, not frozen at first render

### `ui/src/lib/chip-cutover.test.ts` — 2 · *chip cutover (UI-24, AC-46)*
- finds frontend sources to check
- no stylesheet rule, component or token definition references the chip

### `ui/src/lib/debounce.test.ts` — 4 · *debounce*
- coalesces a burst into one trailing call with the last arguments
- each call restarts the window
- a gap longer than the delay starts a new burst
- cancel() drops a pending call

### `ui/src/lib/design-tokens.test.ts` — 5 (two parameterized) · *${rampName}: text-role contrast (AC-43/AC-71/AC-82)* · *${rampName}: selection ground separated from the hover grounds (AC-97)* · *dark ramp: §3.6 mapping as declared (AC-70)*
- ${pair.role} clears 4.5:1 — every text role on every surface it is used on, both ramps (31 pairs per ramp: text-role grounds including the DR-220 selection ground, plus the tertiary grounds; the light set is the AC-82 measurement behind the §3.7 substitutions and the DR-198 danger-text rendering rule, the dark set the AC-71 measurement behind DR-145)
- apart from the row-hover ground (surface-1) — the composited selection ground measures apart from the row-hover ground, CIE76 ΔE ≥ 10 (AC-97; runs per ramp)
- apart from the card-hover ground (surface-2) — the same distance guard against the card-hover ground (AC-97; runs per ramp)
- the declared token set is exactly the mapped one (dark block, 24 tokens — `selection-tint` is the DR-220 addition)
- --${token} equals its mapped value (dark block)

### `ui/src/lib/format.test.ts` — 5 · *format*
- formats counts with the locale's grouping
- formats sizes by magnitude: bytes, kilobytes, megabytes (UI-36, DR-208)
- renders zero through the byte band
- formats dates with the medium date style
- selects plural tags through Intl.PluralRules

### `ui/src/lib/i18n/coverage.test.ts` — 6 · *i18n coverage*
- finds message call sites to check
- every t() key exists in the catalog
- every tp() base carries both plural variants
- no catalog key is orphaned
- no user-facing string names the groups 'category' (UI-42, AC-113, v0.0.8)
- no label ends with an ellipsis except the progress group (UI-20, AC-40)

### `ui/src/lib/i18n/plural-count.test.ts` — 2 · *tp count formatting (R8.1)*
- groups multi-thousand counts like the rest of the product
- keeps single-item counts and plural selection intact

### `ui/src/lib/i18n/shortcuts.test.ts` — 3 · *the import shortcut pair (UI-29, AC-115, DR-270, v0.0.8)*
- the file import combination is a catalog value that splits into its keys
- the folder import combination is a catalog value that splits into its keys
- the menu names the same combinations the empty state renders

### `ui/src/lib/i18n/terminology.test.ts` — 3 · *document terminology (DR-211, AC-95)*
- the library-facing set carries the document phrasing exactly
- the physical-file set is unchanged
- no string outside the audited set names a library entry after a file

### `ui/src/lib/native-title.test.ts` — 1 · *native title removal (UI-37, AC-87)*
- no HTML element in ui/src carries a title attribute (component `title` props are not violations; the rendered half is the flowTooltipSurfaces lane)

### `ui/src/lib/platform.test.ts` — 6 · *platform detection and the window-controls layout (UI-07 MODIFIED, SPEC-v0.0.12 §3.7, D33)*
- recognizes the macOS user-agent strings
- every other platform keeps the released shape
- macOS: the cluster at the left end, close–minimize–maximize
- Windows: the released right end, minimize–maximize–close — unchanged
- Linux: the released right end likewise (the user's decision: right-end is the Linux default)
- the order is always the same three controls, in some order

### `ui/src/lib/searchScope.test.ts` — 8 · *search placeholder (UI-16)*
- names the whole library when no filter is active
- names the one active tag
- counts several active tags
- names Favorites, and the combination with tags
- names Missing
- names Reading, and the combination with tags
- ignores a tag id that no longer exists
- the query text never changes the scope

### `ui/src/lib/skeleton-geometry.test.ts` — 3 · *skeleton geometry source rule (UI-28, AC-50)*
- imports the row geometry constants from virtual.ts
- sizes the cover placeholder from the constants, not from its own numbers
- the cover's own CSS rule defines no dimensions

### `ui/src/lib/state/epubReader.test.ts` — 36 · *the EPUB reader store's math (SPEC-v0.0.9 §3.13, DR-288/DR-303): the text-size ladder · the position model (cumulative uncompressed entry sizes → chapter byte starts, percent ↔ chapter resolution both directions) · the bounded mount window (§3.12 rule 3) · height bookkeeping (estimate → measured → deltas, the reader-relative judgment per DR-407) · link routing through the sanitizer's protocol URLs · the ToC label and current-entry derivations · the ToC pin and its visibility state (UI-48 MODIFIED / UI-49, SPEC-v0.0.10 §3.8, revised by DR-337) · chapter fetching and failure marking · the strip's walk persistence · position memory (FR-28/FR-29, SPEC-v0.0.12 §3.2: read-before-ready, landing + re-anchor, {c,o} recording, write discipline)*
- walks the six fixed steps in both directions
- clamps at the ladder ends and resets an unknown value to the default
- estimates from the uncompressed entry size with a floor
- records a measured height once and returns deltas against what the layout used
- judges aboveReader against the model BEFORE the height enters it (DR-407: measuring a chapter above the reader moves the boundary under the reader — the pre-update judgment is what keeps the compensation from silencing itself)
- names the chunking threshold the mount rule uses
- computes chapter byte starts and the book total
- resolves a percent to the chapter containing it, clamped
- maps the scroll fraction inside a chapter to the book percent
- mounts the touched chapters plus one on each side and pads the rest
- derives the current chapter from the store's own scroll source (DR-155)
- reads 100% once the viewport bottom reaches the end of the last chapter (DR-306 end-of-book rule)
- routes a bare fragment to the current chapter
- resolves protocol URLs — percent-encoded or not — to spine chapters
- refuses anything that is not an in-book chapter
- labels a chapter with the first entry targeting it, else nothing
- highlights the last entry (document order) the scroll has reached
- prefers the ToC label, falls back to the chapter's own heading, else none
- is session state of the window store: unpinned by default, closed through the reset (UI-48 MODIFIED / UI-49, SPEC-v0.0.10 §3.8, OQ-60 → A; DR-337: pin and visibility written together)
- separates visibility from the pin: the strip control and Esc hide and re-show the panel keeping the pinned position (UI-48 MODIFIED as revised by DR-337)
- lands at a chapter's start with the released +1 px snap tolerance, clamped
- resolves a percent jump to the start of the chapter containing it
- routes viewport and notch scrolls through the view's callbacks
- builds the chapter URL from the manifest base and the encoded entry
- deduplicates in-flight requests for the same chapter
- marks a failed fetch on the chapter, not the window
- ignores indexes outside the spine
- the strip's walk persists through the settings store, clamped
- reads the stored position before `ready` and holds the mount until the landing
- the restored chapter is the current page, so it is the first chapter fetched (DR-150)
- without a stored position the released flow is byte-for-byte unchanged
- lands at the chapter start plus o × the current height, then re-anchors exactly at measurement (DR-406 4)
- a landing the reader already left is not re-anchored
- records the {c,o} grammar from the store's own scroll source (DR-155)
- writes once per settle, last value wins; flush writes the last value once (AC-168)
- a failed position write is silent — the next settle re-tries

### `ui/src/lib/state/feedback.test.ts` — 2 · *feedback overlay stack*
- keeps a closed overlay counted until the current task ends
- plain open/close without a race still balances the count

### `ui/src/lib/state/import-discipline.test.ts` — 2 · *store import discipline (R7.5)*
- finds store imports to check
- every store import uses the canonical $lib/state/… spelling

### `ui/src/lib/state/reader.test.ts` — 83 · *page geometry (exact, never measured)* · *navigation* · *view modes (UI-34)* · *the horizontal row math (UI-34 MODIFIED, SPEC-v0.0.10 §3.3)* · *zoom walk (UI-30 as re-anchored by DR-221)* · *the zoom anchor handoff (D23 / AC-119, v0.0.8)* · *the ToC tree (UI-33 as revised by DR-218 and DR-223)* · *the ToC pin and its visibility state (FR-24 / UI-49, SPEC-v0.0.10 §3.8, revised by DR-337)* · *the navigation direction (UI-34 as extended by DR-224)* · *the edge rule and the keyboard notch (UI-34/UI-39 as re-derived by DR-227/DR-228)* · *image request width* · *page serving through the store* · *the scroll position is a single source (DR-155)* · *position grammar (FR-28, SPEC-v0.0.12 §3.2)* · *position restore (FR-28/AC-167, DR-406 1/2)* · *position write discipline (FR-28/AC-168, DR-406 3)* · *view-mode memory (FR-29/AC-169)* · *text layer (FR-30/UI-54, §3.3)*
- fit-width renders every page at the content width and its own aspect ratio
- a fixed percent renders the page's actual size × percent — per page, in every mode (AC-96)
- Actual Size renders each page at its true dimensions (AC-96)
- Fit Page fits the whole page into the viewport, capped at the fit base (AC-96)
- resizing re-derives fit-width — the fit mode follows it, never an absolute width
- the stack offsets carry the gap and the total height, and pages resolve at their tops
- jumps land one pixel inside the target page's stack offset
- targets outside the document clamp to its first and last page
- a page jump with no scroller attached does not fail
- continuous is the default and keeps the released geometry untouched
- facing fit-width is half a pair row minus the gap; percents anchor to actual size in every mode
- a row is the taller page of its pair; an unpaired last page is a row of one
- facing jumps and steps land on the row containing the page; the field names its first page
- the field follows the viewport center: a row crossing it owns the page number
- horizontal fit-width fits the current page to the viewport — the released fit base, not half a row (AC-146)
- the row's offsets are the pages' display widths plus the gap — per-page for mixed sizes (AC-143)
- the field follows the viewport's horizontal center; jumps and steps move one page along the row (AC-143)
- PgUp/PgDn travel one viewport along the row (UI-30 on the x-axis, AC-145)
- in horizontal a zoom change anchors to the page at the row's left edge; at the row's end it names the last page (AC-144, DR-282 rule on the x-axis)
- single mounts the index instead of scrolling; its navigation moves pages
- switching mode keeps the current page: single mounts it, scroll modes record it for the view
- through the row and back: continuous → horizontal → continuous keeps page 2 in view (AC-144)
- − / + move through the fixed steps and clamp at the ends
- a non-ladder value snaps to the nearest step in the pressed direction (AC-96)
- from a fit mode the walk continues from the mode's effective percent of the current page (AC-96)
- the store's changeZoom only ever produces listed steps
- the zoom select's items set exactly the modes and the ladder (AC-96)
- a zoom change records the current page for the reveal after the layout change (D23, AC-119, DR-282)
- a change that cannot change the geometry records no reveal (AC-119)
- single mode records no reveal — its page is index-driven (AC-119)
- a bottom-clamped view anchors to the viewport's bottom page, not the top edge's partial one (AC-119)
- parents are exactly the entries followed by a deeper entry
- the tree initializes collapsed to the first level: only top-level entries are visible (AC-78)
- the outline load performs that initialization through the command path (AC-78)
- expanding a parent reveals its whole subtree up to the next entry of its own level (AC-78)
- the toolbar toggle alternates modes and drives the whole tree (AC-78, DR-223)
- per-entry toggles never change the toggle's mode; its action always applies to the whole tree (AC-78, DR-223)
- toggling a leaf changes nothing
- a document without an outline has no tree and no hidden entries
- is session state of the window store: unpinned by default, closed through the reset (FR-24 / UI-49, SPEC-v0.0.10 §3.8, OQ-60 → A; DR-337: pin and visibility written together)
- separates visibility from the pin: the strip control and Esc hide and re-show the panel keeping the pinned position (UI-49 as revised by DR-337)
- it follows the delta of each navigation and defaults to forward (DR-224)
- clamped navigation records the direction of the movement actually taken (DR-224)
- a delta at a reached edge steps exactly one page — any magnitude (DR-227)
- the edge test carries the 2 px rounding margin (DR-227)
- mid-page deltas never step; the direction decides which edge counts (DR-227)
- a page fully inside the viewport steps in the delta's direction (DR-227)
- the notch dispatch is mode-aware: single hands the edge rule to the view, the scroll modes scroll (AC-98)
- requests device pixels at most 2× the CSS width
- stays inside the command's accepted range
- serves a mounted page once per width: scrolling does not re-request it
- a zoom change re-requests at the new width (NFR-08's re-show path)
- a target above the fast-render cap previews first; the view's previewShown promotes the full stage after the release window (DR-151 as re-derived by DR-316/DR-323)
- a request before the first fit-width measurement stays inside the command's range (DR-153)
- at devicePixelRatio above 1 the preview uses the CSS display width, not the cap (DR-153)
- a target at or below the fast-render cap is a single request (DR-151)
- a typed page failure becomes the window's error state (§3.2)
- currentPage follows the store's scrollTop through the exact offsets
- an unmeasured window claims page 1, the position the scroller is at (DR-154)
- goToPage and currentPage agree: the delivered target names the page it claims
- reads both kinds' grammars
- clamps a stale anchor to the document's current extent (AC-167: relink to a shorter file)
- reads a malformed value as no position at all — the released default takes over
- encodes the exact grammar with the kind's anchor key and three-decimal offsets
- reads the stored position before `ready` and holds the mount until it lands
- a failed position read is the released default: no restore, no error
- continuous lands at the page top plus o × the page's display height — the first page named is the restored one
- facing lands within the pair row (DR-406 2): the row anchor plus o × the row height
- horizontal lands at the page's left edge plus o × its display width (the x-axis model)
- single mounts the restored page directly (index-driven, no scroll)
- a stale position clamps: the relink-to-shorter case lands on the last page
- a no-op restore (page 1, offset 0) still clears the gate so the first page paints
- applying a restore twice is inert, and without a stored position it changes nothing
- scroll calls debounce to one write per settle — last value wins
- flush cancels the pending debounce and writes the last value once — the close path
- a flush with nothing to record writes nothing
- single-mode navigation records without a scroll event (DR-406 3/5)
- a failed write is silent — the next settle re-tries
- a new store starts in the remembered mode
- the released default (continuous) stands until the user first chooses
- choosing a mode stays a per-window action and persists through the settings batch
- maps page points to CSS px at the display scale — deterministic arithmetic, re-derived on zoom
- requests each page once per window session; a failed extraction caches the empty layer

### `ui/src/lib/state/settings.test.ts` — 8 · *settings store*
- stores every key a query change modifies
- coalesces a burst into one write per key, holding the last value
- keeps a view change and a column override written together
- writes the reading mode with the current tags token
- decodes the current form with all four modes
- decodes a v0.0.7 value and a v0.0.1 value unchanged
- writes the log level immediately, outside the settings batch (FR-26, AC-155: the level must be live before the next window's start)
- restores the stored log level and reads a corrupt row as Error (FR-26, AC-155)

### `ui/src/lib/virtual.test.ts` — 17 · *window (uniform items)* · *row heights (DR-85, UI-03 metrics)* · *window (rows of different heights)* · *nearEnd* · *gridLayout (§5.3.5 as modified by DR-99, AC-31)*
- clamps the top of the list
- keeps the overscan band around the visible range in the middle
- clamps the end of the list and leaves no bottom padding
- partitions every scroll position without gaps or overlap
- bounds the mounted count at the largest viewport the app serves
- handles an empty list
- sizes a row by the metadata lines it renders
- prefix offsets give every row its top and the list its total
- mounts exactly the rows the viewport touches
- keeps the padding invariant at every scroll position
- clamps both ends
- triggers slice loading only near the loaded end
- measures the threshold from the last rows' real heights
- Auto derives the column count from the 232 px pitch and stretches the cards to the container
- a manual count wins and widens the cards instead of leaving slack (DR-83 + DR-99)
- AC-31: at 900, 1280 and 1900 px, for Auto and every manual 2–10 count, the card is the derived width capped at 320 and the row never exceeds the container
- the cover keeps the A4 ratio at every width; the 220 px card measures 375 px (the 5 px content inset, UI-03 as modified by DR-245)

---

## 3. UI smoke harness — `scripts/ui-smoke.mjs`

Drives the **real built app** (`src-tauri/target/debug/bookkeeper.exe`) through tauri-driver (W3C WebDriver → msedgedriver); reader flows run on a second runtime — the same exe driven over CDP (DR-147). Prerequisites: `tauri-driver` + version-matched `msedgedriver` on PATH; fixtures generated by `scripts/make-fixtures.mjs` (D15: never committed binaries).

**Entry points.** The standard UI verification (stage 4, RULES R6) is full mode:

|Command|What it is|
|---|---|
|`node scripts/ui-verify.mjs`|**The verification entry point.** Runs every UI test case as its own lane in barrier-separated phases (§3 "Full verification" below); exit 0 only if all lanes pass within the 300 s combined budget. `--report [path]` also writes the final summary to a file (default `tmp/test-reports/`).|
|`node scripts/ui-verify.mjs --coarse`|Documented triage fallback: the two-suite split of DR-162 (core ∥ 10k), less isolation, slower wall.|
|`SMOKE_FLOW=flowX node scripts/ui-smoke.mjs`|One flow standalone against the app's own data directory — the deterministic reproduction harness for debugging a failure.|
|`SMOKE_SET=<area> node scripts/ui-smoke.mjs`|All flows of one feature area plus the mandatory prefix.|
|`node scripts/ui-10k.mjs`|The 10 000-document suite standalone (§4).|
|`SMOKE_10K_FLOW=<flow>[,<flow>…] node scripts/ui-10k.mjs`|One or more 10k flows standalone — the same selector the docs lanes use. Prefix runs first; a selection without `flowImport10k` pays the corpus as a quiet setup import instead of the measured test case (§4 Run modes).|

**Run shapes.** No arguments: full suite on the core tier (functional section, reader section last) — since DR-161 this suite never imports the docs tier: every 10 000-document UI test case runs in the separated 10k suite (§4). `SMOKE_SET=<area>[,<area>…]`: exactly the flows of the named feature areas, core tier. `SMOKE_FLOW=flowX,flowY`: narrows further (debugging aid; a scale-flow name errors with a pointer to `scripts/ui-10k.mjs`). Every flow is timed (D14/AC-54); wall time is checked against the NFR-06 budget; exit 0 only if every check passes. The bodies of the three scale flows live in `ui-smoke.mjs` (they share its geometry/menu/selection helpers) and are exported for the 10k suite; no ui-smoke run shape executes them.

**Full verification: `node scripts/ui-verify.mjs` (DR-162 lanes, DR-163 per-flow split, DR-164 phases, DR-165 isolation, DR-168 phase order).** The entry point that runs every UI test case as its own lane: 1 prefix lane (the two prefix test cases, checks ON, owns the build, runs alone first), 20 measured lanes run **one at a time** (the fifteen reader lanes - the thirteen PDF ones plus `flowEpubReader` and `flowEpubPerformance` (v0.0.9, DR-300); the bounded-job lanes `flowStatusBar`, `flowProgressTransform`, `flowMissingAndRelink`; the picker-request lane `flowSearchField`; the window-strip double-click lane `flowDragRegion` — a measured claim is a statement about single-instance conditions), 1 corpus-import lane (runs alone — its AC-57/NFR-02 claims are measured while nothing else runs; it follows the measured phase so its cover-queue disk flush cannot bleed into the first reader lane — the flake mechanism DR-168 records), and 40 flood lanes (the remaining 36 functional and 3 scale lanes, default cap 6, `SMOKE_VERIFY_LANES` — lowered from 10 at DR-365, the renderer-stall evidence recorded there; the measured-phase cap is `SMOKE_VERIFY_MEASURE_LANES`, default 1, clamped 1..8) — 62 lanes total. The groups are barrier-separated: each starts only after the previous group's last lane exits. Every lane runs against its own data directory (`%APPDATA%\com.pdfkeeper.app\parallel\lanes\<flow>`, via `PDFKEEPER_DATA_DIR`, the same variable the app's resolver reads), its own tauri-driver/WebDriver ports (4444+i / 17556+i; reader lanes also their own CDP debug port 9323+i and per-port WebView2 profile directory), and scoped cleanup that touches only its own process tree (the shared image name and shared WebView2 profile belong to all lanes). The combined clock runs from the build owner's session line to the last lane exit and is asserted against the same ≤ 300 s budget; exit 0 only if every lane passes within it. With `SMOKE_VERIFY_WALL_GATE=0` (DR-357) the wall is measured and reported in the summary but does not gate the exit — the CI lane job's shape (NFR-10, AC-158). Global image-name sweeps run only before anything starts and after all lanes are done. `--coarse` selects the documented triage fallback: the two-suite split of DR-162 (this suite as the `core` lane at `…\parallel\core`, the 10k suite as the `docs` lane at `…\parallel\docs`; core owns the build, docs is released on its session line). Direct runs of either suite keep working unscoped (the app's own data directory, global cleanup); `SMOKE_FLOW`/`SMOKE_SET`/`SMOKE_10K_FLOW` select standalone runs. `--report [path]` (DR-167) additionally writes one final summary file — verdict, mode + caps, lanes ran, check tallies, combined wall vs the budget, the per-lane table, and the failed check names per failed lane (no per-check transcript); default `tmp/test-reports/verify-<UTC stamp>.md` (gitignored); a write failure is reported on stdout and never changes the exit code.

**Prefix rule (AC-60):** every named set starts with the mandatory prefix flows (the app-shell check and the drop-pipeline check the other flows reuse); the 10k suite runs the same prefix, selected or not. With `SMOKE_PREFIX_QUIET=1` — set for every lane of `ui-verify.mjs` — the prefix runs as setup: its passing checks log but are not recorded, a failed setup check is recorded as the lane's failure. The `AREAS` table below and the run sequences are maintained together; the suite itself enforces that every flow appears in at least one area.

### 3.1 Suite-level checks (run by the executed runner's `main()`, not inside a flow)

|Check|Asserts|covers|
|---|---|---|
|`checkTokenSource()`|no token added to `design-tokens.css` beyond the names this release's designs mandate (compared against the released baseline via git)|AC-49 (source half)|
|AC-56 source scan|no fixed `sleep()` longer than 250 ms in either harness file (scans `ui-smoke.mjs` **and** `ui-10k.mjs` — extended by DR-159, DR-161)|AC-56|
|AC-60 orphan scan|every flow in every sequence appears in at least one feature area|AC-60|
|wall-time budget|suite total ≤ 300 s across both runtimes|NFR-06, AC-58|
|scheduler gate (`ui-verify.mjs` full mode)|every lane exits green **and** the combined wall time from the build owner's session line to the last lane exit ≤ 300 s — the orchestrator's own exit gate (not a `check()` call site); `--coarse` asserts the same gate over its two lanes|NFR-06, AC-58|

### 3.2 Prefix flows (2)

**`flowStartup`** — area: getting-started
- startup: app shell + document area mount
- startup: empty-state variant for a fresh library
- *conditional skip:* `startup: empty-state variant for a fresh library` when the library holds documents at startup — a scale lane seeded from the published corpus (DR-362); the fresh-library check itself is unchanged (2026-11-17)

**`flowDropAndSkip`** — area: getting-started · covers FR-01, FR-11
- drag-drop: synthesized drop reaches the app pipeline end-to-end
- non-PDF import is reported as a skip, never silently ignored (FR-11)
- *conditional skip:* `drag-drop: synthesized tauri://drag-drop delivery` when the platform delivery cannot be synthesized (0 skips in all v0.0.6 runs)

### 3.3 Functional flows (43, core tier)

**`flowSearchDebounce`** — searching
- Ctrl+F focuses search (keyboard reachability)
- debounced search reaches the correct result set
- stale responses are never rendered (only the newest query is on screen)
- Esc clears the search and restores the full list

**`flowTags`** — browsing-and-filtering, tags · covers UI-10, AC-03, UI-04, UI-09
- UI-10: Manage tags opens the tags manager
- AC-03 UI: tag creation via keyboard (manager field + Enter) ×2
- Space + ArrowDown select rows and open the bulk bar
- bulk bar opens the tag picker (UI-04)
- picker pre-ticks add for a tag the selection lacks (UI-04)
- picker add checkbox toggles the assignment off and on
- AC-03 UI: a tag filter from Browse shows exactly the assigned documents
- UI-09: the active tag appears as a checked item

**`flowContextMenu`** — opening-and-deleting · covers UI-05, AC-68, UI-31
- right-click opens the context menu on screen (UI-05)
- Esc closes the context menu
- closing the menu with Esc leaves the selection intact (§5.2)
- AC-68: the context menu carries Open then Open in external viewer
- AC-68: the external item calls app_open_document exactly once and not the reader command
- AC-68: the external open of a present file surfaces no failure
- AC-68: the in-app item calls app_open_reader_window and never app_open_document
- AC-68: opening a present file in the reader surfaces no failure
- AC-68: no open action appears on rows — the open path stays in the context menu (the shipped UI-43 row set is intact)

**`flowDelete`** — opening-and-deleting · covers AC-04
- AC-04 UI: Delete key opens the confirmation dialog
- Esc cancels the dialog and keeps the selection (§5.2)
- the same selection re-opens the dialog after cancel
- AC-04 UI: 'Delete from library' removes exactly the selection
- library count reflects the deletion

**`flowFavorites`** — favorites · covers FR-04, UI-09
- favorites flow: a mid-list row is available
- FR-04: starring updates the favorites count
- FR-04: the starred row does not jump mid-scroll (agreed UI)
- FR-04: the Favorites view shows exactly the starred document
- UI-09: the Favorites mode appears as a checked item
- UI-09: clearing the filter in the Browse menu clears that filter and nothing else
- FR-04: the starred document is pinned to the top on the next query
- FR-04: unstarring removes the document from Favorites

**`flowReading`** — reading-collection · covers FR-19, UI-43, AC-121, AC-102, AC-103
- reading flow: a mid-list row is available
- FR-19: the Reading action updates the Reading count
- UI-43: the Reading toggle follows the membership state
- FR-19: the Reading view shows exactly the marked document
- UI-43: the Reading mode appears as a checked item
- AC-102: opening a document leaves Reading membership unchanged
- AC-102: the opened row is still marked
- FR-19: the Reading view composes with search
- FR-19: the Reading action removes the document from the collection
- FR-19: an empty Reading view shows its empty state

**`flowMetadata`** — metadata-editing · covers FR-05, UI-02, FR-03, AC-52
- metadata flow: a row is available
- FR-05: the details modal opens from the row menu
- UI-02: an untouched auto title carries the provenance hint
- UI-02: the hint disappears on the first edit
- FR-05: saving updates the title on the row
- FR-05/FR-03: the edited title is found by search
- delete flow: the renamed row is findable by text
- UI-02: the editor hands the document to the delete flow (singular title)
- UI-02: cancelling the delete keeps both the document and the editor
- UI-02: deleting from the editor removes the document and closes the editor
- UI-02: Escape with unsaved changes opens the discard step and keeps the editor and the draft
- UI-02: 'Keep editing' closes the step and leaves the draft in place
- AC-52: Ctrl+A inside the title field selects the field's text and starts no library selection
- AC-52: the field selection behaves like an editor selection — typing replaces it
- UI-02: discarding closes the editor and the stored title is untouched

**`flowStatusBar`** — status-feedback · covers AC-56, AC-22, AC-53, AC-91, UI-15
- AC-22 / AC-53: the bounded rescan job shows a real progress bar in the bar
- AC-22: the bar is quiet again once a job ends
- AC-22: with nothing to say the status bar shows only the document count
- AC-91: the document count is labeled with its unit, same tertiary tabular style (DR-210)
- AC-22: while filtered the status bar shows the result count, and the document count stays library-wide

**`flowProgressTransform`** — status-feedback · covers AC-42, AC-49
- AC-42: the progress fill transitions transform with a real duration
- AC-42: the fill keeps width 100% of the bar — the progress rides the transform
- AC-49: the progress row gap is a multiple of 4 px

**`flowMissingAndRelink`** — missing-files-relinking · covers FR-15, AC-05, AC-47, UI-25, AC-37, AC-22, UI-02, AC-124, AC-128 (v0.0.9)
- job progress completes; Rescan re-enables after the bounded job drains (FR-15)
- AC-05 UI: manual Rescan reports the missing file
- AC-05 UI: the row is flagged missing
- AC-47: the row flag is plain text in the corrected warning color — no ground, no pill
- AC-47: the row flag's tooltip carries the stored path
- AC-37: the missing document's actions line includes Relink in the same order as its context menu
- AC-37: a present document's actions line shows no Relink
- AC-47: the card overlay keeps a warning-tinted ground with radius at half its height
- AC-47: the card badge's tooltip carries the stored path
- AC-37: the missing document's card actions line includes Relink
- AC-22: with a missing file the status bar shows the warning
- failed command produces a toast with its action (Relink)
- AC-05 UI: the toast action opens the relink dialog
- AC-128: the relink picker for a PDF document presents the PDF filter only
- Esc closes the innermost overlay
- UI-02: the editor's Relink opens the relink dialog
- AC-05 UI: Rescan finds the file again and clears the flag
- AC-22: the status bar stops saying it once nothing is missing
- AC-124: a present EPUB document's menu carries the same capabilities as any document's (no relink path while the file exists)
- AC-124: the rescan flags the moved-out EPUB document as missing
- AC-124: the missing EPUB document's menu carries the relink path and no rename
- AC-128: the relink picker for an EPUB document presents the EPUB filter only
- AC-124: the rescan finds the EPUB file again and clears the flag
- relink flow: the EPUB probe record is removed again

**`flowOpenFolder`** — opening-containing-folder · covers UI-40, AC-111 (v0.0.8; imports its own probe document and removes its record again — the reveal itself is observed in the real app, the harness asserts the typed request at the command boundary)
- AC-111: the context menu carries Open containing folder
- AC-111: the item asks the app to reveal exactly this document's file
- AC-111: the Details dialog carries Open containing folder
- AC-111: the Details action reveals the same file and the editor stays open
- AC-111: a missing document's menu shows no Open containing folder
- open-folder flow: the probe record is removed again

**`flowRename`** — renaming-a-documents-file · covers FR-21, AC-107, AC-108, AC-124 (v0.0.8; renames its own probe copy on disk and removes both records again; runs after the flows that touch its fixture; the extension-swap half is the FR-21 MODIFIED behavior, v0.0.9)
- AC-107: Rename opens the dialog prefilled with the current name, the folder shown read-only above it
- AC-108: an invalid name is refused with the typed inline error and the dialog keeps the field open
- AC-108: a colliding name is refused with the typed inline error and nothing on disk or in the record changes
- AC-108: the refused rename leaves the row exactly as it was
- AC-107: the rename changes the file on disk and the record together — the row keeps its id and shows the new name
- AC-107: the Details editor shows the renamed file's path
- AC-107: a rename from the Details footer keeps the editor open and it follows the new name (path line and title field)
- FR-21 (MODIFIED): a rename between supported extensions is accepted — the file moves to the new extension
- FR-21 (MODIFIED): the record follows the new type — the Details path line carries the .epub name
- FR-21 (MODIFIED): a name outside the supported extensions is refused with the typed error and nothing changes
- §3.4: a missing document's menu carries Relink and no Rename
- rename flow: both probe records are removed again

**`flowMetaColumn`** — importing-pdfs, metadata-editing · covers AC-45, AC-85, UI-36, DR-208
- meta-column flow: the differently-named fixture imported
- AC-45: the meta column's left edge and the figures' right edge are uniform across rows
- AC-45: the meta column reserves the fixed 96 px box
- AC-45: the measured rows actually carry filenames of different lengths
- AC-85: every rendered size cell equals the unit-by-magnitude rendering of the byte size the manifest records (DR-208)
- meta-column flow: the imported row is selected and the delete confirmation opened *(setup/cleanup)*
- meta-column flow: the imported document is gone again *(setup/cleanup)*

**`flowOpen`** — opening-and-deleting · covers AC-12, AC-64, AC-30, DR-66, UI-18, AC-67
- AC-12 / AC-30 / AC-64: a single click opens nothing and, in browse mode, selects nothing (DR-66, UI-18)
- AC-64: a real double-click activates the document (reader window)
- AC-64: the double-click handler activates the document
- AC-64: Enter still activates the focused row
- *conditional skip:* `AC-64: native double-click generation` if the platform cannot generate it (0 skips in all v0.0.6 runs)

**`flowCovers`** — covers · covers AC-06, AC-100, DR-231, DR-235, DR-237, DR-238, DR-240, DR-241
- AC-06 UI: list rows render real cover images from the cache
- AC-06 UI: grid cards render cover images
- AC-06 UI: switching back to list keeps covers
- AC-100: finished covers render in the open view with no reload, search or view switch — the completion event's own re-query did it (the window is established by filtering to `doc010` before the import; the appearance arrives through the completion re-query alone)
- AC-100: the completion re-query leaves selection and scroll state untouched (selection and scroll established on the appeared window; a second cover completion — two further matching files — must leave them standing; the second re-query is proven at the store's rows — the flow issues no query of its own — and the new rows sitting below the viewport at the preserved scroll is the expected behaviour (DR-241))
- AC-100: the unrenderable fixture malformed.pdf keeps the glyph fallback unchanged
- AC-100: the unrenderable fixture encrypted.pdf keeps the glyph fallback unchanged

**`flowMetadataDisplay`** — metadata-editing · covers UI-06, AC-08, AC-24, DR-85, AC-83, AC-84, UI-03, DR-183, DR-184, AC-122 (the token-derived row heights), UI-45, DR-278
- UI-06: row heights come from the base + per-line tokens
- AC-08: author and notes render as their own lines under the title
- AC-08: every ellipsized line carries its full text as a tooltip
- AC-08/AC-24/DR-85: the row is exactly as tall as the metadata lines it renders (157 px base + 18 px per line, UI-03 as modified by DR-189, re-derived by UI-45/DR-278 and UI-51/DR-320)
- AC-08: an empty field renders no line and reserves no space
- AC-08: the grid card shows the author line and no notes line
- AC-83: a grid card mounts no .meta node and no size or date text
- AC-84: at rest the card's actions slot carries no ground
- AC-84: on card hover the actions group renders on an opaque ground — the ramp's --surface-0, the hairline border and the overlay shadow — with the 32×32 boxes unchanged
- AC-84: on card keyboard focus the actions group renders on the same ground

**`flowTooltipSurfaces`** — metadata-editing, settings-and-appearance · covers AC-87, UI-37, DR-187, DR-201
- AC-87 (light ramp + dark ramp, 10 call sites ×2): a clipped row's title line / a row's author line / a row's notes line / a card's title / a card's author line / the missing row flag / the missing card badge / a tag row action / the delete dialog's file-note button / a column stepper control — each shows the app's own tooltip carrying its full text (or the control's own accessible name) on the ramp's --surface-1 ground with its shadow
- AC-87: the theme returns to the default after both passes
- tooltip flow: the probe record is removed again *(setup/cleanup)*

**`flowSortSecondary`** — sorting-and-views · covers AC-09, DR-81
- AC-09: with no secondary key the order is the shipped primary, then id
- AC-09: the secondary key orders the ties exactly primary → secondary → id
- AC-09: the secondary key inherits the primary's direction (DR-81)
- AC-09: setting the secondary key changes the order of a tied primary key
- AC-09: the stored order is applied with no user action
- AC-09: the restored order sorts the rows primary → secondary → id
- AC-09: the View menu shows the restored keys as the active ones
- AC-09: the default ordering (title A→Z, no secondary) is restored

**`flowViewColumns`** — sorting-and-views · covers AC-10, AC-31, AC-39, DR-128, DR-44
- AC-10: Auto is the active choice and matches the width-derived count
- AC-31: Auto derives the card width from the container and the row block is the derived card height
- AC-31: a manual count applies immediately and the cards follow it
- AC-10: the manual count clamps at 10 and the + control disables there
- AC-39: the derived layout holds at manual 10 at 1280 px
- AC-10: the manual count clamps at 2 and the − control disables there
- AC-31: a count below Auto widens the cards up to the 320 cap and the leftover becomes even margin
- AC-10: Auto restores the derived count and the derived card width
- AC-10: the manual column count survives a restart and is applied

**`flowContentGutter`** — sorting-and-views · covers AC-39, AC-86, UI-19, DR-186, DR-200
- AC-39: the grid window carries the 12 px content inset, the token's own value
- AC-39: the derived columns fit the content width — no horizontal overflow
- AC-86 (grid): at the top of the scroll the first card's top edge measures 12 px below the toolbar's bottom edge
- AC-86 (grid): at a mid-scroll position cards render continuously with no gap band at the viewport top
- AC-86 (grid): at the end of the scroll the last card's bottom edge measures 12 px above the status bar's top edge
- AC-86 (list): at the top of the scroll the first row's top edge measures 12 px below the toolbar's bottom edge
- AC-86 (list): at a mid-scroll position rows render continuously with no gap band at the viewport top
- AC-86 (list): at the end of the scroll the last row's bottom edge measures 12 px above the status bar's top edge

**`flowTagsMulti`** — browsing-and-filtering, tags · covers UI-04, AC-17, AC-13, UI-09, AC-23, AC-33
- AC-17: each single tag filter still selects exactly its own set
- AC-17: two active tags select exactly their union
- AC-17: a document in both tags is counted once
- AC-13: the status count and the rows agree with the union
- AC-13 / UI-09: the active tags appear as exactly one checked item each
- AC-23: no filter chip renders anywhere while a filter is active
- AC-13 / UI-09: clearing one tag in the Browse menu drops that filter and nothing else
- AC-17: the tag set combines with a saved search as (A OR …) AND mode
- AC-17: the terms narrow the union instead of replacing it
- AC-17: a broader term returns the whole union, the overlap counted once
- AC-17: an empty tag set is identical to the shipped unfiltered list
- AC-13 / AC-23: the restored last filter is the active filter at startup, displayed in the Browse menu
- AC-17: a v0.0.1 `cat:<id>` setting restores as exactly that one tag
- AC-13: All documents clears every filter

**`flowImportAssign`** — importing-pdfs · covers AC-18
- AC-18: an import with two tags active assigns the document to both
- AC-18: the import adds exactly one document to the library
- AC-18: the assignment is stored on the document, not on the filter
- *conditional skip:* `AC-18: the drop path itself` when synthesized delivery is unavailable

**`flowImportProgress`** — importing-pdfs · covers AC-101, AC-123, FR-10 MODIFIED, DR-233, DR-279
- AC-101: the status bar's progress bar is present from the opening event — processed 0, total = the candidate count
- AC-101: a search typed while the import is in flight returns its results
- AC-101: the completion toast reports the counts as released
- AC-101/AC-123: the bar advances through per-chunk events from the opening event to completion (each step ≤ 5 — the D25 chunk bound, monotonic), without a gap

**`flowTagCrud`** — tags · covers AC-14, UI-10 MODIFIED (v0.0.12, DR-414), FR-32, AC-183
- UI-10 (DR-414): the Tags manager renders as a toolbar dialog — title, close and Delete all tags on the bar above the content area
- UI-10 (DR-414): the Tags manager content area reaches the dialog's side edges with the content inset unchanged, its bottom clear of the rounded corners, and the new-tag field sitting below the area's top edge (DR-415)
- AC-14: rename opens the shipped dialog prefilled with the current name
- AC-14: a duplicate name is refused inline and the dialog stays open
- AC-14: renaming updates the manager list
- AC-14: delete asks for confirmation naming the tag
- AC-14: discarding the confirmation keeps the tag
- AC-14: deleting a tag detaches it from documents and deletes no document
- FR-32: Delete all tags opens a confirmation naming the operation
- FR-32: discarding the confirmation keeps every tag
- FR-32: confirming deletes every tag — the manager shows the empty state, the action disables, the completion toast reports the count, and no document is deleted

**`flowQuietCounts`** — browsing-and-filtering, tags · covers AC-46
- AC-46: a tag row's count computes transparent ground, no border, tabular figures

**`flowBrowseIcons`** — browsing-and-filtering · covers AC-48
- AC-48: every Browse menu item renders exactly one 16 px leading glyph

**`flowScrollbars`** — settings-and-appearance · covers AC-15, UI-11, DR-101
- AC-15: the applied ramp declares its color-scheme so UA-drawn chrome follows it
- UI-11: the ${ramp} ramp defines the agreed thumb token ×2 (per ramp)
- UI-11: both ramps define the thumb token in the loaded stylesheet
- AC-15: the measured surfaces include ones that scroll
- AC-15: every scrolling surface draws the token-colored bar
- AC-15: every scrolling surface reserves the same thin gutter
- UI-11: exactly one rule set defines scrollbars — no component overrides it

**`flowDragRegion`** — the-app-window · covers DR-157, AC-19
- AC-19: the toolbar strip carries the deep drag region and every control in it is excluded
- AC-19: pressing the empty strip starts the window drag, pressing a control does not
- AC-19: a double click on the empty strip toggles maximize and a second one restores
- AC-19: a double click on a control is a click on the control, never a window maximize

**`flowToolbarControls`** — the-app-window · covers AC-20, AC-26
- AC-20: no toolbar control renders visible text and every one carries an accessible name
- AC-20: every toolbar control shows a tooltip carrying its accessible name on keyboard focus
- AC-20: the same tooltip appears on pointer hover
- AC-26: no standalone Sort control renders anywhere

**`flowSearchField`** — searching · covers AC-21, UI-14, AC-25, UI-16, UI-44, AC-114, AC-115 (v0.0.8), AC-126 (v0.0.9)
- AC-21: the field is magnifier, input, divider, one trailing control
- AC-21: the trailing control is the import control, named (UI-44 option A: Import files)
- AC-21 / AC-114: the plus opens exactly Import files and Import folder; AC-115: each item names its shortcut
- AC-21: while the field holds text the clear button occupies that slot
- AC-25: the placeholder names the current scope while the accessible name stays Search
- AC-21 / AC-126 (UI-44 option A, UI-46): both items reach their picker — files with the two-type filter, folder as a directory

**`flowRowDesign`** — sorting-and-views · covers UI-03, AC-24, AC-88, AC-122, DR-188, DR-189, UI-45, DR-278
- AC-24: a row renders no selection checkbox and no tag badge
- AC-24: a list row measures 157 px with a 100×141 cover (UI-03 as modified by DR-189, re-derived by UI-45/DR-278 and UI-51/DR-320)
- AC-88: the cover's top and bottom edges sit 8 px inside the row box — against the separator hairline above and the row's own border-bottom
- AC-24: the star, Details, Reading and Delete are one actions line, hidden at rest (the Reading action joins per UI-43)
- AC-24: the Details action renders the info glyph
- AC-24: the actions line appears on keyboard focus
- AC-24: the actions line appears on hover
- AC-24: the tag action keeps the tag glyph

**`flowActionParity`** — the-app-window · covers DR-157, AC-32, AC-33
- AC-32: every toolbar control centres its glyph in its own box (|dx|,|dy| ≤ 1 px)
- AC-32: a text control still carries the 10 px side padding
- AC-33: the row actions are one control type — 32×32 boxes on the row's centre line
- AC-32: the row action glyphs are centred in their boxes
- AC-33: the star reports role="switch" with aria-checked
- AC-33: toggling the star flips aria-checked and writes through library_set_favorite
- AC-33: every row action shows the app's own tooltip carrying its accessible name on keyboard focus
- AC-33: every row action shows the same tooltip on pointer hover
- AC-33: in grid the actions are the same control type — 32×32 boxes on one centre line, centred glyphs, the star a switch
- AC-33: every card action shows the app's own tooltip carrying its accessible name on keyboard focus
- AC-33: every card action shows the same tooltip on pointer hover
- AC-33: the card star tracks the favorite state and writes through the same command

**`flowContextMenuSuppression`** — the-app-window · covers D20, AC-116 (v0.0.8)
- AC-116: the library window prevents the browser menu at the toolbar, status bar, document area, search field and browse trigger
- AC-116: the bulk bar is prevented as well
- AC-116: the app's own row context menu still opens — the guard respects already-prevented events

**`flowThemeAndSettings`** — settings-and-appearance · covers AC-28, AC-43, AC-89, AC-90, AC-94, AC-99, UI-17, UI-21, AC-29, AC-44, AC-41, UI-22, AC-49, UI-27, DR-209, DR-216, DR-230, DR-234
- AC-28: choosing Light switches the whole window to the light ramp immediately
- AC-43: the running app resolves --accent to the corrected light value
- AC-89: the Settings menu carries one theme action item labeled with the mode in effect, with its icon
- AC-89: activating the theme item keeps the Settings menu open (DR-209) *(the shared `cycleThemeStep` helper, the pattern for the cycle steps below)*
- AC-28: choosing Dark switches it to the dark ramp
- AC-43: the running app resolves --accent to the corrected dark value
- AC-89: the item's icon and label track the current mode
- AC-89: the Auto step renders the half-filled auto glyph and the Auto name
- AC-89: the ramp switches at each cycle step — Auto resolves to the operating system's ramp
- AC-89: the cycle closes back to Light
- AC-28: the chosen ramp is applied again after a restart
- AC-28: System follows the operating system
- AC-90: Settings opens the dialog with its Appearance and Data location sections
- AC-90: the dialog renders no Library section and no counts, name or version rows
- AC-29: the dialog is focus-trapped
- AC-89: the dialog's theme picker shows the same value the menu cycle holds
- AC-44: the Settings section headings compute text-transform none and letter-spacing normal
- AC-90: the Data location row shows the library location, asserted the AC-41 way
- UI-21: the location value is selectable text
- AC-90: the open control is a 32×32 button carrying the app's own tooltip on the overlay ground
- AC-94: with the open-control tooltip shown the settings dialog panel does not scroll (no overflow contribution, the kit's positioned wrapper is fixed) (DR-212/DR-213/DR-216)
- AC-90: the open control calls app_open_data_dir exactly once with no failure surfaced
- AC-49: the settings section gap and section separation are multiples of 4 px
- AC-89: a theme change made in the dialog is live in the menu item without a reload
- AC-29: a view change made in the dialog is live in the View menu and on screen without a reload
- AC-29: the change the View menu made is live in the dialog
- AC-29: Esc closes the settings dialog
- AC-90: the status bar's counts are unchanged
- AC-90: the About dialog is unchanged — it still shows the name and version the app reports
- AC-99: the reader window stands on the ramp that was in effect when it opened (the flow opens it with the library on a chosen ramp, DR-230)
- AC-99: a theme cycle from the library window changes the already-open reader window's rendered ramp, without closing or reloading it (settings_changed re-applies the theme in every window, DR-230; the reader target reached through the driver's DevTools port, DR-234)
- AC-99: the library window's own controls keep tracking the cycle (AC-89 unchanged)
- AC-99: the Auto step resolves in the reader window exactly as in the library window — the OS ramp
- AC-99: a full cycle returns the already-open reader window to its original ramp
- AC-99: a dialog picker choice propagates to the already-open reader window as the cycle does
- AC-99: re-picking Light restores the already-open reader window too
- AC-99: the reader window closes cleanly (the flow's own setup)

**`flowLogs`** — settings-and-appearance · covers FR-26, UI-53, AC-154, AC-155, AC-156 (v0.0.11)
- AC-156: the Settings dialog carries a Log section — the level picker with Error checked by default and the open control
- AC-156: the path row shows the real log file under the data directory
- AC-154 / AC-156: a rejected command is recorded at the default Error level with its command name and error text (the `app_set_setting` theme-value probe)
- AC-154: an import failure is recorded as an Error line; lifecycle stays silent at the default level
- AC-156: the level selector takes effect without a restart — the picker shows Info and the import run line is recorded
- AC-156: the selector controls what is recorded — back at Error the failure line lands and the lifecycle line stops
- AC-156: opening the log file surfaces no failure (`expectNoErrorToast`)
- AC-156: Open log file asks the app to open the file the path row names (the `log_open` request recorded; the file exists on disk)
- AC-156: the level persists across a restart and the restarted app records its lifecycle line
- AC-156: the app records the reader windows it opens (lifecycle line at the live Info level)
- AC-156: the Log section behaves identically in the reader window's Settings dialog — the picker mirrors the current level
- AC-156: a level pick made in the reader window is live in the library window and suppresses the close line

**`flowDarkRamp`** — settings-and-appearance · covers AC-81, AC-82, AC-71 (dark half), AC-83 (the frame-band half), AC-97, UI-38, DR-145, DR-189, DR-220, DR-245, DR-246, DR-278, UI-51/DR-320, UI-49/DR-331 (the token audit pins the 157 px row amendment and the 300 px panel-width token)
- AC-81: the light ramp declares the §3.7 mapping (baseline = the v0.0.6 Ship commit, found by its message)
- AC-81: the light color-token set is exactly the mapped one (the pinned amended set: `row-height` 157 with `row-cover-gap` 8 — DR-189 re-derived by DR-278 and UI-51/DR-320, `card-content-pad` 5 — DR-245, `toc-panel-width` 300 — UI-49)
- AC-81: the metrics, type and motion declarations are unchanged from v0.0.6 except the amended set (DR-189 row metrics, DR-245 card inset, UI-45/UI-51 row re-derivations, UI-49 panel width)
- AC-81: color-scheme: light stays
- AC-81: the dark ramp is unchanged from v0.0.6 except the added --selection-tint declaration (the DR-220 amendment)
- AC-81: the theme mechanism (theme.ts) is unchanged since v0.0.6
- the app applies the light ramp
- AC-82: the library surface renders as Background #FFFBEB
- AC-82: menus render on BackgroundColorLighter #ECE9DF with Foreground text
- AC-97: menu hover still renders the menu-hover tint --accent-quiet (12 % Purple) — the selection treatment is separate (light ramp)
- AC-82: a hovered row renders on the mapped hover ground #ECE9DF
- AC-97: a selected row renders the selection ground — the 20 % Purple tint and the single accent separator line, the row before it included, no inset ring (light ramp, DR-246)
- AC-83: the selected card's frame band is the card's own ground — a point inside the 5 px content inset hit-tests to the card, not the cover (DR-245)
- AC-97: a selected card renders the selection ground — the 20 % Purple tint, the accent border and the inset ring (light ramp)
- the app applies the dark ramp
- AC-71: the library surface renders as Background #282A36
- AC-71: menus render on Background Light #343746 with Foreground text
- AC-97: menu hover still renders the menu-hover tint --accent-quiet (8 % Purple) — the selection treatment is separate (dark ramp)
- AC-71: a hovered row renders on Background Light #343746
- AC-97: a selected row renders the selection ground — the 20 % Purple tint and the single accent separator line, the row before it included, no inset ring (dark ramp, DR-246)
- AC-97: a selected card renders the selection ground — the 20 % Purple tint, the accent border and the inset ring (dark ramp)
- the theme returns to the default (the mechanism is the released one)

**`flowSelectMode`** — selecting-documents · covers DR-157, AC-30, UI-31, AC-64, AC-68, UI-04
- AC-30: Select mode is off at startup
- AC-30 (off): a click on a row changes no selection and takes the row's focus
- AC-30 (off): Space on the focused row selects
- AC-30 (off): Ctrl-click selects and Shift-click extends the range
- AC-30 (off): double-click activates through the reader command, not the OS path
- AC-30: the toolbar toggle turns Select mode on
- AC-30 (on): a click selects and never opens
- AC-30 (on): the BulkBar's favorite action applies to the selection
- AC-30 (on): the BulkBar's favorite action also lifts it again
- AC-30 (on, grid): a click on a card selects it
- AC-30 (off, grid): a click on a card changes no selection
- §5.2: Escape with no overlay open clears the selection
- AC-30: Select mode is off again after a restart

**`flowBulkFavorite`** — selecting-documents · covers UI-41, AC-112 (v0.0.8; restores the selection's favorite state before finishing)
- AC-112: the bulk bar carries exactly one favorite control
- AC-112: a non-favorite selection shows Add to favorites with the outline star
- AC-112: the control adds the selection to the favorites
- AC-112: an all-favorite selection shows Remove from favorites with the filled star
- AC-112: the control removes them again
- AC-112: after the removal the control reads Add to favorites again

**`flowAbout`** — settings-and-appearance · covers AC-27, UI-17, AC-16, AC-49, UI-27
- AC-27: no app-menu control renders anywhere
- AC-27: the Settings menu is reachable from the toolbar with Tab alone
- AC-27: Enter opens the Settings menu
- UI-17: the Settings menu carries the theme cycle item, Settings and About
- AC-16: the About item is reachable and selected with the arrow keys
- AC-16: About opens through the shared modal wrapper
- AC-16: the dialog holds focus inside itself
- AC-16: the dialog shows the application name and version the packaged app reports
- AC-16: no version string exists in the frontend sources
- AC-49: the About dialog's row gap is a multiple of 4 px
- AC-16: Esc closes the About dialog

**`flowWindowChrome`** — the-app-window · covers AC-11, DR-90, DR-88
- AC-11: the toolbar carries Minimize / Maximize / Close as app controls
- AC-11: every window control is keyboard-reachable inside the toolbar
- AC-11: the toolbar strip is the window drag region
- AC-11: the app launches frameless — no title bar or border of its own
- AC-11: Maximize maximizes the window and the control becomes Restore
- AC-11: Restore returns the window to its normal state
- AC-11: Close ends the app
- AC-11: the app relaunches against the same library
- AC-11: Minimize minimizes the window

**`flowKeyboardAudit`** — keyboard-shortcuts
- every toolbar control is keyboard-reachable (tabbable)

**`flowMenuBounds`** — browsing-and-filtering, tags · covers DR-122, AC-34
- AC-34: the Tags submenu panel lies wholly inside the viewport
- AC-34: with 30 tags the submenu scrolls (scrollHeight > clientHeight)
- AC-34: every tag in the submenu is reachable by pointer (fully visible at some scroll position)
- AC-34: the last tag item highlights on pointer hover at the bottom of the scroll
- AC-34: End in the submenu reaches the last item and focuses it
- AC-34: ArrowDown from the first item reaches every tag in the submenu
- AC-34: the View menu panel stays wholly inside the viewport
- AC-34: a row context menu opened beside the window bottom stays wholly inside the viewport

**`flowAppUpdate`** — keeping-up-to-date · covers FR-25, UI-52, AC-151, AC-152, AC-153, DR-329, DR-332 (v0.0.10; the fixture update endpoint at 127.0.0.1:14299 serves a manifest signed with the product key over a generated artifact; the binary carries the fixture updater config only when built with `BOOKKEEPER_UPDATE_FIXTURE=1` — `ui-verify.mjs` sets it for every lane, a standalone run without it hits the skip row below; the app version the flow compares against is read from the backend, never assumed)
- AC-151: a newer-version manifest surfaces the new version, its notes and an Install action
- UI-52: the affordance is a quiet toast with an action — never a modal
- AC-151: a current-version manifest makes the manual check report up to date
- AC-151: an unreachable update source surfaces the typed error as a toast
- AC-153: the main window activates within the released startup budget with the update check in flight — *runs once per scenario (reachable current manifest, unreachable endpoint); the check-completion barrier is observed between window creation and activation*
- AC-153: the startup check ran against the fixture endpoint (the barrier proves completion before activation)
- AC-153: nothing is shown — no affordance, no report, no progress — *runs once per scenario*
- UI-52: the startup check arms the availability affordance — the toast with the Install action
- AC-152: the download renders progress through the released job-progress affordance
- AC-152: the install completes by launching the installer, and the app exits for it (the plugin's install path ends with the app's exit, DR-329 item 4)
- *conditional skip:* `AC-152/AC-153: the update fixture build is not active in this binary` when the binary was not built with the fixture updater config

### 3.4 Reader flows (17, CDP runtime, own instance, multi tier imported through the app; the flows with EPUB cases build their own library state from the fixtures/epub tier and delete it again at their end)

**`flowReaderOpen`** — reading-pdfs · covers AC-64, AC-72, AC-76, AC-87, AC-93, DR-206, DR-215, DR-315, DR-316, DR-318, DR-319, DR-334, DR-341
- NFR-08 / AC-72: activation → first page (192-page core fixture) within 800 ms — *measured through the shared `timingRow()` helper, the pattern for all AC-72 rows below*
- AC-64: a double-click opens a reader window on page 1 with the true page count
- AC-64: Enter opens a reader window for the focused row
- AC-64: the Enter-opened window shows page 1
- AC-76: the control strip renders at the window's bottom edge below the page stack, the chrome strip at the top edge
- AC-87: the reader chrome name shows the app's own tooltip on the ramp's --surface-1 ground with its shadow
- DR-318: the first-paint page image node is tagged for the guard below
- DR-316 + DR-318: at open, the full raster arrives as a cross-fade overlay over the preview (base preview image stays at the preview width, overlay carries the full-stage width on the same rect) and the base image is the same node as at first paint — no rebuild, no re-decode gap; with coinciding stages (dpr 1) no overlay renders and the node is likewise never rebuilt — the guard is what makes the DR-319 staleness rule observable in the flow: a width-stale publish would tear the node down here
- DR-334 as repaired by DR-341 (two-stage path): after the release window the base carries the full raster and the overlay is never a broken image — its src stays a decodable transparent placeholder (naturalWidth > 0), so no settled page reaches the broken-glyph state
- AC-93: the reader window title property stays the full filename
- AC-93: hovering the ellipsized name opens the app tooltip carrying the full name
- AC-93: the chrome name shows in full while the strip has room and truncates with an ellipsis only on real overflow (DR-206)
- DR-315: the window controls keep their 44 px width while a long title ellipsizes, at the narrow viewport and after restoring
- AC-64: re-activating the same document opens no second window
- AC-64: a second document opens its own reader window
- AC-64: the library window stayed interactive throughout

**`flowReaderNavigation`** — reading-pdfs · covers AC-65, AC-72, UI-30, DR-156
- NFR-08 / AC-72: page jump → new image visible (192 pages) within 300 ms (page field 1 → 96)
- AC-65: the page field jumps to any page
- AC-65: next reaches the following page
- AC-65: previous returns to the prior page
- AC-65: PgDn scrolls the viewport and reaches the next page
- AC-65: PgUp returns to the page it left
- AC-65: End reaches the last page
- AC-65: only viewport-near pages are mounted (192-page document)
- AC-65: Home reaches the first page
- AC-65: navigation reaches page 1 000 of the scale fixture
- AC-65: the mounted-page bound holds at 1 000 pages

**`flowReaderZoom`** — reading-pdfs · covers AC-96, AC-72, AC-119, UI-30, DR-221, D23
- NFR-08 / AC-72: zoom change → new image visible within 300 ms (first step from fit width)
- AC-96: fit-width is the default — the page fills the content width and the readout names the mode
- AC-96: from fit width the button walk continues from the mode's effective percent — the step lands on the nearest ladder step below it at actual × percent / 100 (DR-221)
- AC-96: 75 % displays 0.75 × the page's actual size
- AC-96: 50 % displays 0.5 × the actual size
- AC-96: the walk clamps at 50 % — another step down does not leave the ladder
- AC-96: 125 % displays 1.25 × the actual size
- AC-96: 300 % displays 3 × the actual size
- AC-96: the walk clamps at 300 % — another step up does not leave the ladder
- AC-96: the zoom select offers exactly the fit modes and the fixed ladder
- AC-96: the select's Fit width item returns to the fit-width width
- AC-96: Actual Size renders the page's true dimensions
- AC-96: the select's 150 % item displays 1.5 × the actual size
- AC-96: Fit page renders the whole current page inside the viewport
- AC-96: Ctrl 0 selects Fit width
- AC-96: Ctrl 1 selects Fit page
- AC-96: Ctrl 2 selects Actual Size
- AC-96: Ctrl = walks to the nearest step above the mode's effective percent
- AC-96: Ctrl − walks one step down
- AC-96: fit-width re-derives when the window is resized
- AC-96: fit-width re-derives back when the size is restored
- AC-119: at the last page, Fit page renders the page top-aligned
- AC-119: fit → zoom out → fit leaves the last page anchored to the top edge (D23)

**`flowReaderClose`** — reading-pdfs · covers AC-67, DR-149
- AC-67: the reader page cache exists while the window is open
- AC-67: Esc closes the reader window
- AC-67: the reader cache directory is gone after close
- AC-67: closing one reader window leaves another document's window open
- AC-67: the window's Close control closes the reader window
- AC-67: the Close control removes that cache too
- AC-67: the library is unchanged while reader windows open and close
- AC-67: the library window is interactive throughout
- AC-67: deleting the document closes its reader window with the row

**`flowReaderErrors`** — reading-pdfs · covers AC-69, FR-12, AC-76, DR-149
- AC-69: a missing document yields a toast with Relink and no window
- AC-69: an unrenderable file shows the reader error state with the external action
- AC-76: the error state renders no control strip
- AC-69: the action performs the external open exactly once at the command boundary

**`flowReaderToc`** — reading-pdfs · covers AC-78, AC-98, AC-148, FR-18, FR-24, UI-33, UI-37, UI-39, UI-49, DR-207, DR-218, DR-223, DR-226, DR-228, DR-232, DR-236, DR-239, DR-241, DR-242, DR-243, DR-244, DR-327, DR-335, DR-336, DR-337, DR-342
- AC-78: with an outlined document open the ToC control renders first in the bottom strip, before the page-navigation group (DR-207)
- AC-78: the panel opens collapsed to the first level — top-level entries only, titles, right-aligned page numbers; the toolbar is one row carrying the single expand/collapse toggle in its initial expand-all state (DR-218, DR-223)
- AC-78: the toolbar row renders as chrome — the strip ground distinct from the panel ground with a bottom hairline — and the toggle is a compact, left-aligned, icon-only control in the strip's square icon grammar (DR-226 as amended by DR-244) — the box is asserted against the `--control-height` token (32×32); every toolbar-control read (click, hover point, label reads, hit guard) goes through the `.control` element and its accessible name
- AC-78: the panel is bounded inside the reader view — top edge below the chrome, reader-geometry max-height (DR-207)
- AC-78: the toggle's first activation shows the whole tree with each entry's depth indent, the panel staying open, and its label flips to the collapse action (DR-223)
- AC-78: with the tree overflowingly tall the toolbar row stays visible at the panel's top edge while the tree region scrolls to its bottom (DR-232) — the overflow forced by a temporary injected panel cap removed before the next check (DR-236)
- AC-78: the tree's scrollbar lies wholly inside the tree region below the toolbar row's bottom edge, and the panel box itself carries no scrollbar (DR-232)
- AC-78: pointing at the toolbar control opens the app's tooltip naming the action the control carries (DR-232, UI-37)
- AC-78: the toggle's second activation returns the panel to the first level and its label flips back (DR-223)
- AC-78: a click on the toolbar row outside the control does nothing — the panel stays open, the tree stays at the first level, the toggle mode untouched (DR-242) — the click point is measured after the row's rect repeats identically and hit-guarded with `elementFromPoint` (DR-243)
- AC-78: Enter on the highlighted toolbar row runs the control's whole-tree action — expand all, then collapse all — with the panel staying open and the control's accessible name following each flip (DR-242, DR-244)
- AC-78: the chevron expands its parent's subtree with no jump — panel open, page field unchanged
- AC-78: a per-entry expansion does not change the toggle's mode — it still names the expand action (DR-223)
- AC-78: with the panel open ArrowRight on the focused parent runs the panel's own entry key (collapse, then expand) while the page field stands — the arrows belong to the overlay (AC-98/DR-228)
- AC-78: after manual partial state the toggle applies its whole-tree action and flips its label (DR-223)
- AC-78: activating an entry jumps to its page (page field shows 61) and closes the panel
- AC-78: the jump lands on the page — scroll offset within 2 px of the page's computed offset
- AC-78: re-opening keeps the expansion state — the panel returns with the expanded subtree (session scope, DR-218) — and the toggle's mode with it (DR-223)
- AC-78: the toggle's next activation still collapses all after the manual per-entry expansion, and flips back (DR-223)
- AC-78: Esc closes the panel with the window still open
- AC-148: the toolbar row carries the pin control at its right end — icon-only with its name, the toggle affordance, unpressed while the panel is the released overlay (UI-49)
- AC-148: pinning closes the overlay and docks the panel at the left edge — from below the chrome strip to the control strip, at the fixed width; the strip control reflects the open panel (UI-49)
- AC-148: the page area re-lays out beside it — the content box shrinks by the panel column, fit-width follows the shrunken box, and serving delivers a page at the shrunken fit width (continuous, §3.8)
- AC-148: with the panel pinned an entry selection navigates and keeps the panel open (§3.8 — the overlay's close-on-select does not apply)
- AC-148: the pin holds across a mode change — in continuous horizontal the row occupies the shrunken content column beside the docked panel and serves the current page there (§3.8 modes rule)
- AC-148: the pin control's glyph follows the state — the slashed thumbtack in the unpinned overlay state, the plain thumbtack in the pinned state (UI-49, DR-336)
- AC-148: the docked panel's toolbar row renders flush — square corners and the 4 px vertical padding, the controls unchanged (UI-49, DR-335)
- AC-148: the pin control sits at the docked toolbar row's right end as well — its right edge coincides with the row's right content edge within 2 px (UI-49, DR-342)
- AC-148: returning to continuous keeps the docked panel beside the page stack (§3.8 modes rule)
- AC-148: the ToC control hides the pinned panel — no overlay opens, the pin is kept, and the control reflects the hidden panel (UI-49 as revised by DR-337)
- AC-148: the ToC control shows the panel again at the same docked position — the pinned layout is kept, the content column stays shrunk beside it (UI-49 as revised by DR-337)
- AC-148: with the panel pinned, Esc closes the pinned panel first — the window stays open and the pinned position is kept; the ToC control re-shows the panel docked (§3.8, the released guard extended to the docked state; DR-337)
- AC-148: the panel's pin control is the only un-pin path — its click closes the docked panel and the strip control carries the released overlay grammar again (UI-49 as revised by DR-337)
- AC-148: after unpinning, the panel opens again as the released anchored overlay (UI-49)
- AC-78: with multi-1000 open no ToC control is rendered

**`flowReaderViewModes`** — reading-pdfs · covers AC-79, AC-80, AC-92, AC-98, AC-142, UI-34, UI-39, DR-180, DR-181, DR-204, DR-205, DR-215, DR-219, DR-224, DR-227, DR-228
- AC-142: the mode control renders four choices with continuous active by default (re-pins AC-79's three-button control)
- AC-79: the page field reaches page 57 in continuous (the mode-switch starting point)
- AC-79: switching to single page mounts exactly one page, centered on the current page, at the fixture geometry
- AC-79: the container scrolls a fit-width page taller than the viewport to its bottom edge (DR-204)
- AC-79: the first wheel event at the bottom edge advances exactly one page — no accumulation (DR-227)
- AC-79: one wheel event never advances more than one page — further forward events scroll within the new page (DR-227)
- AC-79: the first wheel event at the top edge steps back exactly one page, landing at the previous page's bottom edge (DR-227)
- AC-79: next reaches the following page in single mode
- AC-79: previous returns to the exact page in single mode
- AC-79: the page field jumps to an exact page in single mode
- AC-79: while the page field is focused the navigation keys belong to it — the arrow keys included (the key-map guard, AC-98/DR-228)
- AC-79: PgDn moves one page in single mode
- AC-79: PgUp moves back one page in single mode
- AC-98: ArrowRight advances exactly one page in single mode, landing the same as the controls
- AC-98: ArrowDown inside a taller page scrolls the container by the notch, the page unchanged
- AC-98: ArrowDown at a reached bottom edge advances exactly one page — the keyboard counterpart of the wheel (DR-227)
- AC-98: ArrowUp at a reached top edge steps back exactly one page, landing at the previous page's bottom edge
- AC-98: ArrowLeft steps back exactly one page in single mode
- AC-79: End reaches the last page in single mode
- AC-79: with page 150 mounted and served, requests for every page of 144…156 are issued at the current width, and no page beyond the window is requested (DR-224)
- AC-79: the window is queued movement-direction-first — this jump came from 192, so the backward side leads: 149 is the first neighbour requested (DR-224)
- AC-79: a prev step from a served page presents the new page's image with no serving gap — no new request for 149 (DR-224)
- AC-79: the next step back presents page 150's image with no serving gap — no new request for 150 (DR-224)
- AC-79: a six-step forward walk inside the window stays gap-free — no new request names a landed page (DR-224)
- AC-92: single mode centers the page below fit width and spans the content box at fit width (DR-205)
- AC-79: switching back to continuous restores the released stack at the current page
- AC-98: ArrowDown in continuous scrolls the stack by the notch, the current page unchanged
- AC-98: ArrowRight in continuous jumps to the next page
- AC-98: ArrowLeft in continuous jumps back to the previous page
- AC-92: continuous centers each page below fit width and spans the content box at fit width (DR-205)
- AC-80: entering facing mode lands on the pair containing the current page and names its first page
- AC-80: facing lays consecutive pairs (1|2, 3|4, …) side by side at the fixture's page geometry
- AC-80: fit-width fits the pair to the viewport (each page ≤ (viewport − 12 px) / 2) with the standard 12 px pair gap
- AC-92: facing centers the pair row below fit width and spans the content box at fit width (DR-205)
- AC-80: next moves one pair in facing mode
- AC-80: PgDn moves one pair in facing mode
- AC-80: entering page 60 opens the pair containing it and the page field shows that pair's first page (59)
- AC-80 as superseded by FR-29: a new reader window opens in the last chosen mode — facing here (DR-409)
- AC-80: a 1-page document renders unpaired, alone, in facing mode

**`flowReaderHorizontal`** — reading-pdfs · covers AC-142…AC-147, FR-17, UI-34 (MODIFIED), UI-39 (MODIFIED), DR-290 (v0.0.10; the continuous-horizontal row, running next to the view-mode flow it extends)
- AC-142: the mode control carries four choices with continuous active by default
- AC-144: entering the row from continuous keeps the current page in the field
- AC-143: the row mounts the pages in one left-to-right line, consecutive in reading order, with the standard 12 px gap
- AC-146: fit-width in the row fits the current page to the viewport — each page spans the content box at the fixture's ratio
- AC-143: the row is vertically centered when it fits and start-aligned when taller — a taller page's top edge stays reachable (DR-205's rule on the other axis)
- AC-143: the page field names the page at the viewport's horizontal center
- AC-145: while the page field is focused the row's navigation keys belong to it (the released guard)
- AC-145: ArrowRight advances exactly one page along the row
- AC-145: ArrowLeft steps back exactly one page along the row
- AC-143: next steps one page along the row
- AC-143: previous steps back one page along the row
- AC-145 / AC-143: PgDn travels one viewport along the row and the field names the page at the new center
- AC-145: PgUp travels back one viewport along the row, the field back to the page at the center
- AC-145: ArrowDown in the row scrolls it by the notch, the current page unchanged
- AC-145: ArrowUp scrolls the row back by the notch, the current page unchanged
- AC-145: End reaches the last page in the row
- AC-147: only row-near pages are mounted at the document's end (the released mount bound, x-axis)
- AC-145: Home reaches the first page in the row
- AC-143: entering a number opens that page — page 60 is mounted and visible in the row
- AC-147: the serving window follows the row position — after the jump to 60 the requests cover the row window around it (54…67) and no page beyond the bound (the released window rule, x-axis)
- AC-146: the fit-width binding returns the row to the fit base
- AC-146: a zoom step applies to every page in the row at the same ratio — from fit (118.5 % effective) it lands on 100 % at actual × 100 / 100, and the readout names the step
- AC-144: switching from the row back to continuous keeps page 60 in view
- AC-144: entering the row from continuous keeps page 60 in view
- AC-144: switching from the row to facing lands on the pair containing page 60
- AC-144: switching back from facing to continuous keeps the current page (the pair's first, 59) in view
- AC-142 as superseded by FR-29: a new reader window opens in the last chosen mode — the row here (DR-409)

**`flowReaderSettings`** — reading-pdfs · covers AC-149, FR-24, UI-50 (v0.0.10; revised by DR-338: the settings menu in the reader window's chrome, the main window's complete menu; the main menu's item list is read from the main window itself, never hardcoded; the theme cycle is asserted against the windows' real ramps — the harness' OS theme is read at runtime, never assumed; extended by DR-344: the same control and menu asserted in the EPUB reader window — the flow imports and deletes the EPUB fixture itself — and the *Check for updates* item proven functional in a reader window against the flow's own fixture update server serving the current-version manifest; the flow participates in the fixture-build decision)
- AC-149: the reader chrome carries the settings control at its right end, immediately before the window controls — icon-only, named; the cover control precedes it in the cluster (UI-50 as revised by DR-338; cluster extended by UI-55, DR-409)
- AC-149: the control strip carries no settings control — the released controls only, the view-mode group last (UI-50 as revised by DR-338)
- UI-50: pointing at the settings control opens the app's tooltip naming it (UI-13)
- AC-149: the reader settings menu is the main window's settings menu, complete — the theme cycle item naming the theme in effect, Settings, Check for updates, About, in the main window's order (UI-50 as revised by DR-338)
- AC-149: the first activation advances the cycle to Light, the menu stays open and the item re-renders, the change applies live in the library window
- AC-149: the second activation advances the cycle to Dark — menu open, item re-rendered, every open window on the dark ramp
- AC-149: a reader window opened later carries the stored theme
- AC-149: the third activation advances the cycle to Auto and every open window returns to the OS-resolved ramp
- AC-149: the Settings item opens the settings dialog in the reader window itself (the shared menu component, the released dialog, DR-338)
- UI-50 + UI-49: with the panel pinned, the settings dialog renders above the docked panel — at the overlap point the topmost element is the dialog, not the panel (DR-343)
- AC-149: the EPUB reader's chrome carries the settings control at its right end, immediately before the window controls — the same control as the PDF reader's, the cover control in its cluster (UI-50 as revised by DR-344; cluster extended by UI-55, DR-409)
- AC-149: the EPUB reader's settings menu is the main window's settings menu, complete — the same item list, read against the main window's own menu (UI-50 as revised by DR-344)
- AC-149: the Check for updates item works in a reader window — the check runs against the update source and its result reports in that window (the reader-windows capability carries updater:default, DR-344)

**`flowProtected`** — password-protected-pdfs · covers AC-104, AC-105, FR-20, DR-266 a (v0.0.8; reader runtime, imports `fixtures/special/protected.pdf` itself and deletes it again)
- AC-104: a protected document renders the password prompt — password field, unchecked Remember box, Unlock action — not the error state
- AC-104: a wrong password keeps the prompt with an inline error and the window stays usable (the external action stands)
- AC-105: a stored password reaches the next open as a pre-filled prompt with a Remove password action (the stored-but-wrong edge, DR-266 a)
- AC-105: Remove password clears the stored value and the prompt returns empty
- AC-104: a correct password unlocks the document — the page stack and control strip render in the same window
- AC-105: an unlock with the checkbox unchecked stores nothing — the next open prompts with an empty field
- AC-104: the remembered unlock renders the reader in the same window
- AC-105: a remembered password opens the document directly — no prompt

**`flowReaderContextMenu`** — the-app-window · covers D20, AC-116 (v0.0.8; the reader half of the window-level guard)
- AC-116: the reader window prevents the browser menu at its chrome and its text field
- AC-116: the reader stays open and usable after the prevented events

**`flowReaderPerformance`** — reading-pdfs · covers AC-73, AC-75, NFR-09, NFR-08, DR-196, DR-202, D24/DR-271
- AC-73: the page-jump row measured in the performance flow reaches page 96
- AC-73: cold-scroll page serving costs ≤ 350 ms of app-process CPU per served page (budget calibrated to the measured method, DR-202; baseline M1 ≈ 447 ms/page; the output records the pipeline/plumbing breakdown)
- AC-73: every page the mount window requested is served by the end of the run (served-page count = requested count)
- AC-75: warm back-and-forth scroll serves 0 pages (the cache holds everything the mount window asks for)
- AC-75: warm-scroll cleanliness — app-process CPU ≤ 0.1 s, ScriptDuration ≤ 0.2 s, 0 dropped frames, 0 long tasks
- D24 / NFR-08: first-open activation stays inside the 800 ms budget across 10 fresh windows (the D24 measurement row, DR-271 — the run records the ten rows; run 1 is the process-cold open in a lane run)

**`flowEpubReader`** — reading-epubs · covers AC-125, AC-132…AC-137, AC-148, UI-48 (MODIFIED), UI-49, DR-327, DR-337, DR-342 (v0.0.9; builds its own library state from the fixtures/epub tier and deletes it again at its end; every activation goes through the real card double-click path, AC-64's discipline)
- AC-125: activating an EPUB opens the in-app reader window with the EPUB type in its route (never a PDF reader window)
- AC-125: no PDF reader machinery runs in the EPUB reader window — no page images, no zoom readout, no page field, no view-mode control
- AC-135: the strip carries exactly the three controls — the ToC control first (the package has a ToC), the position readout, the text-size control — and nothing else
- AC-132: activating the same EPUB again keeps one window per document — the activation re-runs the app's own command and no second window opens
- AC-135: at the book top the readout reports 0% of the first chapter
- AC-135: scrolled to the top of chapter 2 the readout reports the model's percent from the archive's entry sizes
- AC-135: inside a chapter the readout rises with the scroll and stays within the chapter's percent range
- AC-135: the jump field resolves 60% to the start of the chapter containing it (chapter 3) and mounts it
- AC-135: the text-size control steps the ladder and applies live (the reading column reflows at 20 px) and persists globally (the settings store carries 20)
- AC-135: the text-size walk returns to the default step and the setting follows
- AC-132: the ToC control presents the package's table of contents — every top-level entry with its title and target
- AC-132: the ToC tree nests the package's sub-entry (expandable, indented under its parent)
- AC-132: a ToC entry jumps to its chapter (chapter 4 mounts, the readout follows)
- AC-135: the ToC panel highlights the entry the reader's scroll currently sits on
- AC-148: the EPUB panel's toolbar row carries the same pin control at its right end (UI-48 MODIFIED / UI-49 — the shared panel, both hosts, no per-host branch)
- AC-148: pinning in the EPUB window docks the panel at the left edge from below the chrome strip to the control strip at the fixed width; the strip control reflects the open panel (UI-48 MODIFIED)
- AC-148: the pin control sits at the EPUB docked toolbar row's right end as well — its right edge coincides with the row's right content edge within 2 px (UI-48 MODIFIED, DR-342)
- AC-148: the reading column narrows with the box — the flow narrows by exactly the panel column, the mounted content stays fully inside the narrowed width and still renders (the reflowable surface follows the smaller box, §3.8)
- AC-148: with the box the binding constraint the column follows it — the flow narrows by the window delta and the reading column re-wraps at the narrowed width, content still rendered (the cap only guards line length, §3.8) — the narrower box is produced by the CDP `setMetrics` simulation (the AC-96 pattern) and the geometry is restored before the flow continues
- AC-148: with the panel pinned an entry selection jumps within the book — landing exactly in the narrowed layout — and keeps the pinned panel open
- AC-148: in the EPUB window the ToC control hides the pinned panel — no overlay opens, the pin is kept, and the control reflects the hidden panel (UI-48 MODIFIED as revised by DR-337)
- AC-148: in the EPUB window the ToC control shows the panel again at the same docked position — the pinned layout is kept, the flow stays narrowed beside it (UI-48 MODIFIED as revised by DR-337)
- AC-148: in the EPUB window Esc closes the pinned panel first — the window stays open and the pinned position is kept; the ToC control re-shows the panel docked (the released guard extended to the docked state, §3.8; DR-337)
- AC-148: in the EPUB window the panel's pin control is the only un-pin path — its click closes the docked panel and the strip control carries the released overlay grammar again (UI-48 MODIFIED as revised by DR-337)
- AC-148: after unpinning the panel opens again as the released overlay (UI-48: 'unpinning restores the released overlay behavior')
- AC-135: an internal link in the fixture jumps to its target chapter and fragment (chapter 3 mounts, #sec-32 lands at the top of the view)
- AC-135: a fragment-only link scrolls to the target inside the current chapter
- AC-135: an external link in book content routes to the OS default browser (the opener call is observed) and the reader window does not navigate
- AC-136: the fixture's image renders through the content protocol (the src is a protocol URL, the pixels arrive)
- AC-136: the fixture's stylesheet applies (the marker paragraph carries the author color through the delivery mechanism)
- AC-136: the fixture's embedded font is delivered through the content protocol and used by the rendered text (the request fires only for rendered text)
- AC-135: PageDown moves exactly one viewport
- AC-135: ArrowDown moves exactly one wheel notch (100 px)
- AC-135: the named exclusions stay excluded — ArrowRight and Ctrl+zoom keys do nothing to the flow
- AC-135: End reaches the book end — the flow bottom with the last chapter mounted and the readout at 100%
- AC-135: Home reaches the book start
- AC-136: no package content was copied to disk outside the delivery mechanism (the app data directory gains none of the archive's entry names)
- AC-137: none of the hostile corpus runs or loads — no body script mutation, no SVG script effect, no on-handler attribute, no javascript: href, no iframe, no script element
- AC-137: the data:-URI image renders through the content protocol
- AC-137: the author CSS url() to a remote host produces no request
- AC-133: a malformed EPUB shows the typed error with the Open-with-the-default-app action (never a silent open)
- AC-133: the error action opens the document through the external-open command (the OS-application path stays available)
- AC-134: the row's Open-in-external-viewer action routes an EPUB to the OS application (the app's own command fires)

**`flowEpubPerformance`** — reading-epubs · covers AC-138, NFR-08, AC-72 (v0.0.9, the DR-300 measured flow; takes fresh windows like the PDF measured flow before it; the budgets are the DR-300 rows, calibrated before the reading code)
- NFR-08 / AC-72: EPUB first paint (1 MB chapter, 3 fresh windows) within 800 ms (the DR-300 row 1 timing measurement — open → first visible text of chapter 1; the run records its rows)
- AC-138 / DR-300-4: the 1 MB-chapter window stays under the Nodes ceiling (12 000) and the heap delta ceiling (+40 MB) across its scroll
- AC-138 / DR-300-2: cold forward scroll on the 500-chapter book serves every chapter the mount window requested (no mounted chapter is empty; every fetch belongs to the run; remounts are served from the parse cache)
- AC-138 / DR-300-2: the cold scroll costs ≤ 100 ms of app-process CPU per chapter mounted during the run (the run reports its full detail)
- AC-138 / DR-300-4: the mounted DOM holds a window of spine items, not the book — flow elements ≤ 2 500 during the 500-chapter scroll
- AC-138 / DR-300-4: the 500-chapter window stays under the heap delta ceiling (+40 MB) across the scroll
- AC-138 / DR-300-3: the warm back-and-forth scroll re-requests 0 chapters it already served (the cache holds everything the mount window asks for)
- AC-138 / DR-300-3: warm-scroll cleanliness — app-process CPU ≤ 100 ms, ScriptDuration ≤ 200 ms, 0 dropped frames, 0 long tasks

**`flowReaderMemory`** — reading-pdfs, reading-epubs · covers FR-28, FR-29, AC-167, AC-168, AC-169, FR-14 (relink plumbing), FR-17 (EPUB start state), DR-407 (v0.0.12; the 24-page fixture + a relink to its 12-page copy + the EPUB case; the storage-state observations go through the typed reads — `app_get_settings` for the batch, `app_reader_position_get` for the stored value)
- FR-28: the released page field reaches page 17 (the position the memory must carry)
- AC-168: the position settles into storage after the reader stops — the debounced write landed
- FR-28: the reader window closes (the close-time flush rides this path)
- AC-167: a re-opened document resumes at the remembered page — the readout says 17 and no page 1 is painted
- AC-169: the mode choice persists through the settings batch
- AC-169: a new reader window opens in the last chosen view mode (a different document, the pressed control)
- FR-14 plumbing: the relink to the shorter copy is accepted and re-derives the automatic title (the position memory survives the relink, per §3.2)
- AC-167: a position beyond the document's new extent clamps to its last page (12 of 12)
- AC-167: the memory is removed with the document (the stored read answers document_not_found)
- FR-17: a fresh EPUB window starts at the top of the book (0% of the first chapter)
- AC-167: the position readout reaches a later chapter (the chapter + offset the memory carries)
- AC-168: the EPUB position settles into storage before close (the debounced write landed)
- AC-167: the re-opened EPUB resumes at the remembered chapter and offset (no chapter-1 stop)

**`flowReaderText`** — reading-pdfs, reading-epubs · covers FR-30, UI-54, AC-170, AC-171, AC-172, FR-17 (EPUB column mount) (v0.0.12; the `text-probe.pdf` selection fixture and the `image-only.pdf` fixture; the drag target is waited to a rect unchanged across two polls after a mode switch or zoom step, per DR-407)
- AC-170: drag-select across the fixture sentence yields exactly that text in all four view modes
- AC-170: the text layer follows the zoom ladder — the selection after a zoom step is the fixture text
- AC-170: a page without embedded text yields no layer and no message — empty runs at the command, no layer in the window, the page rendered
- FR-17: the EPUB reading column mounts its chapter paragraph
- AC-172: the EPUB reading column is selectable — the drag yields the fixture paragraph's text
- AC-172: the copy gesture leaves the reader and its selection intact (the released interactions unchanged)

**`flowCoverChoice`** — covers · covers FR-16 (automatic baseline), FR-31, UI-55, AC-177, AC-178, AC-180, AC-181 (v0.0.12; the 48-page fixture for the picker and the `epub-images.epub` three-image book for the EPUB case, both imported and deleted inside the flow)
- FR-16 baseline: the automatic cover exists before any choice
- UI-55: the reader chrome carries the cover menu with the current-page choice
- AC-180: the choice command returns while the render runs — the library window answers in ms and the reader stays interactive
- AC-177: the chosen page becomes the library cover — cover_path moves to the choice key and the PNG exists
- AC-177: the library card renders the chosen cover (the released cover-batch re-query)
- AC-177: the choice stands after the reader window closes (it is stored, not window state)
- UI-55: the reader cover menu carries the reset choice
- AC-177: "Automatic cover" restores the released automatic key
- AC-181: the picker opens with bounded windows only — never the full 48-page strip (re-anchored by DR-411 from the 12-tile pin, which was layout-bound: a taller content area can legitimately show the sentinel at open and load one follow-on window)
- AC-181: the strip fills the next window as it scrolls (lazy windows, the app responsive throughout — the growth is now measured against the opened state)
- UI-55: the toolbar bar stays pinned while the strip scrolls — the scrolling belongs to the content area (DR-410)
- UI-55: the dialog width fits the tile columns — a full row fills the content area's inner width, no sideways overflow (DR-410, re-anchored to the full-bleed area by DR-411)
- UI-55: the content area is full-bleed on the sides with the content inset unchanged, and its scrollbar stops clear of the rounded bottom corners (DR-411; corner clearance added by DR-412)
- AC-181: the strip's tiles name their pages (the pick target is addressable)
- AC-181: picking a page in the strip sets the choice and updates the card
- UI-55: the picker is a toolbar dialog — the title and the close control sit on the toolbar bar (DR-408)
- AC-181: "Automatic cover" is present in the picker, as a button on the toolbar
- AC-181: the picker's reset restores the automatic cover
- AC-178: the EPUB candidates are the book's embedded manifest images in the book's display order (DR-411)
- AC-178: the declared cover renders as the automatic cover
- AC-181: the picker's EPUB grid lists the book's embedded images
- AC-178: a chosen embedded image becomes the cover (composed through the released contain-fit path)
- AC-178: the reader's EPUB cover menu offers both choices
- AC-178: the grid opens from the reader menu too (the same dialog, the same three candidates)

### 3.5 Area → flow mapping (the `SMOKE_SET` vocabulary; suite-enforced against orphans)

|Area (= guide page name)|Flows|
|---|---|
|getting-started|flowStartup, flowDropAndSkip — *the bulk empty/skeleton case: `scripts/ui-10k.mjs` (DR-161)*|
|importing-pdfs|flowImportAssign, flowMetaColumn, flowImportProgress — *the 10 000-document suite: `scripts/ui-10k.mjs` (DR-159, DR-161)*|
|searching|flowSearchDebounce, flowSearchField|
|browsing-and-filtering|flowTags, flowTagsMulti, flowBrowseIcons, flowMenuBounds, flowQuietCounts|
|tags|flowTags, flowTagsMulti, flowTagCrud, flowMenuBounds, flowQuietCounts|
|favorites|flowFavorites|
|reading-collection|flowReading|
|opening-and-deleting|flowOpen, flowDelete, flowContextMenu|
|reading-pdfs|flowReaderOpen, flowReaderNavigation, flowReaderZoom, flowReaderClose, flowReaderErrors, flowReaderToc, flowReaderViewModes, flowReaderHorizontal, flowReaderSettings, flowReaderMemory, flowReaderText, flowReaderPerformance|
|reading-epubs|flowEpubReader, flowEpubPerformance, flowReaderMemory, flowReaderText|
|password-protected-pdfs|flowProtected|
|metadata-editing|flowMetadata, flowMetadataDisplay, flowMetaColumn, flowTooltipSurfaces|
|missing-files-relinking|flowMissingAndRelink|
|renaming-a-documents-file|flowRename|
|opening-containing-folder|flowOpenFolder|
|covers|flowCovers, flowCoverChoice|
|sorting-and-views|flowSortSecondary, flowViewColumns, flowContentGutter, flowRowDesign — *the scale mount bound + width matrix: `scripts/ui-10k.mjs` (DR-161)*|
|status-feedback|flowStatusBar, flowProgressTransform|
|selecting-documents|flowSelectMode, flowBulkFavorite|
|settings-and-appearance|flowThemeAndSettings, flowLogs, flowDarkRamp, flowTooltipSurfaces, flowAbout, flowScrollbars|
|the-app-window|flowDragRegion, flowWindowChrome, flowToolbarControls, flowActionParity, flowContextMenuSuppression, flowReaderContextMenu|
|keyboard-shortcuts|flowKeyboardAudit|
|keeping-up-to-date|flowAppUpdate|

---

## 4. 10 000-document suite — `scripts/ui-10k.mjs` (DR-159, consolidated by DR-161)

Separated runner holding **every UI test case against the 10 000-document corpus** — since AC-131 (SPEC-v0.0.9 §3.8) the corpus is the mixed one: 9 000 single-page PDFs + 1 000 single-chapter EPUBs from the docs tier, so every scale claim below runs against both document types. No regular ui-smoke run pays the corpus any more. Same app, same tauri-driver session; imports the shared plumbing (`prepareEnvironment`, `startDriver`, `openSession`, `runFlow`, `check`, tier helpers) **and the three exported scale flows** from `ui-smoke.mjs`, which only runs its own suite when executed as the program. Runs the mandatory prefix flows first (AC-60), then its four flows in fixed order: the measured import (it loads the corpus the scale flows then run against), virtualization, the width matrix, empty/skeleton last (it empties the library through the app's own bulk path). Exit 0/1.

**Run modes (DR-163).** No selection: the full fixed-order run above — the corpus is paid exactly once (DR-161), the measured import doubling as the corpus the scale flows run against. `SMOKE_10K_FLOW=<flow>[,<flow>…]`: selects exactly those flows (unknown names error out before any app work; the relative order above is preserved). In both modes the prefix flows run; with `SMOKE_PREFIX_QUIET=1` — set for every lane — they run as setup (passing checks log but are not recorded, failures are recorded). A selection that does not include `flowImport10k` seeds its library from the published shared corpus instead (DR-362): `prepareEnvironment` copies `tmp/corpus` into the lane's data directory before the app starts, `prepareCorpus` finds the library already holding the corpus (fixture-generation identity and document count verified against the manifest) and skips the import; the measured claims (AC-01 ×2, AC-22, NFR-02, FR-11, FR-15, AC-57) belong to the import lane's test case only. A corpus that is missing, unreadable, or built for a different fixture generation falls back to the pre-DR-362 behavior: the same import, check-quiet (internal asserts only, a deliberately more patient 90 s completion wait). The suite-level budget check records in **every** mode, including a single-flow lane.

**Suite-level check (DR-162):** wall-time budget — suite total ≤ 300 s, the same NFR-06/AC-58 assertion the core suite makes, now on this side too. Under `scripts/ui-verify.mjs` full mode each of the four flows is its own lane with its own data directory (`%APPDATA%\com.bookkeeper.app\parallel\lanes\flowImport10k` and `…\lanes\flowVirtualization` / `…flowWidthMatrix` / `…flowEmptyAndSkeleton`); the import group's barrier puts the import lane first, and that lane publishes the shared corpus after its measured import (DR-362): the checkpointed library files plus the identity manifest to `tmp/corpus`. The three scale lanes seed their own copies from it and skip the import — the corpus is imported **once** per full-mode run (measured: the seed's setup is seconds against the ~19 s import it replaces); a lane whose seed is missing or stale pays the check-quiet import as in the pre-DR-362 shape (§4 Run modes). The lane's own budget check is its single suite-level check (the +1 over its flow's call sites). In `--coarse` mode the whole suite runs as the `docs` lane (`…\parallel\docs`). Either way the run is scoped: its own driver ports, and cleanup that touches only its own process tree.

**`flowImport10k`** · covers UI-15, AC-22, AC-57, AC-01, NFR-02, FR-15, FR-11, AC-131, DR-134 — 8 checks
- AC-01 UI: 10,000 documents listed via the app *(count claim derived from the fixture tier size)*
- AC-22: the status bar shows the progress bar while a job runs
- AC-01 UI: whole import completes < 30 s (NFR-02)
- import result toast reports the batch (FR-15)
- AC-131: the 10 000-document corpus is the mixed one (9 000 PDF + 1 000 EPUB from the 5001..10000 band)
- AC-131: an EPUB of the mixed band imported with its declared metadata (title from the package)
- AC-01 UI: duplicate import is skipped with a notice (FR-11)
- AC-57: cover backfill throughput ≥ 2× the 27/s baseline (real app, 10 000-document corpus) — sampled from the run's own status-bar cover progress; the 27/s baseline is the v0.0.4 serial queue measured in SPEC-v0.0.5 §5

**`flowVirtualization`** — scale half of sorting-and-views *(self-sufficient: establishes its own full-list query)* · covers NFR-01
- NFR-01 UI: mounted document nodes stay bounded with 10,000 documents

**`flowWidthMatrix`** — scale half of sorting-and-views · covers AC-56, AC-35, AC-39, AC-59, DR-128 — *parameterized: runs per tested window width*
- AC-35/AC-39 (${width} px, Auto): the grid is the derived layout — cards and height exact, cover inside the card, one border, 12 px gutter, no overflow, mounted bounded
- AC-35/AC-39 (${width} px, manual 2 → ${minExp.columns} columns): the derived layout holds at the lower clamp edge
- AC-35/AC-39 (${width} px, manual 10 → ${maxExp.columns} columns): the derived layout holds at the upper clamp edge
- AC-39 (${width} px, list): the first row starts at the 12 px content inset and the meta column keeps the other edge clear

**`flowEmptyAndSkeleton`** — scale half of getting-started *(scale-only because "Select all N matching" exists only above 500 documents, DR-136)* · covers AC-49, AC-51, AC-115, UI-29, AC-50, AC-122 (the skeleton identity at the new row), UI-28 (UI-29 extended and AC-115 added in v0.0.8)
- empty-library flow: Ctrl+A opened the bulk bar
- the bulk bar's Select-all-matching selects the whole library
- the confirmation names the whole selection
- empty-library flow: the library is empty
- AC-49: the toast stack gap and vertical padding are multiples of 4 px
- AC-115 (DR-272/DR-274): both import actions render as equal neutral buttons — labels as shipped, no primary variant, each keycap combination below its own button
- AC-51 / AC-115: every keycap is monospace, on a surface-2 ground, with a hairline border and 6 px radius
- AC-49: the empty state's vertical rhythm, the action row gap and the per-action column gaps are multiples of 4 px
- AC-115 (DR-272): pressing the *Import folder* button opens the folder picker — the app requests it as a directory
- AC-115: Ctrl+I opens the file picker — the app requests it with the documents filter (UI-44 as modified: PDF and EPUB together)
- AC-115: Ctrl+O opens the folder picker — the app requests it as a directory
- AC-115 (DR-276): Ctrl+Shift+I opens neither picker — the plain import bindings ignore Shift
- AC-115: neither shortcut fires while a text field owns focus
- AC-50: the skeleton row is the real row block (157 px) with the real cover placeholder (100×141)
- AC-50: the meta placeholder reserves the fixed 96 px column at the row's inset

---

## 5. Support scripts (not tests; prerequisites for the layers above)

|Script|Role|
|---|---|
|`scripts/make-fixtures.mjs`|generates every fixture: docs tier (the AC-131 mixed corpus: 9 000 single-page PDFs + 1 000 single-chapter EPUBs — every fifth index from 5005 to 10000 is an EPUB carrying the same "Fixture Doc …" title/author shape, a band chosen so no named fixture path the flows reference individually changes type), core tier (100, equal byte sizes, one without Info, manifests for the harness), multi-page fixtures (12/24/48/96/192-page core + the 1 000-page scale fixture), and the special fixtures (encrypted-looking, malformed, non-PDF, the differently-named meta-column probe, the 72-character long-name fixture the AC-93 chrome-ellipsis half of `flowReaderOpen` imports and deletes, and the v0.0.12 selection fixtures (`text-probe.pdf` — 8 pages each carrying the known sentence "Page N of the selection probe document"; `image-only.pdf` — a real image XObject with no text operators; `dense-text.pdf` — 4 010 separate runs on one page for the cap proof; `epub-images.epub` — the three-image book "Image Plate Book" for the candidate grid). D15: fixtures are generated, never committed|
|`scripts/fetch-pdfium.mjs`|fetches the bundled pdfium the reader engine links against|
|`scripts/make-app-icon.mjs`|generates the packaged app icon|

---

## 6. Maintenance rule (AGENTS.md §2a; DR-160)

- **New feature:** while its feature spec's Technical design is implemented, add its rows to this inventory in the same change as the tests themselves — Rust `#[test]` names, Vitest items, new flows (and their area entries) with their check lists. The Build ledger stays the requirement→test authority; this file records the tests as they exist.
- **Bugfix:** the regression test it ships with (RULES R6 / verification workflow) gets a row here, marked with the bug's DR.
- **Removed or renamed test:** remove or rename its row in the same change; IDs in `covers` columns are never renumbered.
- **At the Ship commit:** the inventory is verified against the tree (the extraction is mechanical: `#[test]`, `it()/test()`, `check()/skip()` call sites) and the counts in the header table are corrected. This document is not frozen at Ship — unlike release specs it keeps living.
- The area table in §3.5 and the `AREAS` map in `ui-smoke.mjs` are one fact: whichever changes, both change together (the suite's AC-60 orphan check enforces the pairing at runtime). The scale halves of `getting-started` and `sorting-and-views` are verified by the 10k suite (§4), not by a named set (DR-161).
