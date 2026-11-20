//! Reader position memory (FR-28, SPEC-v0.0.12 §3.2): per-document reading
//! positions live in the `settings` table under `reader.pos.<id>` keys —
//! app state, not library data. Values are compact JSON: a PDF carries
//! `{"p":<1-based page>,"o":<0–1 offset fraction within the page>}`, an
//! EPUB `{"c":<1-based spine index>,"o":<0–1 fraction within the chapter>}`
//! (OQ-69/OQ-72 closed as A, DR-399). The grammar and number bounds are
//! validated here, at the storage boundary the commands call; clamping to a
//! document's current extent is the reader's job on open (§3.2 restore).
//! Deleting a document removes its keys (`delete_documents` calls
//! [`cleanup`] inside its transaction); relink/rename keep the id, so the
//! memory survives them.

use rusqlite::Connection;

use crate::db;
use crate::error::AppError;

/// The settings key a document's position lives under.
pub fn key_for(id: i64) -> String {
    format!("reader.pos.{id}")
}

/// FR-28: the stored position for a document, or `None` when none was ever
/// written. The id must exist — a position of a removed document is not a
/// position.
pub fn get(conn: &Connection, id: i64) -> Result<Option<String>, AppError> {
    require_document(conn, id)?;
    db::get_setting(conn, &key_for(id))
}

/// FR-28: write a position. The value must pass [`validate`] before it is
/// stored; the id must exist. Deliberately no `settings_changed` event
/// (§3.2): a position is per-document state, not a window setting.
pub fn set(conn: &Connection, id: i64, value: &str) -> Result<(), AppError> {
    require_document(conn, id)?;
    validate(value)?;
    db::set_setting(conn, &key_for(id), value)
}

/// FR-28: remove the position keys of deleted documents. Called by
/// `delete_documents` inside its delete transaction (AC-167: the memory is
/// removed with the document).
pub fn cleanup(conn: &Connection, ids: &[i64]) -> Result<(), AppError> {
    for id in ids {
        conn.execute(
            "DELETE FROM settings WHERE key = ?1",
            rusqlite::params![key_for(*id)],
        )?;
    }
    Ok(())
}

/// The §3.2 value grammar: an object carrying exactly one anchor — `p`
/// (PDF page) or `c` (EPUB spine index), a 1-based integer — plus `o`, a
/// 0–1 offset fraction, and nothing else. Anything else is a malformed
/// value, rejected at the boundary.
pub fn validate(value: &str) -> Result<(), AppError> {
    let parsed: serde_json::Value = serde_json::from_str(value)
        .map_err(|_| AppError::Validation("position value is not valid JSON".into()))?;
    let object = parsed
        .as_object()
        .ok_or_else(|| AppError::Validation("position value must be an object".into()))?;
    if object.len() != 2 {
        return Err(AppError::Validation(
            "position value must carry exactly an anchor and an offset".into(),
        ));
    }
    let page = object.get("p");
    let chapter = object.get("c");
    if page.is_some() == chapter.is_some() {
        return Err(AppError::Validation(
            "position value must carry exactly one of p or c".into(),
        ));
    }
    let anchor = page.or(chapter).and_then(|v| v.as_u64());
    if !anchor.is_some_and(|n| n >= 1) {
        return Err(AppError::Validation(
            "position anchor must be a 1-based integer".into(),
        ));
    }
    let offset = object.get("o").and_then(|v| v.as_f64());
    if !offset.is_some_and(|o| (0.0..=1.0).contains(&o)) {
        return Err(AppError::Validation(
            "position offset must be a number between 0 and 1".into(),
        ));
    }
    Ok(())
}

