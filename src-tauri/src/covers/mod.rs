//! Cover thumbnails (FR-16 as modified by DR-113): a background generation
//! pipeline — a render stage that is a client of the single pdfium owner
//! (`crate::pdf`, SPEC-v0.0.6 §3.1) feeding a PNG-encode pool (D17,
//! DR-134) — and a PNG cache keyed by path+size+mtime+render width
//! (RULES R5). The pipeline keeps its queue, its bounded handoff and its
//! progress tracking (AC-63); only the rendering itself moved to the
//! service, unchanged math. Any render failure simply leaves the document
//! without a cached cover — the UI shows the generic glyph (R5.3) and
//! nothing else fails.
//!
//! SPEC-v0.0.9 §3.3: the render stage branches on the extension — `.pdf`
//! through the service, untouched; `.epub` through `epubmeta::cover_image`
//! composed onto the cover canvas (contain-fit, transparent letterbox per
//! DR-299, never cropped). Both branches feed the same encode pool, the same
//! cache key and the same glyph fallback.

use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, mpsc};

use image::GenericImageView;
use rusqlite::Connection;

use crate::applog;
use crate::epubmeta;
use crate::error::AppError;
use crate::library::EventSink;
use crate::library::JobProgress;
use crate::pdf::RenderService;

/// The renderer's target width. Part of the cache identity (FR-16 as modified
/// by DR-113, R5.2): changing it re-keys every entry instead of serving a
/// smaller stored image forever. 640 px covers a 320 CSS px card at 2× device
/// scaling (D7).
pub const RENDER_TARGET_WIDTH: u32 = 640;
const RENDER_TARGET_HEIGHT: u32 = (RENDER_TARGET_WIDTH as u64 * 309 / 218) as u32; // 905

pub struct CoverJob {
    pub doc_id: i64,
    pub path: String,
    pub size: i64,
    pub mtime: i64,
    /// The row's stored password (FR-20, DR-266): the render passes it to
    /// the loader when present; without one a protected document keeps the
    /// glyph fallback (AC-106). The cache key is unchanged — the password
    /// enables the render, it is not part of the image's identity.
    pub password: Option<String>,
    /// FR-31 (§3.8): the row's stored manual cover choice. `None` is the
    /// automatic cover — the released path, unchanged keys.
    pub cover_choice: Option<CoverChoice>,
}

/// Cloneable handle to the cover pipeline (D17, DR-134).
#[derive(Clone)]
pub struct CoverQueue {
    sender: mpsc::Sender<CoverJob>,
    pending: Arc<AtomicI64>,
    pub dir: PathBuf,
}

impl CoverQueue {
    pub fn enqueue(&self, job: CoverJob) {
        self.pending.fetch_add(1, Ordering::Relaxed);
        // A closed channel means the app is shutting down; dropping the job
        // is correct — it will be re-enqueued from DB state at next startup.
        let _ = self.sender.send(job);
    }
}

/// A queue without a worker thread: service tests observe exactly which
/// jobs were enqueued, with no rendering race (test-only).
#[cfg(test)]
pub fn test_queue(dir: PathBuf) -> (CoverQueue, mpsc::Receiver<CoverJob>) {
    let (sender, receiver) = mpsc::channel::<CoverJob>();
    (
        CoverQueue {
            sender,
            pending: Arc::new(AtomicI64::new(0)),
            dir,
        },
        receiver,
    )
}

/// The encode-pool ceiling and the unconfigured ceiling (D17).
const MAX_COVER_WORKERS: usize = 8;
const DEFAULT_MAX_COVER_WORKERS: usize = 4;

/// Encode-pool size (D17): default `min(4, max(1, ⌊cores/2⌋))`; the
/// `BOOKKEEPER_COVER_WORKERS` override is clamped to 1..8. Takes the override
/// as an argument so tests verify the clamp without touching process env.
fn pool_size(override_value: Option<&str>) -> usize {
    if let Some(value) = override_value
        && let Ok(requested) = value.parse::<usize>()
    {
        return requested.clamp(1, MAX_COVER_WORKERS);
    }
    let cores = std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1);
    (cores / 2).clamp(1, DEFAULT_MAX_COVER_WORKERS)
}

/// Completion bookkeeping shared by the pipeline's workers (R5.1): the queue
/// counters and the throttled progress events. The gate makes the event
/// stream monotonic across workers — every published count is read and
/// stamped under one lock — and the final drain event is guaranteed.
struct ProgressTracker {
    pending: Arc<AtomicI64>,
    processed: Arc<AtomicI64>,
    gate: std::sync::Mutex<i64>,
    sink: Arc<dyn EventSink>,
}

impl ProgressTracker {
    fn complete(&self) {
        self.pending.fetch_sub(1, Ordering::Relaxed);
        self.processed.fetch_add(1, Ordering::Relaxed);
        let remaining = self.pending.load(Ordering::Relaxed);
        if let Ok(mut last_emitted) = self.gate.lock() {
            let done = self.processed.load(Ordering::Relaxed);
            // Throttle progress events; always report the final drain.
            if done - *last_emitted >= 25 || remaining == 0 {
                *last_emitted = done;
                self.sink.progress(&JobProgress {
                    kind: "cover".into(),
                    processed: done,
                    total: done + remaining,
                });
            }
        }
    }
}

/// A rendered cover awaiting its PNG encode: the handoff from the render
/// worker to the encode pool (DR-134 — the encode is where the time goes).
struct RenderedCover {
    image: image::DynamicImage,
    target: PathBuf,
}

/// Start the cover pipeline (RULES R3.5, R5.1; D17 as decided by DR-134,
/// render stage rewired per SPEC-v0.0.6 §3.1): the pipeline's render stage
/// is a client of the single pdfium owner (`crate::pdf::RenderService`),
/// feeding N PNG-encode workers. The encode pool is unchanged — it is pure
/// Rust and thread-safe. A missing pdfium binary resolves every render to
/// `None` and leaves the drain-only behavior (covers stay absent).
pub fn start_pool(
    dir: PathBuf,
    sink: Arc<dyn EventSink>,
    service: Arc<RenderService>,
) -> CoverQueue {
    let requested = std::env::var("BOOKKEEPER_COVER_WORKERS").ok();
    start_pool_with_workers(dir, sink, pool_size(requested.as_deref()), service)
}

