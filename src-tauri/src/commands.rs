//! Tauri commands — thin wrappers only: validate → call service → return
//! data or a typed error payload (RULES R1.2). Long-running operations run
//! on blocking tasks so commands never block the UI thread (RULES R3.5).

use tauri::{AppHandle, Manager, State};

use crate::applog;
use crate::error::{AppError, AppErrorPayload};
use crate::epubsrv;
use crate::library::query::{self, Query};
use crate::library::service;
use crate::library::{
    CoverCandidateDto, DocumentDto, ImportResult, RescanResult, SearchPage, SettingsDto, TagDto,
};
use crate::pdf;
use crate::pdfmeta;
use crate::positions;
use crate::{Covers, Db, Reader, TauriSink};

/// The non-DB half of the command-boundary hook (FR-26 §3.2, M5, DR-354):
/// service calls that do not run through `Db::with` (the import/rescan
/// pipelines, opener calls, window plumbing) log their errors here with the
/// command name before the payload conversion — the same single format as
/// the DB path.
fn boundary<T>(cmd: &'static str, result: Result<T, AppError>) -> Result<T, AppErrorPayload> {
    result.map_err(|error| {
        applog::command_error(cmd, &error);
        error.payload()
    })
}

/// A hand-built boundary error (validation or an internal fault) logged
/// through the same format.
fn fail<T>(cmd: &'static str, error: AppError) -> Result<T, AppErrorPayload> {
    applog::command_error(cmd, &error);
    Err(error.payload())
}

/// Run a service call on a blocking thread with the app's shared state.
async fn background<T: Send + 'static>(
    app: AppHandle,
    cmd: &'static str,
    work: impl FnOnce(&Db, &Covers, &TauriSink) -> Result<T, AppErrorPayload> + Send + 'static,
) -> Result<T, AppErrorPayload> {
    tauri::async_runtime::spawn_blocking(move || {
        let db = app.state::<Db>();
        let covers = app.state::<Covers>();
        let sink = TauriSink::new(app.clone());
        work(db.inner(), covers.inner(), &sink)
    })
    .await
    .map_err(|join_error| {
        let error = AppError::Internal(format!("background job failed: {join_error}"));
        applog::command_error(cmd, &error);
        error.payload()
    })
    // The service call's own payload passes through unchanged.
    .and_then(std::convert::identity)
}

// The shared filter builder for `library_search` / `library_ids`; it mirrors
// their (already-allowed, R1.2) argument lists one-to-one — a struct here
// would add a type with no other use.
#[allow(clippy::too_many_arguments)]
fn build_query(
    q: String,
    tag_ids: Vec<i64>,
    favorites_only: bool,
    missing_only: bool,
    reading_only: bool,
    sort: String,
    dir: String,
    sort_secondary: String,
) -> Query {
    let (sort_key, sort_dir) = query::parse_sort(&sort, &dir);
    Query {
        q,
        tag_ids,
        favorites_only,
        missing_only,
        reading_only,
        sort: sort_key,
        sort_dir,
        sort_secondary: query::parse_sort_secondary(&sort_secondary),
    }
}

// ------------------------------------------------------------------ import

#[tauri::command]
pub async fn library_import_paths(
    app: AppHandle,
    paths: Vec<String>,
    tag_ids: Vec<i64>,
) -> Result<ImportResult, AppErrorPayload> {
    background(app, "library_import_paths", move |db, covers, sink| {
        // DR-233: no outer lock hold — the pipeline acquires the connection
        // itself, per short transaction.
        boundary(
            "library_import_paths",
            service::import_paths(db, sink, &covers.0, &paths, &tag_ids),
        )
    })
    .await
}

#[tauri::command]
pub async fn library_import_folder(
    app: AppHandle,
    dir: String,
    tag_ids: Vec<i64>,
) -> Result<ImportResult, AppErrorPayload> {
    background(app, "library_import_folder", move |db, covers, sink| {
        boundary(
            "library_import_folder",
            service::import_folder(db, sink, &covers.0, &dir, &tag_ids),
        )
    })
    .await
}

// ------------------------------------------------------------- search/list

