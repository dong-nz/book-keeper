//! EPUB package reading for library import (§3.2, FR-22). The two-phase
//! discipline (§3.12 rule 1) is binding here: `extract()` reads only the
//! import-needed facts — `mimetype` → `META-INF/container.xml` → the OPF the
//! rootfile names — and never opens a chapter entry; spine and ToC facts
//! arrive with the reading stage (§3.9 consequence 8).

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;

use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};
use zip::ZipArchive;

use crate::error::AppError;

/// The per-entry decompressed-size guard for the entries `extract()` reads
/// (§3.2; the value and the central-directory check are DR-298's decision):
/// the declared uncompressed size is checked before any read, so a zip bomb
/// is refused at zero read cost. A breach follows M17 like any unreadable
/// package. 64 MiB bounds every known real-world OPF with headroom.
pub(crate) const MAX_ENTRY_BYTES: u64 = 64 * 1024 * 1024;

/// The declared cover as a reference (§3.2): the package entry path plus its
/// declared media type — never decoded bytes. The cover worker (§3.3) reads
/// the bytes; the §3.10 protocol serves the entry to the reader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverRef {
    pub entry: String,
    pub media_type: String,
}

/// The import facts of one EPUB package (§3.2): the metadata the library
/// stores and the cover reference the cover queue consumes.
#[derive(Debug, Default)]
pub struct EpubMeta {
    pub title: Option<String>,
    pub author: Option<String>,
    pub cover: Option<CoverRef>,
}

fn unreadable(path: &str) -> AppError {
    AppError::EpubUnopenable {
        path: path.to_string(),
    }
}

/// Read the import facts of one EPUB package. Every unreadable package —
/// not a zip, missing or wrong `mimetype`, missing container or rootfile,
/// missing or unparseable OPF, size-guard breach — yields the typed
/// `EpubUnopenable` (DR-298). Per M17 the import pipeline treats it as the
/// filename-fallback path; the typed error surfaces only when the app is
/// asked to read the package.
pub fn extract(path: &str) -> Result<EpubMeta, AppError> {
    let Ok(file) = File::open(path) else {
        return Err(unreadable(path));
    };
    let Ok(mut zip) = ZipArchive::new(file) else {
        return Err(unreadable(path));
    };

    // §3.2 read order. The `mimetype` entry must exist and carry the EPUB
    // media type; its STORED-first-position rule is deliberately not
    // enforced (DR-298: real-world writers break it).
    let mimetype = read_entry(&mut zip, path, "mimetype")?;
    if !String::from_utf8_lossy(&mimetype).starts_with("application/epub+zip") {
        return Err(unreadable(path));
    }

    let Ok(container) = String::from_utf8(read_entry(&mut zip, path, "META-INF/container.xml")?)
    else {
        return Err(unreadable(path));
    };
    let Some(opf_path) = find_rootfile(&container) else {
        return Err(unreadable(path));
    };
    let Ok(opf) = String::from_utf8(read_entry(&mut zip, path, &opf_path)?) else {
        return Err(unreadable(path));
    };

    read_opf(&opf, &opf_dir(&opf_path), &mut zip, path)
}

/// §3.3: the declared cover of an EPUB as raw pixels for the cover pipeline:
/// the `CoverRef` resolved through the same two-phase read the import uses
/// (container.xml → OPF only, never a chapter), the entry read, and decoded
/// when its declared media type is a raster one. Every failure — no declared
/// cover, a non-raster declaration (SVG is the glyph fallback this release),
/// an entry over the size guard, an undecodable entry, an unreadable
/// package — is `None`; the caller applies the released glyph fallback.
pub fn cover_image(path: &str) -> Option<image::DynamicImage> {
    let cover = extract(path).ok()?.cover?;
    // §3.3 names the two declared raster types. The `image` crate in this
    // build decodes both: png from our own declaration, jpeg through
    // pdfium-render's feature unification (the Cargo.toml DR-53 note).
    if !matches!(
        cover.media_type.to_ascii_lowercase().as_str(),
        "image/png" | "image/jpeg"
    ) {
        return None;
    }
    let file = File::open(path).ok()?;
    let mut zip = ZipArchive::new(file).ok()?;
    let mut entry = zip.by_name(&cover.entry).ok()?;
    if entry.size() > MAX_ENTRY_BYTES {
        return None;
    }
    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes).ok()?;
    image::load_from_memory(&bytes).ok()
}

// ---------------------------------------------------------------------------
// Reading facts (§3.9 consequence 8, §3.12 rule 1): the reading-stage package
// read — spine order with per-entry facts, the ToC (nav → NCX → spine-derived
// per consequence 1), and the declared language/direction. Called only at
// first open, never by import; the import facts stay in `extract()`.

/// One spine item (§3.9 consequence 8): the resolved package entry, its
/// declared media type, and the uncompressed entry size the position model
/// uses (§3.13) — read from the zip central directory, never from parsing.
/// An entry missing from the archive stays in the spine with size 0
/// (DR-301 item 2): the position model tolerates 0, its protocol request 404s.
#[derive(Debug, Clone, serde::Serialize)]
pub struct EpubSpineItem {
    pub index: usize,
    pub entry: String,
    pub media_type: String,
    pub size: u64,
}

/// The reading facts of one package. The ToC reuses the released
/// `OutlineEntry` shape verbatim (DR-301 item 2): `page` carries the 1-based
/// spine index, `depth` the nesting in the package's own ToC tree.
#[derive(Debug, Clone, serde::Serialize)]
pub struct EpubPackage {
    pub spine: Vec<EpubSpineItem>,
    pub toc: Vec<crate::pdfmeta::OutlineEntry>,
    pub language: Option<String>,
    pub direction: Option<String>,
}

