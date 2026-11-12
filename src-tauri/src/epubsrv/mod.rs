//! EPUB content delivery for the reader window (§3.10 option A, §3.12 rule 4;
//! DR-301, DR-302): package entries served as protocol URLs — zip entry
//! reads under the shared size guard, media-type resolution, the sanitizer,
//! and the bounded sanitized-bytes LRU. Plain functions testable with
//! `cargo test` without Tauri (R1.1); the protocol handler in `lib.rs` is a
//! thin wrapper (DR-302 item 6).

use std::collections::{HashMap, VecDeque};
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};
use quick_xml::{Reader, Writer};
use rusqlite::Connection;
use zip::ZipArchive;

use crate::epubmeta;
use crate::error::AppError;
use crate::library::service;

/// The LRU caps, exactly as DR-301 item 6 records them.
const MAX_TOTAL_BYTES: u64 = 32 * 1024 * 1024;
const MAX_TRACKED_ENTRIES: usize = 128;

/// The reader window's content base URL for one document (DR-301 item 3,
/// DR-297 correction 1): Windows uses the origin wry maps app-defined
/// schemes to; other platforms use the scheme directly. The protocol handler
/// accepts both forms (`parse_request`), so the branch only selects what the
/// frontend and the manifest command are given.
pub fn base_url(doc_id: i64) -> String {
    if cfg!(windows) {
        format!("http://pdfkeeper-epub.localhost/{doc_id}/")
    } else {
        format!("pdfkeeper-epub://{doc_id}/")
    }
}

/// The protocol request split into (doc-id, package entry path). Three URI
/// shapes arrive: the custom form `pdfkeeper-epub://<doc-id>/<path>` (the
/// authority carries the doc-id); the Windows origin form as the engine
/// issues it, `http://pdfkeeper-epub.localhost/<doc-id>/<path>` (empty or
/// `pdfkeeper-epub.localhost` authority); and the same Windows request as
/// wry hands the handler after its scheme rewrite —
/// `pdfkeeper-epub://localhost/<doc-id>/<path>`, authority `localhost`, the
/// doc-id in the first path segment (measured 2026-11-11 in the running
/// app, DR-305; the §3.10 probe did not pin this form down). The entry
/// path is normalized by the §3.2 rule — percent-decode, backslash
/// separators, dot-segment normalization, fragment/query stripped — so a
/// request that would leave the archive namespace (a `..` past the root) or
/// names a non-positive document is `None` (404, §3.10 namespace rule).
pub fn parse_request(authority: &str, path: &str) -> Option<(i64, String)> {
    let (doc_id, entry_path) =
        if authority.is_empty()
            || authority.eq_ignore_ascii_case("pdfkeeper-epub.localhost")
            || authority.eq_ignore_ascii_case("localhost")
        {
            let trimmed = path.trim_start_matches('/');
            let (doc_id, rest) = trimmed.split_once('/')?;
            (doc_id, rest)
        } else {
            (authority, path)
        };
    let doc_id = doc_id.parse::<i64>().ok()?;
    if doc_id <= 0 {
        return None;
    }
    Some((doc_id, epubmeta::resolve_href("", entry_path)?))
}

/// The reader manifest (§3.9 consequence 8, DR-301 items 2/3): the spine
/// with per-entry facts, the ToC in the released `OutlineEntry` shape, the
/// platform-correct base URL, and the declared language/direction.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ManifestDto {
    pub spine: Vec<epubmeta::EpubSpineItem>,
    pub toc: Vec<crate::pdfmeta::OutlineEntry>,
    pub base_url: String,
    pub language: Option<String>,
    pub direction: Option<String>,
}

pub fn manifest(doc_id: i64, path: &str) -> Result<ManifestDto, AppError> {
    let package = epubmeta::read_package(path)?;
    Ok(ManifestDto {
        spine: package.spine,
        toc: package.toc,
        base_url: base_url(doc_id),
        language: package.language,
        direction: package.direction,
    })
}

// ---------------------------------------------------------------------------
// The serving LRU (§3.12 rule 4; DR-301 item 6, DR-302 item 5)
// ---------------------------------------------------------------------------

/// Cache key: (doc-id, entry path, file mtime) — the DR-113 discipline; an
/// mtime change (rename/relink follow-through) invalidates the whole
/// generation (DR-301 item 6).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    doc_id: i64,
    entry: String,
    mtime: i64,
}

/// One served entry: bytes plus the headers the protocol answers with
/// (DR-301 item 6). The ETag is mtime + byte length; the protocol maps
/// every typed error to 404 (DR-301 item 9).
#[derive(Debug, Clone)]
pub struct Served {
    pub content_type: String,
    pub etag: String,
    pub bytes: Vec<u8>,
}

#[derive(Default)]
struct Lru {
    entries: HashMap<CacheKey, Served>,
    order: VecDeque<CacheKey>,
    total_bytes: u64,
    /// Declared manifest media types per (doc-id, mtime) (DR-302 item 2):
    /// built once per file generation from the OPF, consulted before the
    /// extension map. The old generation is dropped when the file changes.
    declared: HashMap<i64, (i64, HashMap<String, String>)>,
}

/// App-managed state (DR-302 item 5): the serving LRU, the same
/// managed-state pattern as `Db`/`Covers`/`Reader`.
pub struct EpubSrv(Mutex<Lru>);

impl Default for EpubSrv {
    fn default() -> Self {
        Self::new()
    }
}

impl EpubSrv {
    pub fn new() -> Self {
        Self(Mutex::new(Lru::default()))
    }

    fn lock(&self) -> MutexGuard<'_, Lru> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn lookup(&self, key: &CacheKey) -> Option<Served> {
        let mut lru = self.lock();
        let served = lru.entries.get(key).cloned();
        if served.is_some() {
            touch(&mut lru, key);
        }
        served
    }

    fn put(&self, key: CacheKey, served: Served) {
        let mut lru = self.lock();
        if let Some(old) = lru.entries.remove(&key) {
            lru.total_bytes -= old.bytes.len() as u64;
        }
        lru.total_bytes += served.bytes.len() as u64;
        lru.entries.insert(key.clone(), served);
        touch(&mut lru, &key);
        while lru.entries.len() > MAX_TRACKED_ENTRIES || lru.total_bytes > MAX_TOTAL_BYTES {
            let Some(oldest) = lru.order.pop_back() else { break };
            if let Some(evicted) = lru.entries.remove(&oldest) {
                lru.total_bytes -= evicted.bytes.len() as u64;
            }
        }
    }

    /// The declared media type of one entry, from the per-generation map
    /// (DR-302 item 2). A type that cannot ride a header safely is not
    /// used (the extension map decides instead).
    fn declared_type(&self, doc_id: i64, mtime: i64, path: &str, entry: &str) -> Option<String> {
        let mut lru = self.lock();
        if lru
            .declared
            .get(&doc_id)
            .is_some_and(|(cached, _)| *cached != mtime)
        {
            lru.declared.remove(&doc_id);
        }
        let (_, map) = lru
            .declared
            .entry(doc_id)
            .or_insert_with(|| (mtime, epubmeta::media_types(path).unwrap_or_default()));
        map.get(entry)
            .filter(|media_type| is_header_safe(media_type))
            .cloned()
    }

    #[cfg(test)]
    fn cached_len(&self) -> usize {
        self.lock().entries.len()
    }
}