// Flat argument lists here are the wire contract mirrored in `ui/src/api.ts`
// (spec §5.3.3); grouping them would change the command payload for no gain.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn library_search(
    app: AppHandle,
    db: State<'_, Db>,
    query: String,
    tag_ids: Vec<i64>,
    favorites_only: bool,
    missing_only: bool,
    reading_only: bool,
    sort: String,
    dir: String,
    sort_secondary: String,
    offset: i64,
    limit: i64,
) -> Result<SearchPage, AppErrorPayload> {
    let limit = limit.clamp(1, 500);
    let offset = offset.max(0);
    let query = build_query(
        query,
        tag_ids,
        favorites_only,
        missing_only,
        reading_only,
        sort,
        dir,
        sort_secondary,
    );
    let covers_dir = app.state::<Covers>().0.dir.clone();
    db.with("library_search", move |conn| {
        service::search_document(conn, &covers_dir, &query, offset, limit)
    })
}

// Same wire-contract rationale as `library_search` above.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn library_ids(
    db: State<'_, Db>,
    query: String,
    tag_ids: Vec<i64>,
    favorites_only: bool,
    missing_only: bool,
    reading_only: bool,
    sort: String,
    dir: String,
    sort_secondary: String,
) -> Result<Vec<i64>, AppErrorPayload> {
    let query = build_query(
        query,
        tag_ids,
        favorites_only,
        missing_only,
        reading_only,
        sort,
        dir,
        sort_secondary,
    );
    db.with("library_ids", move |conn| query::ids(conn, &query))
}

// --------------------------------------------------------------- documents

#[tauri::command]
pub fn library_get_document(
    db: State<'_, Db>,
    app: AppHandle,
    id: i64,
) -> Result<DocumentDto, AppErrorPayload> {
    let covers_dir = app.state::<Covers>().0.dir.clone();
    db.with("library_get_document", move |conn| {
        service::get_document(conn, &covers_dir, id)
    })
}

// Same wire-contract rationale as `library_search`.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn library_update_document(
    app: AppHandle,
    db: State<'_, Db>,
    id: i64,
    title: String,
    author: String,
    notes: String,
    favorite: bool,
    tag_ids: Vec<i64>,
) -> Result<DocumentDto, AppErrorPayload> {
    let covers_dir = app.state::<Covers>().0.dir.clone();
    let sink = TauriSink::new(app);
    db.with("library_update_document", move |conn| {
        service::update_document(
            conn,
            &sink,
            &covers_dir,
            id,
            &service::DocumentEdit {
                title: &title,
                author: &author,
                notes: &notes,
                favorite,
                tag_ids: &tag_ids,
            },
        )
    })
}

#[tauri::command]
pub fn library_set_document_tags(
    app: AppHandle,
    db: State<'_, Db>,
    document_id: i64,
    tag_ids: Vec<i64>,
) -> Result<(), AppErrorPayload> {
    db.with("library_set_document_tags", |conn| {
        service::set_document_tags(conn, &TauriSink::new(app.clone()), document_id, &tag_ids)
    })
}

#[tauri::command]
pub fn library_set_favorite(
    app: AppHandle,
    db: State<'_, Db>,
    id: i64,
    value: bool,
) -> Result<DocumentDto, AppErrorPayload> {
    let covers_dir = app.state::<Covers>().0.dir.clone();
    let sink = TauriSink::new(app);
    db.with("library_set_favorite", move |conn| {
        service::set_favorite(conn, &sink, &covers_dir, id, value)
    })
}

/// FR-19 (AC-121/AC-102): the Reading membership toggle, single ids and
/// batches through one command; the updated rows come back so the store
/// updates the selection in place (DR-266).
#[tauri::command]
pub fn library_set_reading(
    app: AppHandle,
    db: State<'_, Db>,
    ids: Vec<i64>,
    on: bool,
) -> Result<Vec<DocumentDto>, AppErrorPayload> {
    let covers_dir = app.state::<Covers>().0.dir.clone();
    let sink = TauriSink::new(app);
    db.with("library_set_reading", move |conn| {
        service::set_reading(conn, &sink, &covers_dir, &ids, on)
    })
}

