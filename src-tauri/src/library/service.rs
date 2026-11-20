//! Service operations. Every fallible path returns `AppError` (RULES R3.2);
//! multi-table writes run in transactions (RULES R2.4); every mutation path
//! emits `status_updated` (FR-15 invariant).

use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, params, params_from_iter, types::Value as SqlValue};

use crate::applog;
use crate::covers::{self, CoverJob, CoverQueue};
use crate::db;
use crate::epubmeta;
use crate::epubsrv;
use crate::error::{AppError, is_unique_violation};
use crate::pdf;
use crate::Db;

use super::query::{self, Query};
use super::{
    CountsDto, CoverCandidateDto, DocumentDto, EventSink, ImportResult, JobProgress, RescanResult,
    SETTINGS_KEYS, SearchPage, SettingsDto, TagDto, filename_of, is_document_path, normalize_path,
    title_from_filename,
};

/// Chunk bound of the import/rescan pipeline (DR-233 as revised by DR-279):
/// the user's Phase-B report of 2026-11-10 showed that a 25-file step makes
/// the bar jump for any import of 25 files or fewer ("it is gone for one
/// shot"). 5 files keeps the bar visibly incremental for ordinary imports
/// while each write transaction stays a short lock hold; the 10 000-file
/// budget test (NFR-02) bounds the per-transaction overhead.
const CHUNK: usize = 5;

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn mtime_secs(metadata: &fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn emit_status(conn: &Connection, sink: &dyn EventSink) -> Result<(), AppError> {
    let counts = get_counts(conn)?;
    sink.status(&counts);
    Ok(())
}

pub fn get_counts(conn: &Connection) -> Result<CountsDto, AppError> {
    // One pass, no cached counters (FR-15); the Reading count rides the same
    // pass (FR-19 / UI-43, DR-265).
    Ok(conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(favorite), 0), COALESCE(SUM(missing), 0), COALESCE(SUM(reading), 0) FROM documents",
        [],
        |row| {
            Ok(CountsDto {
                total: row.get(0)?,
                favorites: row.get(1)?,
                missing: row.get(2)?,
                reading: row.get(3)?,
            })
        },
    )?)
}

// ---------------------------------------------------------------- documents

pub fn search_document(
    conn: &Connection,
    covers_dir: &Path,
    query: &Query,
    offset: i64,
    limit: i64,
) -> Result<SearchPage, AppError> {
    query::search(conn, query, offset, limit, covers_dir)
}

pub fn get_document(
    conn: &Connection,
    covers_dir: &Path,
    id: i64,
) -> Result<DocumentDto, AppError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {} FROM documents d WHERE d.id = ?1",
        query::ROW_COLUMNS
    ))?;
    let mut rows = stmt.query_map(params![id], |row| query::map_row(row, covers_dir))?;
    match rows.next() {
        Some(Ok(doc)) => Ok(doc),
        Some(Err(e)) => Err(e.into()),
        None => Err(AppError::DocumentNotFound { id }),
    }
}

/// Resolve a document's file (FR-08): a missing document short-circuits to
/// `MissingFile` and no open is attempted. The filename travels with the
/// path because the reader window's title is the document filename (DR-140);
/// the stored password travels too (FR-20, DR-266) — every render path
/// passes it to the loader and only the row holds it. An empty column is
/// `None`: no password is known.
pub struct OpenTarget {
    pub path: String,
    pub filename: String,
    pub password: Option<String>,
}

pub fn open_target(conn: &Connection, id: i64) -> Result<OpenTarget, AppError> {
    let result = conn.query_row(
        "SELECT path, filename, missing, pdf_password FROM documents WHERE id = ?1",
        params![id],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)? != 0,
                row.get::<_, String>(3)?,
            ))
        },
    );
    match result {
        Ok((path, _, true, _)) => Err(AppError::MissingFile {
            path,
            document_id: id,
        }),
        Ok((path, filename, false, password)) => Ok(OpenTarget {
            path,
            filename,
            password: (!password.is_empty()).then_some(password),
        }),
        Err(rusqlite::Error::QueryReturnedNoRows) => Err(AppError::DocumentNotFound { id }),
        Err(e) => Err(e.into()),
    }
}

pub fn set_favorite(
    conn: &Connection,
    sink: &dyn EventSink,
    covers_dir: &Path,
    id: i64,
    value: bool,
) -> Result<DocumentDto, AppError> {
    let changed = conn.execute(
        "UPDATE documents SET favorite = ?2 WHERE id = ?1",
        params![id, value as i64],
    )?;
    if changed == 0 {
        return Err(AppError::DocumentNotFound { id });
    }
    emit_status(conn, sink)?;
    get_document(conn, covers_dir, id)
}

/// FR-19 (AC-121/AC-102): the ONLY writer of the `reading` flag — opening a
/// document never touches it (the manual-only rule, OQ-48 closed as B).
/// Single ids and batches go through the same path; every updated row is
/// returned so the store updates the selection in place (DR-266), and the
/// mutation emits the status counts like every other one (FR-15).
pub fn set_reading(
    conn: &Connection,
    sink: &dyn EventSink,
    covers_dir: &Path,
    ids: &[i64],
    value: bool,
) -> Result<Vec<DocumentDto>, AppError> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let changed = conn.execute(
        &format!("UPDATE documents SET reading = ?1 WHERE id IN ({placeholders})"),
        params_from_iter(
            std::iter::once(SqlValue::Integer(value as i64))
                .chain(ids.iter().map(|id| SqlValue::Integer(*id))),
        ),
    )?;
    if changed == 0 {
        let mut stmt = conn.prepare(&format!(
            "SELECT COUNT(*) FROM documents WHERE id IN ({placeholders})"
        ))?;
        let present: i64 = stmt.query_row(params_from_iter(ids.iter()), |row| row.get(0))?;
        if present == 0 {
            return Err(AppError::DocumentNotFound { id: ids[0] });
        }
    }
    emit_status(conn, sink)?;
    let mut docs = Vec::with_capacity(ids.len());
    for id in ids {
        docs.push(get_document(conn, covers_dir, *id)?);
    }
    Ok(docs)
}

/// The editable fields of one document, grouped so the service signature
/// stays small (RULES R7).
pub struct DocumentEdit<'a> {
    pub title: &'a str,
    pub author: &'a str,
    pub notes: &'a str,
    pub favorite: bool,
    pub tag_ids: &'a [i64],
}

pub fn update_document(
    conn: &mut Connection,
    sink: &dyn EventSink,
    covers_dir: &Path,
    id: i64,
    edit: &DocumentEdit<'_>,
) -> Result<DocumentDto, AppError> {
    let title = edit.title.trim();
    if title.is_empty() {
        return Err(AppError::TitleEmpty);
    }
    let exists = conn
        .query_row("SELECT 1 FROM documents WHERE id = ?1", params![id], |_| {
            Ok(())
        })
        .is_ok();
    if !exists {
        return Err(AppError::DocumentNotFound { id });
    }
    validate_tags(conn, edit.tag_ids)?;

    let tx = conn.transaction()?;
    // title_source flips to 'user' only when the title value itself changes;
    // that flag gates relink title re-derivation (metadata-editing.md).
    tx.execute(
        "UPDATE documents SET title = ?2, author = ?3, notes = ?4, favorite = ?5,
         title_source = CASE WHEN ?2 = title THEN title_source ELSE 'user' END
         WHERE id = ?1",
        params![id, title, edit.author, edit.notes, edit.favorite as i64],
    )?;
    replace_document_tags(&tx, id, edit.tag_ids)?;
    tx.commit()?;
    emit_status(conn, sink)?;
    get_document(conn, covers_dir, id)
}

pub fn set_document_tags(
    conn: &mut Connection,
    sink: &dyn EventSink,
    document_id: i64,
    tag_ids: &[i64],
) -> Result<(), AppError> {
    let exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM documents WHERE id = ?1",
        params![document_id],
        |row| row.get(0),
    )?;
    if exists == 0 {
        return Err(AppError::DocumentNotFound { id: document_id });
    }
    validate_tags(conn, tag_ids)?;
    let tx = conn.transaction()?;
    replace_document_tags(&tx, document_id, tag_ids)?;
    tx.commit()?;
    emit_status(conn, sink)
}

fn replace_document_tags(
    tx: &rusqlite::Transaction<'_>,
    document_id: i64,
    tag_ids: &[i64],
) -> Result<(), AppError> {
    tx.execute(
        "DELETE FROM document_tags WHERE document_id = ?1",
        params![document_id],
    )?;
    for tag_id in tag_ids {
        tx.execute(
            "INSERT OR IGNORE INTO document_tags (document_id, tag_id) VALUES (?1, ?2)",
            params![document_id, tag_id],
        )?;
    }
    Ok(())
}

fn validate_tags(conn: &Connection, tag_ids: &[i64]) -> Result<(), AppError> {
    for tag_id in tag_ids {
        let exists: i64 = conn.query_row(
            "SELECT COUNT(*) FROM tags WHERE id = ?1",
            params![tag_id],
            |row| row.get(0),
        )?;
        if exists == 0 {
            return Err(AppError::TagNotFound { id: *tag_id });
        }
    }
    Ok(())
}

// ------------------------------------------------------------------ import

/// Import pipeline (importing-pdfs.md): filter to the supported document
/// types — `.pdf` and `.epub` (§3.4, UI-46), stat, insert in
/// short per-chunk transactions with duplicate skip (FR-11), metadata
/// extraction with filename fallback (FR-05), tag auto-assignment
/// (FR-10), cover jobs enqueued after each commit — the command returns as
/// soon as the rows are committed. The lock discipline is the pipeline's own
/// (DR-233): extraction runs with no connection held, the connection is
/// acquired only for the short transactions, and the progress events make
/// the import visible from start to completion for imports of any size.
pub fn import_paths(
    db: &Db,
    sink: &dyn EventSink,
    covers_queue: &CoverQueue,
    paths: &[String],
    tag_ids: &[i64],
) -> Result<ImportResult, AppError> {
    // FR-10 as modified (DR-82): every active tag receives the import; an
    // empty set assigns nothing. All ids are validated before any write
    // (UI-42 vocabulary per OQ-50 answer B).
    db.with_raw(|conn| validate_tags(&*conn, tag_ids))?;

    // Plan: filesystem only — no connection is held here.
    let mut candidates: Vec<(String, i64, i64)> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for path in paths {
        let normalized = normalize_path(path);
        if !is_document_path(&normalized) {
            skipped.push(normalized);
            continue;
        }
        match fs::metadata(&normalized) {
            Ok(metadata) if metadata.is_file() => {
                if seen.insert(normalized.clone()) {
                    candidates.push((normalized, metadata.len() as i64, mtime_secs(&metadata)));
                } else {
                    skipped.push(normalized);
                }
            }
            // FR-26 (§3.2, import per-file failures): a document path that
            // cannot be statted is a real failure — unlike the designed
            // skips (non-documents, duplicates), it gets a log line. The
            // pipeline behavior is unchanged: still a typed skip, never a
            // failed import.
            Err(io_error) => {
                applog::error("import", &format!("cannot read {normalized}: {io_error}"));
                skipped.push(normalized);
            }
            Ok(_) => skipped.push(normalized),
        }
    }

    let total = candidates.len() as i64;
    // The bar appears before any per-file work (DR-233): an import of any
    // size, a single file included, is visible from its first moment.
    if total > 0 {
        sink.progress(&JobProgress {
            kind: "import".into(),
            processed: 0,
            total,
        });
    }

    let mut imported = 0i64;
    let mut processed = 0i64;
    for chunk in candidates.chunks(CHUNK) {
        // Extraction is the slow stage (M3): it runs with no connection
        // held, so every other command keeps answering while an import does.
        let prepared: Vec<(String, i64, i64, ExtractedFacts)> = chunk
            .iter()
            .map(|(path, size, mtime)| (path.clone(), *size, *mtime, extract_facts(path)))
            .collect();
        let (added, jobs, duplicates) = db.with_raw(|conn| import_chunk(conn, &prepared, tag_ids))?;
        skipped.extend(duplicates);
        for job in jobs {
            covers_queue.enqueue(job);
        }
        imported += added;
        processed += chunk.len() as i64;
        sink.progress(&JobProgress {
            kind: "import".into(),
            processed,
            total,
        });
    }

    db.with_raw(|conn| emit_status(conn, sink))?;
    // FR-26 Info lifecycle (OQ-67 answer A): import runs with counts — the
    // run summary, not per-file detail (the failure lines above carry that).
    applog::info(
        "import",
        &format!("import run finished: {imported} imported, {} skipped", skipped.len()),
    );
    Ok(ImportResult { imported, skipped })
}

/// The facts the import pipeline stores per document (§3.4 extraction
/// dispatch): `.pdf` → `pdfmeta` untouched, `.epub` → the two-phase package
/// read (§3.2, FR-22). An unreadable EPUB is the filename fallback here like
/// any other unreadable file (M17); its typed error surfaces when the app is
/// asked to open the package. The cover reference needs no carrying here:
/// the cover worker dispatches on the path's extension (§3.3).
struct ExtractedFacts {
    title: Option<String>,
    author: Option<String>,
}

fn extract_facts(path: &str) -> ExtractedFacts {
    if Path::new(path)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("epub"))
    {
        return match crate::epubmeta::extract(path) {
            Ok(meta) => ExtractedFacts {
                title: meta.title,
                author: meta.author,
            },
            Err(error) => {
                // FR-26 (import per-file failures): the filename fallback is
                // the released behavior (M17); the failure behind it is now
                // visible in the log.
                applog::error("import", &format!("cannot read package {path}: {error}"));
                ExtractedFacts {
                    title: None,
                    author: None,
                }
            }
        };
    }
    let extracted = crate::pdfmeta::extract(path);
    ExtractedFacts {
        title: extracted.title,
        author: extracted.author,
    }
}

/// One candidate chunk inside one short transaction (DR-233). The insertion
/// semantics are the released ones unchanged: `INSERT OR IGNORE` carries the
/// duplicate skip (FR-11) exactly as the released tests assert it.
fn import_chunk(
    conn: &mut Connection,
    prepared: &[(String, i64, i64, ExtractedFacts)],
    tag_ids: &[i64],
) -> Result<(i64, Vec<CoverJob>, Vec<String>), AppError> {
    let tx = conn.transaction()?;
    let mut imported = 0i64;
    let mut jobs: Vec<CoverJob> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    let added_at = now_secs();
    for (path, size, mtime, extracted) in prepared {
        let filename = filename_of(path);
        let title = extracted
            .title
            .clone()
            .unwrap_or_else(|| title_from_filename(&filename));
        let author = extracted.author.clone().unwrap_or_default();
        let inserted = tx.execute(
            "INSERT OR IGNORE INTO documents (path, filename, title, author, notes, size, mtime, added_at)
             VALUES (?1, ?2, ?3, ?4, '', ?5, ?6, ?7)",
            params![path, filename, title, author, size, mtime, added_at],
        )?;
        if inserted == 0 {
            skipped.push(path.clone());
            continue;
        }
        let document_id = tx.last_insert_rowid();
        for tag_id in tag_ids {
            tx.execute(
                "INSERT OR IGNORE INTO document_tags (document_id, tag_id) VALUES (?1, ?2)",
                params![document_id, tag_id],
            )?;
        }
        imported += 1;
        // A freshly imported document has no stored password (FR-20): a
        // protected file gets the glyph cover until an unlock stores one
        // (AC-106).
        jobs.push(CoverJob {
            doc_id: document_id,
            path: path.clone(),
            size: *size,
            mtime: *mtime,
            password: None,
            // A fresh import has no choice — the automatic cover (FR-31).
            cover_choice: None,
        });
    }
    tx.commit()?;
    Ok((imported, jobs, skipped))
}

/// One-time folder import: recursive walk for both document types
/// (`*.pdf`, `*.epub` — DR-50a extended by §3.4/AC-126) — the walk
/// is filesystem-only and happens before any connection is acquired
/// (DR-233) — then the same pipeline.
pub fn import_folder(
    db: &Db,
    sink: &dyn EventSink,
    covers_queue: &CoverQueue,
    dir: &str,
    tag_ids: &[i64],
) -> Result<ImportResult, AppError> {
    let mut paths: Vec<String> = Vec::new();
    collect_documents(Path::new(dir), &mut paths);
    import_paths(db, sink, covers_queue, &paths, tag_ids)
}

fn collect_documents(dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            collect_documents(&path, out);
        } else if file_type.is_file()
            && let Some(text) = path.to_str()
            && is_document_path(text)
        {
            out.push(text.to_string());
        }
    }
}