fn start_pool_with_workers(
    dir: PathBuf,
    sink: Arc<dyn EventSink>,
    encoders: usize,
    service: Arc<RenderService>,
) -> CoverQueue {
    let (sender, receiver) = mpsc::channel::<CoverJob>();
    let pending = Arc::new(AtomicI64::new(0));
    let tracker = Arc::new(ProgressTracker {
        pending: pending.clone(),
        processed: Arc::new(AtomicI64::new(0)),
        gate: std::sync::Mutex::new(0i64),
        sink,
    });

    // One bounded handoff per encode worker (capacity 2): in-flight images
    // are bounded, and a full pool backpressures the render worker — which
    // is never the bottleneck (measured 1.2 ms render vs 35 ms encode,
    // DR-134).
    let mut handoffs = Vec::with_capacity(encoders);
    for _ in 0..encoders {
        let (sender, receiver) = mpsc::sync_channel::<RenderedCover>(2);
        let tracker = tracker.clone();
        std::thread::spawn(move || {
            while let Ok(cover) = receiver.recv() {
                // A save failure leaves the glyph as before (R5.3).
                let _ = cover.image.save(&cover.target);
                tracker.complete();
            }
        });
        handoffs.push(sender);
    }

    let worker_dir = dir.clone();
    std::thread::spawn(move || {
        let mut next = 0usize;
        while let Ok(job) = receiver.recv() {
            let target = cache_path(&worker_dir, &job.path, job.size, job.mtime, job.cover_choice.as_ref());
            // A cache hit completes the job without asking the service; a
            // miss is a cover request on the single pdfium owner (§3.1).
            let rendered = if target.exists() {
                None
            } else if is_epub_path(&job.path) {
                // §3.3: pdfium cannot open an EPUB — the declared cover is
                // the render. Every failure (no declared cover, SVG or other
                // non-raster declaration, undecodable entry, malformed
                // package) lands on the same glyph fallback as a failed
                // render (R5.3). Deliberately not a log line: "no declared
                // cover" is the designed case for a plain EPUB, and this
                // API cannot tell it apart from a decode failure — logging
                // would record designed fallbacks as failures.
                // FR-31 (§3.8): a chosen manifest entry renders through
                // `cover_image_at` on the same composition and fallback.
                match &job.cover_choice {
                    Some(CoverChoice::EpubImage { entry, .. }) => {
                        epubmeta::cover_image_at(&job.path, entry).map(compose_epub_cover)
                    }
                    _ => epubmeta::cover_image(&job.path).map(compose_epub_cover),
                }
            } else {
                // FR-31 (§3.8): the chosen page renders under the choice-
                // tagged key; a page number past the document's extent
                // renders the first page — the automatic cover — under the
                // same tagged key: the choice stands, a file lands, and the
                // sweep counts it as expected. No re-enqueue loop (DR-404).
                let page = match job.cover_choice {
                    Some(CoverChoice::Page(page)) => page,
                    _ => 0,
                };
                let image = service.render_cover(
                    &job.path,
                    page,
                    RENDER_TARGET_WIDTH,
                    RENDER_TARGET_HEIGHT,
                    job.password.as_deref(),
                );
                if image.is_none() {
                    // FR-26 (§3.2, cover queue render failures): the glyph
                    // fallback is the designed recovery (R5.3); the failure
                    // behind it is now visible in the log.
                    applog::error("covers", &format!("cover render failed: {}", job.path));
                }
                image
            };
            match rendered {
                Some(image) => {
                    // Round-robin keeps the pool balanced. A disconnected
                    // worker means shutdown: the job simply ends without a
                    // cover and is re-enqueued from DB state next startup.
                    let handoff = &handoffs[next % handoffs.len()];
                    next += 1;
                    if handoff.send(RenderedCover { image, target }).is_err() {
                        tracker.complete();
                    }
                }
                // Glyph fallback (R5.3) or cache hit: the job completes now.
                None => tracker.complete(),
            }
        }
        // Dropping the handoffs here lets the encode pool finish its
        // in-flight work and exit.
    });

    CoverQueue {
        sender,
        pending,
        dir,
    }
}

/// The cover pipeline's type dispatch (§3.3): the extension is the type.
fn is_epub_path(path: &str) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("epub"))
}

/// §3.3 + DR-299: contain-fit composition of a declared EPUB cover onto the
/// cover canvas — scaled to fit the frame with its aspect preserved (never
/// cropped: a jacket cover's text must not silently vanish), centered, on a
/// fully transparent ground so the letterbox bars show the card surface in
/// whatever theme is active. Output is the pipeline's fixed target; the
/// encode pool and cache key are the released ones.
fn compose_epub_cover(source: image::DynamicImage) -> image::DynamicImage {
    let (width, height) = source.dimensions();
    let scale = (RENDER_TARGET_WIDTH as f64 / width as f64)
        .min(RENDER_TARGET_HEIGHT as f64 / height as f64);
    let fitted_width = ((width as f64 * scale).round().max(1.0) as u32).min(RENDER_TARGET_WIDTH);
    let fitted_height = ((height as f64 * scale).round().max(1.0) as u32).min(RENDER_TARGET_HEIGHT);
    let fitted = source.resize_exact(
        fitted_width,
        fitted_height,
        image::imageops::FilterType::Lanczos3,
    );
    let mut canvas = image::RgbaImage::from_pixel(
        RENDER_TARGET_WIDTH,
        RENDER_TARGET_HEIGHT,
        image::Rgba([0, 0, 0, 0]),
    );
    image::imageops::overlay(
        &mut canvas,
        &fitted.to_rgba8(),
        ((RENDER_TARGET_WIDTH - fitted_width) / 2) as i64,
        ((RENDER_TARGET_HEIGHT - fitted_height) / 2) as i64,
    );
    image::DynamicImage::ImageRgba8(canvas)
}

/// FR-31 (§3.8): a stored manual cover choice. The value grammar is
/// validated at the command boundary ([`validate_choice`]); the read sides
/// (enqueue, sweep, the library query) parse leniently ([`parse_choice`]) —
/// a value that cannot be parsed behaves like no choice at all, the same
/// designed fallback as any cover failure (R5.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoverChoice {
    /// A 0-based PDF page index.
    Page(u32),
    /// A manifest entry of the EPUB plus its declared media type.
    EpubImage { entry: String, media: String },
}