/// FR-20 (AC-105): store or clear a document's password — the reader's
/// unlock mechanism and the *Remove password* action (DR-266). An empty
/// value clears the column. Thin wrapper; the service owns the semantics.
#[tauri::command]
pub fn library_set_password(
    app: AppHandle,
    db: State<'_, Db>,
    id: i64,
    password: String,
) -> Result<(), AppErrorPayload> {
    let covers = app.state::<Covers>();
    db.with("library_set_password", |conn| {
        service::set_password(conn, &TauriSink::new(app.clone()), &covers.0, id, &password)
    })
}

/// FR-31 (AC-177/AC-180): set the manual cover choice — the service validates
/// the value's grammar and its match with the document's file type, stores it,
/// and enqueues the render as background work; this command returns the updated
/// row while the cover pipeline catches up. Thin wrapper (R1.2).
#[tauri::command]
pub async fn app_cover_choice(
    app: AppHandle,
    id: i64,
    choice: String,
) -> Result<DocumentDto, AppErrorPayload> {
    background(app, "app_cover_choice", move |db, covers, _sink| {
        db.with("app_cover_choice", |conn| {
            service::set_cover_choice(conn, &covers.0, id, &choice)
        })
    })
    .await
}

/// FR-31 (AC-177): clear the choice — the automatic cover returns under the
/// released untagged key.
#[tauri::command]
pub async fn app_cover_clear(app: AppHandle, id: i64) -> Result<DocumentDto, AppErrorPayload> {
    background(app, "app_cover_clear", move |db, covers, _sink| {
        db.with("app_cover_clear", |conn| service::clear_cover_choice(conn, &covers.0, id))
    })
    .await
}

/// FR-31 (AC-178): the EPUB's decodable manifest images for the picker grid;
/// a PDF is refused — its picker asks `app_pdf_thumbs` instead.
#[tauri::command]
pub async fn app_cover_candidates(
    app: AppHandle,
    id: i64,
) -> Result<Vec<CoverCandidateDto>, AppErrorPayload> {
    background(app, "app_cover_candidates", move |db, _covers, _sink| {
        db.with("app_cover_candidates", |conn| service::cover_candidates(conn, id))
    })
    .await
}

#[tauri::command]
pub fn library_bulk_tags(
    app: AppHandle,
    db: State<'_, Db>,
    document_ids: Vec<i64>,
    add: Vec<i64>,
    remove: Vec<i64>,
) -> Result<(), AppErrorPayload> {
    db.with("library_bulk_tags", |conn| {
        service::bulk_tags(
            conn,
            &TauriSink::new(app.clone()),
            &document_ids,
            &add,
            &remove,
        )
    })
}

/// AC-67: deleting a document closes its reader window with the row. Window
/// handling is boundary plumbing (R1.2): the close runs on the main thread,
/// and the `Destroyed` handler (§3.2) removes the cache directory exactly as
/// any other close does. A close failure never fails the delete — a deleted
/// document's window has nothing left to read.
fn close_reader_windows(app: &AppHandle, ids: &[i64]) {
    let app = app.clone();
    let ids = ids.to_vec();
    let _ = app.clone().run_on_main_thread(move || {
        for id in &ids {
            if let Some(window) = app.get_webview_window(&format!("reader-{id}")) {
                let _ = window.close();
            }
        }
    });
}

#[tauri::command]
pub async fn library_delete_document(app: AppHandle, ids: Vec<i64>) -> Result<(), AppErrorPayload> {
    let reader_dir = app.state::<Reader>().dir.clone();
    let (window_app, window_ids) = (app.clone(), ids.clone());
    let result = background(app, "library_delete_document", move |db, covers, sink| {
        db.with("library_delete_document", |conn| {
            service::delete_documents(conn, sink, &covers.0, &ids, false, &reader_dir)
        })
    })
    .await;
    if result.is_ok() {
        close_reader_windows(&window_app, &window_ids);
    }
    result
}

