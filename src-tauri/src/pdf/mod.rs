//! The single owner of pdfium (FR-17, SPEC-v0.0.6 §3.1). The bundled
//! binary is not safe for concurrent calls (DR-134, pdfium-render #262)
//! and the process-global bind can be taken once, so exactly one service
//! thread binds pdfium and serves every render request: the cover
//! pipeline's render stage and the reader commands are its clients.
//! Requests queue with interactive work (reader pages, document info)
//! taken ahead of background work (covers); each resolves exactly once on
//! its one-shot reply channel. A missing binary resolves every request to
//! `None` — covers keep the glyph (R5.3), the reader reports the typed
//! error (§3.2) — the degraded path released in v0.0.2, unchanged.
//!
//! The service is a process singleton (`shared()`): the bind is
//! process-global, so a process can host exactly one service — the app as
//! well as the test process, which is what keeps rendering deterministic
//! under parallel tests.
//!
//! v0.0.8 (FR-20, DR-266): every request carries the document's stored
//! password and the load presents it to pdfium. A protected document whose
//! password is absent or wrong resolves to `LoadFailure::PasswordRequired`
//! — a state distinct from `Unreadable`, which is what the reader's unlock
//! form keys on (AC-104).
use std::collections::VecDeque;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex, OnceLock};

use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::ImageEncoder;
use pdfium_render::prelude::{
    PdfDocument, PdfRenderConfig, Pdfium, PdfiumError, PdfiumInternalError,
};

use crate::error::AppError;

/// §3.1 rendering math: PDF pages are measured in points (72/inch), CSS
/// pixels are 96/inch. A page request's `target_width` is CSS px; the
/// scale it resolves to is bounded to `[0.2, 4.0]` — UI-30's zoom bounds
/// plus the 2× device-pixel cap.
const CSS_PER_POINT: f32 = 4.0 / 3.0;
const MIN_PAGE_SCALE: f32 = 0.2;
const MAX_PAGE_SCALE: f32 = 4.0;

/// Page geometry in PDF units, exactly as pdfium reports it (§3.2's
/// `PdfDocDto` source; AC-61 asserts the values match the file).
pub struct PageInfo {
    pub width_pt: f32,
    pub height_pt: f32,
}

pub struct DocumentInfo {
    pub page_count: u32,
    pub pages: Vec<PageInfo>,
}

type ImageReply = mpsc::Sender<Result<image::DynamicImage, LoadFailure>>;
type InfoReply = mpsc::Sender<Result<DocumentInfo, LoadFailure>>;
type TextReply = mpsc::Sender<Result<PageText, LoadFailure>>;

/// Why a load failed (FR-20, AC-104): a missing or wrong password is a
/// distinct state from an unreadable file — the reader prompts for a
/// password only for the first; the second keeps the existing error state.
#[derive(Debug)]
pub enum LoadFailure {
    /// pdfium refused the file because it is encrypted and the password
    /// presented was absent or wrong (`FPDF_ERR_PASSWORD`).
    PasswordRequired,
    /// The binary is missing, the file is damaged, or the page is out of
    /// range — every failure that is not a password state.
    Unreadable,
}

enum Request {
    /// One reader page: `page` is 1-based (AC-62), `target_width` CSS px.
    Page {
        path: String,
        page: u32,
        target_width: u32,
        password: Option<String>,
        reply: ImageReply,
    },
    /// Document geometry: page count and per-page sizes in points. The
    /// mechanism §3.2's `app_pdf_page`/`app_pdf_document` need, served on
    /// the same single-owner thread.
    Info {
        path: String,
        password: Option<String>,
        reply: InfoReply,
    },
    /// Cover image: a page at the cover pipeline's fixed target — the
    /// released rendering math, unchanged (AC-63). `page` is 0-based; 0 is
    /// the automatic cover (FR-31 extends it to a chosen page, §3.8).
    Cover {
        path: String,
        page: u32,
        width: u32,
        max_height: u32,
        password: Option<String>,
        reply: ImageReply,
    },
    /// One page's embedded text (FR-30, §3.3): the selectable runs plus the
    /// page size in points. Interactive tier — a reader request.
    Text {
        path: String,
        page: u32,
        password: Option<String>,
        reply: TextReply,
    },
}

/// The service's queue: interactive requests are taken ahead of queued
/// cover jobs (§3.1 — interactive work beats background work).
#[derive(Default)]
struct Queue {
    interactive: VecDeque<Request>,
    covers: VecDeque<Request>,
}

impl Queue {
    fn take(&mut self) -> Option<Request> {
        self.interactive
            .pop_front()
            .or_else(|| self.covers.pop_front())
    }
}

pub struct RenderService {
    queue: Arc<Mutex<Queue>>,
    wake: Arc<Condvar>,
}

static SERVICE: OnceLock<Arc<RenderService>> = OnceLock::new();
/// The extra bind search directories the app injects at setup (D34, §3.6);
/// plain data — the `pdf` module imports no Tauri type (R1.1).
static EXTRA_BIND_DIRS: OnceLock<Vec<PathBuf>> = OnceLock::new();

/// Starting the service with the extra bind search directories
/// (`lib.rs` passes the managed resource directory's `binaries`). The
/// first call wins; every later call — like `shared()` — returns the
/// same service.
pub fn start_service(extra_bind_dirs: Vec<PathBuf>) -> Arc<RenderService> {
    let _ = EXTRA_BIND_DIRS.set(extra_bind_dirs);
    RenderService::shared()
}

impl RenderService {
    /// The one service this process may have (the bind is process-global).
    /// `lib.rs` starts it at setup; tests and services use the same one.
    pub fn shared() -> Arc<RenderService> {
        SERVICE.get_or_init(RenderService::start).clone()
    }

    fn start() -> Arc<RenderService> {
        let queue = Arc::new(Mutex::new(Queue::default()));
        let wake = Arc::new(Condvar::new());
        {
            let queue = queue.clone();
            let wake = wake.clone();
            std::thread::spawn(move || serve(&queue, &wake));
        }
        Arc::new(RenderService { queue, wake })
    }

    fn submit(&self, request: Request, interactive: bool) {
        {
            let mut queue = self
                .queue
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if interactive {
                queue.interactive.push_back(request);
            } else {
                queue.covers.push_back(request);
            }
        }
        self.wake.notify_one();
    }

    /// Renders one page (1-based) at a CSS-px target width, scale bounded
    /// per §3.1. `Err(LoadFailure)`: no binary, unreadable file, or page
    /// out of range is `Unreadable`; a protected document with no or wrong
    /// password is `PasswordRequired` (FR-20) — the caller maps each to its
    /// degraded outcome (typed error, unlock form).
    pub fn render_page(
        &self,
        path: &str,
        page: u32,
        target_width: u32,
        password: Option<&str>,
    ) -> Result<image::DynamicImage, LoadFailure> {
        let (reply, result) = mpsc::channel();
        self.submit(
            Request::Page {
                path: path.to_string(),
                page,
                target_width,
                password: password.map(str::to_string),
                reply,
            },
            true,
        );
        result.recv().unwrap_or(Err(LoadFailure::Unreadable))
    }

    /// Page count and per-page sizes in points, or the typed load failure.
    pub fn document_info(
        &self,
        path: &str,
        password: Option<&str>,
    ) -> Result<DocumentInfo, LoadFailure> {
        let (reply, result) = mpsc::channel();
        self.submit(
            Request::Info {
                path: path.to_string(),
                password: password.map(str::to_string),
                reply,
            },
            true,
        );
        result.recv().unwrap_or(Err(LoadFailure::Unreadable))
    }