/// The stored JSON shape, shared by both sides: `{"kind":"page","page":<u32
/// 0-based>}` or `{"kind":"epub-image","entry":"<manifest entry>","media":
/// "<media type>"}`; nothing else. An out-of-range page number is *not* a
/// grammar error here — the value is grammatical and the render falls back
/// to the automatic cover under the tagged key (DR-404).
fn read_choice(value: &serde_json::Value) -> Result<CoverChoice, String> {
    let object = value.as_object().ok_or("cover choice must be an object")?;
    match object.get("kind").and_then(|kind| kind.as_str()) {
        Some("page") => {
            if object.len() != 2 {
                return Err("a page choice carries kind and page only".into());
            }
            let page = object
                .get("page")
                .and_then(|page| page.as_u64())
                .ok_or("page must be a 0-based integer")?;
            let page = u32::try_from(page).map_err(|_| "page is out of the storable range".to_string())?;
            Ok(CoverChoice::Page(page))
        }
        Some("epub-image") => {
            if object.len() != 3 {
                return Err("an epub-image choice carries kind, entry and media".into());
            }
            let entry = object
                .get("entry")
                .and_then(|entry| entry.as_str())
                .filter(|entry| !entry.is_empty())
                .ok_or("entry must be a non-empty manifest path")?;
            let media = object
                .get("media")
                .and_then(|media| media.as_str())
                .filter(|media| !media.is_empty())
                .ok_or("media must be the declared media type")?;
            Ok(CoverChoice::EpubImage {
                entry: entry.to_string(),
                media: media.to_string(),
            })
        }
        Some(_) => Err("unknown cover choice kind".into()),
        None => Err("a cover choice carries a kind".into()),
    }
}

/// Command-boundary validation (R1.2 thin wrapper's service side): malformed
/// JSON, a bad grammar, or a kind that does not match the row's file type
/// (§3.8) is the typed rejection.
pub fn validate_choice(value: &str, path: &str) -> Result<CoverChoice, AppError> {
    let parsed: serde_json::Value = serde_json::from_str(value)
        .map_err(|_| AppError::Validation("cover choice is not valid JSON".into()))?;
    let choice = read_choice(&parsed).map_err(AppError::Validation)?;
    match (&choice, is_epub_path(path)) {
        (CoverChoice::Page(_), false) | (CoverChoice::EpubImage { .. }, true) => Ok(choice),
        _ => Err(AppError::Validation(
            "cover choice kind does not match the document type".into(),
        )),
    }
}

/// Read-side parse (enqueue, sweep, query): the column only ever carries
/// validated values, so a value that fails to parse is treated as no choice.
pub fn parse_choice(raw: &str) -> Option<CoverChoice> {
    let value = serde_json::from_str(raw).ok()?;
    read_choice(&value).ok()
}

/// The choice part of the cache key (§3.8): `p<page>` for a page choice,
/// `e<entry-hash>` for an EPUB entry choice (the entry path hashed with the
/// same deterministic hasher as the key itself).
pub fn choice_tag(choice: &CoverChoice) -> String {
    match choice {
        CoverChoice::Page(page) => format!("p{page}"),
        CoverChoice::EpubImage { entry, .. } => {
            let mut hasher = std::hash::DefaultHasher::new();
            entry.hash(&mut hasher);
            format!("e{:016x}", hasher.finish())
        }
    }
}

/// The cache key IS the staleness check (FR-16 as modified by DR-113):
/// path + size + mtime + the renderer's current target width. The width is
/// part of the identity so a changed render target can never leave a smaller
/// stored image authoritative. FR-31 (§3.8) adds the choice tag — and only
/// when there is a choice: with `None` nothing is hashed and the key is
/// byte-identical to the released function (AC-180: the migration re-keys
/// nothing).
pub fn cache_key(path: &str, size: i64, mtime: i64, width: u32, choice_tag: Option<&str>) -> String {
    let mut hasher = std::hash::DefaultHasher::new();
    path.hash(&mut hasher);
    size.hash(&mut hasher);
    mtime.hash(&mut hasher);
    width.hash(&mut hasher);
    if let Some(tag) = choice_tag {
        tag.hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

pub fn cache_path(dir: &Path, path: &str, size: i64, mtime: i64, choice: Option<&CoverChoice>) -> PathBuf {
    let tag = choice.map(choice_tag);
    dir.join(format!(
        "{}.png",
        cache_key(path, size, mtime, RENDER_TARGET_WIDTH, tag.as_deref())
    ))
}

/// Queue survives app restart: re-enqueue every document whose cached cover
/// is absent or stale (covers.md).
pub fn enqueue_missing_covers(conn: &Connection, queue: &CoverQueue) -> Result<(), AppError> {
    let mut stmt = conn
        .prepare("SELECT id, path, size, mtime, pdf_password, cover_choice FROM documents ORDER BY id")?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, Option<String>>(5)?,
        ))
    })?;
    for row in rows {
        let (doc_id, path, size, mtime, password, choice_raw) = row?;
        // FR-31: the expected key is the choice-tagged one — a chosen cover
        // is as much "the cover" as the automatic one (§3.8).
        let cover_choice = choice_raw.as_deref().and_then(parse_choice);
        if !cache_path(&queue.dir, &path, size, mtime, cover_choice.as_ref()).exists() {
            queue.enqueue(CoverJob {
                doc_id,
                path,
                size,
                mtime,
                password: (!password.is_empty()).then_some(password),
                cover_choice,
            });
        }
    }
    Ok(())
}