#[tauri::command]
pub async fn library_delete_document_and_file(
    app: AppHandle,
    ids: Vec<i64>,
) -> Result<(), AppErrorPayload> {
    let reader_dir = app.state::<Reader>().dir.clone();
    let (window_app, window_ids) = (app.clone(), ids.clone());
    let result = background(app, "library_delete_document_and_file", move |db, covers, sink| {
        db.with("library_delete_document_and_file", |conn| {
            service::delete_documents(conn, sink, &covers.0, &ids, true, &reader_dir)
        })
    })
    .await;
    if result.is_ok() {
        close_reader_windows(&window_app, &window_ids);
    }
    result
}

// ------------------------------------------------------------------ relink

#[tauri::command]
pub fn library_relink_document(
    app: AppHandle,
    db: State<'_, Db>,
    id: i64,
    new_path: String,
) -> Result<DocumentDto, AppErrorPayload> {
    let covers = app.state::<Covers>();
    db.with("library_relink_document", |conn| {
        service::relink_document(conn, &TauriSink::new(app.clone()), &covers.0, id, &new_path)
    })
}

// ------------------------------------------------------------------ rename

/// FR-21 (AC-107…AC-110): rename the document's file in place. The service
/// owns validation, collision typing, the filesystem rename and the
/// compensating rollback; this is the thin wrapper (R1.2), the same shape
/// `library_relink_document` ships with.
#[tauri::command]
pub fn app_rename_document(
    app: AppHandle,
    db: State<'_, Db>,
    id: i64,
    new_name: String,
) -> Result<DocumentDto, AppErrorPayload> {
    let covers = app.state::<Covers>();
    db.with("app_rename_document", |conn| {
        service::rename_document(
            conn,
            &TauriSink::new(app.clone()),
            &covers.0,
            id,
            &new_name,
        )
    })
}

// ------------------------------------------------------------------ rescan

#[tauri::command]
pub async fn library_rescan(app: AppHandle) -> Result<RescanResult, AppErrorPayload> {
    background(app, "library_rescan", |db, covers, sink| {
        boundary("library_rescan", service::rescan(db, sink, &covers.0, false))
    })
    .await
}

// --------------------------------------------------------------------- tags

#[tauri::command]
pub fn tags_list(db: State<'_, Db>) -> Result<Vec<TagDto>, AppErrorPayload> {
    db.with("tags_list", |conn| service::tags_list(conn))
}

#[tauri::command]
pub fn tags_create(db: State<'_, Db>, name: String) -> Result<TagDto, AppErrorPayload> {
    db.with("tags_create", move |conn| service::tags_create(conn, &name))
}

#[tauri::command]
pub fn tags_rename(db: State<'_, Db>, id: i64, name: String) -> Result<TagDto, AppErrorPayload> {
    db.with("tags_rename", move |conn| service::tags_rename(conn, id, &name))
}

#[tauri::command]
pub fn tags_delete(db: State<'_, Db>, id: i64) -> Result<(), AppErrorPayload> {
    db.with("tags_delete", |conn| service::tags_delete(conn, id))
}

#[tauri::command]
pub fn tags_delete_all(db: State<'_, Db>) -> Result<usize, AppErrorPayload> {
    db.with("tags_delete_all", |conn| service::tags_delete_all(conn))
}

// -------------------------------------------------------------------- app

#[tauri::command]
pub fn app_get_settings(db: State<'_, Db>) -> Result<SettingsDto, AppErrorPayload> {
    db.with("app_get_settings", |conn| service::get_settings(conn))
}

#[tauri::command]
pub fn app_set_setting(
    app: AppHandle,
    key: String,
    value: String,
) -> Result<(), AppErrorPayload> {
    // DR-230: the service emits `settings_changed` on a successful save —
    // every window's settings store re-applies from that event.
    let db = app.state::<Db>();
    let sink = TauriSink::new(app.clone());
    let result = db.with("app_set_setting", |conn| {
        service::set_setting(conn, &sink, &key, &value)
    });
    // FR-26 (M6): the log level is a settings key like any other, and the
    // settings-write path is where the cached flag moves.
    if result.is_ok() && key == "log_level" {
        let _ = applog::set_level(&value);
    }
    result
}