// ------------------------------------------------------------------ rescan

/// Startup + manual Rescan only (FR-12, DR-13b). Marks vanished files
/// missing, refreshes size/mtime, enqueues cover jobs for changed files.
/// `quiet` suppresses progress events (the startup run stays silent unless
/// it finds something — the status counts still update). The lock discipline
/// is the import pipeline's (DR-233): the rows are read in one short
/// transaction, the filesystem answers are collected with no connection
/// held, and the updates run in short per-chunk transactions.
pub fn rescan(
    db: &Db,
    sink: &dyn EventSink,
    covers_queue: &CoverQueue,
    quiet: bool,
) -> Result<RescanResult, AppError> {
    struct Entry {
        id: i64,
        path: String,
        size: i64,
        mtime: i64,
        missing: bool,
        password: Option<String>,
        cover_choice: Option<covers::CoverChoice>,
    }
    let entries: Vec<Entry> = db.with_raw(|conn| {
        let mut entries: Vec<Entry> = Vec::new();
        let mut stmt = conn.prepare(
            "SELECT id, path, size, mtime, missing, pdf_password, cover_choice FROM documents ORDER BY id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Entry {
                id: row.get(0)?,
                path: row.get(1)?,
                size: row.get(2)?,
                mtime: row.get(3)?,
                missing: row.get::<_, i64>(4)? != 0,
                password: {
                    let value = row.get::<_, String>(5)?;
                    (!value.is_empty()).then_some(value)
                },
                cover_choice: row
                    .get::<_, Option<String>>(6)?
                    .as_deref()
                    .and_then(covers::parse_choice),
            })
        })?;
        for entry in rows {
            entries.push(entry?);
        }
        Ok(entries)
    })?;

    let total = entries.len() as i64;
    let mut result = RescanResult {
        missing_found: 0,
        restored: 0,
    };
    let mut changed = false;
    let mut processed = 0i64;
    if !quiet && total > 0 {
        sink.progress(&JobProgress {
            kind: "rescan".into(),
            processed: 0,
            total,
        });
    }

    for chunk in entries.chunks(CHUNK) {
        // Filesystem answers first, with no connection held (DR-233).
        let checked: Vec<(&Entry, Option<fs::Metadata>)> = chunk
            .iter()
            .map(|entry| (entry, fs::metadata(&entry.path).ok()))
            .collect();
        let (partial, jobs) = db.with_raw(|conn| {
            let tx = conn.transaction()?;
            let mut missing_found = 0i64;
            let mut restored = 0i64;
            let mut chunk_changed = false;
            let mut jobs: Vec<CoverJob> = Vec::new();
            for (entry, metadata) in &checked {
                match metadata {
                    None => {
                        if !entry.missing {
                            tx.execute(
                                "UPDATE documents SET missing = 1 WHERE id = ?1",
                                params![entry.id],
                            )?;
                            missing_found += 1;
                            chunk_changed = true;
                        }
                    }
                    Some(metadata) => {
                        let size = metadata.len() as i64;
                        let mtime = mtime_secs(metadata);
                        let changed_row = if entry.missing {
                            tx.execute(
                                "UPDATE documents SET missing = 0, size = ?2, mtime = ?3 WHERE id = ?1",
                                params![entry.id, size, mtime],
                            )?;
                            restored += 1;
                            true
                        } else if size != entry.size || mtime != entry.mtime {
                            tx.execute(
                                "UPDATE documents SET size = ?2, mtime = ?3 WHERE id = ?1",
                                params![entry.id, size, mtime],
                            )?;
                            true
                        } else {
                            false
                        };
                        if changed_row {
                            chunk_changed = true;
                            jobs.push(CoverJob {
                                doc_id: entry.id,
                                path: entry.path.clone(),
                                size,
                                mtime,
                                password: entry.password.clone(),
                                // FR-31: a changed file re-renders the chosen
                                // cover under the tagged key, not page 1.
                                cover_choice: entry.cover_choice.clone(),
                            });
                        }
                    }
                }
            }
            tx.commit()?;
            Ok(((missing_found, restored, chunk_changed), jobs))
        })?;
        result.missing_found += partial.0;
        result.restored += partial.1;
        changed |= partial.2;
        for job in jobs {
            covers_queue.enqueue(job);
        }
        processed += chunk.len() as i64;
        if !quiet {
            sink.progress(&JobProgress {
                kind: "rescan".into(),
                processed,
                total,
            });
        }
    }

    if !quiet || changed {
        db.with_raw(|conn| emit_status(conn, sink))?;
    }
    Ok(result)
}

// ------------------------------------------------------------------ delete

/// FR-09 / DR-50b: for the with-file variant, `remove_file` runs BEFORE the
/// row transaction so a failure leaves every row intact and the operation is
/// retriable (a file already gone counts as deleted). Cover-cache cleanup
/// happens after the commit; no document can leave an orphan cover. The
/// reader page cache is session state of the document too: it goes with the
/// row (AC-62, SPEC-v0.0.6 §3.2), beside the cover-cache cleanup.
pub fn delete_documents(
    conn: &mut Connection,
    sink: &dyn EventSink,
    covers_queue: &CoverQueue,
    ids: &[i64],
    delete_files: bool,
    reader_dir: &Path,
) -> Result<(), AppError> {
    if ids.is_empty() {
        return Ok(());
    }
    let mut rows: Vec<(i64, String, i64, i64, Option<String>)> = Vec::new();
    {
        let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let mut stmt = conn.prepare(&format!(
            "SELECT id, path, size, mtime, cover_choice FROM documents WHERE id IN ({placeholders})"
        ))?;
        let found = stmt.query_map(params_from_iter(ids.iter()), |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })?;
        for row in found {
            rows.push(row?);
        }
    }
    if rows.len() != ids.len() {
        let present: HashSet<i64> = rows.iter().map(|(id, ..)| *id).collect();
        let missing_id = ids
            .iter()
            .find(|id| !present.contains(id))
            .copied()
            .unwrap_or(0);
        return Err(AppError::DocumentNotFound { id: missing_id });
    }

    if delete_files {
        for (_, path, ..) in &rows {
            match fs::remove_file(path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(AppError::Io {
                        path: path.clone(),
                        message: error.to_string(),
                    });
                }
            }
        }
    }

    {
        let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let tx = conn.transaction()?;
        tx.execute(
            &format!("DELETE FROM documents WHERE id IN ({placeholders})"),
            params_from_iter(ids.iter()),
        )?;
        // FR-28 (AC-167): the reader position memory goes with the document,
        // in the same transaction.
        crate::positions::cleanup(&tx, ids)?;
        tx.commit()?;
    }

    for (_, path, size, mtime, choice_raw) in &rows {
        // FR-31: a chosen cover goes with the row too — the tagged file and
        // any still-present superseded automatic file (no document leaves an
        // orphan cover).
        let cover_choice = choice_raw.as_deref().and_then(covers::parse_choice);
        let mut cache_files = vec![covers::cache_path(&covers_queue.dir, path, *size, *mtime, None)];
        if let Some(choice) = cover_choice.as_ref() {
            cache_files.push(covers::cache_path(&covers_queue.dir, path, *size, *mtime, Some(choice)));
        }
        for cache_file in &cache_files {
            match fs::remove_file(cache_file) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => {} // a stale cache file is swept at startup; deletion never fails the flow
            }
        }
    }

    for id in ids {
        pdf::remove_document_cache(reader_dir, *id);
    }

    emit_status(conn, sink)
}

// ------------------------------------------------------------------ relink

/// FR-12: point an entry at a new path. Validation errors are typed so the
/// Relink dialog can show them inline (not a bare toast). Auto-titled
/// documents re-derive their title from the new filename; user titles are
/// preserved. Relink is type-matched (AC-128, §3.4): the new path must
/// carry the stored document's own extension.
pub fn relink_document(
    conn: &mut Connection,
    sink: &dyn EventSink,
    covers_queue: &CoverQueue,
    id: i64,
    new_path: &str,
) -> Result<DocumentDto, AppError> {
    let normalized = normalize_path(new_path);
    if !is_document_path(&normalized) {
        return Err(AppError::UnsupportedType { path: normalized });
    }
    let metadata = fs::metadata(&normalized).map_err(|error| AppError::Io {
        path: normalized.clone(),
        message: error.to_string(),
    })?;
    if !metadata.is_file() {
        return Err(AppError::UnsupportedType { path: normalized });
    }

    let stored: Option<(String, String, Option<String>)> = conn
        .query_row(
            "SELECT path, title_source, cover_choice FROM documents WHERE id = ?1",
            params![id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .ok();
    let Some((stored_path, title_source, choice_raw)) = stored else {
        return Err(AppError::DocumentNotFound { id });
    };
    if !same_document_extension(&stored_path, &normalized) {
        return Err(AppError::UnsupportedType { path: normalized });
    }

    let filename = filename_of(&normalized);
    let size = metadata.len() as i64;
    let mtime = mtime_secs(&metadata);
    let tx = conn.transaction()?;
    let update = if title_source == "auto" {
        tx.execute(
            "UPDATE documents SET path = ?2, filename = ?3, title = ?4, size = ?5, mtime = ?6, missing = 0 WHERE id = ?1",
            params![id, normalized, filename, title_from_filename(&filename), size, mtime],
        )
    } else {
        tx.execute(
            "UPDATE documents SET path = ?2, filename = ?3, size = ?4, mtime = ?5, missing = 0 WHERE id = ?1",
            params![id, normalized, filename, size, mtime],
        )
    };
    match update {
        Ok(_) => {}
        Err(error) if is_unique_violation(&error) => {
            return Err(AppError::PathTaken { path: normalized });
        }
        Err(error) => return Err(error.into()),
    }
    tx.commit()?;

    // FR-31 (§3.8): relink keeps the id and the choice — the job re-renders
    // under the choice-tagged key for the new path.
    covers_queue.enqueue(CoverJob {
        doc_id: id,
        path: normalized,
        size,
        mtime,
        password: stored_password(conn, id)?,
        cover_choice: choice_raw.as_deref().and_then(covers::parse_choice),
    });
    emit_status(conn, sink)?;
    get_document(conn, &covers_queue.dir, id)
}

/// AC-128 (§3.4): a relink keeps the document's type — the stored path's
/// extension and the candidate's extension must agree, case-insensitively.
fn same_document_extension(stored: &str, candidate: &str) -> bool {
    match (
        Path::new(stored).extension(),
        Path::new(candidate).extension(),
    ) {
        (Some(stored_ext), Some(candidate_ext)) => stored_ext.eq_ignore_ascii_case(candidate_ext),
        _ => false,
    }
}

// ------------------------------------------------------------- password (FR-20)

/// The row's stored password; an empty column is `None` (no password known).
fn stored_password(conn: &Connection, id: i64) -> Result<Option<String>, AppError> {
    let value = conn.query_row(
        "SELECT pdf_password FROM documents WHERE id = ?1",
        params![id],
        |row| row.get::<_, String>(0),
    )?;
    Ok((!value.is_empty()).then_some(value))
}

/// FR-20 / DR-266: the ONLY writer of `pdf_password`. The reader calls this
/// on every unlock attempt — session values included — and clears the column
/// at window close when *Remember this password* was left unchecked (the
/// clear is the same call with an empty value, R5.3: a failed clear never
/// fails the close). An empty value is *Remove password*. Every non-empty
/// store re-enqueues the cover with the new password so a real render can
/// replace the glyph (AC-106); clearing leaves the existing cover in place —
/// the cache key never mentions the password.
pub fn set_password(
    conn: &Connection,
    sink: &dyn EventSink,
    covers_queue: &CoverQueue,
    id: i64,
    password: &str,
) -> Result<(), AppError> {
    let changed = conn.execute(
        "UPDATE documents SET pdf_password = ?2 WHERE id = ?1",
        params![id, password],
    )?;
    if changed == 0 {
        return Err(AppError::DocumentNotFound { id });
    }
    if !password.is_empty() {
        let (path, size, mtime, choice_raw) = conn.query_row(
            "SELECT path, size, mtime, cover_choice FROM documents WHERE id = ?1",
            params![id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            },
        )?;
        covers_queue.enqueue(CoverJob {
            doc_id: id,
            path,
            size,
            mtime,
            password: Some(password.to_string()),
            // FR-31: the stored choice rides the re-enqueue — unlocking a
            // protected document renders the chosen cover, not page 1.
            cover_choice: choice_raw.as_deref().and_then(covers::parse_choice),
        });
    }
    emit_status(conn, sink)
}

/// The row data a cover job needs, or the typed `DocumentNotFound`.
fn cover_job_source(
    conn: &Connection,
    id: i64,
) -> Result<(String, i64, i64, Option<String>), AppError> {
    let result = conn.query_row(
        "SELECT path, size, mtime, pdf_password FROM documents WHERE id = ?1",
        params![id],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
            ))
        },
    );
    match result {
        Ok((path, size, mtime, password)) => Ok((
            path,
            size,
            mtime,
            (!password.is_empty()).then_some(password),
        )),
        Err(rusqlite::Error::QueryReturnedNoRows) => Err(AppError::DocumentNotFound { id }),
        Err(error) => Err(error.into()),
    }
}

/// FR-31 (AC-179/AC-180): set the manual cover choice. The value is validated
/// against the row's file type (`covers::validate_choice` owns the grammar and
/// the kind/file-type match — a malformed or mismatched choice never reaches
/// storage), stored verbatim (it passed the grammar), and the cover job is
/// enqueued with the new choice: the command returns while the render is
/// background work (AC-180). The response is the updated row DTO — its
/// `cover_path` is the choice-tagged key the moment the render lands.
pub fn set_cover_choice(
    conn: &Connection,
    covers_queue: &CoverQueue,
    id: i64,
    choice: &str,
) -> Result<DocumentDto, AppError> {
    let (path, size, mtime, password) = cover_job_source(conn, id)?;
    let parsed = covers::validate_choice(choice, &path)?;
    conn.execute(
        "UPDATE documents SET cover_choice = ?2 WHERE id = ?1",
        params![id, choice],
    )?;
    covers_queue.enqueue(CoverJob {
        doc_id: id,
        path,
        size,
        mtime,
        password,
        cover_choice: Some(parsed),
    });
    get_document(conn, &covers_queue.dir, id)
}

/// FR-31 (AC-177): clear the choice — NULL plus re-enqueue of the automatic
/// cover, the released path restored under the untagged key.
pub fn clear_cover_choice(
    conn: &Connection,
    covers_queue: &CoverQueue,
    id: i64,
) -> Result<DocumentDto, AppError> {
    let (path, size, mtime, password) = cover_job_source(conn, id)?;
    conn.execute(
        "UPDATE documents SET cover_choice = NULL WHERE id = ?1",
        params![id],
    )?;
    covers_queue.enqueue(CoverJob {
        doc_id: id,
        path,
        size,
        mtime,
        password,
        cover_choice: None,
    });
    get_document(conn, &covers_queue.dir, id)
}

/// FR-31 (AC-178): the EPUB manifest images the cover pipeline can decode —
/// resolved entries with their declared media types, in manifest order; the
/// UI delivers them through the released `bookkeeper-epub:` protocol. A PDF
/// is refused: its picker is the page-thumbnail strip (`app_pdf_thumbs`).
pub fn cover_candidates(
    conn: &Connection,
    id: i64,
) -> Result<Vec<CoverCandidateDto>, AppError> {
    let target = open_target(conn, id)?;
    if !epubsrv::is_epub_path(&target.path) {
        return Err(AppError::Validation(
            "cover candidates are the EPUB picker's list; a PDF picker uses page thumbnails".into(),
        ));
    }
    Ok(
        epubmeta::cover_candidates(&target.path)
            .into_iter()
            .map(|candidate| CoverCandidateDto {
                entry: candidate.entry,
                media_type: candidate.media_type,
            })
            .collect(),
    )
}

// ------------------------------------------------------------- rename (FR-21)