/// Startup sweep of cached files with no matching row (covers.md).
pub fn sweep_orphans(conn: &Connection, dir: &Path) -> Result<(), AppError> {
    let mut expected: HashSet<String> = HashSet::new();
    {
        let mut stmt = conn.prepare("SELECT path, size, mtime, cover_choice FROM documents")?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })?;
        for row in rows {
            let (path, size, mtime, choice_raw) = row?;
            // FR-31: a row's expected key is its choice-tagged one — the
            // superseded automatic file is an orphan and goes, and a chosen
            // cover (including an out-of-range fallback render, DR-404)
            // survives the sweep (AC-180's no-loop fact).
            let choice = choice_raw.as_deref().and_then(parse_choice);
            let tag = choice.as_ref().map(choice_tag);
            expected.insert(cache_key(
                &path,
                size,
                mtime,
                RENDER_TARGET_WIDTH,
                tag.as_deref(),
            ));
        }
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("png")
            && let Some(stem) = path.file_stem().and_then(|stem| stem.to_str())
            && !expected.contains(stem)
        {
            let _ = std::fs::remove_file(&path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use crate::library::EventSink;
    use crate::pdf::{RenderService, pdfium_binary_present};
    use crate::testutil::{RecordingSink, TempDir, fixture_doc, fixture_multi};
    use std::sync::Arc;
    use std::sync::atomic::Ordering;

    fn epoch_secs(metadata: &std::fs::Metadata) -> i64 {
        metadata
            .modified()
            .map(|when| {
                when.duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64
            })
            .unwrap_or(0)
    }

    #[test]
    fn cache_key_changes_with_any_part_of_the_document_identity() {
        // FR-16 as modified by DR-113 (AC-36): the key IS path + size +
        // mtime + render width; unchanged documents keep their key.
        let base = cache_key("C:/a.pdf", 10, 20, RENDER_TARGET_WIDTH, None);
        assert_eq!(
            base,
            cache_key("C:/a.pdf", 10, 20, RENDER_TARGET_WIDTH, None),
            "same identity, same key"
        );
        assert_ne!(base, cache_key("C:/b.pdf", 10, 20, RENDER_TARGET_WIDTH, None));
        assert_ne!(
            base,
            cache_key("C:/a.pdf", 11, 20, RENDER_TARGET_WIDTH, None),
            "changed size re-keys"
        );
        assert_ne!(
            base,
            cache_key("C:/a.pdf", 10, 21, RENDER_TARGET_WIDTH, None),
            "changed mtime re-keys"
        );
        assert_ne!(
            cache_key("C:/a.pdf", 10, 20, 220, None),
            cache_key("C:/a.pdf", 10, 20, 640, None),
            "AC-36: a changed render width re-keys"
        );
    }

    #[test]
    fn enqueue_missing_covers_queues_only_documents_without_a_cache_file() {
        let conn = open_in_memory().unwrap();
        let temp = TempDir::new("covers-enqueue");
        conn.execute(
            "INSERT INTO documents (id, path, filename, title, size, mtime, added_at)
             VALUES (1, 'C:/one.pdf', 'one.pdf', 'one', 10, 20, 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO documents (id, path, filename, title, size, mtime, added_at)
             VALUES (2, 'C:/two.pdf', 'two.pdf', 'two', 30, 40, 0)",
            [],
        )
        .unwrap();
        std::fs::write(cache_path(temp.path(), "C:/one.pdf", 10, 20, None), "png").unwrap();

        let (queue, receiver) = test_queue(temp.path().to_path_buf());
        enqueue_missing_covers(&conn, &queue).unwrap();
        let jobs: Vec<CoverJob> = receiver.try_iter().collect();
        assert_eq!(
            jobs.iter().map(|job| job.doc_id).collect::<Vec<_>>(),
            vec![2]
        );
        assert_eq!(jobs[0].path, "C:/two.pdf");
    }

    #[test]
    fn sweep_orphans_removes_pngs_no_document_owns() {
        let conn = open_in_memory().unwrap();
        let temp = TempDir::new("covers-sweep");
        conn.execute(
            "INSERT INTO documents (path, filename, title, size, mtime, added_at)
             VALUES ('C:/kept.pdf', 'kept.pdf', 'kept', 10, 20, 0)",
            [],
        )
        .unwrap();
        std::fs::write(cache_path(temp.path(), "C:/kept.pdf", 10, 20, None), "png").unwrap();
        std::fs::write(temp.path().join("stale.png"), "png").unwrap();
        std::fs::write(temp.path().join("keep.txt"), "not a cover").unwrap();

        sweep_orphans(&conn, temp.path()).unwrap();

        assert!(cache_path(temp.path(), "C:/kept.pdf", 10, 20, None).exists());
        assert!(
            !temp.path().join("stale.png").exists(),
            "unowned PNG is swept"
        );
        assert!(
            temp.path().join("keep.txt").exists(),
            "only PNG files are considered"
        );
    }

    #[test]
    fn worker_renders_valid_pdfs_and_leaves_glyph_fallback_for_unrenderable() {
        if !pdfium_binary_present() {
            eprintln!(
                "pdfium binary missing — run scripts/fetch-pdfium.mjs; skipping render proof"
            );
            return;
        }
        let temp = TempDir::new("covers-render");
        let valid = temp.copy_fixture("info-title.pdf");
        let broken = temp.copy_fixture("malformed.pdf");
        let sink: Arc<dyn EventSink> = Arc::new(RecordingSink::default());
        let queue =
            start_pool_with_workers(temp.path().to_path_buf(), sink, 1, RenderService::shared());

        let valid_meta = std::fs::metadata(&valid).unwrap();
        let broken_meta = std::fs::metadata(&broken).unwrap();
        let valid_key = (
            valid.to_string_lossy().into_owned(),
            valid_meta.len() as i64,
            epoch_secs(&valid_meta),
        );
        let broken_key = (
            broken.to_string_lossy().into_owned(),
            broken_meta.len() as i64,
            epoch_secs(&broken_meta),
        );
        queue.enqueue(CoverJob {
            doc_id: 1,
            path: valid_key.0.clone(),
            size: valid_key.1,
            mtime: valid_key.2,
            password: None,
            cover_choice: None,
        });
        queue.enqueue(CoverJob {
            doc_id: 2,
            path: broken_key.0.clone(),
            size: broken_key.1,
            mtime: broken_key.2,
            password: None,
            cover_choice: None,
        });

        // D22 (v0.0.8, M9): 90 s, not 30. The Rust and frontend layers must
        // not run concurrently in one verification pass (ui-verify barriers
        // them; the flake this raises came from manual concurrent runs). The
        // generous deadline makes the test immune to a loaded machine; a real
        // drain finishes in seconds.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(90);
        while queue.pending.load(Ordering::Relaxed) > 0 && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        assert_eq!(
            queue.pending.load(Ordering::Relaxed),
            0,
            "worker drains the queue"
        );

        let cover_file = cache_path(temp.path(), &valid_key.0, valid_key.1, valid_key.2, None);
        assert!(
            cover_file.exists(),
            "AC-06: a valid fixture produces a PNG cover"
        );
        // AC-36: the cached file the queue wrote decodes at ≥ 560 px wide
        // (the renderer targets 640; the floor allows pdfium's page-ratio
        // cap without re-introducing the 220 px generation).
        let (cover_width, _) = image::image_dimensions(&cover_file)
            .expect("AC-36: the cached cover is a decodable PNG");
        assert!(
            cover_width >= 560,
            "AC-36: cached covers are at least 560 px wide, got {cover_width}"
        );
        assert!(
            !cache_path(temp.path(), &broken_key.0, broken_key.1, broken_key.2, None).exists(),
            "R5.3: an unrenderable file produces no cover (the UI shows the glyph) and never fails the batch"
        );
    }

    /// Wait for a started pool to drain (the existing pool tests' deadline
    /// pattern, named once). D22 (v0.0.8, M9): callers pass 90 s, not 30 —
    /// the Rust and frontend layers must not run concurrently in one
    /// verification pass; the deadline absorbs a loaded machine, not slow
    /// product behavior.
    fn drain(queue: &CoverQueue, seconds: u64) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(seconds);
        while queue.pending.load(Ordering::Relaxed) > 0 && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        assert_eq!(queue.pending.load(Ordering::Relaxed), 0, "pool drains");
    }

    #[test]
    fn epub_cover_renders_through_the_pipeline_by_contain_fit() {
        // AC-127: a declared raster cover produces a real cover through the
        // released pipeline — contain-fit, never cropped, the pipeline's
        // 640 px target, the DR-113 cache key — and no pdfium is involved
        // in the EPUB branch (§3.3).
        let temp = TempDir::new("covers-epub");
        let book = temp.copy_fixture("epub-meta.epub");
        let sink: Arc<dyn EventSink> = Arc::new(RecordingSink::default());
        let queue =
            start_pool_with_workers(temp.path().to_path_buf(), sink, 1, RenderService::shared());

        let metadata = std::fs::metadata(&book).unwrap();
        let key = (
            book.to_string_lossy().into_owned(),
            metadata.len() as i64,
            epoch_secs(&metadata),
        );
        queue.enqueue(CoverJob {
            doc_id: 1,
            path: key.0.clone(),
            size: key.1,
            mtime: key.2,
            password: None,
            cover_choice: None,
        });
        drain(&queue, 90);

        let cover_file = cache_path(temp.path(), &key.0, key.1, key.2, None);
        let cover = image::open(&cover_file)
            .expect("AC-127: the declared cover produces a cached cover")
            .to_rgba8();
        assert_eq!(
            (cover.width(), cover.height()),
            (RENDER_TARGET_WIDTH, RENDER_TARGET_HEIGHT),
            "the pipeline's fixed target canvas, unchanged"
        );
        // The fixture cover is 120×160; contain-fit scales it to 640×853 and
        // centers it — the 26 px bars top and bottom are transparent
        // letterbox (DR-299), and every source edge survives (no crop).
        let corner = cover.get_pixel(0, 0);
        assert_eq!(corner[3], 0, "the letterbox bar is transparent");
        let inside_cover = cover.get_pixel(320, 40);
        assert_eq!(
            (inside_cover[3], inside_cover[2]),
            (255, 128),
            "the cover's top edge is visible (the fixture carries blue 128 everywhere)"
        );
        let near_bottom = cover.get_pixel(320, 870);
        assert_eq!(
            (near_bottom[3], near_bottom[2]),
            (255, 128),
            "the cover's bottom edge is visible — contain-fit never crops"
        );
    }

    #[test]
    fn epub_without_a_usable_declared_cover_leaves_the_glyph_fallback() {
        // AC-127 (fallback half): no cover declaration, an SVG declaration,
        // and an unreadable package each end the job with no cached cover —
        // the UI glyph path, exactly like a failed render (R5.3) — and the
        // queue still drains.
        let temp = TempDir::new("covers-epub-glyph");
        let sink: Arc<dyn EventSink> = Arc::new(RecordingSink::default());
        let queue =
            start_pool_with_workers(temp.path().to_path_buf(), sink, 1, RenderService::shared());
        let mut keys = Vec::new();
        for (index, name) in [
            "epub-no-cover.epub",
            "epub-svg-cover.epub",
            "epub-malformed.epub",
        ]
        .iter()
        .enumerate()
        {
            let book = temp.copy_fixture(name);
            let metadata = std::fs::metadata(&book).unwrap();
            let key = (
                book.to_string_lossy().into_owned(),
                metadata.len() as i64,
                epoch_secs(&metadata),
            );
            queue.enqueue(CoverJob {
                doc_id: index as i64 + 1,
                path: key.0.clone(),
                size: key.1,
                mtime: key.2,
                password: None,
                cover_choice: None,
            });
            keys.push((name, key));
        }
        drain(&queue, 90);
        for (name, key) in &keys {
            assert!(
                !cache_path(temp.path(), &key.0, key.1, key.2, None).exists(),
                "{name}: no usable declared cover means no cached cover (the glyph stands)"
            );
        }
    }

    #[test]
    fn a_protected_document_gets_no_cover_until_its_password_is_stored() {
        // AC-106 (FR-20): the import-time cover of a protected document
        // stays absent (the UI keeps the glyph); storing the row's password
        // through the service (the reader's unlock path) re-enqueues it and
        // the real image appears under the unchanged cache key.
        if !pdfium_binary_present() {
            eprintln!(
                "pdfium binary missing — run scripts/fetch-pdfium.mjs; skipping protected-cover proof"
            );
            return;
        }
        let temp = TempDir::new("covers-protected");
        let covers_dir = temp.path().join("covers");
        // The app creates the covers directory at startup (lib.rs); the test
        // mirrors that state before starting the pool.
        std::fs::create_dir_all(&covers_dir).unwrap();
        let sink: Arc<dyn EventSink> = Arc::new(RecordingSink::default());
        let queue = start_pool_with_workers(covers_dir.clone(), Arc::clone(&sink), 1, RenderService::shared());
        let db = crate::Db::for_tests(crate::db::open_in_memory().unwrap());

        let protected = temp.copy_fixture("protected.pdf");
        let path = protected.to_string_lossy().into_owned();
        crate::library::service::import_paths(&db, sink.as_ref(), &queue, std::slice::from_ref(&path), &[])
            .unwrap();
        // The cache key composes from the stored (normalized) row values,
        // so the expectation reads them from the row itself.
        let (row_path, size, mtime) = db
            .with_raw(|conn| {
                conn.query_row(
                    "SELECT path, size, mtime FROM documents WHERE id = 1",
                    [],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?)),
                )
                .map_err(AppError::from)
            })
            .unwrap();
        let cover = cache_path(&covers_dir, &row_path, size, mtime, None);

        // The import enqueues a job with no password (the row has none): the
        // pool runs, and no cover file appears.
        drain(&queue, 90);
        assert!(
            !cover.exists(),
            "AC-106: a protected document without a stored password gets no cover image"
        );

        // Storing the password re-enqueues with it; the render now succeeds.
        db.with_raw(|conn| {
            crate::library::service::set_password(conn, sink.as_ref(), &queue, 1, "sesame")
        })
        .unwrap();
        drain(&queue, 90);
        let (width, _) =
            image::image_dimensions(&cover).expect("AC-106: the stored password produces the real cover");
        assert!(
            width >= 560,
            "the cover renders at the normal target, got {width}"
        );
    }

    #[test]
    fn a_renamed_document_renders_under_the_new_key_and_the_stale_entry_is_swept() {
        // AC-110 (FR-21) end-to-end: the real rename path through the real
        // cover pool — the cover follows the new identity (the key composes
        // from the new path, M3) and the stale entry is removed by the
        // existing orphan sweep.
        if !pdfium_binary_present() {
            eprintln!(
                "pdfium binary missing — run scripts/fetch-pdfium.mjs; skipping rename-cover proof"
            );
            return;
        }
        let temp = TempDir::new("covers-rename");
        let covers_dir = temp.path().join("covers");
        std::fs::create_dir_all(&covers_dir).unwrap();
        let sink: Arc<dyn EventSink> = Arc::new(RecordingSink::default());
        let queue = start_pool_with_workers(covers_dir.clone(), Arc::clone(&sink), 1, RenderService::shared());
        let db = crate::Db::for_tests(crate::db::open_in_memory().unwrap());

        let original = temp.copy_fixture("info-title.pdf");
        let old_path = original.to_string_lossy().into_owned();
        crate::library::service::import_paths(&db, sink.as_ref(), &queue, std::slice::from_ref(&old_path), &[])
            .unwrap();
        let (row_path, size, mtime) = db
            .with_raw(|conn| {
                conn.query_row(
                    "SELECT path, size, mtime FROM documents WHERE id = 1",
                    [],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?)),
                )
                .map_err(AppError::from)
            })
            .unwrap();
        let old_cover = cache_path(&covers_dir, &row_path, size, mtime, None);
        drain(&queue, 90);
        assert!(old_cover.exists(), "the imported document has its cover");

        let renamed = db
            .with_raw(|conn| {
                crate::library::service::rename_document(
                    conn,
                    sink.as_ref(),
                    &queue,
                    1,
                    "Renamed Cover.pdf",
                )
            })
            .unwrap();
        drain(&queue, 90);
        let new_cover = cache_path(&covers_dir, &renamed.path, size, mtime, None);
        assert!(
            new_cover.exists(),
            "AC-110: the cover is regenerated under the new cache key"
        );

        db.with_raw(|conn| sweep_orphans(conn, &covers_dir)).unwrap();
        assert!(!old_cover.exists(), "AC-110: the stale entry is swept");
        assert!(new_cover.exists(), "the live entry survives the sweep");
    }

    #[test]
    fn cover_pool_size_clamps_the_override_and_bounds_the_default() {
        // D17: BOOKKEEPER_COVER_WORKERS clamps to 1..8; the default is
        // min(4, max(1, cores/2)) and never 0 or above 4.
        assert_eq!(pool_size(Some("1")), 1);
        assert_eq!(pool_size(Some("2")), 2);
        assert_eq!(pool_size(Some("100")), 8, "override clamps to 8");
        assert_eq!(pool_size(Some("0")), 1, "zero clamps up to one worker");
        assert_eq!(
            pool_size(Some("not a number")),
            pool_size(None),
            "an unparseable override falls back to the default"
        );
        let default = pool_size(None);
        assert!(
            (1..=DEFAULT_MAX_COVER_WORKERS).contains(&default),
            "default pool size is bounded 1..=4, got {default}"
        );
    }

    #[test]
    fn worker_pool_renders_every_cover_exactly_once_across_two_workers() {
        if !pdfium_binary_present() {
            eprintln!(
                "pdfium binary missing — run scripts/fetch-pdfium.mjs; skipping pool render proof"
            );
            return;
        }
        let temp = TempDir::new("covers-pool");
        let sink = Arc::new(RecordingSink::default());
        let queue = start_pool_with_workers(
            temp.path().to_path_buf(),
            sink.clone(),
            2,
            RenderService::shared(),
        );

        // 30 valid fixtures (enough to cross the 25-job throttle), one
        // unrenderable file, and a duplicate of the first job: the cache
        // must produce exactly one file per key (AC-57), the failure must
        // not block the queue (R5.3).
        let mut jobs = Vec::new();
        for index in 1..=30u32 {
            let path = fixture_doc(index);
            let metadata = std::fs::metadata(&path).unwrap();
            jobs.push((
                path.to_string_lossy().into_owned(),
                metadata.len() as i64,
                epoch_secs(&metadata),
            ));
        }
        let broken = temp.copy_fixture("malformed.pdf");
        let broken_meta = std::fs::metadata(&broken).unwrap();
        let broken_key = (
            broken.to_string_lossy().into_owned(),
            broken_meta.len() as i64,
            epoch_secs(&broken_meta),
        );
        let total_jobs = jobs.len() + 2;
        for (doc_id, (path, size, mtime)) in jobs.iter().enumerate() {
            queue.enqueue(CoverJob {
                doc_id: doc_id as i64 + 1,
                path: path.clone(),
                size: *size,
                mtime: *mtime,
                password: None,
                cover_choice: None,
            });
        }
        queue.enqueue(CoverJob {
            doc_id: 1,
            path: jobs[0].0.clone(),
            size: jobs[0].1,
            mtime: jobs[0].2,
            password: None,
            cover_choice: None,
        });
        queue.enqueue(CoverJob {
            doc_id: 31,
            path: broken_key.0.clone(),
            size: broken_key.1,
            mtime: broken_key.2,
            password: None,
            cover_choice: None,
        });

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while queue.pending.load(Ordering::Relaxed) > 0 && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        assert_eq!(
            queue.pending.load(Ordering::Relaxed),
            0,
            "AC-57: the pool drains the queue"
        );

        let pngs: Vec<_> = std::fs::read_dir(temp.path())
            .unwrap()
            .flatten()
            .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("png"))
            .collect();
        assert_eq!(
            pngs.len(),
            30,
            "AC-57: 30 distinct documents produced exactly 30 covers — the duplicate rendered nothing extra"
        );
        for (path, size, mtime) in &jobs {
            assert!(
                cache_path(temp.path(), path, *size, *mtime, None).exists(),
                "AC-57: every document in the batch has its cover from the pipeline"
            );
        }
        assert!(
            !cache_path(temp.path(), &broken_key.0, broken_key.1, broken_key.2, None).exists(),
            "R5.3: the unrenderable file produced no cover and did not block the queue"
        );

        let events = sink.progress();
        assert!(
            events.len() >= 2,
            "AC-57: the 25-job throttle and the final drain both emitted progress events"
        );
        let mut last_processed = 0i64;
        for event in &events {
            assert!(
                event.processed >= last_processed,
                "AC-57: progress events stay monotonic across workers"
            );
            assert!(event.processed <= event.total);
            last_processed = event.processed;
        }
        let final_event = events.last().unwrap();
        assert_eq!(
            (final_event.processed, final_event.total),
            (total_jobs as i64, total_jobs as i64),
            "AC-57: the final drain event reports the whole batch"
        );
    }

    // ─────────────────────────────── cover choice (FR-31, SPEC-v0.0.12 §3.8)

    #[test]
    fn no_choice_keys_are_byte_identical_to_the_released_function() {
        // AC-180 (test-pinned literals, taken from the released function):
        // with no choice nothing is hashed and the key is unchanged — the
        // migration re-keys nothing, untouched documents never re-render.
        assert_eq!(
            cache_key("C:/a.pdf", 10, 20, RENDER_TARGET_WIDTH, None),
            "91ec229e41afc9b6",
        );
        assert_eq!(
            cache_key("C:/one.pdf", 10, 20, RENDER_TARGET_WIDTH, None),
            "4a236dd366969dcf",
        );

        // A choice changes the key; the tags follow the §3.8 grammar.
        assert_eq!(choice_tag(&CoverChoice::Page(3)), "p3");
        let tag = choice_tag(&CoverChoice::EpubImage {
            entry: "OEBPS/images/cover.png".into(),
            media: "image/png".into(),
        });
        assert!(
            tag.starts_with('e') && tag.len() == 17 && tag[1..].chars().all(|c| c.is_ascii_hexdigit()),
            "an EPUB choice tags as e<entry-hash>, got {tag}"
        );
        assert_ne!(
            cache_key("C:/a.pdf", 10, 20, RENDER_TARGET_WIDTH, Some("p3")),
            "91ec229e41afc9b6",
            "the tagged key differs from the automatic one"
        );
    }

    #[test]
    fn cover_choice_grammar_is_validated_at_the_boundary() {
        // §3.8: kind/file-type match, malformed JSON and malformed page
        // numbers are rejected at the command boundary; an out-of-range page
        // number is grammatical — its fallback is the render's job (DR-404).
        assert_eq!(
            validate_choice(r#"{"kind":"page","page":2}"#, "C:/a.pdf").unwrap(),
            CoverChoice::Page(2)
        );
        assert_eq!(
            validate_choice(r#"{"kind":"page","page":0}"#, "C:/a.pdf").unwrap(),
            CoverChoice::Page(0)
        );
        assert_eq!(
            validate_choice(r#"{"kind":"page","page":999999}"#, "C:/a.pdf").unwrap(),
            CoverChoice::Page(999999),
            "out of range is not a grammar error"
        );
        assert_eq!(
            validate_choice(
                r#"{"kind":"epub-image","entry":"OEBPS/images/i.png","media":"image/png"}"#,
                "C:/a.epub",
            )
            .unwrap(),
            CoverChoice::EpubImage {
                entry: "OEBPS/images/i.png".into(),
                media: "image/png".into(),
            }
        );

        let bad: [(&str, &str); 11] = [
            ("not json", "C:/a.pdf"),
            (r#"{"kind":"page","page":-1}"#, "C:/a.pdf"),
            (r#"{"kind":"page","page":1.5}"#, "C:/a.pdf"),
            (r#"{"kind":"page","page":4294967296}"#, "C:/a.pdf"),
            (r#"{"kind":"page","page":2,"extra":1}"#, "C:/a.pdf"),
            (r#"{"kind":"page"}"#, "C:/a.pdf"),
            (
                r#"{"kind":"epub-image","entry":"i.png","media":"image/png"}"#,
                "C:/a.pdf",
            ),
            (r#"{"kind":"page","page":2}"#, "C:/a.epub"),
            (r#"{"kind":"other","page":2}"#, "C:/a.pdf"),
            (r#"{"kind":"epub-image","entry":"","media":"image/png"}"#, "C:/a.epub"),
            (r#"[1,2]"#, "C:/a.pdf"),
        ];
        for (value, path) in bad {
            assert!(
                matches!(validate_choice(value, path), Err(AppError::Validation(_))),
                "must reject: {value} ({path})"
            );
        }

        // The read side is lenient: an unparseable column value behaves like
        // no choice at all (the automatic cover), never as an error.
        assert_eq!(parse_choice("garbage"), None);
        assert_eq!(parse_choice(r#"{"kind":"page","page":-1}"#), None);
        assert_eq!(parse_choice(r#"{"kind":"page","page":4}"#), Some(CoverChoice::Page(4)));
    }

    #[test]
    fn enqueue_missing_covers_reads_the_choice_column_and_checks_the_tagged_key() {
        // §3.8: the expected key is the choice-tagged one — a document whose
        // chosen cover is missing is re-enqueued even when its superseded
        // automatic file still sits in the cache, and a job carries the
        // parsed choice to the render stage.
        let conn = open_in_memory().unwrap();
        let temp = TempDir::new("covers-choice-enqueue");
        conn.execute(
            "INSERT INTO documents (id, path, filename, title, size, mtime, added_at, cover_choice)
             VALUES (1, 'C:/one.pdf', 'one.pdf', 'one', 10, 20, 0, '{\"kind\":\"page\",\"page\":2}'),
                    (2, 'C:/two.pdf', 'two.pdf', 'two', 30, 40, 0, '{\"kind\":\"page\",\"page\":2}'),
                    (3, 'C:/three.pdf', 'three.pdf', 'three', 50, 60, 0, '{\"kind\":\"page\",\"page\":2}')",
            [],
        )
        .unwrap();
        let choice = CoverChoice::Page(2);
        // doc 2: only the superseded automatic file present → must re-enqueue;
        // doc 3: the chosen cover present → must not.
        std::fs::write(cache_path(temp.path(), "C:/two.pdf", 30, 40, None), "png").unwrap();
        std::fs::write(cache_path(temp.path(), "C:/three.pdf", 50, 60, Some(&choice)), "png").unwrap();

        let (queue, receiver) = test_queue(temp.path().to_path_buf());
        enqueue_missing_covers(&conn, &queue).unwrap();
        let jobs: Vec<CoverJob> = receiver.try_iter().collect();
        assert_eq!(
            jobs.iter().map(|job| job.doc_id).collect::<Vec<_>>(),
            vec![1, 2],
            "missing-chosen covers re-enqueue, present ones do not"
        );
        assert!(jobs.iter().all(|job| job.cover_choice == Some(CoverChoice::Page(2))));
    }

    #[test]
    fn sweep_keeps_the_chosen_cover_and_sweeps_the_superseded_automatic_one() {
        // §3.8: a row's expected key is its choice-tagged one — the chosen
        // cover survives the startup sweep, the superseded automatic file is
        // an orphan.
        let conn = open_in_memory().unwrap();
        let temp = TempDir::new("covers-choice-sweep");
        conn.execute(
            "INSERT INTO documents (path, filename, title, size, mtime, added_at, cover_choice)
             VALUES ('C:/kept.pdf', 'kept.pdf', 'kept', 10, 20, 0, '{\"kind\":\"page\",\"page\":1}')",
            [],
        )
        .unwrap();
        let choice = CoverChoice::Page(1);
        std::fs::write(cache_path(temp.path(), "C:/kept.pdf", 10, 20, None), "png").unwrap();
        std::fs::write(cache_path(temp.path(), "C:/kept.pdf", 10, 20, Some(&choice)), "png").unwrap();
        std::fs::write(temp.path().join("stale.png"), "png").unwrap();

        sweep_orphans(&conn, temp.path()).unwrap();

        assert!(
            cache_path(temp.path(), "C:/kept.pdf", 10, 20, Some(&choice)).exists(),
            "the chosen cover is the row's expected file"
        );
        assert!(
            !cache_path(temp.path(), "C:/kept.pdf", 10, 20, None).exists(),
            "the superseded automatic cover is swept as an orphan"
        );
        assert!(!temp.path().join("stale.png").exists());
    }

    #[test]
    fn a_chosen_page_renders_under_the_tagged_key_and_completes_without_loops() {
        // §3.8 render dispatch + AC-180's no-loop fact: the chosen page
        // renders through the pipeline under the choice-tagged key — the
        // pixels are the chosen page's, not page 1's — and afterwards both
        // the startup enqueue and the sweep treat the entry as complete.
        if !pdfium_binary_present() {
            eprintln!("pdfium binary missing — run scripts/fetch-pdfium.mjs; skipping render proof");
            return;
        }
        let temp = TempDir::new("covers-choice-render");
        let path = fixture_multi("multi-012.pdf").to_string_lossy().into_owned();
        let metadata = std::fs::metadata(&path).unwrap();
        let (size, mtime) = (metadata.len() as i64, epoch_secs(&metadata));
        let conn = open_in_memory().unwrap();
        conn.execute(
            "INSERT INTO documents (id, path, filename, title, size, mtime, added_at, cover_choice)
             VALUES (1, ?1, 'multi-012.pdf', 'm', ?2, ?3, 0, '{\"kind\":\"page\",\"page\":5}')",
            rusqlite::params![path, size, mtime],
        )
        .unwrap();

        let sink: Arc<dyn EventSink> = Arc::new(RecordingSink::default());
        let queue =
            start_pool_with_workers(temp.path().to_path_buf(), sink, 1, RenderService::shared());
        enqueue_missing_covers(&conn, &queue).unwrap(); // the enqueue path carries the choice
        drain(&queue, 90);

        let choice = CoverChoice::Page(5);
        let chosen_file = cache_path(temp.path(), &path, size, mtime, Some(&choice));
        assert!(chosen_file.exists(), "the chosen cover lands under the tagged key");
        assert!(
            !cache_path(temp.path(), &path, size, mtime, None).exists(),
            "nothing is written under the automatic key"
        );

        // Lossless round-trip (the AC-74 pattern): the cached file decodes
        // back as the chosen page's render — and not as page 1.
        let service = RenderService::shared();
        let decoded = image::open(&chosen_file)
            .expect("the chosen cover decodes")
            .into_bytes();
        let chosen_render = service
            .render_cover(&path, 5, RENDER_TARGET_WIDTH, RENDER_TARGET_HEIGHT, None)
            .expect("page 6 (0-based 5) of the fixture renders")
            .into_bytes();
        let first_page_render = service
            .render_cover(&path, 0, RENDER_TARGET_WIDTH, RENDER_TARGET_HEIGHT, None)
            .expect("page 1 renders")
            .into_bytes();
        assert_eq!(decoded, chosen_render, "the rendered image is the chosen page");
        assert_ne!(decoded, first_page_render, "the chosen cover is not the automatic one");

        // No-loop fact (DR-404): the entry is complete for the next startup —
        // neither the enqueue nor the sweep touches it again.
        let (idle_queue, idle_receiver) = test_queue(temp.path().to_path_buf());
        enqueue_missing_covers(&conn, &idle_queue).unwrap();
        assert_eq!(idle_receiver.try_iter().count(), 0, "no re-enqueue loop");
        sweep_orphans(&conn, temp.path()).unwrap();
        assert!(chosen_file.exists(), "the chosen cover survives the sweep");
    }

    #[test]
    fn an_out_of_range_page_choice_renders_the_automatic_cover_under_its_tagged_key() {
        // DR-404 + §3.10 (out-of-range half): the choice stands, page 1 is
        // rendered, the file lands under the tagged key, and the entry is
        // complete — no enqueue/sweep loop.
        if !pdfium_binary_present() {
            eprintln!("pdfium binary missing — run scripts/fetch-pdfium.mjs; skipping render proof");
            return;
        }
        let temp = TempDir::new("covers-choice-out-of-range");
        let path = fixture_multi("multi-012.pdf").to_string_lossy().into_owned();
        let metadata = std::fs::metadata(&path).unwrap();
        let (size, mtime) = (metadata.len() as i64, epoch_secs(&metadata));
        let conn = open_in_memory().unwrap();
        conn.execute(
            "INSERT INTO documents (id, path, filename, title, size, mtime, added_at, cover_choice)
             VALUES (1, ?1, 'multi-012.pdf', 'm', ?2, ?3, 0, '{\"kind\":\"page\",\"page\":999}')",
            rusqlite::params![path, size, mtime],
        )
        .unwrap();

        let sink: Arc<dyn EventSink> = Arc::new(RecordingSink::default());
        let queue =
            start_pool_with_workers(temp.path().to_path_buf(), sink, 1, RenderService::shared());
        enqueue_missing_covers(&conn, &queue).unwrap();
        drain(&queue, 90);

        let choice = CoverChoice::Page(999);
        let tagged_file = cache_path(temp.path(), &path, size, mtime, Some(&choice));
        assert!(
            tagged_file.exists(),
            "the fallback render writes under the requested choice-tagged key"
        );
        assert!(
            !cache_path(temp.path(), &path, size, mtime, None).exists(),
            "the automatic key stays empty — the choice identity is what stands"
        );
        let decoded = image::open(&tagged_file).expect("the fallback cover decodes").into_bytes();
        let first_page_render = RenderService::shared()
            .render_cover(&path, 0, RENDER_TARGET_WIDTH, RENDER_TARGET_HEIGHT, None)
            .expect("page 1 renders")
            .into_bytes();
        assert_eq!(decoded, first_page_render, "the fallback renders the automatic cover");

        let (idle_queue, idle_receiver) = test_queue(temp.path().to_path_buf());
        enqueue_missing_covers(&conn, &idle_queue).unwrap();
        assert_eq!(idle_receiver.try_iter().count(), 0, "the fallback file is complete — no loop");
        sweep_orphans(&conn, temp.path()).unwrap();
        assert!(tagged_file.exists(), "the fallback file survives the sweep");
    }
}