#[tauri::command]
pub fn app_open_document(app: AppHandle, id: i64) -> Result<(), AppErrorPayload> {
    let path = {
        let db = app.state::<Db>();
        db.with("app_open_document", |conn| service::open_target(conn, id))?.path
    };
    boundary(
        "app_open_document",
        tauri_plugin_opener::open_path(&path, None::<&str>).map_err(|error| AppError::Io {
            path,
            message: error.to_string(),
        }),
    )
}

// -------------------------------------------------------------------- reader

/// Payload of `app_open_reader_window` (§3.2): whether the window was created
/// or focused — bookkeeping for the caller; the page data flows through
/// `app_pdf_document` / `app_pdf_page` regardless of which happened.
#[derive(Debug, Clone, serde::Serialize)]
pub struct OpenedReaderDto {
    pub opened: &'static str,
}

/// UI-31/FR-17 activation entry (§3.2): resolve the target through
/// `service::open_target` FIRST, so a missing file yields the typed
/// `MissingFile` error before any window exists (FR-08's error path
/// unchanged); then create the `reader-<id>` window — or focus the existing
/// one, since re-activation is focus, not a second window (§3.3). Creation
/// is Rust-side: the library frontend gains no window capability, so R1.5
/// keeps its single meaning. **Async command on purpose** (R1.2): Tauri runs
/// async commands off the main thread, and `WebviewWindowBuilder::build`
/// deadlocks on Windows when called on the main thread — the documented
/// WebView2 issue; a sync command here would freeze the app (observed in
/// the harness probe). The only DB read is the target resolve; the rest is
/// window plumbing.
#[tauri::command]
pub async fn app_open_reader_window(
    app: AppHandle,
    id: i64,
) -> Result<OpenedReaderDto, AppErrorPayload> {
    let target = {
        let db = app.state::<Db>();
        db.with("app_open_reader_window", |conn| service::open_target(conn, id))?
    };
    let label = format!("reader-{id}");
    if let Some(existing) = app.get_webview_window(&label) {
        return boundary(
            "app_open_reader_window",
            existing
                .set_focus()
                .map_err(|error| AppError::Internal(format!("focus reader window: {error}"))),
        )
        .map(|()| OpenedReaderDto { opened: "focused" });
    }
    let reader_dir = app.state::<Reader>().dir.clone();
    // DR-303 item 1 (SPEC-v0.0.9 §3.5): the type branch happens here, on the
    // resolved path, before the window exists — the window URL names the kind
    // the route renders, so no PDF reader machinery can run for an EPUB and
    // none for a PDF (AC-125). Same window kind, same plumbing.
    let kind = if epubsrv::is_epub_path(&target.path) {
        "epub"
    } else {
        "pdf"
    };
    let built = tauri::WebviewWindowBuilder::new(
        &app,
        &label,
        tauri::WebviewUrl::App(format!("index.html?reader={id}&kind={kind}").into()),
    )
    .title(&target.filename) // DR-140: the window title is the document filename
    .decorations(false) // UI-30/OQ-40-B: frameless like the shipped main window
    .resizable(true)
    .inner_size(1000.0, 700.0)
    .min_inner_size(640.0, 480.0)
    // §3.3: OS cascade placement — `center()` is deliberately not called.
    .build();
    let window = match built {
        Ok(window) => window,
        Err(error) => {
            // Activation race: a concurrent call created this label between
            // the check above and the build. Focus it — re-activation is
            // focus, never an error to the user (§3.3).
            match app.get_webview_window(&label) {
                Some(existing) => {
                    let _ = existing.set_focus();
                    return Ok(OpenedReaderDto { opened: "focused" });
                }
                None => {
                    return fail(
                        "app_open_reader_window",
                        AppError::Internal(format!("create reader window: {error}")),
                    );
                }
            }
        }
    };
    // Window-close cleanup (§3.2/§3.3): the document's session cache goes
    // with its window — the close control, `Esc` and the OS close all end in
    // `Destroyed`. A cleanup failure never fails the close (R5.3 discipline):
    // the next launch sweep removes what a failed one leaves behind.
    window.on_window_event(move |event| {
        if matches!(event, tauri::WindowEvent::Destroyed) {
            pdf::remove_document_cache(&reader_dir, id);
        }
    });
    // FR-26 Info lifecycle (OQ-67 A): the open side; the close side is the
    // `Destroyed` hook in `lib.rs`'s global window handler.
    applog::info("lifecycle", &format!("reader window opened: {label}"));
    Ok(OpenedReaderDto { opened: "created" })
}