/// FR-21 (AC-107…AC-110): rename the file on disk in place and move the
/// record with it. The document keeps its identity — tags, favorite state,
/// notes, Reading membership and the stored password belong to the row, not
/// the name. Validation per DR-266(d): the name is used exactly as entered
/// (leading/trailing whitespace is a rejection, not a trim); renaming to the
/// same name is a no-op success. The extension must stay in the supported
/// set (FR-21 MODIFIED, DR-285): `.pdf` ↔ `.epub` is a rename, anything
/// else is the typed rejection. A failed DB write renames the file back —
/// record and disk stay consistent (spec §3.4 step 4).
pub fn rename_document(
    conn: &mut Connection,
    sink: &dyn EventSink,
    covers_queue: &CoverQueue,
    id: i64,
    new_name: &str,
) -> Result<DocumentDto, AppError> {
    let row = conn.query_row(
        "SELECT path, filename, title_source, size, mtime, pdf_password, cover_choice FROM documents WHERE id = ?1",
        params![id],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, Option<String>>(6)?,
            ))
        },
    );
    let (path, old_filename, title_source, size, mtime, password, choice_raw) = match row {
        Ok(values) => values,
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            return Err(AppError::DocumentNotFound { id })
        }
        Err(error) => return Err(error.into()),
    };
    // The UI offers *Rename…* only when the file exists; the service
    // enforces the same rule with a fresh on-disk check (the stored
    // `missing` flag is rescan state and can be stale) and the same typed
    // error the open path uses.
    if fs::metadata(&path).is_err() {
        return Err(AppError::MissingFile {
            path,
            document_id: id,
        });
    }

    // §3.4 step 1 (DR-266(d)): reserved characters `<>:"/\|?*`, control
    // characters, path separators, leading/trailing whitespace, trailing dot.
    let rejected = new_name.is_empty()
        || new_name != new_name.trim()
        || new_name.contains('/')
        || new_name.contains('\\')
        || new_name.chars().any(|c| matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*'))
        || new_name.chars().any(|c| c < ' ')
        || new_name.ends_with('.')
        || new_name.ends_with(' ');
    if rejected {
        return Err(AppError::InvalidName(new_name.to_string()));
    }

    // FR-21 MODIFIED (§2, DR-285): in a two-type library the extension is
    // part of the name the user may change. A rename between supported
    // extensions (`.pdf` ↔ `.epub`) is accepted — the document behaves as
    // its new type from that moment: the title re-derives under the same
    // `auto` gate below (the filename fallback rule is identical for both
    // types), the cover re-enqueues under the new type's parser through the
    // re-key the released rename already performs (the worker dispatches on
    // the extension, §3.3). A name outside the supported set is the typed
    // rejection, before anything moves.
    if !is_document_path(new_name) {
        return Err(AppError::UnsupportedType {
            path: new_name.to_string(),
        });
    }

    // Same name: no-op success, the row returned unchanged (DR-266(d)).
    if new_name == old_filename {
        return get_document(conn, &covers_queue.dir, id);
    }

    let parent = Path::new(&path)
        .parent()
        .ok_or_else(|| AppError::InvalidName(new_name.to_string()))?;
    let new_path = normalize_path(&parent.join(new_name).to_string_lossy());

    // §3.4 step 2: collision with another library row, or an existing file
    // at the target. On case-insensitive filesystems a case-only rename
    // resolves to the file itself — the documented accepted `name_conflict`.
    let taken_by_row: i64 = conn.query_row(
        "SELECT COUNT(*) FROM documents WHERE path = ?1 AND id != ?2",
        params![new_path, id],
        |row| row.get(0),
    )?;
    if taken_by_row > 0 || fs::metadata(&new_path).is_ok() {
        return Err(AppError::NameConflict { path: new_path });
    }

    // §3.4 step 3.
    fs::rename(&path, &new_path).map_err(|error| AppError::Io {
        path: path.clone(),
        message: error.to_string(),
    })?;

    // §3.4 step 4: the record follows; `auto` titles re-derive from the new
    // filename, `user` titles are untouched (AC-109 — the same title_source
    // gate the released relink uses). On failure: compensating rename back.
    let update = conn.transaction().and_then(|tx| {
        let statement = if title_source == "auto" {
            tx.execute(
                "UPDATE documents SET path = ?2, filename = ?3, title = ?4 WHERE id = ?1",
                params![id, new_path, new_name, title_from_filename(new_name)],
            )
        } else {
            tx.execute(
                "UPDATE documents SET path = ?2, filename = ?3 WHERE id = ?1",
                params![id, new_path, new_name],
            )
        }?;
        tx.commit()?;
        Ok(statement)
    });
    if let Err(error) = update {
        let _ = fs::rename(&new_path, &path); // compensating rename (spec §3.4)
        return Err(error.into());
    }

    // §3.4 step 5: the cover is enqueued for the new identity; the stale
    // cache entry is removed by the existing orphan sweep (AC-110). The
    // password travels with the row — and so does the cover choice (FR-31:
    // rename keeps the id, the choice rides the tagged key).
    covers_queue.enqueue(CoverJob {
        doc_id: id,
        path: new_path,
        size,
        mtime,
        password: (!password.is_empty()).then_some(password),
        cover_choice: choice_raw.as_deref().and_then(covers::parse_choice),
    });
    emit_status(conn, sink)?;
    get_document(conn, &covers_queue.dir, id)
}

// --------------------------------------------------------------------- tags

pub fn tags_list(conn: &Connection) -> Result<Vec<TagDto>, AppError> {
    // Live counts computed in SQL, never cached client-side (tags.md, the
    // released categories.md mechanism — UI-42 renamed it, not changed it).
    let mut stmt = conn.prepare(
        "SELECT t.id, t.name, COUNT(dt.document_id)
         FROM tags t
         LEFT JOIN document_tags dt ON dt.tag_id = t.id
         GROUP BY t.id, t.name
         ORDER BY t.name COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(TagDto {
            id: row.get(0)?,
            name: row.get(1)?,
            count: row.get(2)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn tags_create(conn: &Connection, name: &str) -> Result<TagDto, AppError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::Validation("Tag name cannot be empty".into()));
    }
    let inserted = conn.execute("INSERT INTO tags (name) VALUES (?1)", params![name]);
    let id = match inserted {
        Ok(_) => conn.last_insert_rowid(),
        Err(error) if is_unique_violation(&error) => {
            return Err(AppError::TagNameDuplicate { name: name.into() });
        }
        Err(error) => return Err(error.into()),
    };
    Ok(TagDto {
        id,
        name: name.into(),
        count: 0,
    })
}

pub fn tags_rename(conn: &Connection, id: i64, name: &str) -> Result<TagDto, AppError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::Validation("Tag name cannot be empty".into()));
    }
    let updated = conn.execute("UPDATE tags SET name = ?2 WHERE id = ?1", params![id, name]);
    match updated {
        Ok(0) => return Err(AppError::TagNotFound { id }),
        Ok(_) => {}
        Err(error) if is_unique_violation(&error) => {
            return Err(AppError::TagNameDuplicate { name: name.into() });
        }
        Err(error) => return Err(error.into()),
    }
    // Membership is untouched (FR-02 invariant).
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM document_tags WHERE tag_id = ?1",
        params![id],
        |row| row.get(0),
    )?;
    Ok(TagDto {
        id,
        name: name.into(),
        count,
    })
}

pub fn tags_delete(conn: &Connection, id: i64) -> Result<(), AppError> {
    // Join rows cascade; documents are never touched (FR-02).
    let deleted = conn.execute("DELETE FROM tags WHERE id = ?1", params![id])?;
    if deleted == 0 {
        return Err(AppError::TagNotFound { id });
    }
    Ok(())
}

pub fn tags_delete_all(conn: &Connection) -> Result<usize, AppError> {
    // FR-32: the bulk delete. Join rows cascade exactly as for one tag
    // (documents are never touched, FR-02); the count is the completion
    // report. An empty library answers 0 — not an error.
    Ok(conn.execute("DELETE FROM tags", [])?)
}

/// FR-07: add and/or remove tags over a multi-selection in ONE transaction /
/// one command call.
pub fn bulk_tags(
    conn: &mut Connection,
    sink: &dyn EventSink,
    document_ids: &[i64],
    add: &[i64],
    remove: &[i64],
) -> Result<(), AppError> {
    {
        let placeholders = document_ids
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        let mut present: HashSet<i64> = HashSet::new();
        {
            let mut stmt = conn.prepare(&format!(
                "SELECT id FROM documents WHERE id IN ({placeholders})"
            ))?;
            let rows = stmt.query_map(params_from_iter(document_ids.iter()), |row| {
                row.get::<_, i64>(0)
            })?;
            for id in rows {
                present.insert(id?);
            }
        }
        if let Some(missing) = document_ids.iter().find(|id| !present.contains(id)) {
            return Err(AppError::DocumentNotFound { id: *missing });
        }
    }
    validate_tags(conn, add)?;
    validate_tags(conn, remove)?;

    let tx = conn.transaction()?;
    for document_id in document_ids {
        for tag_id in add {
            tx.execute(
                "INSERT OR IGNORE INTO document_tags (document_id, tag_id) VALUES (?1, ?2)",
                params![document_id, tag_id],
            )?;
        }
        for tag_id in remove {
            tx.execute(
                "DELETE FROM document_tags WHERE document_id = ?1 AND tag_id = ?2",
                params![document_id, tag_id],
            )?;
        }
    }
    tx.commit()?;
    emit_status(conn, sink)
}

// ---------------------------------------------------------------- settings

pub fn get_settings(conn: &Connection) -> Result<SettingsDto, AppError> {
    Ok(SettingsDto {
        view: db::get_setting(conn, "view")?
            .filter(|v| v == "list" || v == "grid")
            .unwrap_or_else(|| "list".into()),
        sort: db::get_setting(conn, "sort")?
            .filter(|v| matches!(v.as_str(), "title" | "date_added" | "size"))
            .unwrap_or_else(|| "title".into()),
        dir: db::get_setting(conn, "dir")?
            .filter(|v| v == "asc" || v == "desc")
            .unwrap_or_else(|| "asc".into()),
        sort_secondary: db::get_setting(conn, "sort_secondary")?
            .filter(|v| matches!(v.as_str(), "none" | "title" | "date_added" | "size"))
            .unwrap_or_else(|| "none".into()),
        grid_columns: db::get_setting(conn, "grid_columns")?
            .filter(|v| is_valid_grid_columns(v))
            .unwrap_or_else(|| "auto".into()),
        theme: db::get_setting(conn, "theme")?
            .filter(|v| is_valid_theme(v))
            .unwrap_or_else(|| "system".into()),
        last_filter: db::get_setting(conn, "last_filter")?
            .filter(|v| is_valid_filter(v))
            .unwrap_or_else(|| "all".into()),
        epub_text_size: db::get_setting(conn, "epub_text_size")?
            .filter(|v| is_valid_text_size(v))
            .unwrap_or_else(|| DEFAULT_TEXT_SIZE.into()),
        log_level: db::get_setting(conn, "log_level")?
            .filter(|v| matches!(v.as_str(), "error" | "info"))
            .unwrap_or_else(|| "error".into()),
        view_mode: db::get_setting(conn, "view_mode")?
            .filter(|v| matches!(v.as_str(), "single" | "continuous" | "facing" | "horizontal"))
            .unwrap_or_else(|| "continuous".into()),
        update_skipped_version: db::get_setting(conn, "update_skipped_version")?
            .filter(|v| is_valid_skipped_version(v))
            .unwrap_or_default(),
    })
}

/// `last_filter` grammar (spec §3.2, extended by FR-19 and UI-42): a mode,
/// optionally followed by `&tags=<id>(,<id>)*`. The v0.0.1 form `cat:<id>`
/// and the v0.0.7 token `cats=` stay valid input so an existing saved
/// filter survives the upgrade instead of being dropped; new values are
/// written as `&tags=` (settings.svelte.ts, AC-113).
fn is_valid_filter(value: &str) -> bool {
    if let Some(id) = value.strip_prefix("cat:") {
        // v0.0.1 wrote exactly one id in this position; anything else is a
        // corrupt row, not a tag list.
        return !id.is_empty()
            && id.chars().all(|c| c.is_ascii_digit())
            && id.parse::<i64>().is_ok();
    }
    let (mode, tags) = match value.split_once("&tags=").or(value.split_once("&cats=")) {
        Some((mode, ids)) => (mode, Some(ids)),
        None => (value, None),
    };
    matches!(
        mode,
        "all" | "favorites" | "missing" | "reading"
    ) && tags.is_none_or(is_valid_id_list)
}

fn is_valid_id_list(ids: &str) -> bool {
    !ids.is_empty()
        && ids.split(',').all(|part| {
            !part.is_empty()
                && part.chars().all(|c| c.is_ascii_digit())
                && part.parse::<i64>().is_ok()
        })
}

/// Grid column override (UI-08): `"auto"` or a manual count in the clamp range
/// the View menu offers (2–10). Anything else is a corrupt row, never a count.
fn is_valid_grid_columns(value: &str) -> bool {
    value == "auto"
        || value
            .parse::<u32>()
            .is_ok_and(|count| (2..=10).contains(&count))
}

/// Color ramp override (UI-17): `system` follows the OS, `light` and `dark`
/// override it. Anything else is a corrupt row, never a theme.
fn is_valid_theme(value: &str) -> bool {
    matches!(value, "system" | "light" | "dark")
}

/// The EPUB reading text-size ladder (SPEC-v0.0.9 §3.13, DR-303 item 5):
/// exactly the six px steps the reader offers. Anything else is a corrupt
/// row, never a size — the same pattern as the grid-column clamp.
const TEXT_SIZE_STEPS: [u32; 6] = [14, 16, 18, 20, 24, 28];
const DEFAULT_TEXT_SIZE: &str = "18";

fn is_valid_text_size(value: &str) -> bool {
    value.parse::<u32>().is_ok_and(|v| TEXT_SIZE_STEPS.contains(&v))
}