    /// One page's embedded text (1-based), top-origin page-point runs
    /// (§3.3). Interactive tier like page rendering; the same load failure
    /// classification, so a protected document answers `PasswordRequired`
    /// and the reader's released unlock path applies to text too.
    pub fn page_text(
        &self,
        path: &str,
        page: u32,
        password: Option<&str>,
    ) -> Result<PageText, LoadFailure> {
        let (reply, result) = mpsc::channel();
        self.submit(
            Request::Text {
                path: path.to_string(),
                page,
                password: password.map(str::to_string),
                reply,
            },
            true,
        );
        result.recv().unwrap_or(Err(LoadFailure::Unreadable))
    }

    /// The cover pipeline's request: page 1 at the pipeline's fixed render
    /// target. Queued behind interactive work — the background path. A
    /// protected document without a usable password yields `None` — no
    /// cover, the glyph fallback (AC-106); cover failures stay untyped.
    pub fn render_cover(
        &self,
        path: &str,
        page: u32,
        width: u32,
        max_height: u32,
        password: Option<&str>,
    ) -> Option<image::DynamicImage> {
        let (reply, result) = mpsc::channel();
        self.submit(
            Request::Cover {
                path: path.to_string(),
                page,
                width,
                max_height,
                password: password.map(str::to_string),
                reply,
            },
            false,
        );
        result
            .recv()
            .ok()
            .unwrap_or(Err(LoadFailure::Unreadable)) // worker died: no cover
            .ok()
    }
}

fn serve(queue: &Arc<Mutex<Queue>>, wake: &Arc<Condvar>) {
    // The process's one pdfium binding, bound on the one thread that ever
    // calls it (DR-134) — the bind order and failure behavior the cover
    // pipeline shipped since v0.0.2, moved here unchanged.
    let pdfium = bind_pdfium();
    let mut guard = queue
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    loop {
        let Some(request) = guard.take() else {
            guard = wake
                .wait(guard)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            continue;
        };
        drop(guard);
        match request {
            Request::Page {
                path,
                page,
                target_width,
                password,
                reply,
            } => {
                let result = pdfium.as_ref().map_or(Err(LoadFailure::Unreadable), |pdfium| {
                    render_page_image(pdfium, &path, page, target_width, password.as_deref())
                });
                let _ = reply.send(result);
            }
            Request::Info {
                path,
                password,
                reply,
            } => {
                let result = pdfium.as_ref().map_or(Err(LoadFailure::Unreadable), |pdfium| {
                    read_document_info(pdfium, &path, password.as_deref())
                });
                let _ = reply.send(result);
            }
            Request::Cover {
                path,
                page,
                width,
                max_height,
                password,
                reply,
            } => {
                let result = pdfium.as_ref().map_or(Err(LoadFailure::Unreadable), |pdfium| {
                    render_cover_image(pdfium, &path, page, width, max_height, password.as_deref())
                });
                let _ = reply.send(result);
            }
            Request::Text {
                path,
                page,
                password,
                reply,
            } => {
                let result = pdfium.as_ref().map_or(Err(LoadFailure::Unreadable), |pdfium| {
                    extract_page_text(pdfium, &path, page, password.as_deref())
                });
                let _ = reply.send(result);
            }
        }
        guard = queue
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
    }
}

// ---------------------------------------------------------------- rendering

/// FR-20 (AC-104): the single load point. The stored password is presented
/// to pdfium when the row holds one; pdfium's password refusal
/// (`FPDF_ERR_PASSWORD`) is classified distinctly from every other load
/// failure — that distinction is what the reader's unlock form keys on.
fn load_pdf<'a>(
    pdfium: &'a Pdfium,
    path: &str,
    password: Option<&str>,
) -> Result<PdfDocument<'a>, LoadFailure> {
    pdfium.load_pdf_from_file(path, password).map_err(|error| {
        if matches!(
            error,
            PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError)
        ) {
            LoadFailure::PasswordRequired
        } else {
            LoadFailure::Unreadable
        }
    })
}

/// §3.1: `scale = target_width / (width_pt × 4/3)` bounded `[0.2, 4.0]`;
/// the page renders at exactly that scale.
fn render_page_image(
    pdfium: &Pdfium,
    path: &str,
    page: u32,
    target_width: u32,
    password: Option<&str>,
) -> Result<image::DynamicImage, LoadFailure> {
    let document = load_pdf(pdfium, path, password)?;
    let pages = document.pages();
    let index = i32::try_from(page.checked_sub(1).ok_or(LoadFailure::Unreadable)?).map_err(
        |_| LoadFailure::Unreadable,
    )?;
    let page = pages.get(index).map_err(|_| LoadFailure::Unreadable)?;
    let size = page.page_size();
    let width_pt = size.width().value;
    let height_pt = size.height().value;
    if width_pt <= 0.0 || height_pt <= 0.0 {
        return Err(LoadFailure::Unreadable);
    }
    let css_width = width_pt * CSS_PER_POINT;
    let css_height = height_pt * CSS_PER_POINT;
    let scale = (target_width as f32 / css_width).clamp(MIN_PAGE_SCALE, MAX_PAGE_SCALE);
    let config = PdfRenderConfig::new()
        .set_target_width((css_width * scale).max(1.0) as i32)
        .set_maximum_height((css_height * scale).max(1.0) as i32);
    page.render_with_config(&config)
        .map_err(|_| LoadFailure::Unreadable)?
        .as_image()
        .map_err(|_| LoadFailure::Unreadable)
}

/// The released cover math (D7, DR-134) unchanged: fixed target width with
/// a height cap, no scale bound — AC-63's regression net depends on it.
fn render_cover_image(
    pdfium: &Pdfium,
    path: &str,
    page: u32,
    width: u32,
    max_height: u32,
    password: Option<&str>,
) -> Result<image::DynamicImage, LoadFailure> {
    let config = PdfRenderConfig::new()
        .set_target_width(width as i32)
        .set_maximum_height(max_height as i32);
    let document = load_pdf(pdfium, path, password)?;
    // FR-31 (§3.8): the requested 0-based page; a page number past the
    // document's extent renders page 1 — the automatic cover — so the
    // choice-tagged file still lands and the choice stands (DR-404).
    let page_count = usize::try_from(document.pages().len()).unwrap_or(0);
    let index = usize::try_from(page)
        .ok()
        .filter(|index| *index < page_count)
        .unwrap_or(0);
    let page = document
        .pages()
        .iter()
        .nth(index)
        .ok_or(LoadFailure::Unreadable)?;
    page.render_with_config(&config)
        .map_err(|_| LoadFailure::Unreadable)?
        .as_image()
        .map_err(|_| LoadFailure::Unreadable)
}

fn read_document_info(
    pdfium: &Pdfium,
    path: &str,
    password: Option<&str>,
) -> Result<DocumentInfo, LoadFailure> {
    let document = load_pdf(pdfium, path, password)?;
    let pages = document.pages();
    let count = pages.len();
    let mut infos = Vec::with_capacity(usize::try_from(count).map_err(|_| LoadFailure::Unreadable)?);
    for index in 0..count {
        let size = pages.page_size(index).map_err(|_| LoadFailure::Unreadable)?;
        infos.push(PageInfo {
            width_pt: size.width().value,
            height_pt: size.height().value,
        });
    }
    Ok(DocumentInfo {
        page_count: u32::try_from(count).map_err(|_| LoadFailure::Unreadable)?,
        pages: infos,
    })
}