/// Read the reading facts of one EPUB package. Same typed errors as
/// `extract()` (DR-298): every unreadable package yields `EpubUnopenable`.
pub fn read_package(path: &str) -> Result<EpubPackage, AppError> {
    let Ok(file) = File::open(path) else {
        return Err(unreadable(path));
    };
    let Ok(mut zip) = ZipArchive::new(file) else {
        return Err(unreadable(path));
    };
    let mimetype = read_entry(&mut zip, path, "mimetype")?;
    if !String::from_utf8_lossy(&mimetype).starts_with("application/epub+zip") {
        return Err(unreadable(path));
    }
    let Ok(container) = String::from_utf8(read_entry(&mut zip, path, "META-INF/container.xml")?)
    else {
        return Err(unreadable(path));
    };
    let Some(opf_path) = find_rootfile(&container) else {
        return Err(unreadable(path));
    };
    let Ok(opf) = String::from_utf8(read_entry(&mut zip, path, &opf_path)?) else {
        return Err(unreadable(path));
    };
    let Some(facts) = read_opf_facts(&opf) else {
        return Err(unreadable(path));
    };
    let opf_directory = opf_dir(&opf_path);

    // Spine order (§3.9 consequence 1: "the spine is reading order"). A spine
    // idref naming no manifest item, or an href that is a failed lookup, is
    // skipped — it is not part of a readable order.
    let mut spine: Vec<EpubSpineItem> = Vec::new();
    let mut index_by_entry: HashMap<String, usize> = HashMap::new();
    for idref in &facts.spine_ids {
        let Some(item) = facts.items.get(idref.as_str()) else {
            continue;
        };
        let Some(entry) = resolve_href(&opf_directory, &item.href) else {
            continue;
        };
        let size = zip.by_name(&entry).map(|file| file.size()).unwrap_or(0);
        let index = spine.len() + 1;
        index_by_entry.entry(entry.clone()).or_insert(index);
        spine.push(EpubSpineItem {
            index,
            entry,
            media_type: item.media_type.clone(),
            size,
        });
    }

    // ToC source order (consequence 1): EPUB3 nav → EPUB2 NCX →
    // spine-derived. A nav/NCX document that cannot be read falls through to
    // the next source like any unreadable fact.
    let toc = facts
        .items
        .values()
        .find(|item| item.properties.iter().any(|property| property == "nav"))
        .and_then(|item| {
            let document = read_toc_document(&mut zip, path, &opf_directory, item)?;
            Some(nav_toc(&document.text, &entry_dir(&document.entry), &index_by_entry))
        })
        .or_else(|| {
            let toc_item = facts.toc_id.as_deref().and_then(|id| facts.items.get(id))?;
            let document = read_toc_document(&mut zip, path, &opf_directory, toc_item)?;
            Some(ncx_toc(
                &document.text,
                &entry_dir(&document.entry),
                &index_by_entry,
            ))
        })
        .unwrap_or_else(|| spine_derived_toc(&spine));

    Ok(EpubPackage {
        spine,
        toc,
        language: facts.language.or(facts.package_language),
        direction: facts.direction.or(facts.package_direction),
    })
}

/// The manifest's declared media types keyed by resolved entry path
/// (DR-302 item 2): what `epubsrv` consults before its extension map. Same
/// two-phase read as `read_package` — container and OPF only, never a
/// content entry; `None` for an unreadable package or a manifest that
/// declares nothing, after which the serving path falls back to extensions.
pub fn media_types(path: &str) -> Option<HashMap<String, String>> {
    let Ok(file) = File::open(path) else {
        return None;
    };
    let Ok(mut zip) = ZipArchive::new(file) else {
        return None;
    };
    let Ok(mimetype) = read_entry(&mut zip, path, "mimetype") else {
        return None;
    };
    if !String::from_utf8_lossy(&mimetype).starts_with("application/epub+zip") {
        return None;
    }
    let Ok(container) = String::from_utf8(read_entry(&mut zip, path, "META-INF/container.xml").ok()?)
    else {
        return None;
    };
    let opf_path = find_rootfile(&container)?;
    let Ok(opf) = String::from_utf8(read_entry(&mut zip, path, &opf_path).ok()?) else {
        return None;
    };
    let facts = read_opf_facts(&opf)?;
    let opf_directory = opf_dir(&opf_path);
    let mut map = HashMap::new();
    for item in facts.items.values() {
        if let Some(entry) = resolve_href(&opf_directory, &item.href) {
            map.insert(entry, item.media_type.clone());
        }
    }
    (!map.is_empty()).then_some(map)
}

struct OpfItem {
    href: String,
    media_type: String,
    properties: Vec<String>,
}

#[derive(Default)]
struct OpfFacts {
    items: HashMap<String, OpfItem>,
    spine_ids: Vec<String>,
    toc_id: Option<String>,
    language: Option<String>,
    direction: Option<String>,
    package_language: Option<String>,
    package_direction: Option<String>,
}