/// Skipped update version (FR-25, SPEC-v0.0.13 §3.2): `""` (nothing skipped —
/// the default) or a canonical three-segment `x.y.z` version, the shape the
/// update manifest and the plugin's `Version` use. Anything else is a corrupt
/// row, never a version — the same pattern as the other value validators.
/// The comparison the UI performs is a plain equality against the manifest
/// version (the plugin only reports strictly newer versions, spec F13).
fn is_valid_skipped_version(value: &str) -> bool {
    if value.is_empty() {
        return true;
    }
    let parts: Vec<&str> = value.split('.').collect();
    parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

pub fn set_setting(
    conn: &Connection,
    sink: &dyn EventSink,
    key: &str,
    value: &str,
) -> Result<(), AppError> {
    if !SETTINGS_KEYS.contains(&key) {
        return Err(AppError::Validation(format!("Unknown setting: {key}")));
    }
    let valid = match key {
        "view" => value == "list" || value == "grid",
        "sort" => matches!(value, "title" | "date_added" | "size"),
        "dir" => value == "asc" || value == "desc",
        "sort_secondary" => matches!(value, "none" | "title" | "date_added" | "size"),
        "grid_columns" => is_valid_grid_columns(value),
        "theme" => is_valid_theme(value),
        "last_filter" => is_valid_filter(value),
        "epub_text_size" => is_valid_text_size(value),
        "log_level" => matches!(value, "error" | "info"),
        "view_mode" => matches!(value, "single" | "continuous" | "facing" | "horizontal"),
        "update_skipped_version" => is_valid_skipped_version(value),
        _ => false,
    };
    if !valid {
        return Err(AppError::Validation(format!("Invalid value for {key}")));
    }
    db::set_setting(conn, key, value)?;
    // DR-230: the save is the single choke point — every window learns the
    // new value from this event, only after the value is durably stored.
    sink.settings_changed(key, value);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use crate::library::query::{Query, SortDir, SortKey};
    use crate::testutil::{RecordingSink, TempDir, fixture, fixture_doc, fixture_epub};
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    fn list_query(q: &str) -> Query {
        Query {
            q: q.to_string(),
            tag_ids: Vec::new(),
            favorites_only: false,
            missing_only: false,
            reading_only: false,
            sort: SortKey::Title,
            sort_dir: SortDir::Asc,
            sort_secondary: None,
        }
    }

    /// Wrap a prepared connection in the shared library type so pipeline
    /// tests run through the real short-transaction path (DR-233).
    fn test_db(conn: Connection) -> Db {
        Db::for_tests(conn)
    }

    /// Run a non-pipeline call or assertion against the test library.
    fn on<T>(db: &Db, f: impl FnOnce(&mut Connection) -> T) -> T {
        db.with_raw(|conn| Ok(f(conn))).unwrap()
    }

    /// Import a copied fixture so mutation tests never touch the shared tree.
    fn import_copy(db: &Db, temp: &TempDir, name: &str, sink: &dyn EventSink) -> DocumentDto {
        let path = temp.copy_fixture(name).to_string_lossy().into_owned();
        let (queue, receiver) = covers::test_queue(temp.path().join("covers"));
        let result =
            import_paths(db, sink, &queue, std::slice::from_ref(&path), &[]).expect("import");
        assert_eq!(result.imported, 1, "fixture must import: {path}");
        let id = on(db, |conn| conn.last_insert_rowid());
        drop(receiver);
        on(db, |conn| get_document(conn, &queue.dir, id).expect("imported document"))
    }

    #[test]
    fn import_extracts_info_titles_and_falls_back_to_filename() {
        let db = test_db(open_in_memory().unwrap());
        let temp = TempDir::new("import-meta");
        let sink = RecordingSink::default();
        let (queue, receiver) = covers::test_queue(temp.path().join("covers"));
        let paths = [
            "info-title.pdf",
            "info-none.pdf",
            "encrypted.pdf",
            "malformed.pdf",
            "notes.txt",
        ]
        .iter()
        .map(|name| temp.copy_fixture(name).to_string_lossy().into_owned())
        .collect::<Vec<_>>();
        let result = import_paths(&db, &sink, &queue, &paths, &[]).unwrap();
        drop(receiver);

        assert_eq!(result.imported, 4);
        assert_eq!(
            result.skipped,
            vec![paths[4].clone()],
            "non-PDF is skipped with notice"
        );

        let rows = on(&db, |conn| {
            search_document(conn, &queue.dir, &list_query(""), 0, 100)
                .unwrap()
                .rows
        });
        let by_title = |needle: &str| {
            rows.iter()
                .find(|row| row.title == needle)
                .unwrap_or_else(|| {
                    panic!(
                        "no row titled {needle} in {:?}",
                        rows.iter().map(|r| &r.title).collect::<Vec<_>>()
                    )
                })
        };
        // FR-05: Info-dictionary hit; every failure mode lands on the filename.
        let hit = by_title("Real Info Title");
        assert_eq!(hit.author, "Real Info Author");
        assert_eq!(hit.title_source, "auto");
        for fallback in ["info-none", "encrypted", "malformed"] {
            let row = by_title(fallback);
            assert_eq!(row.author, "");
            assert_eq!(row.title_source, "auto");
        }
    }

    #[test]
    fn import_accepts_epub_documents_and_uses_the_package_metadata() {
        // AC-126 (pipeline half) + AC-139 (pipeline half): one pipeline,
        // both types; the EPUB title/author come from the package (§3.2),
        // an unreadable package lands on the filename fallback like any
        // other unreadable file (M17), and non-documents are skipped.
        let db = test_db(open_in_memory().unwrap());
        let temp = TempDir::new("import-epub");
        let sink = RecordingSink::default();
        let (queue, receiver) = covers::test_queue(temp.path().join("covers"));
        let paths = [
            "epub-meta.epub",
            "epub-no-meta.epub",
            "epub-malformed.epub",
            "info-title.pdf",
            "notes.txt",
        ]
        .iter()
        .map(|name| temp.copy_fixture(name).to_string_lossy().into_owned())
        .collect::<Vec<_>>();
        let result = import_paths(&db, &sink, &queue, &paths, &[]).unwrap();
        drop(receiver);

        assert_eq!(result.imported, 4);
        assert_eq!(
            result.skipped,
            vec![paths[4].clone()],
            "non-document is skipped with notice"
        );

        let rows = on(&db, |conn| {
            search_document(conn, &queue.dir, &list_query(""), 0, 100)
                .unwrap()
                .rows
        });
        let by_title = |needle: &str| {
            rows.iter()
                .find(|row| row.title == needle)
                .unwrap_or_else(|| {
                    panic!(
                        "no row titled {needle} in {:?}",
                        rows.iter().map(|r| &r.title).collect::<Vec<_>>()
                    )
                })
        };
        let epub = by_title("Real EPUB Title");
        assert_eq!(epub.author, "Real EPUB Author");
        assert_eq!(epub.title_source, "auto");
        // M17: the unreadable package is imported under its filename.
        let fallback = by_title("epub-malformed");
        assert_eq!(fallback.author, "");
        assert_eq!(fallback.title_source, "auto");
        // AC-140: a package without metadata follows the filename rule.
        let no_meta = by_title("epub-no-meta");
        assert_eq!(no_meta.author, "");
        assert_eq!(no_meta.title_source, "auto");
        // The PDF path is untouched by the dispatch.
        let pdf = by_title("Real Info Title");
        assert_eq!(pdf.author, "Real Info Author");
    }

    #[test]
    fn folder_walk_collects_both_document_types() {
        // AC-126 (walk half): the recursive walk accepts `.epub` alongside
        // `.pdf` in one tree and still ignores everything else.
        let db = test_db(open_in_memory().unwrap());
        let temp = TempDir::new("walk-epub");
        let sink = RecordingSink::default();
        fs::create_dir(temp.path().join("sub")).unwrap();
        fs::copy(fixture("info-none.pdf"), temp.path().join("root-copy.pdf")).unwrap();
        fs::copy(fixture("info-none.pdf"), temp.path().join("sub/kept.pdf")).unwrap();
        fs::copy(
            fixture_epub("epub-meta.epub"),
            temp.path().join("sub/book.epub"),
        )
        .unwrap();
        fs::write(temp.path().join("sub/notes.txt"), b"not a document").unwrap();
        let (queue, receiver) = covers::test_queue(temp.path().join("covers"));
        let result = import_folder(
            &db,
            &sink,
            &queue,
            &temp.path().to_string_lossy(),
            &[],
        )
        .unwrap();
        drop(receiver);

        assert_eq!(result.imported, 3, "pdf at the root, pdf nested, epub nested");
        assert!(result.skipped.is_empty(), "the walk collects documents only");
        let titles = on(&db, |conn| {
            search_document(conn, &queue.dir, &list_query(""), 0, 100)
                .unwrap()
                .rows
                .iter()
                .map(|row| row.title.clone())
                .collect::<Vec<_>>()
        });
        assert!(
            titles.contains(&"Real EPUB Title".to_string()),
            "the nested EPUB entered the pipeline with its package title: {titles:?}"
        );
        assert_eq!(titles.iter().filter(|t| t.as_str() == "root-copy").count(), 1);
    }

    #[test]
    fn import_skips_duplicates_and_auto_assigns_the_selected_tag() {
        let db = test_db(open_in_memory().unwrap());
        let temp = TempDir::new("import-dup");
        let sink = RecordingSink::default();
        let (queue, receiver) = covers::test_queue(temp.path().join("covers"));
        let path = temp
            .copy_fixture("info-title.pdf")
            .to_string_lossy()
            .into_owned();

        let tag = on(&db, |conn| tags_create(conn, "Reading")).unwrap();
        let first = import_paths(
            &db,
            &sink,
            &queue,
            std::slice::from_ref(&path),
            &[tag.id],
        )
        .unwrap();
        assert_eq!(first.imported, 1);

        let second = import_paths(
            &db,
            &sink,
            &queue,
            std::slice::from_ref(&path),
            &[tag.id],
        )
        .unwrap();
        assert_eq!(second.imported, 0);
        assert_eq!(
            second.skipped,
            vec![path.clone()],
            "duplicate path is skipped with notice"
        );

        // FR-01 (DR-13a): import auto-assigns the currently selected tag.
        let document = on(&db, |conn| get_document(conn, &queue.dir, 1)).unwrap();
        assert_eq!(document.tags, vec!["Reading".to_string()]);

        let unknown = import_paths(
            &db,
            &sink,
            &queue,
            &["z:/nope.pdf".to_string()],
            &[999],
        );
        assert!(matches!(
            unknown,
            Err(AppError::TagNotFound { id: 999 })
        ));
        drop(receiver);
    }

    #[test]
    fn import_of_ten_thousand_fixtures_stays_within_budget() {
        // AC-01 + NFR-02 (DR-42): the batch completes in under 30 s.
        // AC-131 (SPEC-v0.0.9 §3.8): the corpus is the mixed one — 9 000
        // PDFs + 1 000 EPUBs; the budget claim covers the mix, and the
        // title probe checks one document of each type so the EPUB metadata
        // path is exercised at the same scale as the PDF path.
        let db = test_db(open_in_memory().unwrap());
        let temp = TempDir::new("import-10k");
        let sink = RecordingSink::default();
        let (queue, receiver) = covers::test_queue(temp.path().join("covers"));
        let paths = (1..=10_000)
            .map(|i| fixture_doc(i).to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        let started = Instant::now();
        let result = import_paths(&db, &sink, &queue, &paths, &[]).unwrap();
        let elapsed = started.elapsed();

        assert_eq!(result.imported, 10_000);
        assert_eq!(result.skipped, Vec::<String>::new());
        let counts = on(&db, |conn| get_counts(conn)).unwrap();
        assert_eq!(counts.total, 10_000);
        // Titles come from the fixtures' Info dictionaries end to end.
        let page = on(&db, |conn| {
            search_document(conn, &queue.dir, &list_query("Fixture Doc doc00042"), 0, 10)
        })
        .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.rows[0].title, "Fixture Doc doc00042");
        // The EPUB half: the OPF title indexed through epubmeta (AC-129).
        let epub_page = on(&db, |conn| {
            search_document(conn, &queue.dir, &list_query("Fixture Doc doc05005"), 0, 10)
        })
        .unwrap();
        assert_eq!(epub_page.total, 1);
        assert_eq!(epub_page.rows[0].title, "Fixture Doc doc05005");
        assert!(
            elapsed.as_secs() < 30,
            "10 000-document import took {elapsed:?}, budget is 30 s"
        );
        drop(receiver);
    }

    #[test]
    fn search_of_ten_thousand_documents_stays_within_budget() {
        // AC-02 (R6.3): prefix hits across all four fields at fixture scale,
        // timing asserted in the test. Rows are seeded directly — search
        // semantics do not need the files on disk (import is covered by
        // the AC-01 test).
        let conn = open_in_memory().unwrap();
        let temp = TempDir::new("search-10k");
        for i in 0..10_000i64 {
            conn.execute(
                "INSERT INTO documents (path, filename, title, author, notes, size, mtime, added_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                rusqlite::params![
                    format!("{}:{i:05}.pdf", temp.path().display()),
                    format!("{i:05}.pdf"),
                    format!("Report {i:05}"),
                    format!("Author {i:05}"),
                    format!("notes about widget {i:05}"),
                    i,
                    i,
                    i
                ],
            )
            .unwrap();
        }

        let cases = [
            ("rep", 10_000, "title"),
            ("auth", 10_000, "author"),
            ("widg", 10_000, "notes"),
            ("00012", 1, "filename of row 12 only"),
        ];
        for (needle, expected, field) in cases {
            let started = Instant::now();
            let hits = search_document(&conn, temp.path(), &list_query(needle), 0, 500).unwrap();
            let elapsed = started.elapsed();
            assert_eq!(hits.total, expected, "{needle} must prefix-hit {field}");
            assert!(hits.rows.len() <= 500, "page respects the limit");
            assert!(
                elapsed < Duration::from_millis(100),
                "AC-02 budget: prefix query over 10 000 documents took {elapsed:?}"
            );
        }
    }

    #[test]
    fn tags_membership_bulk_edit_and_delete_cascade() {
        // AC-03: many-to-many, bulk add/remove in one call, delete cascades
        // the join rows only (FR-02).
        let db = test_db(open_in_memory().unwrap());
        let temp = TempDir::new("tags");
        let sink = RecordingSink::default();
        let (queue, receiver) = covers::test_queue(temp.path().join("covers"));
        drop(receiver);

        fs::write(
            temp.path().join("second.pdf"),
            "%PDF-1.4\nnot parsed but imported\n%%EOF\n",
        )
        .unwrap();
        let paths = [
            temp.copy_fixture("info-title.pdf")
                .to_string_lossy()
                .into_owned(),
            temp.path()
                .join("second.pdf")
                .to_string_lossy()
                .into_owned(),
        ];
        let result = import_paths(&db, &sink, &queue, &paths, &[]).unwrap();
        assert_eq!(result.imported, 2);

        let a = on(&db, |conn| tags_create(conn, "A")).unwrap();
        let b = on(&db, |conn| tags_create(conn, "B")).unwrap();
        let c = on(&db, |conn| tags_create(conn, "C")).unwrap();
        assert!(
            matches!(
                on(&db, |conn| tags_create(conn, "a")),
                Err(AppError::TagNameDuplicate { .. })
            ),
            "names are unique case-insensitively"
        );

        on(&db, |conn| set_document_tags(conn, &sink, 1, &[a.id, b.id])).unwrap();
        let document = on(&db, |conn| get_document(conn, &queue.dir, 1)).unwrap();
        assert_eq!(document.tags, vec!["A".to_string(), "B".to_string()]);

        // One call, multiple documents: add C to both, remove A from doc 1.
        on(&db, |conn| bulk_tags(conn, &sink, &[1, 2], &[c.id], &[a.id])).unwrap();
        let listed = on(&db, |conn| tags_list(conn)).unwrap();
        let counts = |name: &str| {
            listed
                .iter()
                .find(|tag| tag.name == name)
                .map(|tag| tag.count)
        };
        assert_eq!(counts("A"), Some(0));
        assert_eq!(counts("B"), Some(1));
        assert_eq!(counts("C"), Some(2));

        // Deleting a tag never deletes documents (FR-02).
        on(&db, |conn| tags_delete(conn, b.id)).unwrap();
        let document = on(&db, |conn| get_document(conn, &queue.dir, 1)).unwrap();
        assert_eq!(document.tags, vec!["C".to_string()]);
        let counts = on(&db, |conn| get_counts(conn)).unwrap();
        assert_eq!(counts.total, 2);
    }

    #[test]
    fn tags_delete_all_detaches_every_membership_and_keeps_documents() {
        // AC-182 (FR-32): the bulk delete removes every tag row, cascades the
        // memberships, touches no document, and reports the count; an empty
        // library answers 0, not an error.
        let db = test_db(open_in_memory().unwrap());
        let temp = TempDir::new("tags-all");
        let sink = RecordingSink::default();
        let (queue, _receiver) = covers::test_queue(temp.path().join("covers"));
        let first = import_copy(&db, &temp, "info-none.pdf", &sink);
        let second = import_copy(&db, &temp, "info-title.pdf", &sink);

        let a = on(&db, |conn| tags_create(conn, "Bulk A")).unwrap();
        let b = on(&db, |conn| tags_create(conn, "Bulk B")).unwrap();
        on(&db, |conn| set_document_tags(conn, &sink, first.id, &[a.id, b.id])).unwrap();
        on(&db, |conn| set_document_tags(conn, &sink, second.id, &[b.id])).unwrap();

        let deleted = on(&db, |conn| tags_delete_all(conn)).unwrap();
        assert_eq!(deleted, 2);
        assert!(
            on(&db, |conn| tags_list(conn)).unwrap().is_empty(),
            "every tag row is removed"
        );
        let document = on(&db, |conn| get_document(conn, &queue.dir, first.id)).unwrap();
        assert!(
            document.tags.is_empty(),
            "memberships are detached, the document keeps its row"
        );
        let counts = on(&db, |conn| get_counts(conn)).unwrap();
        assert_eq!(counts.total, 2, "documents are never deleted (FR-02)");

        let deleted = on(&db, |conn| tags_delete_all(conn)).unwrap();
        assert_eq!(deleted, 0);
    }

    #[test]
    fn reading_membership_single_and_bulk_with_view_and_counts() {
        // AC-121/AC-103 (FR-19): the toggle works per document and across a
        // selection, the Reading view lists exactly the members, and the
        // status counts follow every change.
        let db = test_db(open_in_memory().unwrap());
        let temp = TempDir::new("reading");
        let sink = RecordingSink::default();
        let (queue, _receiver) = covers::test_queue(temp.path().join("covers"));
        let first = import_copy(&db, &temp, "info-none.pdf", &sink);
        let second = import_copy(&db, &temp, "info-title.pdf", &sink);

        // Single toggle: resolves to the updated rows so the store updates in
        // place (DR-266 e).
        let updated = on(&db, |conn| {
            set_reading(conn, &sink, &queue.dir, &[first.id], true)
        })
        .unwrap();
        assert_eq!(updated.len(), 1);
        assert!(updated[0].reading);

        // Bulk toggle across a selection (AC-121).
        let bulk = on(&db, |conn| {
            set_reading(conn, &sink, &queue.dir, &[first.id, second.id], true)
        })
        .unwrap();
        assert_eq!(bulk.len(), 2);
        assert!(bulk.iter().all(|document| document.reading));

        // The view lists exactly the members (AC-102).
        let mut query = list_query("");
        query.reading_only = true;
        let page = on(&db, |conn| search_document(conn, &queue.dir, &query, 0, 100)).unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(page.rows.len(), 2);
        assert!(page.rows.iter().all(|document| document.reading));

        // Membership is reversible; the stored flag rides into every read path.
        on(&db, |conn| set_reading(conn, &sink, &queue.dir, &[second.id], false)).unwrap();
        let page = on(&db, |conn| search_document(conn, &queue.dir, &query, 0, 100)).unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.rows[0].id, first.id);
        let document = on(&db, |conn| get_document(conn, &queue.dir, first.id)).unwrap();
        assert!(document.reading);

        // The status-bar count is live (AC-103's data source).
        let counts = on(&db, |conn| get_counts(conn)).unwrap();
        assert_eq!((counts.total, counts.reading), (2, 1));

        // Unknown ids are refused like every other mutation.
        assert!(matches!(
            on(&db, |conn| set_reading(conn, &sink, &queue.dir, &[999], true)),
            Err(AppError::DocumentNotFound { id: 999 })
        ));
    }

    #[test]
    fn reading_view_combines_with_search_favorites_tags_and_missing() {
        // AC-103: the Reading predicate composes with every other filter
        // under the same AND rule (spec §3.2).
        let db = test_db(open_in_memory().unwrap());
        let temp = TempDir::new("reading-combine");
        let sink = RecordingSink::default();
        let (queue, _receiver) = covers::test_queue(temp.path().join("covers"));
        let plain = import_copy(&db, &temp, "info-none.pdf", &sink);
        let titled = import_copy(&db, &temp, "info-title.pdf", &sink);

        let tag = on(&db, |conn| tags_create(conn, "Novels")).unwrap();
        on(&db, |conn| set_document_tags(conn, &sink, plain.id, &[tag.id])).unwrap();
        on(&db, |conn| {
            set_favorite(conn, &sink, &queue.dir, titled.id, true)
        })
        .unwrap();
        on(&db, |conn| {
            set_reading(conn, &sink, &queue.dir, &[plain.id, titled.id], true)
        })
        .unwrap();

        let ids_of = |query: &Query| -> Vec<i64> {
            on(&db, |conn| search_document(conn, &queue.dir, query, 0, 100))
                .unwrap()
                .rows
                .iter()
                .map(|document| document.id)
                .collect()
        };
        let mut query = list_query("");
        query.reading_only = true;

        // Reading + tag: only the document that carries both.
        query.tag_ids = vec![tag.id];
        assert_eq!(ids_of(&query), vec![plain.id]);
        query.tag_ids = Vec::new();

        // Reading + favorites: only the starred member.
        query.favorites_only = true;
        assert_eq!(ids_of(&query), vec![titled.id]);
        query.favorites_only = false;

        // Reading + query: the search narrows the members (UI-01 unchanged).
        query.q = "Real Info".to_string();
        assert_eq!(ids_of(&query), vec![titled.id]);
        query.q = String::new();

        // Reading + missing: both files are present, so the view is empty.
        query.missing_only = true;
        assert_eq!(ids_of(&query), Vec::<i64>::new());
        query.missing_only = false;

        // Reading off: the full library again (the view is the filter, not a mode).
        query.reading_only = false;
        let all = ids_of(&query);
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn opening_relinking_and_rescan_never_change_reading_membership() {
        // AC-102 (FR-19, the manual-only rule, OQ-48 closed as B): the
        // `reading` flag is written only by the Reading action. The paths an
        // opened or repaired document takes — rescan, relink, an edited
        // record — leave membership exactly as it was; opening itself writes
        // nothing (no service function is involved in an open).
        let db = test_db(open_in_memory().unwrap());
        let temp = TempDir::new("reading-untouched");
        let sink = RecordingSink::default();
        let document = import_copy(&db, &temp, "info-none.pdf", &sink);
        let (queue, receiver) = covers::test_queue(temp.path().join("covers"));
        on(&db, |conn| {
            set_reading(conn, &sink, &queue.dir, &[document.id], true)
        })
        .unwrap();

        // Rescan marks a moved file missing — the member stays a member.
        let old_path = PathBuf::from(&document.path);
        let new_path = temp.path().join("moved-later.pdf");
        fs::rename(&old_path, &new_path).unwrap();
        rescan(&db, &sink, &queue, false).unwrap();
        assert!(on(&db, |conn| get_document(conn, &queue.dir, document.id))
            .unwrap()
            .reading);

        // Relink restores the entry — membership survives the path change,
        // and the view still lists exactly this member.
        on(&db, |conn| {
            relink_document(conn, &sink, &queue, document.id, &new_path.to_string_lossy())
        })
        .unwrap();
        assert!(on(&db, |conn| get_document(conn, &queue.dir, document.id))
            .unwrap()
            .reading);
        let mut query = list_query("");
        query.reading_only = true;
        let ids: Vec<i64> = on(&db, |conn| search_document(conn, &queue.dir, &query, 0, 100))
            .unwrap()
            .rows
            .iter()
            .map(|row| row.id)
            .collect();
        assert_eq!(ids, vec![document.id]);

        // An edited record (the Details save) changes content, not membership.
        let edit = DocumentEdit {
            title: "Renamed By Hand",
            author: "",
            notes: "",
            favorite: false,
            tag_ids: &[],
        };
        on(&db, |conn| update_document(conn, &sink, &queue.dir, document.id, &edit)).unwrap();
        assert!(on(&db, |conn| get_document(conn, &queue.dir, document.id))
            .unwrap()
            .reading);

        // The badge count the Reading view carries still reports the member.
        assert_eq!(on(&db, |conn| get_counts(conn)).unwrap().reading, 1);
        drop(receiver);
    }

    #[test]
    fn stored_password_round_trips_and_requeues_the_cover() {
        // AC-105 (FR-20, DR-266 a): store, overwrite, clear; every non-empty
        // store re-enqueues the cover with the new password; clearing is
        // *Remove password* and re-renders nothing.
        let db = test_db(open_in_memory().unwrap());
        let temp = TempDir::new("password");
        let sink = RecordingSink::default();
        let covers_dir = temp.path().join("covers");
        let (queue, receiver) = covers::test_queue(covers_dir.clone());
        let document = import_copy_with(&db, &temp, "protected.pdf", &sink, &covers_dir);

        // Store: the row is the only channel (DR-266 a), and the cover job
        // carries the password (AC-106's render half lives in the covers tests).
        on(&db, |conn| set_password(conn, &sink, &queue, document.id, "sesame")).unwrap();
        let job = receiver.try_recv().expect("a store re-enqueues the cover");
        assert_eq!(job.doc_id, document.id);
        assert_eq!(job.password.as_deref(), Some("sesame"));

        // The stored value is what every open path presents (FR-20).
        let target = on(&db, |conn| open_target(conn, document.id)).unwrap();
        assert_eq!(target.password.as_deref(), Some("sesame"));

        // Overwriting is the same call; a session unlock updates the row too.
        on(&db, |conn| set_password(conn, &sink, &queue, document.id, "session-value")).unwrap();
        let job = receiver.try_recv().expect("every non-empty store re-enqueues");
        assert_eq!(job.password.as_deref(), Some("session-value"));

        // Clearing empties the column and leaves the existing cover in place.
        on(&db, |conn| set_password(conn, &sink, &queue, document.id, "")).unwrap();
        assert!(
            receiver.try_recv().is_err(),
            "clearing must not re-render the cover"
        );
        let target = on(&db, |conn| open_target(conn, document.id)).unwrap();
        assert_eq!(target.password, None);

        // The password is never a DTO field (FR-20): the row reads back with
        // its other fields unchanged.
        let listed = on(&db, |conn| get_document(conn, &covers_dir, document.id)).unwrap();
        assert_eq!(listed.id, document.id);
        assert_eq!(listed.filename, document.filename);

        // Unknown documents are refused like every other mutation.
        assert!(matches!(
            on(&db, |conn| set_password(conn, &sink, &queue, 999, "x")),
            Err(AppError::DocumentNotFound { id: 999 })
        ));
    }

    /// Reads the raw `cover_choice` column — the tests assert on the stored
    /// value itself, not on a DTO that deliberately does not carry it.
    fn stored_choice(db: &Db, id: i64) -> Option<String> {
        on(db, |conn| {
            conn.query_row(
                "SELECT cover_choice FROM documents WHERE id = ?1",
                rusqlite::params![id],
                |row| row.get::<_, Option<String>>(0),
            )
        })
        .unwrap()
    }

    #[test]
    fn cover_choice_set_and_clear_store_the_value_and_enqueue_the_render() {
        // AC-177/AC-180 (FR-31): a valid choice is stored verbatim and the
        // enqueued job carries the parsed choice; clearing NULLs the column
        // and re-enqueues the automatic cover; both return the updated row.
        let db = test_db(open_in_memory().unwrap());
        let temp = TempDir::new("cover-choice");
        let sink = RecordingSink::default();
        let covers_dir = temp.path().join("covers");
        let document = import_copy_with(&db, &temp, "info-none.pdf", &sink, &covers_dir);
        let (queue, receiver) = covers::test_queue(covers_dir.clone());

        let updated = on(&db, |conn| {
            set_cover_choice(conn, &queue, document.id, r#"{"kind":"page","page":2}"#)
        })
        .unwrap();
        assert_eq!(updated.id, document.id);
        assert!(
            updated.cover_path.is_empty(),
            "the tagged key has no file yet — the card keeps its glyph until the render lands"
        );
        assert_eq!(stored_choice(&db, document.id).as_deref(), Some(r#"{"kind":"page","page":2}"#));
        let job = receiver.try_recv().expect("setting a choice enqueues the render");
        assert_eq!(job.doc_id, document.id);
        assert_eq!(job.cover_choice, Some(covers::CoverChoice::Page(2)));

        let cleared = on(&db, |conn| clear_cover_choice(conn, &queue, document.id)).unwrap();
        assert_eq!(cleared.id, document.id);
        assert_eq!(stored_choice(&db, document.id), None);
        let job = receiver.try_recv().expect("clearing re-enqueues the automatic cover");
        assert_eq!(job.doc_id, document.id);
        assert_eq!(job.cover_choice, None);

        // An out-of-range page is grammatical (§3.8): it stores and renders
        // the fallback under the tagged key — the covers tests own the
        // render half; here it must simply not be rejected.
        on(&db, |conn| {
            set_cover_choice(conn, &queue, document.id, r#"{"kind":"page","page":9999}"#)
        })
        .unwrap();
        assert_eq!(stored_choice(&db, document.id).as_deref(), Some(r#"{"kind":"page","page":9999}"#));
        let job = receiver.try_recv().expect("a grammatical choice enqueues");
        assert_eq!(job.cover_choice, Some(covers::CoverChoice::Page(9999)));

        assert!(matches!(
            on(&db, |conn| set_cover_choice(conn, &queue, 999, r#"{"kind":"page","page":0}"#)),
            Err(AppError::DocumentNotFound { id: 999 })
        ));
        assert!(matches!(
            on(&db, |conn| clear_cover_choice(conn, &queue, 999)),
            Err(AppError::DocumentNotFound { id: 999 })
        ));
    }

    #[test]
    fn cover_choice_rejects_bad_grammar_and_type_mismatch_before_storage() {
        // AC-179 (FR-31): malformed JSON, a bad grammar, and a kind that
        // does not match the row's file type are the typed rejection at the
        // service boundary — the stored value never changes.
        let db = test_db(open_in_memory().unwrap());
        let temp = TempDir::new("cover-choice-reject");
        let sink = RecordingSink::default();
        let covers_dir = temp.path().join("covers");
        let pdf = import_copy_with(&db, &temp, "info-none.pdf", &sink, &covers_dir);
        let epub = import_copy_with(&db, &temp, "epub-images.epub", &sink, &covers_dir);
        let (queue, receiver) = covers::test_queue(covers_dir.clone());

        let rejected: &[(&str, &str)] = &[
            ("not JSON at all", "not JSON at all"),
            (r#"{"kind":"page"}"#, "page value missing"),
            (r#"{"kind":"page","page":-1}"#, "negative page"),
            (r#"{"kind":"page","page":1,"extra":true}"#, "extra member"),
            (r#"{"kind":"epub-image","entry":"OEBPS/images/cover.png","media":"image/png"}"#, "epub kind on a PDF"),
            (r#"{"kind":"unknown"}"#, "unknown kind"),
        ];
        for (value, why) in rejected {
            let result = on(&db, |conn| set_cover_choice(conn, &queue, pdf.id, value));
            assert!(
                matches!(result, Err(AppError::Validation(_))),
                "{why} must be a typed rejection, got {result:?}"
            );
        }
        // The kind/file-type match cuts both ways.
        let result = on(&db, |conn| {
            set_cover_choice(conn, &queue, epub.id, r#"{"kind":"page","page":0}"#)
        });
        assert!(matches!(result, Err(AppError::Validation(_))), "page kind on an EPUB must be rejected, got {result:?}");

        assert_eq!(stored_choice(&db, pdf.id), None, "a rejected choice never reaches storage");
        assert_eq!(stored_choice(&db, epub.id), None);
        assert!(
            receiver.try_recv().is_err(),
            "a rejected choice enqueues nothing"
        );
    }

    #[test]
    fn cover_candidates_lists_the_epub_manifest_images_and_refuses_a_pdf() {
        // AC-178 (FR-31): the picker grid's list — every manifest raster the
        // pipeline can decode, in the book's display order (DR-411): the
        // fixture's chapter 1 shows plate-two, chapter 2 shows plate-one, so
        // they lead in that order; the declared cover, shown by no chapter,
        // follows in manifest order; a PDF is the typed refusal (its picker
        // is the thumbnail strip).
        let db = test_db(open_in_memory().unwrap());
        let temp = TempDir::new("cover-candidates");
        let sink = RecordingSink::default();
        let covers_dir = temp.path().join("covers");
        let pdf = import_copy_with(&db, &temp, "info-none.pdf", &sink, &covers_dir);
        let epub = import_copy_with(&db, &temp, "epub-images.epub", &sink, &covers_dir);

        let candidates = on(&db, |conn| cover_candidates(conn, epub.id)).unwrap();
        assert_eq!(
            candidates
                .iter()
                .map(|candidate| candidate.entry.as_str())
                .collect::<Vec<_>>(),
            vec![
                "OEBPS/images/plate-two.png",
                "OEBPS/images/plate-one.png",
                "OEBPS/images/cover.png",
            ],
            "display order first, the unreferenced declared cover last"
        );
        assert!(
            candidates
                .iter()
                .all(|candidate| candidate.media_type == "image/png"),
            "the declared media type travels with every candidate"
        );

        let result = on(&db, |conn| cover_candidates(conn, pdf.id));
        assert!(matches!(result, Err(AppError::Validation(_))), "a PDF must be refused, got {result:?}");
        assert!(matches!(
            on(&db, |conn| cover_candidates(conn, 999)),
            Err(AppError::DocumentNotFound { id: 999 })
        ));
    }

    #[test]
    fn rename_moves_the_file_and_the_record_and_preserves_identity() {
        // AC-107/AC-109 (FR-21): file and record move together; tags,
        // favorite, notes, Reading membership and the stored password belong
        // to the row, not the name; relink and missing detection then work
        // against the new path.
        let db = test_db(open_in_memory().unwrap());
        let temp = TempDir::new("rename");
        let sink = RecordingSink::default();
        let covers_dir = temp.path().join("covers");
        let (queue, receiver) = covers::test_queue(covers_dir.clone());
        let auto = import_copy_with(&db, &temp, "info-none.pdf", &sink, &covers_dir);

        let tag = on(&db, |conn| tags_create(conn, "Keep")).unwrap();
        on(&db, |conn| set_document_tags(conn, &sink, auto.id, &[tag.id])).unwrap();
        on(&db, |conn| {
            set_favorite(conn, &sink, &covers_dir, auto.id, true)
        })
        .unwrap();
        on(&db, |conn| set_reading(conn, &sink, &covers_dir, &[auto.id], true)).unwrap();
        // Metadata edit with the title value unchanged: title_source flips
        // only when the title itself changes, so the row stays auto-sourced
        // and the rename below proves AC-109's auto branch (the user branch
        // has its own test).
        let current = on(&db, |conn| get_document(conn, &covers_dir, auto.id)).unwrap();
        on(&db, |conn| update_document(
            conn,
            &sink,
            temp.path(),
            auto.id,
            &DocumentEdit {
                title: &current.title,
                author: "A. Writer",
                notes: "kept notes",
                favorite: true,
                tag_ids: &[tag.id],
            },
        ))
        .unwrap();
        on(&db, |conn| set_password(conn, &sink, &queue, auto.id, "sesame")).unwrap();
        let _ = receiver.try_recv(); // the password store's re-enqueue

        let renamed = on(&db, |conn| {
            rename_document(conn, &sink, &queue, auto.id, "Renamed Chapter.pdf")
        })
        .unwrap();

        // AC-107: the file moved in place — same folder, new name, old gone.
        assert_eq!(renamed.filename, "Renamed Chapter.pdf");
        assert_eq!(Path::new(&renamed.path).parent(), Path::new(&auto.path).parent());
        assert!(fs::exists(&renamed.path).unwrap());
        assert!(!fs::exists(&auto.path).unwrap());

        // AC-109: an auto title re-derives from the new filename.
        assert_eq!(renamed.title, "Renamed Chapter");
        assert_eq!(renamed.title_source, "auto");

        // AC-107: identity survives — same row, same tags, favorite, Reading,
        // notes and stored password.
        assert_eq!(renamed.id, auto.id);
        assert_eq!(renamed.tags, vec!["Keep".to_string()]);
        assert!(renamed.favorite && renamed.reading);
        assert_eq!(renamed.notes, "kept notes");
        assert_eq!(renamed.size, auto.size);
        let target = on(&db, |conn| open_target(conn, auto.id)).unwrap();
        assert_eq!(target.path, renamed.path);
        assert_eq!(target.password.as_deref(), Some("sesame"));

        // AC-110 (first half): the cover is enqueued for the new identity,
        // with the row's password (the render-and-sweep half is the covers
        // test of the same AC).
        let job = receiver.try_recv().expect("a rename re-enqueues the cover");
        assert_eq!(job.doc_id, auto.id);
        assert_eq!(job.path, renamed.path);
        assert_eq!(job.password.as_deref(), Some("sesame"));

        // Renaming to the same name is a no-op success (DR-266 d).
        let same = on(&db, |conn| {
            rename_document(conn, &sink, &queue, auto.id, "Renamed Chapter.pdf")
        })
        .unwrap();
        assert_eq!(same.path, renamed.path);
        assert!(receiver.try_recv().is_err(), "a no-op enqueues nothing");

        // AC-107 (second half): missing detection and relink resolve by the
        // new path. The file was just removed, so the rescan finds exactly
        // this one row missing (first detection).
        let bytes = fs::read(&renamed.path).unwrap();
        fs::remove_file(&renamed.path).unwrap();
        let rescan = rescan(&db, &sink, &queue, false).unwrap();
        assert_eq!((rescan.missing_found, rescan.restored), (1, 0));
        let missing = on(&db, |conn| get_document(conn, &covers_dir, auto.id)).unwrap();
        assert!(missing.missing);
        // The relink dialog hands the service a path where the file exists;
        // re-creating it at the renamed path proves the row resolves by the
        // new path, not the original one.
        fs::write(&renamed.path, bytes).unwrap();
        let restored = on(&db, |conn| {
            relink_document(conn, &sink, &queue, auto.id, &renamed.path)
        })
        .unwrap();
        assert!(!restored.missing);
        assert_eq!(restored.path, renamed.path);
    }

    #[test]
    fn a_user_title_survives_the_rename_an_auto_title_re_derives() {
        // AC-109: `title_source = 'user'` is unchanged by a rename; the
        // extracted (auto) title follows the new filename — the same gate
        // the released relink uses.
        let db = test_db(open_in_memory().unwrap());
        let temp = TempDir::new("rename-title");
        let sink = RecordingSink::default();
        let covers_dir = temp.path().join("covers");
        let (queue, _receiver) = covers::test_queue(covers_dir.clone());
        let extracted = import_copy_with(&db, &temp, "info-title.pdf", &sink, &covers_dir);
        assert_eq!(extracted.title, "Real Info Title");
        assert_eq!(extracted.title_source, "auto");

        // A user edit pins the title (the released FR-06 rule).
        on(&db, |conn| {
            update_document(
                conn,
                &sink,
                temp.path(),
                extracted.id,
                &DocumentEdit {
                    title: "Chosen Title",
                    author: "",
                    notes: "",
                    favorite: false,
                    tag_ids: &[],
                },
            )
        })
        .unwrap();
        let renamed = on(&db, |conn| {
            rename_document(conn, &sink, &queue, extracted.id, "New File Name.pdf")
        })
        .unwrap();
        assert_eq!(renamed.title, "Chosen Title");
        assert_eq!(renamed.title_source, "user");

        // Without a user edit the auto title re-derives from the filename.
        let plain = import_copy_with(&db, &temp, "info-title.pdf", &sink, &covers_dir);
        let renamed = on(&db, |conn| {
            rename_document(conn, &sink, &queue, plain.id, "Second Name.pdf")
        })
        .unwrap();
        assert_eq!(renamed.title, "Second Name");
        assert_eq!(renamed.title_source, "auto");
    }

    #[test]
    fn rename_rejects_invalid_and_colliding_names_and_changes_nothing() {
        // AC-108 (FR-21, DR-266 d): typed rejections happen before any file
        // or row moves.
        let db = test_db(open_in_memory().unwrap());
        let temp = TempDir::new("rename-reject");
        let sink = RecordingSink::default();
        let covers_dir = temp.path().join("covers");
        let (queue, receiver) = covers::test_queue(covers_dir.clone());
        let first = import_copy_with(&db, &temp, "info-none.pdf", &sink, &covers_dir);
        let second = import_copy_with(&db, &temp, "info-title.pdf", &sink, &covers_dir);
        fs::write(temp.path().join("occupied.pdf"), "not in the library").unwrap();

        // Invalid names (DR-266 d): empty, whitespace-padded, separators,
        // reserved characters, control characters, trailing dot or space.
        for name in [
            "",
            "   ",
            "a/b.pdf",
            "a\\b.pdf",
            "a:b.pdf",
            "a<b.pdf",
            "a>b.pdf",
            "a\"b.pdf",
            "a|b.pdf",
            "a?b.pdf",
            "a*b.pdf",
            "a\u{1}b.pdf",
            "trailing dot.",
            "trailing space .pdf ",
        ] {
            let result = on(&db, |conn| rename_document(conn, &sink, &queue, first.id, name));
            assert!(
                matches!(result, Err(AppError::InvalidName(_))),
                "{name:?} must be rejected as invalid"
            );
        }

        // Collisions: another library document's name, or a file already on
        // disk in the same folder.
        let collision = on(&db, |conn| {
            rename_document(conn, &sink, &queue, first.id, "info-title.pdf")
        });
        assert!(
            matches!(collision, Err(AppError::NameConflict { .. })),
            "a library path collision is rejected"
        );
        let occupied = on(&db, |conn| {
            rename_document(conn, &sink, &queue, first.id, "occupied.pdf")
        });
        assert!(
            matches!(occupied, Err(AppError::NameConflict { .. })),
            "an existing file at the target is rejected"
        );

        // Nothing moved by any rejected attempt: both files and both records
        // are untouched, and no cover was enqueued (DR-266 d: typed
        // rejections happen before any file or row moves). These claims hold
        // on every filesystem because everything so far was rejected. The
        // order matters (DR-391): this block used to follow the case-only
        // probe below — on case-sensitive filesystems that rename is accepted
        // and moves its file, which made the test red on Linux.
        assert!(fs::exists(&first.path).unwrap() && fs::exists(&second.path).unwrap());
        let first_row = on(&db, |conn| get_document(conn, &covers_dir, first.id)).unwrap();
        assert_eq!(first_row.filename, "info-none.pdf");
        let second_row = on(&db, |conn| get_document(conn, &covers_dir, second.id)).unwrap();
        assert_eq!(second_row.filename, "info-title.pdf");
        assert!(receiver.try_recv().is_err(), "rejected renames enqueue nothing");

        // A case-only rename is filesystem-shaped, per DR-266 d: on a
        // case-insensitive filesystem it resolves to the file itself — the
        // documented accepted `name_conflict`, nothing moves; on a
        // case-sensitive filesystem it is an ordinary accepted rename that
        // behaves like one (file moves, row follows, auto title re-derives,
        // exactly one cover enqueues). The probe distinguishes the two.
        let case_probe = fs::metadata(temp.path().join("info-none.PDF")).is_ok();
        let case_only = on(&db, |conn| {
            rename_document(conn, &sink, &queue, first.id, "INFO-NONE.PDF")
        });
        if case_probe {
            assert!(
                matches!(case_only, Err(AppError::NameConflict { .. })),
                "on a case-insensitive filesystem a case-only rename is a conflict"
            );
            assert!(fs::exists(&first.path).unwrap(), "the conflict moved nothing");
            assert!(receiver.try_recv().is_err(), "the conflict enqueued nothing");
        } else {
            let renamed = case_only.expect("on a case-sensitive filesystem it is a rename");
            assert_eq!(renamed.filename, "INFO-NONE.PDF");
            assert!(
                fs::exists(temp.path().join("INFO-NONE.PDF")).unwrap()
                    && !fs::exists(&first.path).unwrap(),
                "the file follows the new name"
            );
            let moved_row = on(&db, |conn| get_document(conn, &covers_dir, first.id)).unwrap();
            assert_eq!(moved_row.filename, "INFO-NONE.PDF");
            assert_eq!(moved_row.title, "INFO-NONE", "the auto title re-derives (AC-109 gate)");
            let job = receiver.try_recv().expect("the accepted rename enqueues its cover");
            assert_eq!(job.doc_id, first.id);
            assert!(receiver.try_recv().is_err(), "one accepted rename enqueues exactly one job");
        }

        // A missing document cannot be renamed (AC-108's dialog only offers
        // the action when the file exists; the service enforces it too).
        fs::remove_file(&second.path).unwrap();
        let missing = on(&db, |conn| {
            rename_document(conn, &sink, &queue, second.id, "never.pdf")
        });
        assert!(
            matches!(missing, Err(AppError::MissingFile { .. })),
            "a missing document cannot be renamed"
        );
    }

    #[test]
    fn rename_between_supported_extensions_is_accepted_and_re_derives() {
        // FR-21 MODIFIED (AC-124 rename half, DR-285): `.pdf` ↔ `.epub` is
        // a rename. The record follows the new name, the `auto` title
        // re-derives under the same gate (the filename fallback rule is the
        // same for both types), and the cover re-enqueues under the new key
        // — the worker picks the new type's parser from the extension (§3.3).
        let db = test_db(open_in_memory().unwrap());
        let temp = TempDir::new("rename-type");
        let sink = RecordingSink::default();
        let covers_dir = temp.path().join("covers");
        let (queue, receiver) = covers::test_queue(covers_dir.clone());
        let as_pdf = import_copy_with(&db, &temp, "info-none.pdf", &sink, &covers_dir);
        let as_epub = import_copy_with(&db, &temp, "epub-meta.epub", &sink, &covers_dir);

        let to_epub = on(&db, |conn| {
            rename_document(conn, &sink, &queue, as_pdf.id, "renamed-book.epub")
        })
        .expect("a supported extension swap is a rename");
        assert_eq!(to_epub.filename, "renamed-book.epub");
        assert_eq!(to_epub.title, "renamed-book", "auto title follows the name");
        assert!(!fs::exists(&as_pdf.path).unwrap(), "the old path is gone");
        assert!(fs::exists(&to_epub.path).unwrap(), "the file moved in place");

        let to_pdf = on(&db, |conn| {
            rename_document(conn, &sink, &queue, as_epub.id, "now-a-pdf.pdf")
        })
        .expect("the other direction is the same rule");
        assert_eq!(to_pdf.filename, "now-a-pdf.pdf");
        assert_eq!(
            to_pdf.title, "now-a-pdf",
            "the package title was auto-derived, so the rename re-derives from the name"
        );

        // The covers re-enqueue carries the new paths (the released rename
        // already re-keys; only the two renames enqueued).
        let mut enqueued = Vec::new();
        while let Ok(job) = receiver.try_recv() {
            enqueued.push(job.path);
        }
        assert_eq!(
            enqueued,
            vec![to_epub.path.clone(), to_pdf.path.clone()],
            "both renames re-enqueue the cover under the new key"
        );
    }

    #[test]
    fn rename_to_an_unsupported_extension_is_rejected_and_changes_nothing() {
        // FR-21 MODIFIED (AC-124 rename half): an extension outside the
        // supported set is the typed rejection naming the supported types —
        // file, record and covers untouched.
        let db = test_db(open_in_memory().unwrap());
        let temp = TempDir::new("rename-unsupported");
        let sink = RecordingSink::default();
        let covers_dir = temp.path().join("covers");
        let (queue, receiver) = covers::test_queue(covers_dir.clone());
        let document = import_copy_with(&db, &temp, "epub-meta.epub", &sink, &covers_dir);

        for name in ["notes.txt", "book.epub.pdf.txt", "no-extension"] {
            let result = on(&db, |conn| rename_document(conn, &sink, &queue, document.id, name));
            assert!(
                matches!(result, Err(AppError::UnsupportedType { .. })),
                "{name:?} must be the typed rejection"
            );
        }

        assert!(fs::exists(&document.path).unwrap(), "the file did not move");
        let row = on(&db, |conn| get_document(conn, &covers_dir, document.id)).unwrap();
        assert_eq!(row.filename, "epub-meta.epub");
        assert_eq!(row.title, "Real EPUB Title", "the record is untouched");
        assert!(
            receiver.try_recv().is_err(),
            "a rejected rename enqueues nothing"
        );
    }

    #[test]
    fn library_capabilities_work_on_an_epub_document() {
        // AC-124 (capabilities half): the released capabilities are keyed by
        // row id and behave identically on an EPUB row — search by title and
        // by author, tag add/remove, bulk tags, favorite, Reading toggle,
        // missing detection. (The rename and relink halves are the two tests
        // above; the UI flows are the UI stage's.)
        let db = test_db(open_in_memory().unwrap());
        let temp = TempDir::new("epub-capabilities");
        let sink = RecordingSink::default();
        let covers_dir = temp.path().join("covers");
        let epub = import_copy_with(&db, &temp, "epub-meta.epub", &sink, &covers_dir);
        let pdf = import_copy_with(&db, &temp, "info-none.pdf", &sink, &covers_dir);

        let found_by = |needle: &str| {
            on(&db, |conn| {
                search_document(conn, &covers_dir, &list_query(needle), 0, 100)
            })
            .unwrap()
            .rows
            .iter()
            .any(|row| row.id == epub.id)
        };
        assert!(found_by("Real EPUB Title"), "search by title");
        assert!(found_by("Real EPUB Author"), "search by author");

        let tag = on(&db, |conn| tags_create(conn, "shelf")).unwrap();
        on(&db, |conn| set_document_tags(conn, &sink, epub.id, &[tag.id])).unwrap();
        let tagged = on(&db, |conn| get_document(conn, &covers_dir, epub.id)).unwrap();
        assert_eq!(tagged.tags, vec!["shelf".to_string()], "tag add");

        // Bulk operation over a mixed selection: both rows lose the tag.
        on(&db, |conn| {
            bulk_tags(conn, &sink, &[epub.id, pdf.id], &[], &[tag.id])
        })
        .unwrap();
        let untagged = on(&db, |conn| get_document(conn, &covers_dir, epub.id)).unwrap();
        assert!(untagged.tags.is_empty(), "tag remove through bulk_tags");

        let favored = on(&db, |conn| set_favorite(conn, &sink, &covers_dir, epub.id, true)).unwrap();
        assert!(favored.favorite, "favorite");
        let reading = on(&db, |conn| set_reading(conn, &sink, &covers_dir, &[epub.id], true))
            .unwrap();
        assert!(reading[0].reading, "Reading toggle");

        // Missing detection (AC-124's rescan half; §3.4 verified rescan
        // carries no extension filter): the EPUB row marks missing exactly
        // like a PDF row.
        fs::remove_file(&epub.path).unwrap();
        let (queue, receiver) = covers::test_queue(covers_dir.clone());
        let result = rescan(&db, &sink, &queue, false).unwrap();
        assert_eq!(result.missing_found, 1);
        let marked = on(&db, |conn| get_document(conn, &covers_dir, epub.id)).unwrap();
        assert!(marked.missing);
        drop(receiver);
    }
    #[test]
    fn delete_library_only_keeps_the_file_delete_with_file_removes_it_and_its_cover() {
        // AC-04 + R5.2, and AC-62's delete half: the reader page cache of
        // a document goes with the row, other documents' caches untouched.
        let db = test_db(open_in_memory().unwrap());
        let temp = TempDir::new("delete");
        let sink = RecordingSink::default();
        let covers_dir = temp.path().join("covers");
        let reader_dir = temp.path().join("reader");

        let kept = import_copy_with(&db, &temp, "info-none.pdf", &sink, &covers_dir);
        let removed = import_copy_with(&db, &temp, "info-title.pdf", &sink, &covers_dir);

        // Fake cache entries so removal can be observed without rendering.
        fs::create_dir_all(&covers_dir).unwrap();
        fs::create_dir_all(reader_dir.join(kept.id.to_string())).unwrap();
        fs::create_dir_all(reader_dir.join(removed.id.to_string())).unwrap();
        fs::write(reader_dir.join(format!("{}/3-1600.png", kept.id)), "png").unwrap();
        fs::write(reader_dir.join(format!("{}/1-800.png", removed.id)), "png").unwrap();
        let kept_mtime = mtime_secs(&fs::metadata(&kept.path).unwrap());
        let removed_mtime = mtime_secs(&fs::metadata(&removed.path).unwrap());
        let kept_cover = covers::cache_path(&covers_dir, &kept.path, kept.size, kept_mtime, None);
        let removed_cover =
            covers::cache_path(&covers_dir, &removed.path, removed.size, removed_mtime, None);
        fs::write(&kept_cover, "png").unwrap();
        fs::write(&removed_cover, "png").unwrap();

        let queue = queue_for(&temp, &covers_dir);
        on(
            &db,
            |conn| {
                delete_documents(conn, &sink, &queue, &[kept.id], false, &reader_dir)
            },
        )
        .unwrap();
        assert!(
            fs::exists(&kept.path).unwrap(),
            "library-only delete leaves the file (FR-09)"
        );
        assert!(
            !fs::exists(&kept_cover).unwrap(),
            "R5.2: cache entry goes with the entry"
        );
        assert!(
            !reader_dir.join(kept.id.to_string()).exists(),
            "AC-62: the reader page cache goes with the row"
        );
        assert!(
            fs::exists(&removed_cover).unwrap(),
            "other entries are untouched"
        );
        assert!(
            reader_dir.join(removed.id.to_string()).exists(),
            "other documents keep their reader cache"
        );

        on(
            &db,
            |conn| {
                delete_documents(conn, &sink, &queue, &[removed.id], true, &reader_dir)
            },
        )
        .unwrap();
        assert!(
            !fs::exists(&removed.path).unwrap(),
            "delete-with-file removes it (FR-11)"
        );
        assert!(!fs::exists(&removed_cover).unwrap());
        assert!(!reader_dir.join(removed.id.to_string()).exists());

        let counts = on(&db, |conn| get_counts(conn)).unwrap();
        assert_eq!(counts.total, 0);
    }

    fn queue_for(temp: &TempDir, covers_dir: &Path) -> covers::CoverQueue {
        let _ = temp;
        let (queue, _receiver) = covers::test_queue(covers_dir.to_path_buf());
        queue
    }

    fn import_copy_with(
        db: &Db,
        temp: &TempDir,
        name: &str,
        sink: &dyn EventSink,
        covers_dir: &Path,
    ) -> DocumentDto {
        let path = temp.copy_fixture(name).to_string_lossy().into_owned();
        let (queue, receiver) = covers::test_queue(covers_dir.to_path_buf());
        import_paths(db, sink, &queue, &[path], &[]).unwrap();
        drop(receiver);
        on(db, |conn| {
            let id = conn.last_insert_rowid();
            get_document(conn, covers_dir, id)
        })
        .unwrap()
    }

    #[test]
    fn rescan_marks_missing_and_relink_restores_the_entry() {
        // AC-05: move outside → rescan marks missing → relink restores.
        let db = test_db(open_in_memory().unwrap());
        let temp = TempDir::new("relink");
        let sink = RecordingSink::default();
        let document = import_copy(&db, &temp, "info-none.pdf", &sink);
        let (queue, receiver) = covers::test_queue(temp.path().join("covers"));

        let old_path = PathBuf::from(&document.path);
        let new_path = temp.path().join("moved-here.pdf");
        fs::rename(&old_path, &new_path).unwrap();

        let result = rescan(&db, &sink, &queue, false).unwrap();
        assert_eq!(result.missing_found, 1);
        assert_eq!(result.restored, 0);
        let marked = on(&db, |conn| get_document(conn, &queue.dir, document.id)).unwrap();
        assert!(marked.missing);
        assert_eq!(on(&db, |conn| get_counts(conn)).unwrap().missing, 1);

        let restored = on(&db, |conn| {
            relink_document(conn, &sink, &queue, document.id, &new_path.to_string_lossy())
        })
        .unwrap();
        assert!(!restored.missing);
        assert_eq!(restored.path, new_path.to_string_lossy());
        assert_eq!(on(&db, |conn| get_counts(conn)).unwrap().missing, 0);

        // Typed validation for the Relink dialog (relinking.md).
        let unsupported = on(&db, |conn| {
            relink_document(
                conn,
                &sink,
                &queue,
                document.id,
                &temp.path().join("x.txt").to_string_lossy(),
            )
        });
        assert!(matches!(unsupported, Err(AppError::UnsupportedType { .. })));

        let other = temp.path().join("other.pdf");
        fs::copy(fixture("info-none.pdf"), &other).unwrap();
        import_paths(
            &db,
            &sink,
            &queue,
            &[other.to_string_lossy().into_owned()],
            &[],
        )
        .unwrap();
        let taken = on(&db, |conn| {
            relink_document(conn, &sink, &queue, document.id, &other.to_string_lossy())
        });
        assert!(
            matches!(taken, Err(AppError::PathTaken { .. })),
            "a path already in the library is refused"
        );
        drop(receiver);
    }

    #[test]
    fn relink_is_type_matched() {
        // AC-128 (§3.4): a missing EPUB is relinked to an EPUB; a type
        // mismatch — in either direction — is the typed rejection.
        let db = test_db(open_in_memory().unwrap());
        let temp = TempDir::new("relink-type");
        let sink = RecordingSink::default();
        let epub = import_copy(&db, &temp, "epub-meta.epub", &sink);
        let pdf = import_copy(&db, &temp, "info-none.pdf", &sink);
        let (queue, receiver) = covers::test_queue(temp.path().join("covers"));

        let moved_epub = temp.path().join("moved-book.epub");
        fs::copy(fixture_epub("epub-no-cover.epub"), &moved_epub).unwrap();
        let moved = on(&db, |conn| {
            relink_document(conn, &sink, &queue, epub.id, &moved_epub.to_string_lossy())
        })
        .expect("an EPUB relinks to an EPUB");
        assert_eq!(moved.path, moved_epub.to_string_lossy());
        assert!(!moved.missing);

        let pdf_file = temp.path().join("wrong-type.pdf");
        fs::copy(fixture("info-none.pdf"), &pdf_file).unwrap();
        let epub_to_pdf = on(&db, |conn| {
            relink_document(conn, &sink, &queue, epub.id, &pdf_file.to_string_lossy())
        });
        assert!(
            matches!(epub_to_pdf, Err(AppError::UnsupportedType { .. })),
            "an EPUB is never relinked to a PDF"
        );

        let epub_file = temp.path().join("wrong-type.epub");
        fs::copy(fixture_epub("epub-no-cover.epub"), &epub_file).unwrap();
        let pdf_to_epub = on(&db, |conn| {
            relink_document(conn, &sink, &queue, pdf.id, &epub_file.to_string_lossy())
        });
        assert!(
            matches!(pdf_to_epub, Err(AppError::UnsupportedType { .. })),
            "a PDF is never relinked to an EPUB"
        );
        drop(receiver);
    }

    #[test]
    fn rescan_restores_entries_when_files_come_back() {
        let db = test_db(open_in_memory().unwrap());
        let temp = TempDir::new("rescan-restore");
        let sink = RecordingSink::default();
        let document = import_copy(&db, &temp, "info-none.pdf", &sink);
        let (queue, receiver) = covers::test_queue(temp.path().join("covers"));

        let original = PathBuf::from(&document.path);
        let hidden = temp.path().join("hidden.pdf");
        fs::rename(&original, &hidden).unwrap();
        assert_eq!(rescan(&db, &sink, &queue, true).unwrap().missing_found, 1);
        fs::rename(&hidden, &original).unwrap();
        let result = rescan(&db, &sink, &queue, false).unwrap();
        assert_eq!(result.restored, 1);
        assert_eq!(result.missing_found, 0);
        assert!(
            !on(&db, |conn| get_document(conn, &queue.dir, document.id))
                .unwrap()
                .missing
        );
        drop(receiver);
    }

    #[test]
    fn title_source_transitions_gate_relink_re_derivation() {
        // metadata-editing.md: `auto` on import, `user` on any manual edit;
        // only `auto` titles follow a relink.
        let db = test_db(open_in_memory().unwrap());
        let temp = TempDir::new("title-source");
        let sink = RecordingSink::default();
        let auto = import_copy(&db, &temp, "info-none.pdf", &sink);
        assert_eq!(auto.title, "info-none");
        assert_eq!(auto.title_source, "auto");

        let (queue, receiver) = covers::test_queue(temp.path().join("covers"));

        // A relink of an untouched `auto` entry re-derives from the filename.
        let renamed = temp.path().join("renamed-file.pdf");
        fs::rename(&auto.path, &renamed).unwrap();
        let relinked = on(&db, |conn| {
            relink_document(conn, &sink, &queue, auto.id, &renamed.to_string_lossy())
        })
        .unwrap();
        assert_eq!(relinked.title, "renamed-file");
        assert_eq!(relinked.title_source, "auto");

        // Manual edit flips to `user` and survives a later relink.
        let edited = on(&db, |conn| {
            update_document(
                conn,
                &sink,
                &queue.dir,
                auto.id,
                &DocumentEdit {
                    title: "My Kept Title",
                    author: "",
                    notes: "",
                    favorite: false,
                    tag_ids: &[],
                },
            )
        })
        .unwrap();
        assert_eq!(edited.title_source, "user");
        let moved_again = temp.path().join("moved-again.pdf");
        fs::rename(&renamed, &moved_again).unwrap();
        let relinked = on(&db, |conn| {
            relink_document(conn, &sink, &queue, auto.id, &moved_again.to_string_lossy())
        })
        .unwrap();
        assert_eq!(relinked.title, "My Kept Title", "user titles are preserved");
        assert_eq!(relinked.title_source, "user");

        // Saving the same title does NOT flip the flag.
        let same = on(&db, |conn| {
            update_document(
                conn,
                &sink,
                &queue.dir,
                auto.id,
                &DocumentEdit {
                    title: "My Kept Title",
                    author: "later author",
                    notes: "",
                    favorite: false,
                    tag_ids: &[],
                },
            )
        })
        .unwrap();
        assert_eq!(same.author, "later author");
        drop(receiver);
    }

    #[test]
    fn update_rejects_empty_title_at_the_service_boundary() {
        let db = test_db(open_in_memory().unwrap());
        let temp = TempDir::new("empty-title");
        let sink = RecordingSink::default();
        let document = import_copy(&db, &temp, "info-none.pdf", &sink);
        let (queue, receiver) = covers::test_queue(temp.path().join("covers"));
        let result = on(&db, |conn| {
            update_document(
                conn,
                &sink,
                &queue.dir,
                document.id,
                &DocumentEdit {
                    title: "   ",
                    author: "",
                    notes: "",
                    favorite: false,
                    tag_ids: &[],
                },
            )
        });
        assert!(matches!(result, Err(AppError::TitleEmpty)));
        drop(receiver);
    }

    #[test]
    fn every_mutation_emits_matching_status_counts() {
        // status-feedback.md: counts match fixture state after each mutation.
        let db = test_db(open_in_memory().unwrap());
        let temp = TempDir::new("status");
        let sink = RecordingSink::default();
        let first = import_copy(&db, &temp, "info-none.pdf", &sink);
        let second = import_copy(&db, &temp, "info-title.pdf", &sink);

        on(&db, |conn| {
            set_favorite(conn, &sink, &temp.path().join("covers"), first.id, true)
        })
        .unwrap();
        on(&db, |conn| {
            update_document(
                conn,
                &sink,
                temp.path(),
                second.id,
                &DocumentEdit {
                    title: "edited",
                    author: "",
                    notes: "",
                    favorite: true,
                    tag_ids: &[],
                },
            )
        })
        .unwrap();
        on(&db, |conn| {
            delete_documents(
                conn,
                &sink,
                &queue_for(&temp, &temp.path().join("covers")),
                &[first.id],
                false,
                &temp.path().join("reader"),
            )
        })
        .unwrap();

        // One event per mutation, each matching the DB state at that point.
        let expected = [
            CountsDto {
                total: 1,
                favorites: 0,
                missing: 0,
                reading: 0,
            }, // import 1
            CountsDto {
                total: 2,
                favorites: 0,
                missing: 0,
                reading: 0,
            }, // import 2
            CountsDto {
                total: 2,
                favorites: 1,
                missing: 0,
                reading: 0,
            }, // favorite
            CountsDto {
                total: 2,
                favorites: 2,
                missing: 0,
                reading: 0,
            }, // favorite via edit
            CountsDto {
                total: 1,
                favorites: 1,
                missing: 0,
                reading: 0,
            }, // delete
        ];
        assert_eq!(sink.statuses(), expected.to_vec());
        assert_eq!(on(&db, |conn| get_counts(conn)).unwrap(), *expected.last().unwrap());
    }

    #[test]
    fn import_progress_events_cover_the_whole_batch() {
        let db = test_db(open_in_memory().unwrap());
        let temp = TempDir::new("progress");
        let sink = RecordingSink::default();
        let (queue, receiver) = covers::test_queue(temp.path().join("covers"));
        let paths = (1..=2500)
            .map(|i| fixture_doc(i).to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        import_paths(&db, &sink, &queue, &paths, &[]).unwrap();
        drop(receiver);
        let progress = sink.progress();
        assert!(!progress.is_empty());
        let last = progress.last().unwrap();
        assert_eq!(
            (last.kind.as_str(), last.processed, last.total),
            ("import", 2500, 2500)
        );
    }

    #[test]
    fn settings_carry_secondary_sort_and_grid_override() {
        let conn = open_in_memory().unwrap();
        let sink = RecordingSink::default();
        // Defaults with nothing written: no secondary key (FR-13), auto columns (UI-08).
        let fresh = get_settings(&conn).unwrap();
        assert_eq!(
            (fresh.sort_secondary.as_str(), fresh.grid_columns.as_str()),
            ("none", "auto")
        );

        set_setting(&conn, &sink, "sort_secondary", "size").unwrap();
        set_setting(&conn, &sink, "grid_columns", "4").unwrap();
        let stored = get_settings(&conn).unwrap();
        assert_eq!(
            (stored.sort_secondary.as_str(), stored.grid_columns.as_str()),
            ("size", "4")
        );

        // Out-of-range values are refused, so the clamp the UI offers is the range stored.
        for (key, value) in [
            ("sort_secondary", "bogus"),
            ("grid_columns", "11"),
            ("grid_columns", "1"),
            ("grid_columns", "-3"),
            ("grid_columns", "4.5"),
        ] {
            assert!(
                matches!(set_setting(&conn, &sink, key, value), Err(AppError::Validation(_))),
                "{key}={value} should be rejected"
            );
        }

        // A corrupt row (written outside this boundary) falls back to the default.
        db::set_setting(&conn, "sort_secondary", "garbage").unwrap();
        db::set_setting(&conn, "grid_columns", "99").unwrap();
        let repaired = get_settings(&conn).unwrap();
        assert_eq!(
            (
                repaired.sort_secondary.as_str(),
                repaired.grid_columns.as_str()
            ),
            ("none", "auto")
        );
    }

    /// FR-29 (SPEC-v0.0.12 §3.3): the reader view mode rides the existing
    /// settings pair; the grammar is exactly the four mode names, no row is
    /// the default `continuous` (the released behavior), a corrupt row falls
    /// back to it, and every window hears the change through `settings_changed`.
    #[test]
    fn view_mode_setting_round_trips_with_the_four_modes() {
        let conn = open_in_memory().unwrap();
        let sink = RecordingSink::default();
        assert_eq!(get_settings(&conn).unwrap().view_mode, "continuous");

        for mode in ["single", "continuous", "facing", "horizontal"] {
            set_setting(&conn, &sink, "view_mode", mode).unwrap();
            assert_eq!(get_settings(&conn).unwrap().view_mode, mode);
        }

        for value in ["bogus", "", "Continuous", "two-page"] {
            assert!(
                matches!(set_setting(&conn, &sink, "view_mode", value), Err(AppError::Validation(_))),
                "view_mode={value} should be rejected"
            );
        }

        db::set_setting(&conn, "view_mode", "garbage").unwrap();
        assert_eq!(get_settings(&conn).unwrap().view_mode, "continuous");

        // DR-230 event path: the change is announced after the durable write.
        set_setting(&conn, &sink, "view_mode", "facing").unwrap();
        assert!(
            sink
                .settings()
                .iter()
                .any(|(key, value)| key == "view_mode" && value == "facing"),
            "every window learns the new mode through settings_changed"
        );
    }

    /// AC-193 (FR-25, SPEC-v0.0.13 §3.2): the Skip key round-trips, is
    /// validated as a canonical version or empty, corrupt rows fall back to
    /// "nothing skipped", and the change reaches every window through the
    /// DR-230 event path.
    #[test]
    fn update_skipped_version_setting_round_trips_and_validates_versions() {
        let conn = open_in_memory().unwrap();
        let sink = RecordingSink::default();
        assert_eq!(
            get_settings(&conn).unwrap().update_skipped_version,
            "",
            "nothing is skipped by default"
        );

        for value in ["99.0.0", "0.0.13", "1.2.3456"] {
            set_setting(&conn, &sink, "update_skipped_version", value).unwrap();
            assert_eq!(get_settings(&conn).unwrap().update_skipped_version, value);
        }
        // The empty value is the clear path: it is a valid value, not a gap.
        set_setting(&conn, &sink, "update_skipped_version", "").unwrap();
        assert_eq!(get_settings(&conn).unwrap().update_skipped_version, "");

        for value in ["bogus", "1.2", "1.2.3.4", "1..2", "v1.2.3", " 1.2.3", "1.2.3 "] {
            assert!(
                matches!(
                    set_setting(&conn, &sink, "update_skipped_version", value),
                    Err(AppError::Validation(_))
                ),
                "update_skipped_version={value} should be rejected"
            );
        }
        assert!(
            matches!(
                set_setting(&conn, &sink, "update_skipped_ver", "1.2.3"),
                Err(AppError::Validation(_))
            ),
            "unknown keys stay rejected"
        );

        db::set_setting(&conn, "update_skipped_version", "garbage").unwrap();
        assert_eq!(
            get_settings(&conn).unwrap().update_skipped_version,
            "",
            "a corrupt row is never a version"
        );

        // DR-230 event path: the change is announced after the durable write.
        set_setting(&conn, &sink, "update_skipped_version", "99.0.0").unwrap();
        assert!(
            sink
                .settings()
                .iter()
                .any(|(key, value)| key == "update_skipped_version" && value == "99.0.0"),
            "every window learns the skip through settings_changed"
        );
    }

    /// AC-166 no-bloat half (§3.3): per-document position rows never reach
    /// the settings DTO — the DTO's serialized fields are exactly the
    /// settings-store keys, and a `reader.pos.<id>` row changes nothing in it.
    #[test]
    fn position_rows_do_not_bloat_the_settings_dto() {
        let conn = open_in_memory().unwrap();
        let sink = RecordingSink::default();
        conn.execute(
            "INSERT INTO documents (id, path, filename, title, size, mtime, added_at) VALUES (1, '/a.pdf', 'a.pdf', 'A', 1, 1, 1)",
            [],
        )
        .unwrap();
        crate::positions::set(&conn, 1, r#"{"p":12,"o":0.4}"#).unwrap();

        let settings = get_settings(&conn).unwrap();
        assert_eq!(settings.view_mode, "continuous", "positions are not settings");

        let fields: Vec<String> = serde_json::to_value(&settings)
            .unwrap()
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        let mut expected: Vec<&str> = SETTINGS_KEYS.to_vec();
        expected.sort_unstable();
        let mut fields_sorted = fields.clone();
        fields_sorted.sort_unstable();
        assert_eq!(fields_sorted, expected, "the DTO is exactly the settings keys — nothing per-document rides it");
        let serialized = serde_json::to_string(&settings).unwrap();
        assert!(!serialized.contains("reader.pos"), "no position key reaches any window");
        let _ = sink;
    }

    /// FR-28 (AC-167): `delete_documents` removes the position keys in the
    /// same transaction; relink keeps the id, so the memory survives (§3.2).
    #[test]
    fn delete_documents_removes_position_memory_relink_keeps_it() {
        let db = test_db(open_in_memory().unwrap());
        let temp = TempDir::new("pos-memory");
        let sink = RecordingSink::default();
        let document = import_copy(&db, &temp, "info-none.pdf", &sink);
        let (queue, receiver) = covers::test_queue(temp.path().join("covers"));
        let reader_dir = temp.path().join("reader");

        on(&db, |conn| {
            crate::positions::set(conn, document.id, r#"{"p":5,"o":0.5}"#)
        })
        .unwrap();

        // Relink: same id, new path — the memory stays (AC-167 relink half).
        let new_path = temp.path().join("moved.pdf");
        fs::rename(&document.path, &new_path).unwrap();
        rescan(&db, &sink, &queue, false).unwrap();
        on(&db, |conn| {
            relink_document(conn, &sink, &queue, document.id, &new_path.to_string_lossy())
        })
        .unwrap();
        assert_eq!(
            on(&db, |conn| crate::positions::get(conn, document.id)).unwrap().as_deref(),
            Some(r#"{"p":5,"o":0.5}"#),
            "relink keeps the position"
        );

        // Delete: the keys go with the row, in the same transaction.
        on(&db, |conn| {
            delete_documents(conn, &sink, &queue, &[document.id], false, &reader_dir)
        })
        .unwrap();
        let stored = on(&db, |conn| {
            db::get_setting(conn, &crate::positions::key_for(document.id))
        })
        .unwrap();
        assert_eq!(stored, None, "AC-167: the position memory is removed with the document");
        drop(receiver);
    }

    /// FR-26 (SPEC-v0.0.11 §3.8): the level persists through the settings
    /// table and survives a reopen; the grammar is exactly the two values;
    /// no row is the default `error`.
    #[test]
    fn log_level_grammar_and_reopen_survival() {
        let temp = TempDir::new("log-level-setting");
        {
            let conn = db::open(temp.path()).unwrap();
            let sink = RecordingSink::default();
            // No row: the default is Error (the released settings pattern).
            assert_eq!(get_settings(&conn).unwrap().log_level, "error");

            set_setting(&conn, &sink, "log_level", "info").unwrap();
            assert_eq!(get_settings(&conn).unwrap().log_level, "info");

            for value in ["warn", "", "Info", "ERROR"] {
                assert!(
                    matches!(
                        set_setting(&conn, &sink, "log_level", value),
                        Err(AppError::Validation(_))
                    ),
                    "log_level={value} should be rejected"
                );
            }
            // The settings_changed emission is the released one (DR-230):
            // the level rides the same event as every other key.
            assert!(
                sink
                    .settings()
                    .iter()
                    .any(|(key, value)| key == "log_level" && value == "info"),
                "the save notifies every window's mirror"
            );
        }
        // Reopen: the stored value reads back through the same path.
        let conn = db::open(temp.path()).unwrap();
        assert_eq!(get_settings(&conn).unwrap().log_level, "info");
        // A corrupt row written outside this boundary reads as the default.
        db::set_setting(&conn, "log_level", "verbose").unwrap();
        assert_eq!(get_settings(&conn).unwrap().log_level, "error");
    }

    #[test]
    fn theme_defaults_to_system_and_accepts_only_the_three_states() {
        let conn = open_in_memory().unwrap();
        let sink = RecordingSink::default();
        // Nothing written: the app follows the OS, which is the released behavior (UI-17).
        assert_eq!(get_settings(&conn).unwrap().theme, "system");

        for value in ["light", "dark", "system"] {
            set_setting(&conn, &sink, "theme", value).unwrap();
            assert_eq!(get_settings(&conn).unwrap().theme, value);
        }

        for value in ["auto", "LIGHT", "light ", ""] {
            assert!(
                matches!(
                    set_setting(&conn, &sink, "theme", value),
                    Err(AppError::Validation(_))
                ),
                "theme={value} should be rejected"
            );
        }

        // A corrupt row written outside this boundary reads back as the default.
        db::set_setting(&conn, "theme", "midnight").unwrap();
        assert_eq!(get_settings(&conn).unwrap().theme, "system");

        // AC-99 (DR-230): every accepted save announces itself exactly once,
        // with the stored value; a rejected save announces nothing.
        assert_eq!(
            sink.settings(),
            [
                ("theme".to_string(), "light".to_string()),
                ("theme".to_string(), "dark".to_string()),
                ("theme".to_string(), "system".to_string()),
            ]
            .to_vec()
        );
    }

    #[test]
    fn epub_text_size_accepts_only_the_ladder_steps() {
        // SPEC-v0.0.9 §3.9 consequence 6 / §3.13, DR-303 item 5.
        let conn = open_in_memory().unwrap();
        let sink = RecordingSink::default();
        // Nothing written: the ladder's default step.
        assert_eq!(get_settings(&conn).unwrap().epub_text_size, "18");

        for value in ["14", "16", "18", "20", "24", "28"] {
            set_setting(&conn, &sink, "epub_text_size", value).unwrap();
            assert_eq!(get_settings(&conn).unwrap().epub_text_size, value);
        }

        for value in ["13", "19", "100", "0", "18px", "18 ", ""] {
            assert!(
                matches!(
                    set_setting(&conn, &sink, "epub_text_size", value),
                    Err(AppError::Validation(_))
                ),
                "epub_text_size={value} should be rejected"
            );
        }

        // A corrupt row written outside this boundary reads back as the default.
        db::set_setting(&conn, "epub_text_size", "999").unwrap();
        assert_eq!(get_settings(&conn).unwrap().epub_text_size, "18");
    }

    #[test]
    fn last_filter_grammar_accepts_the_new_form_and_the_legacy_one() {
        // AC-113 (UI-42): `reading` is a mode; `&tags=` is the current list
        // form; `&cats=` and `cat:` keep their meaning for values a
        // pre-upgrade library stored.
        let conn = open_in_memory().unwrap();
        let sink = RecordingSink::default();
        for value in [
            "all",
            "favorites",
            "missing",
            "reading",
            "all&tags=7",
            "favorites&tags=1,2",
            "missing&tags=3,4,5",
            "reading&tags=3",
            "all&cats=7",
            "favorites&cats=1,2",
            "missing&cats=3,4,5",
            "cat:5", // written by v0.0.1: readable, never dropped by the upgrade
        ] {
            set_setting(&conn, &sink, "last_filter", value)
                .unwrap_or_else(|error| panic!("{value} must be accepted: {error:?}"));
            assert_eq!(get_settings(&conn).unwrap().last_filter, value);
        }
        for value in [
            "",
            "bogus",
            "favorites&tags=",
            "favorites&tags=1,,2",
            "favorites&tags=x",
            "all&tags=1,2&tags=3",
            "all&cats=1,2&cats=3",
            "all&tags=1&cats=2",
            "all&cats=1&tags=2",
            "cat:",
            "cat:1,2",
        ] {
            assert!(
                matches!(
                    set_setting(&conn, &sink, "last_filter", value),
                    Err(AppError::Validation(_))
                ),
                "{value} should be rejected"
            );
        }
    }

    /// The document row a stored path belongs to. Tests MUST NOT rely on
    /// `last_insert_rowid` once a write touches another rowid table.
    fn document_id_for(conn: &Connection, path: &str) -> i64 {
        conn.query_row(
            "SELECT id FROM documents WHERE path = ?1",
            rusqlite::params![normalize_path(path)],
            |row| row.get(0),
        )
        .unwrap()
    }

    #[test]
    fn import_assigns_every_active_tag() {
        let db = test_db(open_in_memory().unwrap());
        let temp = TempDir::new("assign-tags");
        let sink = RecordingSink::default();
        let (queue, _receiver) = covers::test_queue(temp.path().join("covers"));
        let alpha = on(&db, |conn| tags_create(conn, "Alpha")).unwrap();
        let beta = on(&db, |conn| tags_create(conn, "Beta")).unwrap();

        // FR-10 as modified (DR-82): one import, every active tag.
        let first = fixture_doc(1).to_string_lossy().into_owned();
        import_paths(
            &db,
            &sink,
            &queue,
            std::slice::from_ref(&first),
            &[alpha.id, beta.id],
        )
        .unwrap();
        let filed = on(&db, |conn| {
            get_document(conn, &queue.dir, document_id_for(conn, &first))
        })
        .unwrap();
        assert_eq!(
            filed.tags,
            vec!["Alpha".to_string(), "Beta".to_string()]
        );

        // No active tag: nothing is assigned.
        let second = fixture_doc(2).to_string_lossy().into_owned();
        import_paths(&db, &sink, &queue, std::slice::from_ref(&second), &[]).unwrap();
        let unfiled = on(&db, |conn| {
            get_document(conn, &queue.dir, document_id_for(conn, &second))
        })
        .unwrap();
        assert!(unfiled.tags.is_empty());

        // An unknown id in the set is refused before any row is written.
        let third = fixture_doc(3).to_string_lossy().into_owned();
        let rejected = import_paths(&db, &sink, &queue, &[third], &[alpha.id, 999]);
        assert!(matches!(
            rejected,
            Err(AppError::TagNotFound { id: 999 })
        ));
        let total = on(&db, |conn| {
            conn.query_row("SELECT COUNT(*) FROM documents", [], |row| row.get::<_, i64>(0))
                .unwrap()
        });
        assert_eq!(total, 2, "a rejected import writes nothing");
    }

    /// Recording sink with a lock probe (AC-101, DR-233): every progress
    /// event records whether the connection was free at the moment of the
    /// event — the pipeline must emit progress only outside a lock hold.
    struct ProbingSink {
        inner: RecordingSink,
        db: std::sync::Arc<Db>,
        lock_free: std::sync::Mutex<Vec<bool>>,
    }

    impl EventSink for ProbingSink {
        fn status(&self, counts: &CountsDto) {
            self.inner.status(counts);
        }
        fn progress(&self, job: &JobProgress) {
            let free = self.db.try_with_raw(|_| Ok(())).is_some();
            self.lock_free.lock().unwrap().push(free);
            <RecordingSink as EventSink>::progress(&self.inner, job);
        }
        fn settings_changed(&self, key: &str, value: &str) {
            self.inner.settings_changed(key, value);
        }
    }

    #[test]
    fn import_pipeline_events_cover_every_chunk_and_fire_outside_the_lock() {
        // AC-101 (DR-233, chunk bound revised to 5 by DR-279): the bar
        // appears before any file is processed, advances in steps of at most
        // CHUNK, covers the whole import, and is emitted while the connection
        // is free — never from inside a lock hold. 60 fixtures = twelve
        // full chunks of 5.
        let db = std::sync::Arc::new(test_db(open_in_memory().unwrap()));
        let temp = TempDir::new("progress-structure");
        let sink = ProbingSink {
            inner: RecordingSink::default(),
            db: std::sync::Arc::clone(&db),
            lock_free: std::sync::Mutex::new(Vec::new()),
        };
        let (queue, receiver) = covers::test_queue(temp.path().join("covers"));
        let paths = (1..=60)
            .map(|i| fixture_doc(i).to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        let result = import_paths(&db, &sink, &queue, &paths, &[]).unwrap();
        drop(receiver);
        assert_eq!(result.imported, 60);

        let progress = sink.inner.progress();
        // Opening event: the total is known before any file is processed.
        assert_eq!(progress.len(), 13, "opening event + one per chunk of 5");
        assert_eq!(
            (
                progress[0].kind.as_str(),
                progress[0].processed,
                progress[0].total
            ),
            ("import", 0, 60),
            "the bar appears before the first file is processed"
        );
        // Chunk events: the exact chunk boundaries of the pipeline (DR-279).
        let processed: Vec<i64> = progress.iter().map(|event| event.processed).collect();
        assert_eq!(
            processed,
            vec![0, 5, 10, 15, 20, 25, 30, 35, 40, 45, 50, 55, 60]
        );
        assert!(progress.iter().all(|event| event.kind == "import"));
        assert!(progress.iter().all(|event| event.total == 60));

        // Every progress event fired while the connection was free.
        let lock_free = sink.lock_free.lock().unwrap().clone();
        assert!(
            lock_free.iter().all(|free| *free),
            "progress events must fire outside the lock (DR-233)"
        );

        // Completion: the final status event matches the imported state.
        let last_status = *sink.inner.statuses().last().expect("status event");
        assert_eq!(last_status.total, 60);
    }
}