fn touch(lru: &mut Lru, key: &CacheKey) {
    lru.order.retain(|candidate| candidate != key);
    lru.order.push_front(key.clone());
}

// ---------------------------------------------------------------------------
// serve(): the request pipeline
// ---------------------------------------------------------------------------

/// Serve one protocol request for a library document (DR-302 item 1): the
/// document resolves through the released `open_target` — a missing row or
/// file yields the typed errors the reader commands produce, before any file
/// read — and a non-EPUB document is the typed `UnsupportedType`. The entry
/// is answered from the LRU or from the archive: HTML-family and SVG entries
/// are sanitized, everything else is served raw (DR-301 item 4). Every
/// failure inside the archive (unreadable package, absent entry, size
/// guard) is `EpubUnopenable`; the protocol maps all typed errors to 404
/// (DR-301 item 9).
pub fn serve(
    conn: &Connection,
    srv: &EpubSrv,
    doc_id: i64,
    entry: &str,
) -> Result<Served, AppError> {
    let target = service::open_target(conn, doc_id)?;
    if !is_epub_path(&target.path) {
        return Err(AppError::UnsupportedType { path: target.path });
    }
    let mtime = file_mtime(&target.path, doc_id)?;
    let key = CacheKey {
        doc_id,
        entry: entry.to_string(),
        mtime,
    };
    if let Some(served) = srv.lookup(&key) {
        return Ok(served);
    }

    let mut archive = open_archive(&target.path, doc_id)?;
    // Synthetic `data:` entries (DR-301 item 5): the source chapter rides in
    // the entry path (`data/<chapter path>/<hash>.<ext>` — the hash is the
    // last segment), so a post-eviction request re-derives deterministically.
    if let Some(rest) = entry.strip_prefix("data/") {
        let (chapter, _) = rest.rsplit_once('/').ok_or_else(|| unreadable(&target.path))?;
        let served = derive_synthetic(&mut archive, &target.path, doc_id, chapter, entry, mtime)?;
        srv.put(key, served.clone());
        return Ok(served);
    }

    let mut file = archive.by_name(entry).map_err(|_| unreadable(&target.path))?;
    if file.size() > epubmeta::MAX_ENTRY_BYTES {
        return Err(unreadable(&target.path));
    }
    let mut raw = Vec::new();
    if file.read_to_end(&mut raw).is_err() {
        return Err(unreadable(&target.path));
    }

    let declared = srv.declared_type(doc_id, mtime, &target.path, entry);
    let extension = entry
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default();
    let served = if is_html_family(&declared)
        || (declared.is_none() && matches!(extension.as_str(), "xhtml" | "htm" | "html"))
    {
        let (bytes, data_entries) = sanitize(&raw, doc_id, entry);
        for data in data_entries {
            srv.put(
                CacheKey {
                    doc_id,
                    entry: data.entry,
                    mtime,
                },
                Served {
                    etag: etag(mtime, &data.bytes),
                    content_type: data.content_type,
                    bytes: data.bytes,
                },
            );
        }
        Served {
            content_type: "text/html".to_string(),
            etag: etag(mtime, &bytes),
            bytes,
        }
    } else if declared.as_deref() == Some("image/svg+xml") || (declared.is_none() && extension == "svg") {
        let (bytes, _) = sanitize(&raw, doc_id, entry);
        Served {
            content_type: "image/svg+xml".to_string(),
            etag: etag(mtime, &bytes),
            bytes,
        }
    } else if declared
        .as_deref()
        .is_some_and(|media_type| media_type.eq_ignore_ascii_case("text/css"))
        || (declared.is_none() && extension == "css")
    {
        // DR-307: CSS is delivered like chapter `<style>` text — `url()`
        // tokens rewritten to protocol URLs resolved from the CSS entry's own
        // directory, so a face hoisted to document scope (the reader's
        // embedded-font path) carries an absolute URL. Synthetic `data:`
        // entries referenced from CSS are registered here; a post-eviction
        // request for one re-derives like any other synthetic entry.
        let (bytes, data_entries) = rewrite_css_asset(&raw, doc_id, entry);
        for data in data_entries {
            srv.put(
                CacheKey {
                    doc_id,
                    entry: data.entry,
                    mtime,
                },
                Served {
                    etag: etag(mtime, &data.bytes),
                    content_type: data.content_type,
                    bytes: data.bytes,
                },
            );
        }
        Served {
            content_type: "text/css".to_string(),
            etag: etag(mtime, &bytes),
            bytes,
        }
    } else {
        let content_type = declared
            .or_else(|| extension_media_type(&extension).map(str::to_string))
            .unwrap_or_else(|| "application/octet-stream".to_string());
        Served {
            etag: etag(mtime, &raw),
            content_type,
            bytes: raw,
        }
    };
    srv.put(key, served.clone());
    Ok(served)
}

fn unreadable(path: &str) -> AppError {
    AppError::EpubUnopenable {
        path: path.to_string(),
    }
}

fn open_archive(path: &str, doc_id: i64) -> Result<ZipArchive<File>, AppError> {
    let file = File::open(path).map_err(|_| AppError::MissingFile {
        path: path.to_string(),
        document_id: doc_id,
    })?;
    ZipArchive::new(file).map_err(|_| unreadable(path))
}

/// The §3.4 type check (DR-302 item 1): the same extension rule as the
/// import pipeline's, shared with the manifest command.
pub fn is_epub_path(path: &str) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("epub"))
}

fn file_mtime(path: &str, doc_id: i64) -> Result<i64, AppError> {
    fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|since| since.as_secs() as i64)
        .ok_or(AppError::MissingFile {
            path: path.to_string(),
            document_id: doc_id,
        })
}