// ------------------------------------------------------------- text layer

/// §3.3 cap: at most 4 000 runs per page; anything beyond that is dropped
/// with the flag set, and the frontend renders no layer past the cap.
const MAX_TEXT_RUNS: usize = 4_000;

/// One selectable run in **top-origin page-point coordinates** (§3.3): `x`/`y`
/// is the run rect's top-left — the coordinate space the reader's exact
/// geometry math already uses (M4), so the frontend maps runs to spans
/// arithmetically at any zoom, never measuring.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TextRun {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// One page's extraction: the runs, the cap flag, and the page size in
/// points — the `app_pdf_text` DTO shape (§3.3) and the reader cache file's
/// shape, one structure for both.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PageText {
    pub runs: Vec<TextRun>,
    pub truncated: bool,
    pub page_width_pt: f32,
    pub page_height_pt: f32,
}

/// Extracts one page's embedded text (1-based) through the pdfium text SDK
/// (M7): the page's text segments, each run's string and bounds, with the
/// bottom-origin PDF rects flipped to top-origin page points (§3.3) —
/// `y = page_height − rect.top`. Whitespace-only segments are skipped (they
/// carry nothing to select). A page without embedded text — a scan — yields
/// an empty vec, not an error: the reader simply shows no layer.
fn extract_page_text(
    pdfium: &Pdfium,
    path: &str,
    page: u32,
    password: Option<&str>,
) -> Result<PageText, LoadFailure> {
    let document = load_pdf(pdfium, path, password)?;
    let pages = document.pages();
    let index = i32::try_from(page.checked_sub(1).ok_or(LoadFailure::Unreadable)?).map_err(
        |_| LoadFailure::Unreadable,
    )?;
    let page = pages.get(index).map_err(|_| LoadFailure::Unreadable)?;
    let size = page.page_size();
    let page_width_pt = size.width().value;
    let page_height_pt = size.height().value;
    if page_width_pt <= 0.0 || page_height_pt <= 0.0 {
        return Err(LoadFailure::Unreadable);
    }
    let text = page.text().map_err(|_| LoadFailure::Unreadable)?;
    let mut runs: Vec<TextRun> = Vec::new();
    let mut truncated = false;
    for segment in text.segments().iter() {
        let string = segment.text();
        if string.trim().is_empty() {
            continue;
        }
        if runs.len() >= MAX_TEXT_RUNS {
            truncated = true;
            break;
        }
        let bounds = segment.bounds();
        runs.push(TextRun {
            text: string,
            x: bounds.left().value,
            y: page_height_pt - bounds.top().value,
            w: bounds.width().value,
            h: bounds.height().value,
        });
    }
    Ok(PageText {
        runs,
        truncated,
        page_width_pt,
        page_height_pt,
    })
}

// ------------------------------------------------------------ pdfium bind

/// The bind candidate list as a pure function of the process paths and
/// injected extra directories (D34, §3.6): the packaged macOS `.app` and
/// `.deb` layouts become provable with synthetic paths, and the real call
/// sites stay thin wrappers supplying the process values. Order is the
/// released DR-50c order — exe directory → `binaries/` beside the exe →
/// the injected directories (the app's resource directory) → `binaries/`
/// in the working directory (dev); an unknown path contributes nothing,
/// mirroring today's `current_exe`/`current_dir` failure behavior.
pub fn bind_candidates(
    exe_path: Option<&Path>,
    cwd: Option<&Path>,
    extra_dirs: &[PathBuf],
) -> Vec<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(exe_dir) = exe_path.and_then(Path::parent) {
        candidates.push(exe_dir.to_path_buf());
        candidates.push(exe_dir.join("binaries"));
    }
    candidates.extend(extra_dirs.iter().cloned());
    if let Some(cwd) = cwd {
        candidates.push(cwd.join("binaries"));
    }
    candidates
}

fn process_bind_candidates() -> Vec<PathBuf> {
    bind_candidates(
        std::env::current_exe().ok().as_deref(),
        std::env::current_dir().ok().as_deref(),
        EXTRA_BIND_DIRS.get().map(Vec::as_slice).unwrap_or_default(),
    )
}

fn bind_pdfium() -> Option<Pdfium> {
    // pdfium-render binds process-globally: the first successful bind wins
    // and every later attempt returns AlreadyInitialized. The service thread
    // binds exactly once — it is the only thread that ever calls pdfium
    // (R5.1, DR-134).
    for dir in process_bind_candidates() {
        let name = Pdfium::pdfium_platform_library_name_at_path(&dir);
        if let Ok(library) = Pdfium::bind_to_library(&name) {
            return Some(Pdfium::new(library));
        }
    }
    Pdfium::bind_to_system_library().ok().map(Pdfium::new)
}

/// Whether a Pdfium library file sits in the bind search path. Tests use
/// this instead of `bind_pdfium` because probing would consume the
/// process-global bind the service thread needs.
pub fn pdfium_binary_present() -> bool {
    process_bind_candidates()
        .iter()
        .any(|dir| Pdfium::pdfium_platform_library_name_at_path(dir).is_file())
}

// ----------------------------------------------------------- reader cache

/// The reader page cache (FR-17, §3.2): `<reader_dir>/<doc-key>/<page>-<width>.png`
/// — the document id is the key, mirroring the `reader-<id>` window label,
/// so window-close and delete cleanup address exactly one directory.
/// Session-scoped: written by `app_pdf_page`, removed when a reader window
/// closes, when the document is deleted, and wholesale at startup.
pub fn reader_page_path(reader_dir: &Path, doc_id: i64, page: u32, width: u32) -> PathBuf {
    reader_dir
        .join(doc_id.to_string())
        .join(format!("{page}-{width}.png"))
}

/// The reader text cache file (§3.3): `<reader_dir>/<doc-key>/text-<page>.json`
/// — the same document directory as the page images, so the released
/// session-scope cleanup (window close, document delete, startup sweep)
/// clears text caches with the identical code path.
pub fn reader_text_path(reader_dir: &Path, doc_id: i64, page: u32) -> PathBuf {
    reader_dir
        .join(doc_id.to_string())
        .join(format!("text-{page}.json"))
}

/// Removing one document's reader cache never fails a flow (AC-62/AC-67):
/// absence is success, any other failure leaves stale files the reader
/// simply re-requests — the same cleanup discipline the cover cache ships.
pub fn remove_document_cache(reader_dir: &Path, doc_id: i64) {
    remove_dir_ignoring_absence(&reader_dir.join(doc_id.to_string()));
}

/// Startup sweep: the reader cache is session-scoped, so leftovers from a
/// previous run are removed wholesale (the released sweep pattern, §3.2).
pub fn clear_reader_cache(reader_dir: &Path) {
    remove_dir_ignoring_absence(reader_dir);
}

fn remove_dir_ignoring_absence(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir);
}

// ------------------------------------------------------- reader page serving

/// One page's geometry, in points — the `app_pdf_document` payload (§3.2);
/// the reader view sizes each page at its exact aspect ratio.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PdfPageDto {
    pub width_pt: f32,
    pub height_pt: f32,
}

/// The `app_pdf_document` payload (§3.2).
#[derive(Debug, Clone, serde::Serialize)]
pub struct PdfDocDto {
    pub page_count: u32,
    pub pages: Vec<PdfPageDto>,
}