/// The document geometry the reader view needs for exact page sizing (§3.2):
/// resolved through `service::open_target` first — a file deleted after the
/// window opened yields the typed error, which the reader window shows as
/// its error state (UI-31).
#[tauri::command]
pub async fn app_pdf_document(app: AppHandle, id: i64) -> Result<pdf::PdfDocDto, AppErrorPayload> {
    let render = app.state::<Reader>().render.clone();
    background(app, "app_pdf_document", move |db, _covers, _sink| {
        let target = db.with("app_pdf_document", |conn| service::open_target(conn, id))?;
        // FR-20: the row's stored password rides into the load; a protected
        // document yields the typed `password-required` payload with it.
        boundary(
            "app_pdf_document",
            pdf::document_dto(&render, &target.path, target.password.as_deref()),
        )
    })
    .await
}

/// Serve one reader page (§3.2): `page` is 1-based; `target_width` is
/// validated at the boundary (R1.2). The render goes through the single
/// pdfium owner; the PNG encode and cache write happen on the blocking
/// thread (R3.5); the payload is the cache path (R5.4).
#[tauri::command]
pub async fn app_pdf_page(
    app: AppHandle,
    id: i64,
    page: u32,
    target_width: u32,
) -> Result<String, AppErrorPayload> {
    if page == 0 {
        return fail(
            "app_pdf_page",
            AppError::Validation("page must be 1 or greater".into()),
        );
    }
    if !(100..=4000).contains(&target_width) {
        return fail(
            "app_pdf_page",
            AppError::Validation("target width must be between 100 and 4000 px".into()),
        );
    }
    let (render, reader_dir) = {
        let reader = app.state::<Reader>();
        (reader.render.clone(), reader.dir.clone())
    };
    background(app, "app_pdf_page", move |db, _covers, _sink| {
        let target = db.with("app_pdf_page", |conn| service::open_target(conn, id))?;
        boundary(
            "app_pdf_page",
            pdf::serve_page(
                &render,
                &reader_dir,
                id,
                &target.path,
                page,
                target_width,
                target.password.as_deref(),
            ),
        )
    })
    .await
}

/// FR-30 (AC-169): one page's embedded text — selectable runs in top-origin
/// page-point coordinates plus the page size in points (the §3.3 DTO). Served
/// from the reader directory's per-document text cache; extraction runs on the
/// render service's thread off the UI thread (R3.5). A page without embedded
/// text answers an empty run list — the reader shows no layer, no message.
/// A protected document answers the released `password-required` payload.
#[tauri::command]
pub async fn app_pdf_text(
    app: AppHandle,
    id: i64,
    page: u32,
) -> Result<pdf::PageText, AppErrorPayload> {
    if page == 0 {
        return fail("app_pdf_text", AppError::Validation("page must be 1 or greater".into()));
    }
    let (render, reader_dir) = {
        let reader = app.state::<Reader>();
        (reader.render.clone(), reader.dir.clone())
    };
    background(app, "app_pdf_text", move |db, _covers, _sink| {
        let target = db.with("app_pdf_text", |conn| service::open_target(conn, id))?;
        boundary(
            "app_pdf_text",
            pdf::serve_text(
                &render,
                &reader_dir,
                id,
                &target.path,
                page,
                target.password.as_deref(),
            ),
        )
    })
    .await
}

/// §3.8 (AC-181): a window of cover-picker page thumbnails at the picker's
/// fixed width (a server-side constant — the command carries only the page
/// window). Served through the same two-stage path as reader pages, under the
/// reader directory's `picker/` subdirectory (DR-404), returned as file paths
/// in page order. A malformed window is rejected before any rendering.
#[tauri::command]
pub async fn app_pdf_thumbs(
    app: AppHandle,
    id: i64,
    from: u32,
    to: u32,
) -> Result<Vec<String>, AppErrorPayload> {
    let (render, reader_dir) = {
        let reader = app.state::<Reader>();
        (reader.render.clone(), reader.dir.clone())
    };
    background(app, "app_pdf_thumbs", move |db, _covers, _sink| {
        let target = db.with("app_pdf_thumbs", |conn| service::open_target(conn, id))?;
        boundary(
            "app_pdf_thumbs",
            pdf::serve_thumbs(
                &render,
                &reader_dir,
                id,
                &target.path,
                from,
                to,
                target.password.as_deref(),
            ),
        )
    })
    .await
}