/// Re-derive one synthetic `data:` entry (DR-301 item 5): read + sanitize
/// the source chapter and pick the entry by its hash segment — the same
/// bytes the original sanitize produced.
fn derive_synthetic(
    archive: &mut ZipArchive<File>,
    path: &str,
    doc_id: i64,
    chapter: &str,
    requested: &str,
    mtime: i64,
) -> Result<Served, AppError> {
    let mut file = archive.by_name(chapter).map_err(|_| unreadable(path))?;
    if file.size() > epubmeta::MAX_ENTRY_BYTES {
        return Err(unreadable(path));
    }
    let mut raw = Vec::new();
    if file.read_to_end(&mut raw).is_err() {
        return Err(unreadable(path));
    }
    let (_, data_entries) = sanitize(&raw, doc_id, chapter);
    let data = data_entries
        .into_iter()
        .find(|candidate| candidate.entry == requested)
        .ok_or_else(|| unreadable(path))?;
    Ok(Served {
        content_type: data.content_type,
        etag: etag(mtime, &data.bytes),
        bytes: data.bytes,
    })
}

fn etag(mtime: i64, bytes: &[u8]) -> String {
    format!("\"{mtime:x}-{:x}\"", bytes.len())
}

// ---------------------------------------------------------------------------
// Media types (DR-301 item 4, DR-302 item 2)
// ---------------------------------------------------------------------------

fn is_html_family(media_type: &Option<String>) -> bool {
    media_type.as_deref().is_some_and(|media_type| {
        matches!(
            media_type.to_ascii_lowercase().as_str(),
            "application/xhtml+xml" | "text/html" | "application/html+xml"
        )
    })
}

/// The extension map for entries the manifest does not declare (DR-302 item
/// 2). An undeclared entry with an unmapped extension is octet-stream.
fn extension_media_type(extension: &str) -> Option<&'static str> {
    Some(match extension {
        "xhtml" | "htm" | "html" => "text/html",
        "css" => "text/css",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "js" => "text/javascript",
        "json" => "application/json",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        "mp4" => "video/mp4",
        _ => return None,
    })
}

/// Whether a declared type can ride a `Content-Type` header: the RFC 7231
/// token character set (author-declared noise is filtered out here, per
/// DR-302 item 2 — the extension map decides instead).
fn is_header_safe(media_type: &str) -> bool {
    !media_type.is_empty()
        && media_type.len() <= 128
        && media_type.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(byte,
                    b'!' | b'#' | b'$' | b'%' | b'&' | b'\'' | b'+' | b'-' | b'.' | b'^'
                    | b'_' | b'`' | b'|' | b'~' | b'/' | b':')
        })
}

fn data_extension(media_type: &str) -> &'static str {
    match media_type {
        "image/png" => "png",
        "image/jpeg" | "image/jpg" => "jpg",
        "image/gif" => "gif",
        "image/svg+xml" => "svg",
        "image/webp" => "webp",
        "image/bmp" => "bmp",
        "text/css" => "css",
        "text/javascript" => "js",
        "application/json" => "json",
        _ => "bin",
    }
}

// ---------------------------------------------------------------------------
// The sanitizer (DR-301 item 5; AC-137)
// ---------------------------------------------------------------------------

/// A decoded `data:` URI re-served as a synthetic protocol entry
/// (DR-301 item 5): the path is `data/<source-chapter path>/<fnv1a64>.<ext>`
/// — the chapter path with literal separators, the hash as the last segment
/// — so a cold request re-derives it from the chapter. The path is
/// percent-encoded only at the URL boundary (`encode_path`), because a
/// protocol request is normalized back by the §3.2 rule before `serve`
/// splits it.
struct DataEntry {
    entry: String,
    content_type: String,
    bytes: Vec<u8>,
}

struct Sanitizer {
    base: String,
    doc_dir: String,
    chapter: String,
    data: Vec<DataEntry>,
}

/// Sanitize one chapter (or SVG) document. The filters: `<script>`,
/// `<iframe>`, `<object>`, `<embed>` subtrees removed; every `on*`
/// attribute removed; `javascript:` hrefs removed; resource and link
/// attributes referencing package entries rewritten to protocol URLs
/// resolved from the document's own location; `url()` tokens in `<style>`
/// text and `style` attributes rewritten by the same rule; `data:` URIs
/// decoded into synthetic entries (the released CSP's `img-src` does not
/// admit `data:`); `loading="lazy"` added to every `<img>` (§3.12
/// build-stage optimization). Fragment-only and external URLs pass through
/// unchanged (§3.13: the UI resolves internal links, the OS browser gets
/// external ones).
///
/// Best-effort by contract (DR-302 item 4): an unparseable or non-UTF-8
/// document is returned unchanged — the reading surface never mounts book
/// documents (DR-297 correction 3) and the CSP stands behind this layer.
fn sanitize(raw: &[u8], doc_id: i64, document_entry: &str) -> (Vec<u8>, Vec<DataEntry>) {
    let Ok(text) = std::str::from_utf8(raw) else {
        return (raw.to_vec(), Vec::new());
    };
    let mut sanitizer = Sanitizer {
        base: base_url(doc_id),
        doc_dir: epubmeta::opf_dir(document_entry),
        chapter: document_entry.to_string(),
        data: Vec::new(),
    };
    match sanitize_document(text, &mut sanitizer) {
        Some(bytes) => (bytes, sanitizer.data),
        None => (raw.to_vec(), Vec::new()),
    }
}

/// Rewrite one CSS asset (DR-307): the `url()` tokens get the same treatment
/// as chapter `<style>` text, resolved from the CSS entry's own directory.
/// Best-effort by the same contract as `sanitize`: non-UTF-8 bytes are
/// returned unchanged.
fn rewrite_css_asset(raw: &[u8], doc_id: i64, entry: &str) -> (Vec<u8>, Vec<DataEntry>) {
    let Ok(text) = std::str::from_utf8(raw) else {
        return (raw.to_vec(), Vec::new());
    };
    let mut sanitizer = Sanitizer {
        base: base_url(doc_id),
        doc_dir: epubmeta::opf_dir(entry),
        chapter: entry.to_string(),
        data: Vec::new(),
    };
    (sanitizer.rewrite_css(text).into_bytes(), sanitizer.data)
}