/// The OPF's reading-stage facts: manifest items, spine idrefs in order,
/// the spine's `toc` attribute, `dc:language`, `<meta name="dir">`, and the
/// package root's `xml:lang`/`dir` as fallbacks (DR-301 item 8). `None` for
/// an unparseable OPF.
fn read_opf_facts(opf: &str) -> Option<OpfFacts> {
    let mut reader = Reader::from_str(opf);
    reader.config_mut().trim_text(true);
    let mut facts = OpfFacts::default();
    let mut in_metadata = false;
    let mut capturing_language = false;
    let mut capture = String::new();
    loop {
        let event = match reader.read_event() {
            Ok(event) => event,
            Err(_) => return None,
        };
        match event {
            Event::Start(e) => match e.local_name().as_ref() {
                "package" => {
                    facts.package_language = attr_value(&e, "xml:lang");
                    facts.package_direction = attr_value(&e, "dir");
                }
                "metadata" => in_metadata = true,
                "language" if in_metadata && facts.language.is_none() => {
                    capturing_language = true;
                    capture.clear();
                }
                "meta" if in_metadata && attr_value(&e, "name").as_deref() == Some("dir") => {
                    facts.direction = attr_value(&e, "content");
                }
                "item" => {
                    if let (Some(id), Some(href), Some(media_type)) = (
                        attr_value(&e, "id"),
                        attr_value(&e, "href"),
                        attr_value(&e, "media-type"),
                    ) {
                        let properties = attr_value(&e, "properties")
                            .map(|value| value.split_whitespace().map(str::to_string).collect())
                            .unwrap_or_default();
                        facts
                            .items
                            .insert(id, OpfItem { href, media_type, properties });
                    }
                }
                "spine" => {
                    facts.toc_id = attr_value(&e, "toc");
                }
                "itemref" => {
                    if let Some(idref) = attr_value(&e, "idref") {
                        facts.spine_ids.push(idref);
                    }
                }
                _ => {}
            },
            Event::Empty(e) => match e.local_name().as_ref() {
                "item" => {
                    if let (Some(id), Some(href), Some(media_type)) = (
                        attr_value(&e, "id"),
                        attr_value(&e, "href"),
                        attr_value(&e, "media-type"),
                    ) {
                        let properties = attr_value(&e, "properties")
                            .map(|value| value.split_whitespace().map(str::to_string).collect())
                            .unwrap_or_default();
                        facts
                            .items
                            .insert(id, OpfItem { href, media_type, properties });
                    }
                }
                // `itemref` is written self-closed by every writer we read.
                "itemref" => {
                    if let Some(idref) = attr_value(&e, "idref") {
                        facts.spine_ids.push(idref);
                    }
                }
                "meta" if in_metadata && attr_value(&e, "name").as_deref() == Some("dir") => {
                    facts.direction = attr_value(&e, "content");
                }
                _ => {}
            },
            Event::Text(e) if capturing_language => {
                capture.push_str(&e.into_inner());
            }
            Event::End(e) => match e.local_name().as_ref() {
                "language" => {
                    if !capture.trim().is_empty() && facts.language.is_none() {
                        facts.language = Some(capture.trim().to_string());
                    }
                    capturing_language = false;
                    capture.clear();
                }
                "metadata" => in_metadata = false,
                "spine" => break, // the reading facts are complete
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    Some(facts)
}

/// Read a ToC document (nav or NCX) named by a manifest item. A failed
/// lookup or an unreadable document is `None` — the caller falls through to
/// the next ToC source (consequence 1).
struct TocDocument {
    entry: String,
    text: String,
}

fn read_toc_document(
    zip: &mut ZipArchive<File>,
    path: &str,
    opf_directory: &str,
    item: &OpfItem,
) -> Option<TocDocument> {
    let entry = resolve_href(opf_directory, &item.href)?;
    let bytes = read_entry(zip, path, &entry).ok()?;
    let text = String::from_utf8(bytes).ok()?;
    Some(TocDocument { entry, text })
}

fn entry_dir(entry_path: &str) -> String {
    match entry_path.rfind('/') {
        Some(index) => entry_path[..=index].to_string(),
        None => String::new(),
    }
}

/// EPUB3 nav parsing (consequence 1): anchors inside the `nav` element, in
/// document order, with depth from `<ol>` nesting — in the released
/// `OutlineEntry` convention, depth 1 is the top level (the FR-18 shape).
/// An entry whose href resolves to no spine item is skipped — it is not
/// reading order (DR-301 item 7); fragment-only hrefs resolve to no chapter
/// and are skipped too.
fn nav_toc(xml: &str, doc_dir: &str, spine_index: &HashMap<String, usize>) -> Vec<crate::pdfmeta::OutlineEntry> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut out = Vec::new();
    let mut in_nav = false;
    let mut ol_depth = 0usize;
    let mut anchor: Option<(String, String)> = None;
    loop {
        let event = match reader.read_event() {
            Ok(event) => event,
            Err(_) => return out, // a broken nav document yields what was parsed — the caller's fall-through handles empties
        };
        match event {
            Event::Start(e) => match e.local_name().as_ref() {
                "nav" => in_nav = true,
                "ol" if in_nav => ol_depth += 1,
                "a" if in_nav => {
                    anchor = Some((attr_value(&e, "href").unwrap_or_default(), String::new()));
                }
                _ => {}
            },
            Event::Text(e) if anchor.is_some() => {
                if let Some((_, label)) = anchor.as_mut() {
                    label.push_str(&e.into_inner());
                }
            }
            Event::End(e) => match e.local_name().as_ref() {
                "a" if anchor.is_some() => {
                    let (href, label) = anchor.take().expect("checked");
                    if let Some(index) = resolve_href(doc_dir, &href)
                        .as_ref()
                        .and_then(|entry| spine_index.get(entry))
                    {
                        out.push(crate::pdfmeta::OutlineEntry {
                            title: label.trim().to_string(),
                            page: *index as u32,
                            depth: ol_depth.max(1) as u32,
                        });
                    }
                }
                "ol" if in_nav => ol_depth = ol_depth.saturating_sub(1),
                "nav" => in_nav = false,
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    out
}

/// EPUB2 NCX parsing (consequence 1): navPoints in the navMap, depth from
/// navPoint nesting; the label from `navLabel/text`, the target from
/// `content@src`. Same skip rule as the nav parser.
fn ncx_toc(xml: &str, doc_dir: &str, spine_index: &HashMap<String, usize>) -> Vec<crate::pdfmeta::OutlineEntry> {
    struct Frame {
        in_label: bool,
        label: String,
        src: Option<String>,
        slot: usize,
    }
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    // Tree order (parents before children, the FR-18 flattening): a navPoint
    // reserves its output slot when it opens, filled when it closes; entries
    // that resolve to no spine item leave the slot empty and are dropped.
    let mut out: Vec<Option<crate::pdfmeta::OutlineEntry>> = Vec::new();
    let mut stack: Vec<Frame> = Vec::new();
    loop {
        let event = match reader.read_event() {
            Ok(event) => event,
            Err(_) => return out.into_iter().flatten().collect(),
        };
        match event {
            Event::Start(e) => match e.local_name().as_ref() {
                "navPoint" => {
                    out.push(None);
                    stack.push(Frame {
                        in_label: false,
                        label: String::new(),
                        src: None,
                        slot: out.len() - 1,
                    });
                }
                "navLabel" if !stack.is_empty() => stack.last_mut().expect("checked").in_label = true,
                "content" if !stack.is_empty() && stack.last().expect("checked").src.is_none() => {
                    stack.last_mut().expect("checked").src = attr_value(&e, "src");
                }
                _ => {}
            },
            Event::Empty(e)
                if e.local_name().as_ref() == "content"
                    && !stack.is_empty()
                    && stack.last().expect("checked").src.is_none() =>
            {
                stack.last_mut().expect("checked").src = attr_value(&e, "src");
            }
            Event::Text(e) if !stack.is_empty() && stack.last().expect("checked").in_label => {
                stack.last_mut().expect("checked").label.push_str(&e.into_inner());
            }
            Event::End(e) if e.local_name().as_ref() == "navLabel" && !stack.is_empty() => {
                stack.last_mut().expect("checked").in_label = false;
            }
            Event::End(e) if e.local_name().as_ref() == "navPoint" && !stack.is_empty() => {
                let frame = stack.pop().expect("checked");
                let depth = stack.len() as u32 + 1; // released convention: 1 is the top level
                if let Some(index) = frame
                    .src
                    .as_deref()
                    .and_then(|src| resolve_href(doc_dir, src))
                    .as_ref()
                    .and_then(|entry| spine_index.get(entry))
                {
                    out[frame.slot] = Some(crate::pdfmeta::OutlineEntry {
                        title: frame.label.trim().to_string(),
                        page: *index as u32,
                        depth,
                    });
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    out.into_iter().flatten().collect()
}

/// The spine-derived fallback (consequence 1, DR-301 item 7): one entry per
/// spine item, titled with the entry's file-name stem — the only titles the
/// package supplies without a ToC.
fn spine_derived_toc(spine: &[EpubSpineItem]) -> Vec<crate::pdfmeta::OutlineEntry> {
    spine
        .iter()
        .map(|item| {
            let file_name = item.entry.rsplit('/').next().unwrap_or(&item.entry);
            let stem = file_name.rsplit_once('.').map_or(file_name, |(stem, _)| stem);
            crate::pdfmeta::OutlineEntry {
                title: stem.to_string(),
                page: item.index as u32,
                depth: 1,
            }
        })
        .collect()
}

fn read_entry(zip: &mut ZipArchive<File>, path: &str, name: &str) -> Result<Vec<u8>, AppError> {
    let Ok(mut file) = zip.by_name(name) else {
        return Err(unreadable(path));
    };
    if file.size() > MAX_ENTRY_BYTES {
        return Err(unreadable(path));
    }
    let mut bytes = Vec::new();
    if file.read_to_end(&mut bytes).is_err() {
        return Err(unreadable(path));
    }
    Ok(bytes)
}

/// The OPF path named by the container's first `rootfile`, normalized to the
/// archive namespace (backslash separators are a real-world deviation §3.2
/// names). A container without a rootfile is unreadable (M17).
fn find_rootfile(container: &str) -> Option<String> {
    let mut reader = Reader::from_str(container);
    reader.config_mut().trim_text(true);
    loop {
        let Ok(event) = reader.read_event() else {
            return None;
        };
        match event {
            Event::Start(e) | Event::Empty(e) if e.local_name().as_ref() == "rootfile" => {
                return attr_value(&e, "full-path").map(|path| path.replace('\\', "/"));
            }
            Event::Eof => return None,
            _ => {}
        }
    }
}

fn attr_value(element: &BytesStart, key: &str) -> Option<String> {
    for attribute in element.attributes().flatten() {
        if attribute.key.as_ref() == key {
            // §3.2: attribute values are entity-decoded (`&amp;` and friends).
            // The packages at hand declare XML 1.0; the version argument names
            // that contract explicitly (quick-xml 0.42).
            return attribute
                .normalized_value(XmlVersion::Explicit1_0)
                .ok()
                .map(|value| value.into_owned());
        }
    }
    None
}

#[derive(Clone, Copy)]
enum Field {
    Title,
    Author,
}

/// OPF scan (§3.2): first title, first creator, `<meta name="cover">` →
/// manifest item → cover reference. Matching is by local name inside the
/// metadata — the `dc:`, `dcterms:`, and unprefixed forms are all accepted
/// (DR-298); values are trimmed, empty means absent. The scan stops as soon
/// as the import facts are complete: after `</metadata>` when no cover is
/// declared, or when the cover item has been seen — the spine and the rest
/// of the manifest are never needed here (§3.12 rule 2's early stop).
fn read_opf(
    opf: &str,
    opf_dir: &str,
    zip: &mut ZipArchive<File>,
    path: &str,
) -> Result<EpubMeta, AppError> {
    struct Scan<'z> {
        meta: EpubMeta,
        cover_id: Option<String>,
        in_metadata: bool,
        capturing: Option<Field>,
        capture: String,
        opf_dir: String,
        zip: &'z mut ZipArchive<File>,
    }

    impl Scan<'_> {
        /// Handles `Event::Start`/`Event::Empty`; returns true when the cover
        /// has been decided (resolved or declared-but-broken) — the signal to
        /// stop scanning.
        fn open(&mut self, e: &BytesStart) -> bool {
            match e.local_name().as_ref() {
                "metadata" => self.in_metadata = true,
                name @ ("title" | "creator") if self.in_metadata => {
                    self.capturing = Some(if name == "title" {
                        Field::Title
                    } else {
                        Field::Author
                    });
                    self.capture.clear();
                }
                "meta"
                    if self.in_metadata
                        && self.cover_id.is_none()
                        && attr_value(e, "name").as_deref() == Some("cover") =>
                {
                    self.cover_id = attr_value(e, "content");
                }
                "item"
                    if self.meta.cover.is_none()
                        && self
                            .cover_id
                            .as_deref()
                            .is_some_and(|cover_id| attr_value(e, "id").as_deref() == Some(cover_id)) =>
                {
                    return self.resolve_cover(e);
                }
                _ => {}
            }
            false
        }

        /// §3.2: the declared cover becomes a `CoverRef` only when its entry
        /// exists in the central directory; a failed lookup yields no
        /// reference — the glyph fallback (M17). The scan stops either way.
        fn resolve_cover(&mut self, e: &BytesStart) -> bool {
            let (Some(href), Some(media_type)) = (attr_value(e, "href"), attr_value(e, "media-type"))
            else {
                return true;
            };
            let resolved = resolve_href(&self.opf_dir, &href)
                .filter(|entry| self.zip.by_name(entry).is_ok());
            self.meta.cover = resolved.map(|entry| CoverRef {
                entry,
                media_type,
            });
            true
        }

        fn close_capture(&mut self) {
            let value = self.capture.trim();
            match self.capturing.take() {
                Some(Field::Title) if !value.is_empty() && self.meta.title.is_none() => {
                    self.meta.title = Some(value.to_string());
                }
                Some(Field::Author) if !value.is_empty() && self.meta.author.is_none() => {
                    self.meta.author = Some(value.to_string());
                }
                _ => {}
            }
            self.capture.clear();
        }
    }

    let mut reader = Reader::from_str(opf);
    reader.config_mut().trim_text(true);
    let mut scan = Scan {
        meta: EpubMeta::default(),
        cover_id: None,
        in_metadata: false,
        capturing: None,
        capture: String::new(),
        opf_dir: opf_dir.to_string(),
        zip,
    };

    loop {
        let event = match reader.read_event() {
            Ok(event) => event,
            Err(_) => return Err(unreadable(path)), // an unparseable OPF is unreadable (§3.2, M17)
        };
        let cover_decided = match event {
            Event::Start(e) => scan.open(&e),
            Event::Empty(e) => {
                let decided = scan.open(&e);
                // A self-closed title/creator carries no text; close the
                // capture so it cannot bleed into a following element.
                scan.close_capture();
                decided
            }
            Event::Text(e) => {
                if scan.capturing.is_some() {
                    scan.capture.push_str(&e.into_inner());
                }
                false
            }
            Event::End(e) => {
                match e.local_name().as_ref() {
                    "title" | "creator" => scan.close_capture(),
                    "metadata" => {
                        scan.in_metadata = false;
                        if scan.cover_id.is_none() {
                            break; // no cover declared: the import facts are complete
                        }
                    }
                    _ => {}
                }
                false
            }
            Event::Eof => break,
            _ => false,
        };
        if cover_decided {
            break;
        }
    }

    Ok(scan.meta)
}

pub(crate) fn opf_dir(opf_path: &str) -> String {
    match opf_path.rfind('/') {
        Some(index) => opf_path[..=index].to_string(),
        None => String::new(),
    }
}

/// §3.2 href resolution: fragment/query stripped → percent-decoded →
/// backslashes normalized → resolved against the OPF directory → dot-segments
/// normalized. A path that would escape the archive root, and a decoded name
/// that is not valid UTF-8, are failed lookups (None → M17).
pub(crate) fn resolve_href(opf_dir: &str, href: &str) -> Option<String> {
    let href = href.split(['#', '?']).next()?;
    let decoded = percent_decode(href)?;
    let joined = match decoded.strip_prefix('/') {
        Some(absolute) => absolute.to_string(),
        None => format!("{}{}", opf_dir, decoded.replace('\\', "/")),
    };
    let mut segments: Vec<&str> = Vec::new();
    for segment in joined.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            other => segments.push(other),
        }
    }
    if segments.is_empty() {
        return None;
    }
    Some(segments.join("/"))
}

pub(crate) fn percent_decode(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            match (
                bytes.get(index + 1).and_then(|b| hex_digit(*b)),
                bytes.get(index + 2).and_then(|b| hex_digit(*b)),
            ) {
                (Some(high), Some(low)) => {
                    out.push(high * 16 + low);
                    index += 3;
                    continue;
                }
                // A stray percent sign stays literal (real-world hrefs contain them).
                _ => out.push(b'%'),
            }
        } else {
            out.push(bytes[index]);
        }
        index += 1;
    }
    String::from_utf8(out).ok()
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// tests (§3.2; AC-139/AC-140/AC-141 unit halves — the pipeline halves arrive
// with the extraction dispatch)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{fixture_epub, TempDir};
    use std::fs::File;
    use std::io::Write;

    fn extract_fixture(name: &str) -> Result<EpubMeta, AppError> {
        extract(fixture_epub(name).to_string_lossy().as_ref())
    }

    // AC-139 (unit half): the package's declared metadata is what extract reads.
    #[test]
    fn extracts_title_author_and_cover_from_the_opf() {
        let meta = extract_fixture("epub-meta.epub").unwrap();
        assert_eq!(meta.title.as_deref(), Some("Real EPUB Title"));
        assert_eq!(meta.author.as_deref(), Some("Real EPUB Author"));
        assert_eq!(
            meta.cover,
            Some(CoverRef {
                entry: "OEBPS/images/cover.png".into(),
                media_type: "image/png".into(),
            })
        );
    }

    // The STORED-only archive path (§3.8): the same facts from an archive with
    // no deflated entries.
    #[test]
    fn extracts_metadata_from_a_stored_only_archive() {
        let meta = extract_fixture("epub-stored.epub").unwrap();
        assert_eq!(meta.title.as_deref(), Some("Stored Book"));
        assert_eq!(meta.author.as_deref(), Some("Stored Author"));
        assert_eq!(meta.cover, None);
    }

    // AC-140 (unit half): no metadata is not an error — the pipeline's
    // filename fallback is what the empty fields mean.
    #[test]
    fn no_metadata_yields_no_fields() {
        let meta = extract_fixture("epub-no-meta.epub").unwrap();
        assert_eq!(meta.title, None);
        assert_eq!(meta.author, None);
        assert_eq!(meta.cover, None);
    }

    #[test]
    fn missing_cover_declaration_yields_no_cover() {
        let meta = extract_fixture("epub-no-cover.epub").unwrap();
        assert_eq!(meta.title.as_deref(), Some("No Cover Book"));
        assert_eq!(meta.cover, None);
    }

    // §3.2: a declared cover is a reference, not bytes — the SVG decision
    // (glyph fallback) belongs to the cover worker (§3.3), not to extract.
    #[test]
    fn declared_svg_cover_is_a_reference_not_bytes() {
        let meta = extract_fixture("epub-svg-cover.epub").unwrap();
        assert_eq!(
            meta.cover,
            Some(CoverRef {
                entry: "OEBPS/images/cover.svg".into(),
                media_type: "image/svg+xml".into(),
            })
        );
    }

    // AC-141 (unit half): a package that cannot be read is the typed error;
    // the import pipeline applies the M17 fallback around it.
    #[test]
    fn malformed_package_is_a_typed_error() {
        let result = extract_fixture("epub-malformed.epub");
        assert!(
            matches!(result, Err(AppError::EpubUnopenable { .. })),
            "expected the typed error, got {result:?}"
        );
    }

    // The `mimetype` rule of §3.2's read order (DR-298's strictness level).
    #[test]
    fn mimetype_rules_reject_a_zip_that_is_not_an_epub() {
        let temp = TempDir::new("epub-mimetype");
        let opf = opf("<dc:title>T</dc:title>", "");
        let without_mimetype = write_zip(
            &temp,
            "no-mimetype.zip",
            &[
                ("META-INF/container.xml", container().as_bytes()),
                ("OEBPS/package.opf", opf.as_bytes()),
            ],
        );
        assert!(matches!(
            extract(&without_mimetype),
            Err(AppError::EpubUnopenable { .. })
        ));

        let wrong_mimetype = write_zip(
            &temp,
            "wrong-mimetype.zip",
            &[
                ("mimetype", b"text/plain"),
                ("META-INF/container.xml", container().as_bytes()),
                ("OEBPS/package.opf", opf.as_bytes()),
            ],
        );
        assert!(matches!(
            extract(&wrong_mimetype),
            Err(AppError::EpubUnopenable { .. })
        ));
    }

    #[test]
    fn missing_container_or_rootfile_is_a_typed_error() {
        let temp = TempDir::new("epub-container");
        let no_container = write_zip(&temp, "no-container.zip", &[("mimetype", MIMETYPE)]);
        assert!(matches!(
            extract(&no_container),
            Err(AppError::EpubUnopenable { .. })
        ));

        let no_rootfile = write_zip(
            &temp,
            "no-rootfile.zip",
            &[
                ("mimetype", MIMETYPE),
                (
                    "META-INF/container.xml",
                    br#"<?xml version="1.0"?><container xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles/></container>"#
                        .as_slice(),
                ),
            ],
        );
        assert!(matches!(
            extract(&no_rootfile),
            Err(AppError::EpubUnopenable { .. })
        ));
    }

    // The §3.2 href resolution rule, stated as a pure function: percent
    // decoding, backslash separators, dot-segment normalization, fragment
    // stripping, and the archive-namespace boundary (a `../` can never
    // escape the package).
    #[test]
    fn href_resolution_rules() {
        assert_eq!(
            resolve_href("OEBPS/", "images/cover%20one.png").as_deref(),
            Some("OEBPS/images/cover one.png")
        );
        assert_eq!(
            resolve_href("OEBPS/", "images\\sub\\cover.png").as_deref(),
            Some("OEBPS/images/sub/cover.png")
        );
        assert_eq!(
            resolve_href("OEBPS/text/", "../images/x.png").as_deref(),
            Some("OEBPS/images/x.png")
        );
        assert_eq!(
            resolve_href("OEBPS/text/", "ch%301.xhtml").as_deref(),
            Some("OEBPS/text/ch01.xhtml")
        );
        assert_eq!(
            resolve_href("OEBPS/", "nav.xhtml#toc").as_deref(),
            Some("OEBPS/nav.xhtml")
        );
        assert_eq!(
            resolve_href("OEBPS/", "100%25.png").as_deref(),
            Some("OEBPS/100%.png")
        );
        assert_eq!(resolve_href("OEBPS/", "/root.png").as_deref(), Some("root.png"));
        // Climbing to the archive root is legal; popping past it is the failed lookup.
        assert_eq!(resolve_href("OEBPS/", "../escape.png").as_deref(), Some("escape.png"));
        assert_eq!(resolve_href("OEBPS/text/", "a/../../../../up.png"), None);
        assert_eq!(resolve_href("OEBPS/", "bad%FF%FF.png"), None);
    }

    // The percent-decoded href resolved through the real central-directory
    // lookup: the entry name contains the decoded space.
    #[test]
    fn cover_href_resolves_through_the_archive_namespace() {
        let temp = TempDir::new("epub-href");
        let path = write_zip(
            &temp,
            "cover-href.zip",
            &[
                ("mimetype", MIMETYPE),
                ("META-INF/container.xml", container().as_bytes()),
                (
                    "OEBPS/package.opf",
                    opf(
                        r#"<dc:title>Href Book</dc:title><meta name="cover" content="c"/>"#,
                        r#"<item id="c" href="images/cover%20one.png" media-type="image/png"/>"#,
                    )
                    .as_bytes(),
                ),
                ("OEBPS/images/cover one.png", b"not a real png, the reference is what matters"),
            ],
        );
        let meta = extract(&path).unwrap();
        assert_eq!(
            meta.cover,
            Some(CoverRef {
                entry: "OEBPS/images/cover one.png".into(),
                media_type: "image/png".into(),
            })
        );
    }

    // §3.2: "a failed lookup follows M17" — a declared cover whose entry is
    // absent yields no reference, not an error.
    #[test]
    fn cover_pointing_at_a_missing_entry_yields_no_cover() {
        let temp = TempDir::new("epub-missing-cover");
        let path = write_zip(
            &temp,
            "missing-cover.zip",
            &[
                ("mimetype", MIMETYPE),
                ("META-INF/container.xml", container().as_bytes()),
                (
                    "OEBPS/package.opf",
                    opf(
                        r#"<dc:title>Gone Cover</dc:title><meta name="cover" content="c"/>"#,
                        r#"<item id="c" href="images/gone.png" media-type="image/png"/>"#,
                    )
                    .as_bytes(),
                ),
            ],
        );
        let meta = extract(&path).unwrap();
        assert_eq!(meta.title.as_deref(), Some("Gone Cover"));
        assert_eq!(meta.cover, None);
    }

    // The zip-bomb guard (DR-298): the declared size is refused before any
    // read — a 65 MiB OPF entry is the typed error.
    #[test]
    fn oversized_metadata_entry_is_refused() {
        let temp = TempDir::new("epub-zipbomb");
        let bomb = vec![b'0'; 65 * 1024 * 1024];
        let path = write_zip(
            &temp,
            "bomb.zip",
            &[
                ("mimetype", MIMETYPE),
                ("META-INF/container.xml", container().as_bytes()),
                ("OEBPS/package.opf", &bomb),
            ],
        );
        assert!(matches!(
            extract(&path),
            Err(AppError::EpubUnopenable { .. })
        ));
    }

    // The two-phase rule (§3.12 rule 1) proven behaviorally: a 70 MiB chapter
    // entry never slows or fails an import read — extract never opens it.
    #[test]
    fn extract_never_opens_oversized_chapter_entries() {
        let temp = TempDir::new("epub-bigchapter");
        let chapter = vec![b'0'; 70 * 1024 * 1024];
        let path = write_zip(
            &temp,
            "big-chapter.zip",
            &[
                ("mimetype", MIMETYPE),
                ("META-INF/container.xml", container().as_bytes()),
                (
                    "OEBPS/package.opf",
                    opf(r#"<dc:title>Big Chapter</dc:title>"#, "").as_bytes(),
                ),
                ("OEBPS/text/ch01.xhtml", &chapter),
            ],
        );
        let meta = extract(&path).unwrap();
        assert_eq!(meta.title.as_deref(), Some("Big Chapter"));
    }

    #[test]
    fn whitespace_only_title_is_no_title() {
        let temp = TempDir::new("epub-emptytitle");
        let path = write_zip(
            &temp,
            "empty-title.zip",
            &[
                ("mimetype", MIMETYPE),
                ("META-INF/container.xml", container().as_bytes()),
                (
                    "OEBPS/package.opf",
                    opf("<dc:title>   </dc:title>", "").as_bytes(),
                ),
            ],
        );
        let meta = extract(&path).unwrap();
        assert_eq!(meta.title, None);
    }

    // --- read_package (§3.9 consequence 8; the reading-facts unit halves) ---

    fn read_fixture(name: &str) -> Result<EpubPackage, AppError> {
        read_package(fixture_epub(name).to_string_lossy().as_ref())
    }

    // The spine is reading order (§3.9 consequence 1): manifest order through
    // the idref list, resolved entries, declared media types, 1-based indices.
    #[test]
    fn read_package_returns_the_spine_in_reading_order() {
        let package = read_fixture("epub-meta.epub").unwrap();
        let entries: Vec<&str> = package.spine.iter().map(|item| item.entry.as_str()).collect();
        assert_eq!(
            entries,
            vec![
                "OEBPS/text/ch01.xhtml",
                "OEBPS/text/ch02.xhtml",
                "OEBPS/text/ch03.xhtml",
            ]
        );
        let indices: Vec<usize> = package.spine.iter().map(|item| item.index).collect();
        assert_eq!(indices, vec![1, 2, 3]);
        assert!(package
            .spine
            .iter()
            .all(|item| item.media_type == "application/xhtml+xml"));
        // The language rides from the package root (DR-301 item 8's fallback);
        // this package declares no direction.
        assert_eq!(package.language.as_deref(), Some("en"));
        assert_eq!(package.direction, None);
    }

    // Spine sizes are the central directory's uncompressed bytes (the §3.13
    // position model, DR-301 item 2) — not the compressed stream — and an
    // entry missing from the archive stays in the spine at size 0.
    #[test]
    fn spine_sizes_are_uncompressed_and_missing_entries_read_as_zero() {
        let temp = TempDir::new("epub-sizes");
        let body = vec![b'a'; 4000]; // deflatable: the stored stream is far smaller
        let path = write_zip(
            &temp,
            "sizes.zip",
            &[
                ("mimetype", MIMETYPE),
                ("META-INF/container.xml", container().as_bytes()),
                (
                    "OEBPS/package.opf",
                    full_opf(
                        "",
                        r#"<item id="ch1" href="text/ch01.xhtml" media-type="application/xhtml+xml"/>
                           <item id="ch2" href="text/gone.xhtml" media-type="application/xhtml+xml"/>"#,
                        r#"<itemref idref="ch1"/><itemref idref="ch2"/>"#,
                        "",
                    )
                    .as_bytes(),
                ),
                ("OEBPS/text/ch01.xhtml", &body),
            ],
        );
        let package = read_package(&path).unwrap();
        assert_eq!(package.spine.len(), 2);
        assert_eq!(package.spine[0].size, 4000);
        assert_eq!(package.spine[1].entry, "OEBPS/text/gone.xhtml");
        assert_eq!(
            package.spine[1].size, 0,
            "a missing entry stays in the spine at size 0 (DR-301)"
        );
    }

    // ToC source order, first branch (consequence 1): the EPUB3 nav document,
    // with depth from `ol` nesting in the released 1-is-top convention, and
    // `page` carrying the 1-based spine index (DR-301 item 2).
    #[test]
    fn read_package_builds_the_toc_from_the_nav_document() {
        let package = read_fixture("epub-meta.epub").unwrap();
        assert_eq!(
            package.toc,
            vec![
                outline("Chapter 1", 1, 1),
                outline("Chapter 2", 2, 1),
                outline("Sub Two A", 2, 2),
                outline("Chapter 3", 3, 1),
            ]
        );
    }

    // ToC source order, middle branch: the EPUB2 NCX navMap (the fixture no
    // package carried before DR-301 item 10). Nested navPoints get depth 2;
    // the entry whose src resolves to no spine item is skipped.
    #[test]
    fn read_package_falls_back_to_the_ncx_navmap() {
        let package = read_fixture("epub-ncx.epub").unwrap();
        assert_eq!(
            package.toc,
            vec![
                outline("Chapter One", 1, 1),
                outline("Chapter Two", 2, 1),
                outline("Sub Two A", 2, 2),
            ],
            "the navMap entry resolving outside the spine is not reading order"
        );
    }

    // ToC source order, last branch: no nav, no NCX — the spine-derived
    // fallback, titled with the entry file-name stems (DR-301 item 7). The
    // 500-chapter stress fixture exercises it at scale.
    #[test]
    fn read_package_derives_a_toc_from_the_spine_when_no_toc_exists() {
        let package = read_fixture("epub-stress-500ch.epub").unwrap();
        assert_eq!(package.toc.len(), 500);
        assert_eq!(package.toc[0], outline("ch01", 1, 1));
        assert_eq!(package.toc[499], outline("ch500", 500, 1));
        assert!(package.toc.iter().all(|entry| entry.depth == 1));
    }

    // DR-301 item 8: declared language and direction — `dc:language` and
    // `<meta name="dir">` take precedence over the package root attributes.
    #[test]
    fn read_package_reads_declared_language_and_direction() {
        let temp = TempDir::new("epub-langdir");
        let path = write_zip(
            &temp,
            "langdir.zip",
            &[
                ("mimetype", MIMETYPE),
                ("META-INF/container.xml", container().as_bytes()),
                (
                    "OEBPS/package.opf",
                    full_opf(
                        r#"<dc:language>en-US</dc:language><meta name="dir" content="rtl"/>"#,
                        r#"<item id="ch1" href="text/ch01.xhtml" media-type="application/xhtml+xml"/>"#,
                        r#"<itemref idref="ch1"/>"#,
                        r#" xml:lang="fr" dir="ltr""#,
                    )
                    .as_bytes(),
                ),
                ("OEBPS/text/ch01.xhtml", b"<html><body/></html>"),
            ],
        );
        let package = read_package(&path).unwrap();
        assert_eq!(package.language.as_deref(), Some("en-US"));
        assert_eq!(package.direction.as_deref(), Some("rtl"));
    }

    // Same typed errors as the import read (DR-298): the reader command path
    // maps them to its payload (AC-133/AC-141's open half).
    #[test]
    fn read_package_rejects_a_malformed_package_like_import() {
        let result = read_fixture("epub-malformed.epub");
        assert!(
            matches!(result, Err(AppError::EpubUnopenable { .. })),
            "expected the typed error, got {result:?}"
        );
    }

    // --- synthetic-package helpers ------------------------------------------

    fn full_opf(metadata: &str, manifest: &str, spine: &str, package_attrs: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><package xmlns="http://www.idref.org/2005/OPF" xmlns:dc="http://purl.org/dc/elements/1.1/" version="3.0"{package_attrs}><metadata>{metadata}</metadata><manifest>{manifest}</manifest><spine>{spine}</spine></package>"#
        )
    }

    fn outline(title: &str, page: u32, depth: u32) -> crate::pdfmeta::OutlineEntry {
        crate::pdfmeta::OutlineEntry {
            title: title.to_string(),
            page,
            depth,
        }
    }

    const MIMETYPE: &[u8] = b"application/epub+zip";

    fn container() -> String {
        r#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/package.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#
            .to_string()
    }

    fn opf(metadata: &str, manifest: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><package xmlns="http://www.idref.org/2005/OPF" xmlns:dc="http://purl.org/dc/elements/1.1/" version="3.0"><metadata>{metadata}</metadata><manifest>{manifest}</manifest><spine><itemref idref="ch1"/></spine></package>"#
        )
    }

    fn write_zip(temp: &TempDir, name: &str, entries: &[(&str, &[u8])]) -> String {
        let path = temp.path().join(name);
        let file = File::create(&path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (entry, bytes) in entries {
            writer.start_file(*entry, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap();
        path.to_string_lossy().into_owned()
    }
}