/// FR-28 (AC-166): the stored reading position of a document — the stored
/// value or null. Deliberately no `settings_changed` event (§3.2): a position
/// is per-document state, not a window setting.
#[tauri::command]
pub async fn app_reader_position_get(
    app: AppHandle,
    id: i64,
) -> Result<Option<String>, AppErrorPayload> {
    background(app, "app_reader_position_get", move |db, _covers, _sink| {
        db.with("app_reader_position_get", |conn| positions::get(conn, id))
    })
    .await
}

/// FR-28 (AC-167): write a reading position. The value grammar and the id's
/// existence are validated at the storage boundary (`positions::set`); clamping
/// to the document's current extent is the reader's job on open (§3.2).
#[tauri::command]
pub async fn app_reader_position_set(
    app: AppHandle,
    id: i64,
    value: String,
) -> Result<(), AppErrorPayload> {
    background(
        app,
        "app_reader_position_set",
        move |db, _covers, _sink| {
            db.with("app_reader_position_set", |conn| {
                positions::set(conn, id, &value)
            })
        },
    )
    .await
}

/// Outline tree for the reader's table of contents (FR-18, SPEC-v0.0.7 §3.5):
/// resolved through `service::open_target` first — a missing row yields the
/// typed `MissingFile` payload before any file is read (AC-77). Structure
/// reading never errors on partial data: a document without an outline
/// returns the empty list; an unparseable file yields `PdfUnopenable`.
#[tauri::command]
pub async fn app_pdf_outline(
    app: AppHandle,
    id: i64,
) -> Result<Vec<pdfmeta::OutlineEntry>, AppErrorPayload> {
    background(app, "app_pdf_outline", move |db, _covers, _sink| {
        let target = db.with("app_pdf_outline", |conn| service::open_target(conn, id))?;
        boundary("app_pdf_outline", pdfmeta::outline(&target.path))
    })
    .await
}

/// Reading manifest for the EPUB reader window (§3.9 consequence 8, FR-23,
/// DR-301 item 11): resolved through `service::open_target` first — a
/// missing row or file yields the typed payload before any file read
/// (the AC-77 pattern) — then the extension check (a PDF document asked for
/// an EPUB manifest is the typed `UnsupportedType`), then the reading-stage
/// package read (an unreadable package is `EpubUnopenable`, DR-298). The
/// reading facts are read once at first open, never by import (§3.12 rule 1).
#[tauri::command]
pub async fn app_epub_manifest(
    app: AppHandle,
    id: i64,
) -> Result<epubsrv::ManifestDto, AppErrorPayload> {
    background(app, "app_epub_manifest", move |db, _covers, _sink| {
        let target = db.with("app_epub_manifest", |conn| service::open_target(conn, id))?;
        if !epubsrv::is_epub_path(&target.path) {
            return fail("app_epub_manifest", AppError::UnsupportedType { path: target.path });
        }
        boundary(
            "app_epub_manifest",
            epubsrv::manifest(id, &target.path),
        )
    })
    .await
}

/// Application identity for the About dialog (UI-12). No service module and no
/// business logic: these are Tauri's own package metadata — `tauri.conf.json`
/// at runtime — which is the release's single version source (DR-79, R1.6).
#[derive(Debug, Clone, serde::Serialize)]
pub struct AboutDto {
    pub name: String,
    pub version: String,
}

#[tauri::command]
pub fn app_about(app: AppHandle) -> Result<AboutDto, AppErrorPayload> {
    let info = app.package_info();
    Ok(AboutDto {
        name: info.name.clone(),
        version: info.version.to_string(),
    })
}