fn sanitize_document(text: &str, sanitizer: &mut Sanitizer) -> Option<Vec<u8>> {
    let mut reader = Reader::from_str(text);
    reader.config_mut().trim_text(false); // XHTML prose whitespace is content (DR-302 item 4)
    let mut writer = Writer::new(Vec::new());
    let mut skip: Option<(String, usize)> = None; // (local name, subtree depth)
    let mut style: Option<(BytesStart<'static>, String, String)> = None; // (start tag, full name, text)
    loop {
        let event = reader.read_event().ok()?;
        match event {
            Event::Start(e) => {
                if let Some((_, depth)) = skip.as_mut() {
                    *depth += 1;
                    continue;
                }
                if style.is_some() {
                    continue; // malformed markup inside <style>: dropped
                }
                let local = e.local_name();
                if matches!(local.as_ref(), "script" | "iframe" | "object" | "embed") {
                    skip = Some((local.as_ref().to_string(), 1));
                    continue;
                }
                if local.as_ref() == "style" {
                    style = Some((
                        rewrite_start(&e, sanitizer),
                        e.name().as_ref().to_string(),
                        String::new(),
                    ));
                    continue;
                }
                writer.write_event(Event::Start(rewrite_start(&e, sanitizer))).ok()?;
            }
            Event::Empty(e) => {
                if skip.is_some()
                    || matches!(e.local_name().as_ref(), "script" | "iframe" | "object" | "embed")
                {
                    continue;
                }
                writer.write_event(Event::Empty(rewrite_start(&e, sanitizer))).ok()?;
            }
            Event::Text(e) => {
                if let Some((_, _, captured)) = style.as_mut() {
                    captured.push_str(&e);
                    continue;
                }
                if skip.is_some() {
                    continue;
                }
                writer.write_event(Event::Text(e)).ok()?;
            }
            Event::CData(e) => {
                if let Some((_, _, captured)) = style.as_mut() {
                    captured.push_str(&e);
                    continue;
                }
                if skip.is_some() {
                    continue;
                }
                writer.write_event(Event::CData(e)).ok()?;
            }
            Event::End(e) => {
                if skip.is_some() {
                    let mut closes = false;
                    if let Some((skip_name, depth)) = skip.as_mut() {
                        if *depth == 1 && e.local_name().as_ref() == skip_name.as_str() {
                            closes = true;
                        } else {
                            *depth -= 1;
                        }
                    }
                    if closes {
                        skip = None;
                    }
                    continue;
                }
                if e.local_name().as_ref() == "style" {
                    if let Some((start, name, captured)) = style.take() {
                        let rewritten = sanitizer.rewrite_css(&captured);
                        writer.write_event(Event::Start(start)).ok()?;
                        writer
                            .write_event(Event::Text(BytesText::from_escaped(rewritten)))
                            .ok()?;
                        writer.write_event(Event::End(BytesEnd::new(name))).ok()?;
                    }
                    continue;
                }
                if style.is_some() {
                    continue;
                }
                writer.write_event(Event::End(e)).ok()?;
            }
            Event::Eof => break,
            other => {
                // Declarations, comments, PIs and entity references pass
                // through outside the filtered regions (DR-302 item 4).
                if skip.is_none() && style.is_none() {
                    writer.write_event(other).ok()?;
                }
            }
        }
    }
    Some(writer.into_inner())
}

/// Rebuild one start/empty tag with the attribute filters applied. Values
/// that pass unchanged keep their source escaping (raw copy — quick-xml
/// hands attribute values as they appear in the source); rewritten values
/// are escaped on write (the 0.42 writer does not escape attribute content).
fn rewrite_start(e: &BytesStart, sanitizer: &mut Sanitizer) -> BytesStart<'static> {
    let mut out = BytesStart::new(e.name().as_ref().to_string());
    let is_img = e.local_name().as_ref() == "img";
    let mut has_loading = false;
    for attribute in e.attributes().with_checks(false).flatten() {
        let key = attribute.key.as_ref();
        let local = key.rsplit(':').next().unwrap_or(key);
        let value = attribute.value.as_ref();
        if local.starts_with("on") && local.len() > 2 {
            continue; // event-handler attributes (AC-137)
        }
        if local == "href" && value.trim_start().to_ascii_lowercase().starts_with("javascript:") {
            continue; // javascript: pseudo-links (DR-301 item 5)
        }
        if local == "loading" {
            has_loading = true;
        }
        // Rewritten values need no escaping: `encode_path` percent-encodes
        // everything outside URL-safe characters, and `rewrite_css` keeps the
        // token structure. Unchanged values are copied raw (source escaping).
        match local {
            "src" | "poster" => match sanitizer.rewrite_reference(value, true) {
                Some(url) => out.push_attribute((key, url.as_str())),
                None => out.push_attribute((key, value)),
            },
            "srcset" => {
                let rewritten = rewrite_srcset(value, sanitizer);
                out.push_attribute((key, rewritten.as_str()));
            }
            "style" => {
                let rewritten = sanitizer.rewrite_css(value);
                out.push_attribute((key, rewritten.as_str()));
            }
            "href" => match sanitizer.rewrite_reference(value, false) {
                Some(url) => out.push_attribute((key, url.as_str())),
                None => out.push_attribute((key, value)),
            }
            _ => out.push_attribute((key, value)),
        }
    }
    if is_img && !has_loading {
        out.push_attribute(("loading", "lazy"));
    }
    out
}