/// Document geometry for the reader view. An unopenable file yields the
/// typed `PdfUnopenable` — the same classification the cover path gives the
/// same failure, which is what UI-31's error state keys on (AC-69). A
/// protected document yields `PasswordRequired` with the row's stored
/// password (FR-20, AC-104): `Some` pre-fills the unlock form, `None`
/// leaves it empty.
pub fn document_dto(
    render: &RenderService,
    path: &str,
    password: Option<&str>,
) -> Result<PdfDocDto, AppError> {
    let info = render.document_info(path, password).map_err(|error| {
        password_required_or_unopenable(error, path, password)
    })?;
    Ok(PdfDocDto {
        page_count: info.page_count,
        pages: info
            .pages
            .iter()
            .map(|page| PdfPageDto {
                width_pt: page.width_pt,
                height_pt: page.height_pt,
            })
            .collect(),
    })
}

fn password_required_or_unopenable(
    error: LoadFailure,
    path: &str,
    password: Option<&str>,
) -> AppError {
    match error {
        LoadFailure::PasswordRequired => AppError::PasswordRequired {
            path: path.to_string(),
            stored: password.map(str::to_string),
        },
        LoadFailure::Unreadable => AppError::PdfUnopenable {
            path: path.to_string(),
        },
    }
}

/// Serve one reader page (§3.2): a cache hit is served as-is with no
/// rendering; a miss renders through the single owner at the requested width
/// (the service clamps scale to `MAX_SCALE` — the cache key records the
/// *requested* width, so re-requesting the same size is always a hit),
/// PNG-encodes and writes the cache file. The payload is the path (R5.4);
/// callers run this on a blocking thread, never the UI thread (R3.5).
pub fn serve_page(
    render: &RenderService,
    reader_dir: &Path,
    doc_id: i64,
    document_path: &str,
    page: u32,
    target_width: u32,
    password: Option<&str>,
) -> Result<String, AppError> {
    let cache = reader_page_path(reader_dir, doc_id, page, target_width);
    if cache.exists() {
        return Ok(cache.to_string_lossy().into_owned());
    }
    let image = render
        .render_page(document_path, page, target_width, password)
        .map_err(|error| {
            password_required_or_unopenable(error, document_path, password)
        })?;
    if let Some(parent) = cache.parent() {
        std::fs::create_dir_all(parent).map_err(|error| AppError::Io {
            path: parent.to_string_lossy().into_owned(),
            message: error.to_string(),
        })?;
    }
    // D19 (DR-196): the reader page path encodes with the measured
    // `Fast + Sub` row — 332→207 ms at 1 932 px, lossless by construction
    // (AC-74). The cache contract is untouched: same key, same PNG format,
    // same two-stage serving. Covers keep the balanced default encoder.
    let file = std::fs::File::create(&cache).map_err(|error| AppError::Io {
        path: cache.to_string_lossy().into_owned(),
        message: error.to_string(),
    })?;
    PngEncoder::new_with_quality(BufWriter::new(file), CompressionType::Fast, FilterType::Sub)
        .write_image(
            image.as_bytes(),
            image.width(),
            image.height(),
            image.color().into(),
        )
        .map_err(|error| AppError::Io {
            path: cache.to_string_lossy().into_owned(),
            message: error.to_string(),
        })?;
    Ok(cache.to_string_lossy().into_owned())
}

/// Serve one page's text (§3.3), mirroring `serve_page`'s two-stage
/// serving: a cache hit is returned without extraction; a miss extracts
/// through the single owner and writes the file. A cache file that no
/// longer parses counts as a miss. Callers run this on a blocking thread,
/// never the UI thread (R3.5).
pub fn serve_text(
    render: &RenderService,
    reader_dir: &Path,
    doc_id: i64,
    document_path: &str,
    page: u32,
    password: Option<&str>,
) -> Result<PageText, AppError> {
    let cache = reader_text_path(reader_dir, doc_id, page);
    if let Ok(contents) = std::fs::read_to_string(&cache)
        && let Ok(text) = serde_json::from_str::<PageText>(&contents)
    {
        return Ok(text);
    }
    let text = render
        .page_text(document_path, page, password)
        .map_err(|error| password_required_or_unopenable(error, document_path, password))?;
    if let Some(parent) = cache.parent() {
        std::fs::create_dir_all(parent).map_err(|error| AppError::Io {
            path: parent.to_string_lossy().into_owned(),
            message: error.to_string(),
        })?;
    }
    let body = serde_json::to_string(&text).map_err(|error| AppError::Io {
        path: cache.to_string_lossy().into_owned(),
        message: error.to_string(),
    })?;
    std::fs::write(&cache, body).map_err(|error| AppError::Io {
        path: cache.to_string_lossy().into_owned(),
        message: error.to_string(),
    })?;
    Ok(text)
}

// ------------------------------------------------------------ cover picker

/// §3.8: the cover picker's fixed render width in CSS px — strip tiles are
/// small; the service's scale bounds apply unchanged. The width is a
/// server-side constant: the command takes only a page window.
const PICKER_WIDTH: u32 = 240;

/// §3.8: the largest page window one request may render — the picker fills
/// lazily in small windows, and a request past this bound is malformed.
const MAX_THUMB_BATCH: u32 = 32;