/// Read-only location of the app's own data directory (UI-21, DR-111,
/// DR-162): reads the `DataDir` state `lib.rs` resolved at setup — the exact
/// directory the app places `library.db` and `covers/` in, override included.
/// No state write, no business logic (R1.2, R1.4).
#[tauri::command]
pub fn app_data_dir(data: State<'_, crate::data_dir::DataDir>) -> Result<String, AppErrorPayload> {
    Ok(data.0.to_string_lossy().into_owned())
}

/// Open the app's own data directory in the OS file manager (UI-21, DR-191):
/// takes the managed `DataDir` state — the one source the app writes to
/// (R1.4, DR-162) — and hands it to the already-bundled opener plugin
/// (`app_open_document`'s released shape); a failure maps to `AppError::Io`
/// with the path. No business logic (R1.2), no new dependency, no capability
/// change — a Rust-side call does not route through the JS ACL.
#[tauri::command]
pub fn app_open_data_dir(data: State<'_, crate::data_dir::DataDir>) -> Result<(), AppErrorPayload> {
    boundary(
        "app_open_data_dir",
        tauri_plugin_opener::open_path(&data.0, None::<&str>).map_err(|error| AppError::Io {
            path: data.0.to_string_lossy().into_owned(),
            message: error.to_string(),
        }),
    )
}

/// UI-40 (AC-111): open the document's folder in the system file manager
/// with the file selected, through the already-bundled opener plugin's
/// reveal capability (M6; the permission added to the capability file).
/// Resolution is `service::open_target` — the same typed resolution
/// `app_open_reader_window` uses: a gone row or file yields
/// `document_not_found` / `missing_file` before any external call (R1.2).
#[tauri::command]
pub fn app_open_containing_folder(app: AppHandle, id: i64) -> Result<(), AppErrorPayload> {
    let path = {
        let db = app.state::<Db>();
        db.with("app_open_containing_folder", |conn| service::open_target(conn, id))?.path
    };
    boundary(
        "app_open_containing_folder",
        tauri_plugin_opener::reveal_item_in_dir(&path).map_err(|error| AppError::Io {
            path,
            message: error.to_string(),
        }),
    )
}

// ---------------------------------------------------------------------- logs

/// FR-26 (§3.2): persist the log level through the settings store —
/// `log_level` joins `SETTINGS_KEYS`, so validation is the settings
/// grammar and the `settings_changed` event keeps every window's mirror in
/// step (DR-230) — and refresh the cached flag on the same path. Thin
/// wrapper; the service owns the storage semantics.
#[tauri::command]
pub fn log_level_set(
    app: AppHandle,
    db: State<'_, Db>,
    level: String,
) -> Result<(), AppErrorPayload> {
    let sink = TauriSink::new(app);
    let result = db.with("log_level_set", |conn| {
        service::set_setting(conn, &sink, "log_level", &level)
    });
    if result.is_ok() {
        // The value was validated by the settings grammar; `set_level`
        // re-validates (its own grammar) and moves the cached flag.
        let _ = applog::set_level(&level);
    }
    result
}

/// FR-26: the log file path for the Settings *Log* section's read-only row.
/// Reads the same handle the writer owns — the path display and the open
/// action can never disagree (R1.4).
#[tauri::command]
pub fn log_path() -> Result<String, AppErrorPayload> {
    match applog::file_path() {
        Some(path) => Ok(path.to_string_lossy().into_owned()),
        None => fail("log_path", AppError::Internal("logging not initialized".into())),
    }
}

/// FR-26: open the log file in the OS handler — the released
/// `app_open_data_dir` shape: a Rust-side opener call (no capability
/// change, M7 audit), creating the empty file first so the action never
/// fails on a clean install (§3.2).
#[tauri::command]
pub fn log_open() -> Result<(), AppErrorPayload> {
    let path = match applog::ensure_file() {
        Ok(path) => path,
        Err(error) => return fail("log_open", error),
    };
    boundary(
        "log_open",
        tauri_plugin_opener::open_path(&path, None::<&str>).map_err(|error| AppError::Io {
            path: path.to_string_lossy().into_owned(),
            message: error.to_string(),
        }),
    )
}