fn require_document(conn: &Connection, id: i64) -> Result<(), AppError> {
    let found: i64 = conn.query_row(
        "SELECT COUNT(*) FROM documents WHERE id = ?1",
        rusqlite::params![id],
        |row| row.get(0),
    )?;
    if found == 0 {
        return Err(AppError::DocumentNotFound { id });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    fn conn_with_document(id: i64) -> Connection {
        let conn = open_in_memory().unwrap();
        conn.execute(
            "INSERT INTO documents (id, path, filename, title, size, mtime, added_at) VALUES (?1, ?2, 'a.pdf', 'A', 1, 1, 1)",
            rusqlite::params![id, format!("/a{id}.pdf")],
        )
        .unwrap();
        conn
    }

    #[test]
    fn position_round_trip_get_set() {
        // AC-167 storage half: a written position is what a get returns;
        // an unwritten one is None.
        let conn = conn_with_document(1);
        assert_eq!(get(&conn, 1).unwrap(), None);
        set(&conn, 1, r#"{"p":7,"o":0.5}"#).unwrap();
        assert_eq!(get(&conn, 1).unwrap().as_deref(), Some(r#"{"p":7,"o":0.5}"#));
        set(&conn, 1, r#"{"p":9,"o":0.0}"#).unwrap(); // last value wins
        assert_eq!(get(&conn, 1).unwrap().as_deref(), Some(r#"{"p":9,"o":0.0}"#));
        set(&conn, 1, r#"{"c":3,"o":0.25}"#).unwrap(); // the EPUB shape stores alike
        assert_eq!(get(&conn, 1).unwrap().as_deref(), Some(r#"{"c":3,"o":0.25}"#));
    }

    #[test]
    fn malformed_values_are_rejected_at_the_boundary() {
        // §3.2 grammar: malformed JSON, out-of-range bounds, both anchors,
        // a 0 anchor, extra members — none reaches storage.
        let conn = conn_with_document(1);
        let bad = [
            "not json",
            r#""p""#,
            r#"{"p":7}"#, // no offset
            r#"{"p":7,"o":0.5,"x":1}"#,
            r#"{"p":0,"o":0.5}"#, // 0 is not 1-based
            r#"{"p":-2,"o":0.5}"#,
            r#"{"p":7,"o":1.5}"#, // out of range
            r#"{"p":7,"o":-0.1}"#,
            r#"{"p":7,"c":2,"o":0.5}"#, // both anchors
            r#"{"o":0.5}"#, // no anchor
            r#"{"p":7.5,"o":0.5}"#, // fractional page
        ];
        for value in bad {
            let result = set(&conn, 1, value);
            assert!(matches!(result, Err(AppError::Validation(_))), "must reject: {value}");
        }
        assert_eq!(get(&conn, 1).unwrap(), None, "nothing was stored");
    }

    #[test]
    fn unknown_id_is_rejected() {
        let conn = open_in_memory().unwrap();
        let result = set(&conn, 42, r#"{"p":1,"o":0}"#);
        assert!(matches!(result, Err(AppError::DocumentNotFound { id: 42 })));
        let result = get(&conn, 42);
        assert!(matches!(result, Err(AppError::DocumentNotFound { id: 42 })));
    }

    #[test]
    fn cleanup_removes_only_the_named_documents_keys() {
        let conn = conn_with_document(1);
        conn.execute(
            "INSERT INTO documents (id, path, filename, title, size, mtime, added_at) VALUES (2, '/b.pdf', 'b.pdf', 'B', 1, 1, 1)",
            [],
        )
        .unwrap();
        set(&conn, 1, r#"{"p":3,"o":0.1}"#).unwrap();
        set(&conn, 2, r#"{"p":4,"o":0.2}"#).unwrap();
        db::set_setting(&conn, "view", "grid").unwrap(); // a window setting is not a position

        cleanup(&conn, &[1]).unwrap();

        assert_eq!(get(&conn, 1).unwrap(), None);
        assert_eq!(get(&conn, 2).unwrap().as_deref(), Some(r#"{"p":4,"o":0.2}"#));
        assert_eq!(db::get_setting(&conn, "view").unwrap().as_deref(), Some("grid"));
    }
}