fn rewrite_srcset(value: &str, sanitizer: &mut Sanitizer) -> String {
    value
        .split(',')
        .map(|candidate| {
            let candidate = candidate.trim();
            let (url, descriptor) = candidate
                .split_once(char::is_whitespace)
                .unwrap_or((candidate, ""));
            let rewritten = sanitizer
                .rewrite_reference(url, true)
                .unwrap_or_else(|| url.to_string());
            if descriptor.is_empty() {
                rewritten
            } else {
                format!("{rewritten} {descriptor}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

impl Sanitizer {
    /// Rewrite one resource/link attribute value. `None` means "keep the
    /// original": fragment-only anchors (the chapter owns them, §3.13) and
    /// external URLs (the OS browser, DR-301 item 5). A `data:` URI becomes
    /// a synthetic protocol entry when it decodes, and is dropped when it
    /// does not (the CSP does not admit `data:`). An unresolvable relative
    /// reference is kept as written — the reader's chapter-error surface
    /// handles the failed request.
    fn rewrite_reference(&mut self, value: &str, allow_data: bool) -> Option<String> {
        let value = value.trim();
        if value.is_empty() || value.starts_with('#') {
            return None;
        }
        if allow_data && let Some(rest) = value.strip_prefix("data:") {
            return self.rewrite_data_uri(rest);
        }
        if value.starts_with("//") || has_external_scheme(value) {
            return None;
        }
        // The fragment rides along on the rewritten URL: the UI's link
        // routing needs the entry (→ spine index) and the in-chapter
        // target (§3.13).
        let (target, fragment) = value.split_once('#').unwrap_or((value, ""));
        let entry = epubmeta::resolve_href(&self.doc_dir, target)?;
        let url = format!("{}{}", self.base, encode_path(&entry));
        if fragment.is_empty() {
            Some(url)
        } else {
            Some(format!("{url}#{fragment}"))
        }
    }

    fn rewrite_data_uri(&mut self, rest: &str) -> Option<String> {
        let (bytes, media_type) = decode_data_uri(rest)?;
        let extension = data_extension(&media_type);
        let content_type = if is_header_safe(&media_type) {
            media_type
        } else {
            extension_media_type(extension)
                .unwrap_or("application/octet-stream")
                .to_string()
        };
        let entry = format!("data/{}/{}.{extension}", self.chapter, fnv1a64(&bytes));
        let url = format!("{}{}", self.base, encode_path(&entry));
        self.data.push(DataEntry {
            entry,
            content_type,
            bytes,
        });
        Some(url)
    }

    /// Rewrite every `url(...)` token in CSS text (a `<style>` body or a
    /// `style` attribute value); everything outside the tokens is copied
    /// byte-for-byte.
    fn rewrite_css(&mut self, text: &str) -> String {
        let bytes = text.as_bytes();
        let lower = text.to_ascii_lowercase();
        let mut out = String::with_capacity(text.len());
        let mut index = 0;
        let lower = lower.as_bytes();
        while let Some(offset) = lower[index..].windows(4).position(|window| window == b"url(") {
            let content = index + offset + 4;
            let Some((value_start, value_end, after, quote)) = find_url_token(bytes, content) else {
                out.push_str(&text[index..]);
                return out;
            };
            out.push_str(&text[index..content]);
            let value = String::from_utf8_lossy(&bytes[value_start..value_end]).into_owned();
            match self.rewrite_reference(&value, true) {
                // The token's own delimiter style is kept; the rewritten URL
                // is percent-encoded (`encode_path`), so it needs no quoting
                // or escaping of its own.
                Some(url) => {
                    if quote != 0 {
                        out.push(quote as char);
                    }
                    out.push_str(&url);
                    if quote != 0 {
                        out.push(quote as char);
                    }
                    out.push(')');
                }
                None => out.push_str(&text[content..after]),
            }
            index = after;
        }
        out.push_str(&text[index..]);
        out
    }
}

/// Locate one `url(` token body: returns (value range, index after the
/// closing paren, delimiter byte — 0 when unquoted). Quoted values tolerate
/// any characters (a `data:` URI may carry parentheses); a token without a
/// closing delimiter is not a token.
fn find_url_token(bytes: &[u8], content: usize) -> Option<(usize, usize, usize, u8)> {
    let first = *bytes.get(content)?;
    let quote = match first {
        b'"' | b'\'' => first,
        _ => 0,
    };
    let value_start = content + usize::from(quote != 0);
    let value_end = if quote != 0 {
        value_start + bytes[value_start..].iter().position(|byte| *byte == quote)?
    } else {
        value_start + bytes[value_start..].iter().position(|byte| *byte == b')')?
    };
    let close_paren = if quote != 0 {
        value_end + 1 + bytes[value_end + 1..].iter().position(|byte| *byte == b')')?
    } else {
        value_end
    };
    Some((value_start, value_end, close_paren + 1, quote))
}

fn has_external_scheme(value: &str) -> bool {
    let Some((scheme, _)) = value.split_once(':') else {
        return false;
    };
    !scheme.is_empty()
        && scheme.chars().next().is_some_and(|ch| ch.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.'))
}

fn decode_data_uri(rest: &str) -> Option<(Vec<u8>, String)> {
    let (meta, payload) = rest.split_once(',')?;
    let media_type = meta.split(';').next().unwrap_or("").trim().to_string();
    let base64 = meta
        .split(';')
        .skip(1)
        .any(|part| part.trim().eq_ignore_ascii_case("base64"));
    let payload = payload.trim();
    let bytes = if base64 {
        base64_decode(payload)?
    } else {
        epubmeta::percent_decode(payload)?.into_bytes()
    };
    Some((bytes, media_type))
}

/// The standard-alphabet base64 decoder, hand-rolled so the build adds no
/// dependency (DR-302 item 3). Whitespace and `=` padding are ignored —
/// the lenient reading the data-URI spec describes.
fn base64_decode(input: &str) -> Option<Vec<u8>> {
    let mut accumulator: u32 = 0;
    let mut bits: u32 = 0;
    let mut out: Vec<u8> = Vec::new();
    for byte in input.as_bytes() {
        let value: u32 = match byte {
            b'A'..=b'Z' => u32::from(*byte - b'A'),
            b'a'..=b'z' => u32::from(*byte - b'a') + 26,
            b'0'..=b'9' => u32::from(*byte - b'0') + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' | b' ' | b'\t' | b'\n' | b'\r' => continue,
            _ => return None,
        };
        accumulator = (accumulator << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((accumulator >> bits) as u8);
        }
    }
    Some(out)
}

/// FNV-1a 64-bit (DR-302 item 3): synthetic entries live only in this
/// process's LRU; a cryptographic hash would buy nothing and cost a
/// dependency.
fn fnv1a64(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// Percent-encode an entry path for a URL: everything outside RFC 3986
/// unreserved is escaped, per segment, `/` separators stay literal. A
/// protocol request is normalized back by the §3.2 rule (DR-301 item 5) —
/// the encoding exists only for the wire.
fn encode_path(path: &str) -> String {
    path.split('/')
        .map(|segment| {
            let mut out = String::with_capacity(segment.len());
            for byte in segment.as_bytes() {
                match byte {
                    b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                        out.push(*byte as char);
                    }
                    other => {
                        out.push('%');
                        out.push_str(&format!("{other:02X}"));
                    }
                }
            }
            out
        })
        .collect::<Vec<_>>()
        .join("/")
}

// ---------------------------------------------------------------------------
// tests (§3.10, §3.12 rule 4; the serving unit halves of AC-132/AC-136/AC-137)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{self, fixture_epub, TempDir};
    use std::fs;

    // --- parse_request: the URL forms and the namespace rule ---

    #[test]
    fn parse_request_accepts_all_url_forms() {
        // The custom scheme: the authority carries the doc-id.
        assert_eq!(
            parse_request("12", "/OEBPS/text/ch01.xhtml"),
            Some((12, "OEBPS/text/ch01.xhtml".to_string()))
        );
        // The Windows origin form: the first path segment is the doc-id.
        assert_eq!(
            parse_request(
                "pdfkeeper-epub.localhost",
                "/12/OEBPS/text/ch01.xhtml"
            ),
            Some((12, "OEBPS/text/ch01.xhtml".to_string()))
        );
        // The form wry actually hands the handler on Windows after its
        // scheme rewrite (measured in the running app, DR-305): authority
        // `localhost`, doc-id still in the first path segment.
        assert_eq!(
            parse_request("localhost", "/12/OEBPS/text/ch01.xhtml"),
            Some((12, "OEBPS/text/ch01.xhtml".to_string()))
        );
    }

    #[test]
    fn parse_request_normalizes_and_refuses_hostile_paths() {
        // Percent-encoded paths decode to entry names (§3.2).
        assert_eq!(
            parse_request("3", "/OEBPS/text/ch%2001.xhtml"),
            Some((3, "OEBPS/text/ch 01.xhtml".to_string()))
        );
        // A `..` past the archive root never escapes it (§3.10 namespace rule).
        assert_eq!(parse_request("3", "/../secrets/keys.txt"), None);
        assert_eq!(
            parse_request("3", "/OEBPS/../OEBPS/text/ch01.xhtml"),
            Some((3, "OEBPS/text/ch01.xhtml".to_string())),
            "internal dot-segments normalize inside the namespace"
        );
        // Fragments and queries are not part of an entry key.
        assert_eq!(
            parse_request("3", "/OEBPS/text/ch01.xhtml#sub-a"),
            Some((3, "OEBPS/text/ch01.xhtml".to_string()))
        );
        assert_eq!(parse_request("not-a-number", "/text/ch01.xhtml"), None);
        assert_eq!(parse_request("0", "/text/ch01.xhtml"), None);
    }

    #[test]
    fn base_url_follows_the_platform_branch() {
        let url = base_url(7);
        if cfg!(windows) {
            assert_eq!(url, "http://pdfkeeper-epub.localhost/7/");
        } else {
            assert_eq!(url, "pdfkeeper-epub://7/");
        }
    }

    #[test]
    fn encoding_helpers() {
        assert_eq!(encode_path("OEBPS/text/ch01.xhtml"), "OEBPS/text/ch01.xhtml");
        assert_eq!(encode_path("images/cover one.png"), "images/cover%20one.png");
        // FNV-1a is deterministic and distinguishes content (synthetic keys).
        assert_eq!(fnv1a64(b"abc"), fnv1a64(b"abc"));
        assert_ne!(fnv1a64(b"abc"), fnv1a64(b"abd"));
        assert_eq!(fnv1a64(b"abc").len(), 16);
    }

    #[test]
    fn base64_and_data_uri_decoding() {
        // The 1×1 PNG the hostile fixture carries, decoded.
        let (bytes, media_type) = decode_data_uri(
            "image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
        )
        .expect("the fixture data URI decodes");
        assert_eq!(media_type, "image/png");
        assert!(bytes.starts_with(&[0x89, b'P', b'N', b'G']));
        // Percent-encoded payloads decode too.
        let (bytes, media_type) = decode_data_uri("text/plain,caf%C3%A9").expect("decodes");
        assert_eq!(bytes, "café".as_bytes());
        assert_eq!(media_type, "text/plain");
        assert!(decode_data_uri("text/plain").is_none(), "a comma is required");
        assert!(base64_decode("!!!").is_none());
    }

    // --- the sanitizer (AC-137 unit half) ---

    fn sanitize_text(document: &str) -> (String, Vec<DataEntry>) {
        let (bytes, data) = sanitize(document.as_bytes(), 1, "OEBPS/text/doc.xhtml");
        (
            String::from_utf8(bytes).expect("sanitized output is UTF-8"),
            data,
        )
    }

    #[test]
    fn sanitizer_removes_executable_content() {
        let document = r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>t</title></head><body><p onclick="run()">text</p><script>document.body.appendChild(document.createElement("p"));</script><iframe src="other.xhtml"></iframe><object data="o.bin"></object><embed src="e.bin"/><!-- kept --><svg xmlns="http://www.w3.org/2000/svg"><script>runs()</script><rect width="5" height="5"/></svg><a href="javascript:alert(1)">link</a></body></html>"#;
        let (output, _) = sanitize_text(document);
        assert!(!output.contains("<script"), "no script element survives");
        assert!(!output.contains("appendChild"), "script bodies are removed");
        assert!(!output.contains("<iframe"), "no iframe survives");
        assert!(!output.contains("<object"), "no object survives");
        assert!(!output.contains("<embed"), "no embed survives");
        assert!(!output.contains("onclick"), "event-handler attributes are removed");
        assert!(!output.contains("javascript:"), "javascript: hrefs are removed");
        assert!(output.contains("text"), "the prose survives");
        assert!(output.contains("<rect"), "the SVG shape survives");
        assert!(output.contains("<!-- kept -->"), "comments pass through");
    }

    #[test]
    fn sanitizer_rewrites_package_references_to_protocol_urls() {
        let document = r##"<html xmlns="http://www.w3.org/1999/xhtml"><head><link rel="stylesheet" href="../styles/style.css"/></head><body><img src="images/cover.png" alt="c"/><img src="images\\sub\\x.png"/><p style="background: url('bg.png')">x</p><style>body { background: url("bg2.png"); }</style><a href="ch02.xhtml#sub-a">next</a><a href="#fn1">footnote</a><a href="https://example.com/x">external</a><svg xmlns="http://www.w3.org/2000/svg"><image xlink:href="diagram.svg"/></svg></body></html>"##;
        let (output, data) = sanitize_text(document);
        let base = base_url(1);
        assert!(output.contains(&format!("href=\"{base}OEBPS/styles/style.css\"")), "relative links resolve from the document's own location: {output}");
        assert!(output.contains(&format!("src=\"{base}OEBPS/text/images/cover.png\"")));
        assert!(output.contains(&format!("src=\"{base}OEBPS/text/images/sub/x.png\"")), "backslash separators normalize (the §3.2 rule)");
        // The token keeps its delimiter style; the writer entity-encodes the
        // quotes inside the attribute value (`&apos;`) — valid XML, decoded
        // before CSS sees it.
        assert!(output.contains(&format!("url(&apos;{base}OEBPS/text/bg.png&apos;)")), "style-attribute url() tokens rewrite: {output}");
        assert!(output.contains(&format!("url(\"{base}OEBPS/text/bg2.png\")")), "<style> text url() tokens rewrite");
        // Entry targets keep their fragments for the UI's link routing (§3.13).
        assert!(output.contains(&format!("href=\"{base}OEBPS/text/ch02.xhtml#sub-a\"")));
        assert!(output.contains("href=\"#fn1\""), "fragment-only anchors pass through");
        assert!(output.contains("href=\"https://example.com/x\""), "external URLs pass through");
        assert!(output.contains(&format!("xlink:href=\"{base}OEBPS/text/diagram.svg\"")));
        assert!(output.contains("loading=\"lazy\""), "images get the lazy attribute (§3.12)");
        assert!(data.is_empty(), "no data: URIs here");
    }

    #[test]
    fn sanitizer_rewrites_data_uris_to_synthetic_entries() {
        let payload = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
        let document = format!(
            r#"<html xmlns="http://www.w3.org/1999/xhtml"><body><img src="data:image/png;base64,{payload}" alt="d"/></body></html>"#
        );
        let (output, data) = sanitize_text(&document);
        assert_eq!(data.len(), 1);
        assert_eq!(data[0].content_type, "image/png");
        assert!(data[0].bytes.starts_with(&[0x89, b'P', b'N', b'G']));
        // The synthetic path carries the source chapter with literal
        // separators; the hash is the last segment (DR-301 item 5).
        assert!(data[0].entry.starts_with("data/OEBPS/text/doc.xhtml/"));
        assert!(data[0].entry.ends_with(".png"));
        let url = format!("{}{}", base_url(1), encode_path(&data[0].entry));
        assert!(output.contains(&format!("src=\"{url}\"")), "the data: URI is rewritten to the protocol: {output}");
        assert!(!output.contains("data:image/png"), "no data: URI survives in the document");
    }

    #[test]
    fn sanitizer_is_best_effort_on_unparseable_documents() {
        let broken = "<html><body><p>unclosed";
        let (bytes, data) = sanitize(broken.as_bytes(), 1, "OEBPS/text/doc.xhtml");
        assert_eq!(
            bytes,
            broken.as_bytes(),
            "an unparseable chapter is served as found (DR-302 item 4)"
        );
        assert!(data.is_empty());
    }

    // --- serve(): the pipeline over real fixtures ---

    struct SrvFixture {
        temp: TempDir,
        conn: Connection,
        srv: EpubSrv,
        doc_id: i64,
    }

    fn serving_fixture(name: &str) -> SrvFixture {
        let temp = TempDir::new("epubsrv");
        let conn = crate::db::open_in_memory().expect("in-memory db");
        let target = temp.path().join(name);
        fs::copy(fixture_epub(name), &target).expect("fixture copy");
        conn.execute(
            "INSERT INTO documents (path, filename, title, title_source, size, mtime, added_at, missing)
             VALUES (?1, ?2, ?3, 'auto', 1, 0, 0, 0)",
            rusqlite::params![target.to_string_lossy(), name, name.trim_end_matches(".epub")],
        )
        .expect("document row");
        let doc_id = conn.last_insert_rowid();
        SrvFixture {
            temp,
            conn,
            srv: EpubSrv::new(),
            doc_id,
        }
    }

    fn served(fixture: &SrvFixture, request: &str) -> Result<Served, AppError> {
        let (_, entry) = parse_request("1", request).expect("request parses");
        serve(&fixture.conn, &fixture.srv, fixture.doc_id, &entry)
    }

    /// Pull the synthetic entry path out of a sanitized body (the rewritten
    /// `src` the reader would request).
    fn synthetic_entry(body: &str, base: &str) -> String {
        let marker = format!("{base}data/");
        let start = body.find(&marker).expect("a rewritten data: src");
        let tail = &body[start + marker.len()..];
        let entry = tail.split('"').next().expect("the quoted src value");
        format!("data/{entry}")
    }

    #[test]
    fn serve_returns_the_spine_chapter_sanitized_as_text_html() {
        let fixture = serving_fixture("epub-hostile.epub");
        let response = served(&fixture, "/OEBPS/text/ch01.xhtml").expect("chapter serves");
        assert_eq!(response.content_type, "text/html");
        let body = String::from_utf8_lossy(&response.bytes).into_owned();
        // AC-137 corpus, served: none of the executable content survives.
        assert!(body.contains("Hostile Chapter"), "the prose is served");
        assert!(!body.contains("<script"), "no script reaches the reader");
        assert!(!body.contains("onclick"));
        assert!(!body.contains("javascript:"));
        assert!(!body.contains("<iframe"));
        // The stylesheet link rewrote to the protocol (the fixture's own CSS).
        assert!(body.contains(&format!(
            "href=\"{}OEBPS/text/hostile.css\"",
            base_url(fixture.doc_id)
        )));
        // The data: image rewrote to a synthetic entry (AC-137's allowed set),
        // and the synthetic entry itself serves with the decoded PNG bytes.
        let base = base_url(fixture.doc_id);
        let entry = synthetic_entry(&body, &base);
        let image = serve(&fixture.conn, &fixture.srv, fixture.doc_id, &entry)
            .expect("the synthetic entry serves");
        assert_eq!(image.content_type, "image/png");
        assert!(image.bytes.starts_with(&[0x89, b'P', b'N', b'G']));
    }

    #[test]
    fn synthetic_entries_re_derive_after_eviction() {
        // DR-301 item 5: the source chapter rides in the synthetic path, so
        // a cold request re-derives the same bytes.
        let fixture = serving_fixture("epub-hostile.epub");
        let first = served(&fixture, "/OEBPS/text/ch01.xhtml").expect("chapter serves");
        let body = String::from_utf8_lossy(&first.bytes).into_owned();
        let entry = synthetic_entry(&body, &base_url(fixture.doc_id));
        // A fresh LRU: the entry is not cached anywhere.
        let cold = EpubSrv::new();
        let derived = serve(&fixture.conn, &cold, fixture.doc_id, &entry).expect("re-derivation");
        assert_eq!(derived.content_type, "image/png");
        let warm = serve(&fixture.conn, &fixture.srv, fixture.doc_id, &entry)
            .expect("served warm too");
        assert_eq!(derived.bytes, warm.bytes);
    }

    #[test]
    fn serve_answers_assets_with_declared_types_and_raw_bytes() {
        let fixture = serving_fixture("epub-hostile.epub");
        let css = served(&fixture, "/OEBPS/text/hostile.css").expect("css serves");
        assert_eq!(css.content_type, "text/css");
        assert!(
            String::from_utf8_lossy(&css.bytes).contains("remote.invalid"),
            "external url() tokens pass through unchanged (DR-307; the CSP is the guard)"
        );
        // The declared cover type wins over any extension guess (DR-301 item 4),
        // and the bytes are the full entry content.
        let meta = serving_fixture("epub-meta.epub");
        let cover = served(&meta, "/OEBPS/images/cover.png").expect("cover serves");
        assert_eq!(cover.content_type, "image/png");
        let mut archive =
            ZipArchive::new(File::open(fixture_epub("epub-meta.epub")).expect("fixture"))
                .expect("zip");
        let declared_size = archive
            .by_name("OEBPS/images/cover.png")
            .expect("entry")
            .size();
        assert_eq!(cover.bytes.len() as u64, declared_size);
    }

    #[test]
    fn serve_rewrites_css_url_tokens_to_protocol_urls() {
        // DR-307: the reading fixture's stylesheet references its embedded
        // font relatively; the served CSS carries the protocol URL resolved
        // from the CSS entry's own directory (OEBPS/styles/../fonts/…).
        let fixture = serving_fixture("epub-reading.epub");
        let css = served(&fixture, "/OEBPS/styles/reading.css").expect("css serves");
        assert_eq!(css.content_type, "text/css");
        let body = String::from_utf8_lossy(&css.bytes).into_owned();
        assert!(
            body.contains(&format!("{}OEBPS/fonts/reading.ttf", base_url(fixture.doc_id))),
            "the relative token is rewritten: {body}"
        );
        assert!(
            !body.contains("../fonts/reading.ttf"),
            "no relative token is left: {body}"
        );
    }

    #[test]
    fn serve_sanitizes_svg_entries() {
        let fixture = serving_fixture("epub-svg-cover.epub");
        let svg = served(&fixture, "/OEBPS/images/cover.svg").expect("svg serves");
        assert_eq!(svg.content_type, "image/svg+xml");
        let body = String::from_utf8_lossy(&svg.bytes).into_owned();
        assert!(body.contains("<rect"), "the shape survives");
        assert!(!body.contains("<script"), "scripts inside SVG are removed (AC-137)");
    }

    #[test]
    fn typed_errors_before_and_around_the_file() {
        let fixture = serving_fixture("epub-hostile.epub");
        // A request for a document that is not in the library.
        let missing = serve(&fixture.conn, &fixture.srv, 9999, "OEBPS/text/ch01.xhtml");
        assert!(matches!(missing, Err(AppError::DocumentNotFound { id: 9999 })));
        // A PDF document asked for protocol content: the typed rejection,
        // before any file read.
        let pdf_path = fixture.temp.path().join("info-none.pdf");
        fs::copy(testutil::fixture("info-none.pdf"), &pdf_path).expect("pdf copy");
        fixture
            .conn
            .execute(
                "INSERT INTO documents (path, filename, title, title_source, size, mtime, added_at, missing)
                 VALUES (?1, ?2, ?3, 'auto', 1, 0, 0, 0)",
                rusqlite::params![pdf_path.to_string_lossy(), "info-none.pdf", "info-none"],
            )
            .expect("pdf row");
        let pdf_id = fixture.conn.last_insert_rowid();
        let rejected = serve(&fixture.conn, &fixture.srv, pdf_id, "OEBPS/text/ch01.xhtml");
        assert!(matches!(rejected, Err(AppError::UnsupportedType { .. })));
        // A row whose file is gone: the typed MissingFile.
        fs::remove_file(fixture.temp.path().join("epub-hostile.epub")).expect("remove");
        let gone = served(&fixture, "/OEBPS/text/ch01.xhtml");
        assert!(matches!(gone, Err(AppError::MissingFile { .. })));
    }

    #[test]
    fn absent_entries_and_unreadable_packages_are_typed_errors() {
        let fixture = serving_fixture("epub-hostile.epub");
        let absent = served(&fixture, "/OEBPS/text/nope.xhtml");
        assert!(
            matches!(absent, Err(AppError::EpubUnopenable { .. })),
            "an entry outside the central directory is the typed unreadable (DR-301 item 9 → 404)"
        );
        let malformed = serving_fixture("epub-malformed.epub");
        let unreadable = served(&malformed, "/OEBPS/text/ch01.xhtml");
        assert!(matches!(unreadable, Err(AppError::EpubUnopenable { .. })));
    }

    #[test]
    fn the_lru_caches_output_and_invalidates_on_mtime() {
        let fixture = serving_fixture("epub-hostile.epub");
        let first = served(&fixture, "/OEBPS/text/ch01.xhtml").expect("serves");
        let again = served(&fixture, "/OEBPS/text/ch01.xhtml").expect("serves again");
        assert_eq!(first.bytes, again.bytes);
        assert_eq!(first.etag, again.etag, "a hit answers with the same ETag");
        // Bump the file's mtime: the generation changes, and the ETag —
        // mtime + size (DR-301 item 6) — changes with it.
        let path = fixture.temp.path().join("epub-hostile.epub");
        let current = fs::metadata(&path).expect("metadata").modified().expect("mtime");
        let file = fs::OpenOptions::new().write(true).open(&path).expect("open");
        file.set_modified(current + std::time::Duration::from_secs(10))
            .expect("set mtime");
        drop(file);
        let after = served(&fixture, "/OEBPS/text/ch01.xhtml").expect("serves after");
        assert_ne!(after.etag, first.etag, "an mtime change invalidates the generation");
    }

    #[test]
    fn the_lru_holds_its_caps() {
        // DR-301 item 6: at most 128 entries and 32 MiB.
        let srv = EpubSrv::new();
        for index in 0..140 {
            srv.put(
                CacheKey {
                    doc_id: 1,
                    entry: format!("OEBPS/text/ch{index:03}.xhtml"),
                    mtime: 1,
                },
                Served {
                    content_type: "text/html".to_string(),
                    etag: String::new(),
                    bytes: vec![b'x'; 1000],
                },
            );
        }
        assert_eq!(srv.cached_len(), MAX_TRACKED_ENTRIES);
        // The 32 MiB cap: 40 entries of 1 MiB each keep at most 32.
        let srv = EpubSrv::new();
        for index in 0..40 {
            srv.put(
                CacheKey {
                    doc_id: 1,
                    entry: format!("big/{index}"),
                    mtime: 1,
                },
                Served {
                    content_type: "application/octet-stream".to_string(),
                    etag: String::new(),
                    bytes: vec![0u8; 1024 * 1024],
                },
            );
        }
        assert!(srv.cached_len() <= 32, "32 MiB cap: {}/40 kept", srv.cached_len());
    }

    // --- manifest (the command's service half) ---

    #[test]
    fn manifest_composes_the_reading_facts_with_the_base_url() {
        let fixture = serving_fixture("epub-meta.epub");
        let manifest = manifest(
            fixture.doc_id,
            &fixture.temp.path().join("epub-meta.epub").to_string_lossy(),
        )
        .expect("manifest");
        assert_eq!(manifest.spine.len(), 3);
        assert_eq!(manifest.toc.len(), 4);
        assert_eq!(manifest.base_url, base_url(fixture.doc_id));
        assert_eq!(manifest.language.as_deref(), Some("en"));
        assert_eq!(manifest.direction, None);
    }
}