/// Batched cover-picker thumbnails (§3.8): pages `from..=to` (1-based,
/// inclusive) at the picker width, each served exactly like `serve_page`
/// — a cached file is served as-is, a miss renders through the service's
/// interactive tier and writes. The files live under a `picker/`
/// subdirectory of the existing reader directory (DR-404): already inside
/// the asset-protocol scope, no new protocol or top-level path, ephemeral
/// under the released reader-directory teardown. A malformed window
/// (`from < 1`, `from > to`, or past `MAX_THUMB_BATCH`) is rejected
/// before any rendering; a page the document cannot answer fails the
/// window with the released error kinds. The response is the file paths
/// in page order — the released page-image transport, batched.
pub fn serve_thumbs(
    render: &RenderService,
    reader_dir: &Path,
    doc_id: i64,
    document_path: &str,
    from: u32,
    to: u32,
    password: Option<&str>,
) -> Result<Vec<String>, AppError> {
    if from < 1 || from > to || to - from + 1 > MAX_THUMB_BATCH {
        return Err(AppError::Validation(format!(
            "thumbnail window must start at page 1 and span at most {MAX_THUMB_BATCH} pages"
        )));
    }
    let picker_dir = reader_dir.join("picker");
    let span = usize::try_from(to - from + 1).unwrap_or(0);
    let mut paths: Vec<String> = Vec::with_capacity(span);
    for page in from..=to {
        paths.push(serve_page(
            render,
            &picker_dir,
            doc_id,
            document_path,
            page,
            PICKER_WIDTH,
            password,
        )?);
    }
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{TempDir, fixture, fixture_multi};

    // D34 (§3.6, AC-175): synthetic packaged layouts. No test layer has
    // ever run a packaged-app layout (the CI and these tests run the
    // source tree); these units close that class mechanically.
    #[test]
    fn bind_candidates_reach_the_packaged_macos_and_deb_layouts() {
        let temp = TempDir::new("bind-layouts");

        // The published macOS `.app` layout: the binary at
        // Contents/MacOS/bookkeeper, the library at Contents/Resources/binaries.
        let app = temp.path().join("Book Keeper.app");
        let macos_dir = app.join("Contents/MacOS");
        let resources = app.join("Contents/Resources/binaries");
        std::fs::create_dir_all(&macos_dir).unwrap();
        std::fs::create_dir_all(&resources).unwrap();
        let app_exe = macos_dir.join("bookkeeper");
        std::fs::write(&app_exe, "app binary").unwrap();
        std::fs::write(
            Pdfium::pdfium_platform_library_name_at_path(&resources),
            "libpdfium",
        )
        .unwrap();

        // The published `.deb` layout: /usr/bin/bookkeeper with the library
        // under the package lib dir's binaries.
        let deb_root = temp.path().join("debroot");
        let deb_bin = deb_root.join("usr/bin");
        let deb_libs = deb_root.join("usr/lib/Book Keeper/binaries");
        std::fs::create_dir_all(&deb_bin).unwrap();
        std::fs::create_dir_all(&deb_libs).unwrap();
        let deb_exe = deb_bin.join("bookkeeper");
        std::fs::write(&deb_exe, "deb binary").unwrap();
        std::fs::write(
            Pdfium::pdfium_platform_library_name_at_path(&deb_libs),
            "libpdfium",
        )
        .unwrap();

        // The search, run exactly as `bind_pdfium` runs it (the file check
        // only — probing would consume the process-global bind).
        for (exe, extra) in [
            (&app_exe, vec![resources.clone()]),
            (&deb_exe, vec![deb_libs.clone()]),
        ] {
            let candidates = bind_candidates(Some(exe), Some(temp.path()), &extra);
            assert!(
                candidates.iter().any(|dir| Pdfium::pdfium_platform_library_name_at_path(dir).is_file()),
                "the bind search must reach the packaged library for {exe:?}"
            );
        }
    }

    #[test]
    fn bind_candidates_keep_the_released_dev_and_next_to_binary_paths() {
        // The source-tree and Windows/AppImage behavior is untouched:
        // exe directory → binaries/ beside the exe → injected directories →
        // binaries/ in the working directory; unknown process paths
        // contribute nothing, as today's current_exe/current_dir failures.
        let exe = Path::new("C:/Program Files/Book Keeper/bookkeeper.exe");
        let cwd = Path::new("D:/projects/pdf-keeper/src-tauri");
        assert_eq!(
            bind_candidates(
                Some(exe),
                Some(cwd),
                &[PathBuf::from("/usr/lib/Book Keeper/binaries")],
            ),
            vec![
                PathBuf::from("C:/Program Files/Book Keeper"),
                PathBuf::from("C:/Program Files/Book Keeper/binaries"),
                PathBuf::from("/usr/lib/Book Keeper/binaries"),
                PathBuf::from("D:/projects/pdf-keeper/src-tauri/binaries"),
            ]
        );
        assert_eq!(bind_candidates(None, None, &[]), Vec::<PathBuf>::new());
    }

    #[test]
    fn the_queue_takes_interactive_requests_ahead_of_cover_jobs() {
        // §3.1: interactive work beats background work, regardless of the
        // order requests arrived in.
        let (page_reply, _pr) = mpsc::channel();
        let (cover_reply, _cr) = mpsc::channel();
        let (info_reply, _ir) = mpsc::channel();
        let mut queue = Queue::default();
        queue.covers.push_back(Request::Cover {
            path: "C:/a.pdf".into(),
            page: 0,
            width: 640,
            max_height: 905,
            password: None,
            reply: cover_reply,
        });
        queue.covers.push_back(Request::Cover {
            path: "C:/b.pdf".into(),
            page: 0,
            width: 640,
            max_height: 905,
            password: None,
            reply: mpsc::channel().0,
        });
        queue.interactive.push_back(Request::Page {
            path: "C:/a.pdf".into(),
            page: 1,
            target_width: 800,
            password: None,
            reply: page_reply,
        });
        queue.interactive.push_back(Request::Info {
            path: "C:/a.pdf".into(),
            password: None,
            reply: info_reply,
        });
        let first = queue.take().expect("page request first");
        assert!(matches!(first, Request::Page { .. }));
        let second = queue.take().expect("info request second");
        assert!(matches!(second, Request::Info { .. }));
        let third = queue.take().expect("covers follow");
        assert!(matches!(third, Request::Cover { .. }));
        assert!(matches!(queue.take(), Some(Request::Cover { .. })));
        assert!(queue.take().is_none());
    }

    #[test]
    fn reader_cache_paths_address_one_directory_per_document() {
        let dir = Path::new("C:/appdata/reader");
        assert_eq!(
            reader_page_path(dir, 7, 3, 1600),
            PathBuf::from("C:/appdata/reader/7/3-1600.png")
        );
        assert_eq!(
            reader_page_path(dir, 7, 1, 100),
            PathBuf::from("C:/appdata/reader/7/1-100.png")
        );
        assert_ne!(
            reader_page_path(dir, 7, 3, 1600),
            reader_page_path(dir, 8, 3, 1600),
            "different documents never share a cache file"
        );
        // §3.3: text caches sit in the same per-document directory, so the
        // released cleanup (window close, delete, startup sweep) clears them
        // with the identical code path.
        assert_eq!(
            reader_text_path(dir, 7, 3),
            PathBuf::from("C:/appdata/reader/7/text-3.json")
        );
        assert_eq!(
            reader_text_path(dir, 7, 3).parent(),
            reader_page_path(dir, 7, 3, 1600).parent(),
            "one directory per document covers both caches"
        );
    }

    #[test]
    fn reader_cache_cleanup_removes_the_document_directory_and_tolerates_absence() {
        // AC-62/AC-67 cleanup mechanics: window close and delete both call
        // this; absence is success; other documents' caches are untouched.
        let temp = TempDir::new("reader-cache");
        let reader_dir = temp.path().join("reader");
        std::fs::create_dir_all(reader_dir.join("7")).unwrap();
        std::fs::create_dir_all(reader_dir.join("8")).unwrap();
        std::fs::write(reader_dir.join("7/3-1600.png"), "png").unwrap();
        std::fs::write(reader_dir.join("8/1-800.png"), "png").unwrap();

        remove_document_cache(&reader_dir, 7);
        assert!(!reader_dir.join("7").exists());
        assert!(
            reader_dir.join("8/1-800.png").exists(),
            "other documents keep their cache"
        );
        remove_document_cache(&reader_dir, 7); // absent is success
        remove_document_cache(&reader_dir, 99); // never existed

        clear_reader_cache(&reader_dir);
        assert!(!reader_dir.exists());
        clear_reader_cache(&reader_dir); // absent is success
    }

    fn service() -> Arc<RenderService> {
        RenderService::shared()
    }

    #[test]
    fn document_info_reports_the_fixture_page_count_and_sizes() {
        // AC-61: page count correct, sizes match the file (the generator's
        // Letter MediaBox: 612 × 792 pt).
        if !pdfium_binary_present() {
            eprintln!("pdfium binary missing — run scripts/fetch-pdfium.mjs; skipping info proof");
            return;
        }
        let path = fixture_multi("multi-024.pdf");
        let info = service()
            .document_info(&path.to_string_lossy(), None)
            .expect("AC-61: a valid multi-page fixture yields its info");
        assert_eq!(info.page_count, 24);
        assert_eq!(info.pages.len(), 24);
        for page in &info.pages {
            assert_eq!((page.width_pt, page.height_pt), (612.0, 792.0));
        }
        // An unrenderable file yields Err(Unreadable) — the typed-error
        // contract (§3.2).
        assert!(
            service()
                .document_info(&fixture("malformed.pdf").to_string_lossy(), None)
                .is_err_and(|error| matches!(error, LoadFailure::Unreadable))
        );
    }

    #[test]
    fn a_protected_document_prompts_and_unlocks_with_the_right_password() {
        // AC-104 (FR-20, DR-266 b): a protected document with no or wrong
        // password is `PasswordRequired` on every request path; the stored
        // password opens it. The fixture is genuinely RC4-encrypted with the
        // user password "sesame" (make-fixtures.mjs).
        if !pdfium_binary_present() {
            eprintln!(
                "pdfium binary missing — run scripts/fetch-pdfium.mjs; skipping password proof"
            );
            return;
        }
        let path = fixture("protected.pdf").to_string_lossy().into_owned();
        let service = service();

        // No password: the load refuses on both interactive paths.
        assert!(
            service
                .document_info(&path, None)
                .is_err_and(|error| matches!(error, LoadFailure::PasswordRequired)),
            "a protected document without a password is the prompt state"
        );
        assert!(
            service
                .render_page(&path, 1, 800, None)
                .is_err_and(|error| matches!(error, LoadFailure::PasswordRequired)),
            "page rendering shares the prompt state"
        );
        // A wrong password is the same state — the reader's failed attempt.
        assert!(
            service
                .render_page(&path, 1, 800, Some("wrong"))
                .is_err_and(|error| matches!(error, LoadFailure::PasswordRequired)),
            "a wrong password is still the prompt state, not Unreadable"
        );

        // The right password: the document behaves like any other.
        let info = service
            .document_info(&path, Some("sesame"))
            .expect("the stored password opens the document");
        assert_eq!(info.page_count, 1);
        let page = service
            .render_page(&path, 1, 800, Some("sesame"))
            .expect("the protected page renders with the password");
        assert!(
            (page.width() as i64 - 800).abs() <= 2,
            "rendered at the requested width, got {}",
            page.width()
        );
        // The decrypted content stream really carries the drawn text: the
        // page is not blank (the per-object RC4 key is part of the fixture).
        let luma = page.to_luma8();
        let dark = luma.as_raw().iter().filter(|byte| **byte < 128).count();
        assert!(dark > 100, "the protected page renders its text, got {dark} dark pixels");

        // The typed error carries the stored (rejected) password for the
        // unlock form's prefill (DR-266 b).
        let error = document_dto(&service, &path, Some("wrong")).expect_err("stays protected");
        match error {
            AppError::PasswordRequired { stored, .. } => {
                assert_eq!(stored.as_deref(), Some("wrong"));
            }
            other => panic!("expected password-required, got {other:?}"),
        }
        let unopened = document_dto(&service, &path, None).expect_err("stays protected");
        match unopened {
            AppError::PasswordRequired { stored, .. } => assert_eq!(stored, None),
            other => panic!("expected password-required, got {other:?}"),
        }

        // AC-106 (render half): the cover path degrades to no-image — the
        // glyph fallback — without a usable password, and renders with one.
        assert!(service.render_cover(&path, 0, 320, 456, None).is_none());
        assert!(service.render_cover(&path, 0, 320, 456, Some("wrong")).is_none());
        assert!(service.render_cover(&path, 0, 320, 456, Some("sesame")).is_some());
    }

    #[test]
    fn page_rendering_honours_the_scale_bounds_and_page_range() {
        // AC-61/AC-62: pages render at the requested CSS width; §3.1 bounds
        // the scale to [0.2, 4.0]; pages outside the document resolve None.
        if !pdfium_binary_present() {
            eprintln!(
                "pdfium binary missing — run scripts/fetch-pdfium.mjs; skipping render proof"
            );
            return;
        }
        let path = fixture_multi("multi-012.pdf")
            .to_string_lossy()
            .into_owned();
        let service = service();
        // Letter: css width 612 × 4/3 = 816 px.
        let exact = service
            .render_page(&path, 2, 800, None)
            .expect("AC-61: page 2 of a valid multi-page fixture renders");
        assert!(
            (exact.width() as i64 - 800).abs() <= 2,
            "AC-62: rendered at the requested width, got {}",
            exact.width()
        );
        // 4000 / 816 = 4.9 → clamped to 4.0 → 3264 px.
        let capped = service.render_page(&path, 1, 4000, None).expect("page 1 renders");
        assert!(
            (capped.width() as i64 - 3264).abs() <= 2,
            "§3.1: scale capped at 4.0, got {}",
            capped.width()
        );
        // 100 / 816 = 0.12 → clamped to 0.2 → 163 px.
        let floored = service
            .render_page(&path, 12, 100, None)
            .expect("the last page renders");
        assert!(
            (floored.width() as i64 - 163).abs() <= 2,
            "§3.1: scale floored at 0.2, got {}",
            floored.width()
        );
        assert!(
            service.render_page(&path, 13, 800, None).is_err(),
            "AC-62: a page outside the document resolves Err"
        );
        assert!(
            service.render_page(&path, 0, 800, None).is_err(),
            "pages are 1-based; 0 is out of range"
        );
        assert!(
            service
                .render_page(&fixture("malformed.pdf").to_string_lossy(), 1, 800, None)
                .is_err_and(|error| matches!(error, LoadFailure::Unreadable)),
            "an unreadable file resolves Err(Unreadable) (the caller reports the typed error)"
        );
    }

    #[test]
    fn rendered_pages_are_distinguishable_by_the_drawn_page_number() {
        // AC-61: "a rendered page is distinguishable" — the generator draws
        // the page number into each page's content stream; two rendered
        // pages of the same document must differ in pixels.
        if !pdfium_binary_present() {
            eprintln!(
                "pdfium binary missing — run scripts/fetch-pdfium.mjs; skipping render proof"
            );
            return;
        }
        let path = fixture_multi("multi-012.pdf")
            .to_string_lossy()
            .into_owned();
        let service = service();
        let page1 = service.render_page(&path, 1, 400, None).expect("page 1 renders");
        let page2 = service.render_page(&path, 2, 400, None).expect("page 2 renders");
        assert_eq!(page1.width(), page2.width());
        assert_eq!(page1.height(), page2.height());
        assert_ne!(
            page1.into_bytes(),
            page2.into_bytes(),
            "AC-61: page 1 and page 2 render differently (the drawn page number)"
        );
    }

    #[test]
    fn the_scale_fixture_serves_its_last_page_like_its_first() {
        // AC-65's fixture contract: the 1 000-page document opens, reports
        // 1 000 pages, and its last page renders (the mount-bound test
        // itself is the UI harness flow).
        if !pdfium_binary_present() {
            eprintln!(
                "pdfium binary missing — run scripts/fetch-pdfium.mjs; skipping render proof"
            );
            return;
        }
        let path = fixture_multi("multi-1000.pdf")
            .to_string_lossy()
            .into_owned();
        let service = service();
        let info = service
            .document_info(&path, None)
            .expect("AC-65 fixture: the scale document opens");
        assert_eq!(info.page_count, 1000);
        assert!(
            service.render_page(&path, 1000, 800, None).is_ok(),
            "the last page of a 1 000-page document renders"
        );
    }

    // FR-30 (§3.3): text extraction — the selection probe, the scan case,
    // the run cap, the released password path, and the reader text cache.
    fn joined_text(text: &PageText) -> String {
        text.runs
            .iter()
            .map(|run| run.text.as_str())
            .collect::<Vec<_>>()
            .join("")
    }

    #[test]
    fn page_text_returns_top_origin_runs_for_a_text_bearing_page() {
        // The probe fixture draws its known sentence per page at (72, 700)
        // bottom-origin on a 612 × 792 page. The runs the reader consumes
        // are top-origin: this line's y sits near the page top (page height
        // minus the rect's top), not near 700 — the coordinate flip is the
        // whole point of the proof (AC-169's geometry half).
        if !pdfium_binary_present() {
            eprintln!("pdfium binary missing — run scripts/fetch-pdfium.mjs; skipping text proof");
            return;
        }
        let path = fixture("text-probe.pdf").to_string_lossy().into_owned();
        let service = service();
        let text = service
            .page_text(&path, 7, None)
            .expect("the probe's page 7 carries text");
        assert_eq!((text.page_width_pt, text.page_height_pt), (612.0, 792.0));
        assert!(!text.truncated, "one sentence per page is far below the cap");
        assert!(
            joined_text(&text).contains("Page 7 of the selection probe document"),
            "the document's own text: {:?}",
            text.runs
        );
        let run = &text.runs[0];
        assert!((68.0..=76.0).contains(&run.x), "the line starts at x = 72 pt: {run:?}");
        assert!(
            (0.0..200.0).contains(&run.y),
            "top-origin coordinates — a bottom-origin y would sit near 700: {run:?}"
        );
        assert!(run.w > 100.0 && run.h > 8.0, "the run's own rectangle: {run:?}");
        // A page past the extent behaves exactly like page rendering.
        assert!(matches!(
            service.page_text(&path, 9, None),
            Err(LoadFailure::Unreadable)
        ));
    }

    #[test]
    fn a_page_without_text_extracts_nothing_and_is_not_an_error() {
        // The image-only fixture: a real image XObject, no text operators
        // anywhere. A scan yields no layer — an empty vec, not a failure
        // (§3.3: "a page without embedded text gets no layer and no message").
        if !pdfium_binary_present() {
            eprintln!("pdfium binary missing — run scripts/fetch-pdfium.mjs; skipping scan proof");
            return;
        }
        let path = fixture("image-only.pdf").to_string_lossy().into_owned();
        let text = service()
            .page_text(&path, 1, None)
            .expect("an image-only page loads normally");
        assert!(text.runs.is_empty(), "no embedded text: {:?}", text.runs);
        assert!(!text.truncated);
    }

    #[test]
    fn a_dense_page_is_capped_at_four_thousand_runs_with_the_flag_set() {
        // The dense fixture carries 4 010 separate runs on one page; the
        // extraction serves 4 000 and flags the rest (§3.3 cap).
        if !pdfium_binary_present() {
            eprintln!("pdfium binary missing — run scripts/fetch-pdfium.mjs; skipping cap proof");
            return;
        }
        let path = fixture("dense-text.pdf").to_string_lossy().into_owned();
        let text = service()
            .page_text(&path, 1, None)
            .expect("the dense fixture loads");
        assert_eq!(text.runs.len(), MAX_TEXT_RUNS, "the cap bounds the run list");
        assert!(text.truncated, "runs beyond the cap are flagged");
    }

    #[test]
    fn page_text_follows_the_released_password_path() {
        // §3.3: "text extraction reuses it, no new prompt" — the protected
        // fixture answers `PasswordRequired` without a password (the reader's
        // existing unlock form applies) and extracts with the right one.
        if !pdfium_binary_present() {
            eprintln!("pdfium binary missing — run scripts/fetch-pdfium.mjs; skipping password proof");
            return;
        }
        let path = fixture("protected.pdf").to_string_lossy().into_owned();
        let service = service();
        assert!(matches!(
            service.page_text(&path, 1, None),
            Err(LoadFailure::PasswordRequired)
        ));
        let text = service
            .page_text(&path, 1, Some("sesame"))
            .expect("the stored password opens it");
        assert!(
            joined_text(&text).contains("Protected fixture page"),
            "{:?}",
            text.runs
        );
    }

    #[test]
    fn serve_text_caches_the_extraction_and_serves_the_cached_page_after() {
        // §3.3 mirrors the page cache's serving rule: a miss extracts and
        // writes; a hit is served from the file with no re-extraction —
        // proven by replacing the cache with a sentinel JSON and requesting
        // again. A file that no longer parses counts as a miss.
        if !pdfium_binary_present() {
            eprintln!("pdfium binary missing — run scripts/fetch-pdfium.mjs; skipping text cache proof");
            return;
        }
        let temp = TempDir::new("serve-text");
        let reader_dir = temp.path().join("reader");
        let path = fixture("text-probe.pdf").to_string_lossy().into_owned();
        let first = serve_text(&service(), &reader_dir, 42, &path, 7, None)
            .expect("a valid document serves its text");
        assert!(!first.runs.is_empty());
        let cache = reader_text_path(&reader_dir, 42, 7);
        assert!(cache.exists(), "the extraction was written to the reader cache");
        std::fs::write(
            &cache,
            r#"{"runs":[{"text":"cached run","x":1.0,"y":2.0,"w":3.0,"h":4.0}],"truncated":false,"page_width_pt":612.0,"page_height_pt":792.0}"#,
        )
        .unwrap();
        let again = serve_text(&service(), &reader_dir, 42, &path, 7, None).unwrap();
        assert_eq!(again.runs.len(), 1);
        assert_eq!(
            again.runs[0].text, "cached run",
            "the cache hit is served untouched — no re-extraction overwrote it"
        );
        std::fs::write(&cache, "not json").unwrap();
        let reextracted = serve_text(&service(), &reader_dir, 42, &path, 7, None).unwrap();
        assert!(
            joined_text(&reextracted).contains("Page 7 of the selection probe document"),
            "a cache file that no longer parses is a miss: {:?}",
            reextracted.runs
        );
    }

    #[test]
    fn serve_text_maps_the_password_state_to_the_released_typed_error() {
        // The command-side classification is the released one (AC-104's
        // mapping): serve_text hands the password state through unchanged.
        if !pdfium_binary_present() {
            eprintln!("pdfium binary missing — run scripts/fetch-pdfium.mjs; skipping typed-error proof");
            return;
        }
        let temp = TempDir::new("serve-text-password");
        let reader_dir = temp.path().join("reader");
        let path = fixture("protected.pdf").to_string_lossy().into_owned();
        let result = serve_text(&service(), &reader_dir, 42, &path, 1, None);
        assert!(
            matches!(result, Err(AppError::PasswordRequired { .. })),
            "expected the typed password error, got {result:?}"
        );
    }

    // §3.8, AC-181's Rust half: batched picker thumbnails.
    #[test]
    fn serve_thumbs_renders_the_window_at_picker_width_and_caches_it() {
        // A window of pages comes back as file paths in page order, under
        // the reader directory's `picker/` subdirectory (DR-404) at the
        // picker width; a second request is served from the cache — proven
        // with the same sentinel method as the page cache.
        if !pdfium_binary_present() {
            eprintln!("pdfium binary missing — run scripts/fetch-pdfium.mjs; skipping thumbs proof");
            return;
        }
        let temp = TempDir::new("serve-thumbs");
        let reader_dir = temp.path().join("reader");
        let path = fixture_multi("multi-012.pdf")
            .to_string_lossy()
            .into_owned();
        let first = serve_thumbs(&service(), &reader_dir, 42, &path, 3, 5, None)
            .expect("a valid document serves its thumbnail window");
        assert_eq!(first.len(), 3, "one path per page in the window, in order");
        for (offset, page) in (3..=5).enumerate() {
            let expected = reader_page_path(&reader_dir.join("picker"), 42, page, PICKER_WIDTH);
            assert_eq!(first[offset], expected.to_string_lossy());
            assert!(expected.is_file(), "the window was rendered to the cache");
        }
        std::fs::write(reader_page_path(
            &reader_dir.join("picker"),
            42,
            4,
            PICKER_WIDTH,
        ), "sentinel")
        .unwrap();
        let again = serve_thumbs(&service(), &reader_dir, 42, &path, 3, 5, None).unwrap();
        assert_eq!(again, first, "the same window resolves to the same keys");
        assert_eq!(
            std::fs::read_to_string(reader_page_path(
                &reader_dir.join("picker"),
                42,
                4,
                PICKER_WIDTH
            ))
            .unwrap(),
            "sentinel",
            "the cached pages are served untouched — no re-render"
        );
    }

    #[test]
    fn serve_thumbs_rejects_malformed_windows_before_rendering_anything() {
        // The existing validation kind (the `app_pdf_page` bounds pattern):
        // a bad window is refused with no rendering work done.
        let temp = TempDir::new("serve-thumbs-malformed");
        let reader_dir = temp.path().join("reader");
        let path = fixture_multi("multi-012.pdf")
            .to_string_lossy()
            .into_owned();
        for (from, to) in [(0, 3), (5, 4), (1, MAX_THUMB_BATCH + 1)] {
            let result = serve_thumbs(&service(), &reader_dir, 42, &path, from, to, None);
            assert!(
                matches!(result, Err(AppError::Validation(_))),
                "window {from}..={to} must be a Validation error, got {result:?}"
            );
        }
        assert!(
            !reader_dir.join("picker").exists(),
            "a refused request renders nothing"
        );
    }

    #[test]
    fn serve_thumbs_fails_a_window_past_the_document_with_the_released_error() {
        // A page the document cannot answer fails the window with the
        // released typed error (the picker asks only within the manifest it
        // already has; the guard is the same classification as page serving).
        if !pdfium_binary_present() {
            eprintln!("pdfium binary missing — run scripts/fetch-pdfium.mjs; skipping range proof");
            return;
        }
        let temp = TempDir::new("serve-thumbs-range");
        let reader_dir = temp.path().join("reader");
        let path = fixture_multi("multi-012.pdf")
            .to_string_lossy()
            .into_owned();
        let result = serve_thumbs(&service(), &reader_dir, 42, &path, 11, 13, None);
        assert!(
            matches!(result, Err(AppError::PdfUnopenable { .. })),
            "expected the typed unopenable error, got {result:?}"
        );
    }

    #[test]
    fn serve_thumbs_honours_the_released_password_path() {
        // The batch carries the stored password like every other request:
        // no password is the typed prompt state, the right password renders.
        if !pdfium_binary_present() {
            eprintln!("pdfium binary missing — run scripts/fetch-pdfium.mjs; skipping thumbs password proof");
            return;
        }
        let temp = TempDir::new("serve-thumbs-password");
        let reader_dir = temp.path().join("reader");
        let path = fixture("protected.pdf").to_string_lossy().into_owned();
        let service = service();
        let without = serve_thumbs(&service, &reader_dir, 42, &path, 1, 2, None);
        assert!(
            matches!(without, Err(AppError::PasswordRequired { .. })),
            "expected the typed password error, got {without:?}"
        );
        let with = serve_thumbs(&service, &reader_dir, 42, &path, 1, 1, Some("sesame"))
            .expect("the stored password renders the window");
        assert_eq!(with.len(), 1);
        assert!(std::fs::File::open(&with[0]).is_ok());
    }

    #[test]
    fn serve_page_writes_the_cache_once_and_serves_the_cached_file_after() {
        // §3.2's serving rule: a miss renders and writes; a hit is served
        // as-is, with no rendering — proven by replacing the cache file's
        // bytes with a sentinel and re-requesting the same page and width.
        if !pdfium_binary_present() {
            eprintln!("pdfium binary missing — run scripts/fetch-pdfium.mjs; skipping serve proof");
            return;
        }
        let temp = TempDir::new("serve-page");
        let reader_dir = temp.path().join("reader");
        let path = fixture_multi("multi-012.pdf")
            .to_string_lossy()
            .into_owned();
        let served = super::serve_page(&service(), &reader_dir, 42, &path, 2, 800, None)
            .expect("a valid document serves its page");
        assert_eq!(
            served,
            reader_page_path(&reader_dir, 42, 2, 800).to_string_lossy(),
            "the payload is the cache path (R5.4)"
        );
        std::fs::write(&served, "sentinel").unwrap();
        let again = super::serve_page(&service(), &reader_dir, 42, &path, 2, 800, None).unwrap();
        assert_eq!(again, served);
        assert_eq!(
            std::fs::read_to_string(&served).unwrap(),
            "sentinel",
            "the cache hit is served untouched — no re-render overwrote it"
        );
    }

    #[test]
    fn the_chosen_encoder_writes_lossless_pixels_at_the_unchanged_cache_key() {
        // AC-74: `serve_page` writes the cache file through the chosen
        // encoder (DR-196); the file decodes back through the `image` crate
        // with identical dimensions and identical pixels, and the cache path
        // key is unchanged.
        if !pdfium_binary_present() {
            eprintln!("pdfium binary missing — run scripts/fetch-pdfium.mjs; skipping encode proof");
            return;
        }
        let temp = TempDir::new("encode");
        let reader_dir = temp.path().join("reader");
        let path = fixture_multi("multi-012.pdf")
            .to_string_lossy()
            .into_owned();
        let served = super::serve_page(&service(), &reader_dir, 44, &path, 1, 800, None)
            .expect("a valid document serves its page");
        assert_eq!(
            served,
            reader_page_path(&reader_dir, 44, 1, 800).to_string_lossy(),
            "the cache path key is unchanged (§3.2)"
        );
        let decoded = image::open(&served).expect("the cache file decodes through the image crate");
        let rendered = service()
            .render_page(&path, 1, 800, None)
            .expect("the same page renders again");
        assert_eq!(decoded.width(), rendered.width());
        assert_eq!(decoded.height(), rendered.height());
        assert_eq!(
            decoded.into_bytes(),
            rendered.into_bytes(),
            "PNG is lossless: the file decodes back with identical pixels"
        );
    }

    #[test]
    fn the_reader_helpers_report_an_unopenable_document_as_a_typed_error() {
        // AC-69's Rust half: the reader commands' error state keys on the
        // typed classification — both page serving and document info yield
        // `PdfUnopenable`, the same classification the cover path gives.
        if !pdfium_binary_present() {
            eprintln!("pdfium binary missing — run scripts/fetch-pdfium.mjs; skipping error proof");
            return;
        }
        let temp = TempDir::new("serve-error");
        let reader_dir = temp.path().join("reader");
        let broken = fixture("malformed.pdf").to_string_lossy().into_owned();
        let page_error =
            super::serve_page(&service(), &reader_dir, 43, &broken, 1, 800, None)
                .expect_err("typed");
        assert!(
            matches!(page_error, AppError::PdfUnopenable { .. }),
            "page serving yields the typed classification: {page_error}"
        );
        let info_error = super::document_dto(&service(), &broken, None).expect_err("typed");
        assert!(
            matches!(info_error, AppError::PdfUnopenable { .. }),
            "document info yields the typed classification: {info_error}"
        );
        assert!(
            !reader_dir.join("43").exists(),
            "a failed serve leaves no cache directory or half-written file behind"
        );
    }
}
