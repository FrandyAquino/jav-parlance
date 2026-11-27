use std::path::Path;

use parlance_domain::{TranscriptSegment, Translation};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, thiserror::Error)]
pub enum HistoryError {
    #[error("history database error: {0}")]
    Database(#[from] rusqlite::Error),
}

/// Ordered schema migrations; `PRAGMA user_version` records how many ran.
const MIGRATIONS: &[&str] = &["
    CREATE TABLE sessions (
        id INTEGER PRIMARY KEY,
        started_at_ms INTEGER NOT NULL,
        ended_at_ms INTEGER,
        target_language TEXT NOT NULL,
        title TEXT NOT NULL
    );
    CREATE TABLE lines (
        session_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
        segment_id INTEGER NOT NULL,
        start_ms INTEGER NOT NULL,
        end_ms INTEGER NOT NULL,
        language TEXT,
        text TEXT NOT NULL,
        translation TEXT,
        PRIMARY KEY (session_id, segment_id)
    );
"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSession {
    pub started_at_ms: i64,
    pub target_language: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HistoryEntry {
    #[ts(type = "number")]
    pub id: i64,
    #[ts(type = "number")]
    pub started_at_ms: i64,
    #[ts(type = "number | null")]
    pub ended_at_ms: Option<i64>,
    pub title: String,
    pub target_language: String,
    #[ts(type = "number")]
    pub line_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HistoryLine {
    #[ts(type = "number")]
    pub start_ms: i64,
    pub language: Option<String>,
    pub text: String,
    pub translation: Option<String>,
}

#[derive(Debug)]
pub struct HistoryStore {
    connection: Connection,
}

impl HistoryStore {
    pub fn open(path: &Path) -> Result<Self, HistoryError> {
        Self::migrate(Connection::open(path)?)
    }

    pub fn in_memory() -> Result<Self, HistoryError> {
        Self::migrate(Connection::open_in_memory()?)
    }

    fn migrate(connection: Connection) -> Result<Self, HistoryError> {
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "foreign_keys", true)?;
        let applied: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        for (version, migration) in (1u32..).zip(MIGRATIONS).skip(applied as usize) {
            connection.execute_batch(migration)?;
            connection.pragma_update(None, "user_version", version)?;
        }
        Ok(Self { connection })
    }

    pub fn create_session(&self, session: &NewSession) -> Result<i64, HistoryError> {
        self.connection.execute(
            "INSERT INTO sessions (started_at_ms, target_language, title) VALUES (?1, ?2, ?3)",
            params![session.started_at_ms, session.target_language, session.title],
        )?;
        Ok(self.connection.last_insert_rowid())
    }

    pub fn end_session(&self, id: i64, ended_at_ms: i64) -> Result<(), HistoryError> {
        self.connection.execute("UPDATE sessions SET ended_at_ms = ?2 WHERE id = ?1", params![id, ended_at_ms])?;
        Ok(())
    }

    /// Final segments are immutable, so a later revision simply replaces text.
    pub fn save_segment(&self, session_id: i64, segment: &TranscriptSegment) -> Result<(), HistoryError> {
        self.connection.execute(
            "INSERT INTO lines (session_id, segment_id, start_ms, end_ms, language, text)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT (session_id, segment_id) DO UPDATE SET text = excluded.text, end_ms = excluded.end_ms",
            params![
                session_id,
                segment.id.0 as i64,
                segment.start_ms as i64,
                segment.end_ms as i64,
                segment.language.as_ref().map(|l| l.as_str().to_owned()),
                segment.text(),
            ],
        )?;
        Ok(())
    }

    pub fn save_translation(&self, session_id: i64, translation: &Translation) -> Result<(), HistoryError> {
        self.connection.execute(
            "UPDATE lines SET translation = ?3 WHERE session_id = ?1 AND segment_id = ?2",
            params![session_id, translation.segment_id.0 as i64, translation.text],
        )?;
        Ok(())
    }

    pub fn list(&self, limit: u32) -> Result<Vec<HistoryEntry>, HistoryError> {
        let mut statement = self.connection.prepare(
            "SELECT s.id, s.started_at_ms, s.ended_at_ms, s.title, s.target_language,
                    (SELECT COUNT(*) FROM lines l WHERE l.session_id = s.id)
             FROM sessions s ORDER BY s.started_at_ms DESC LIMIT ?1",
        )?;
        let rows = statement.query_map([limit], |row| {
            Ok(HistoryEntry {
                id: row.get(0)?,
                started_at_ms: row.get(1)?,
                ended_at_ms: row.get(2)?,
                title: row.get(3)?,
                target_language: row.get(4)?,
                line_count: row.get(5)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn transcript(&self, session_id: i64) -> Result<Vec<HistoryLine>, HistoryError> {
        let mut statement = self.connection.prepare(
            "SELECT start_ms, language, text, translation FROM lines WHERE session_id = ?1 ORDER BY segment_id",
        )?;
        let rows = statement.query_map([session_id], |row| {
            Ok(HistoryLine {
                start_ms: row.get(0)?,
                language: row.get(1)?,
                text: row.get(2)?,
                translation: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn delete(&self, session_id: i64) -> Result<bool, HistoryError> {
        Ok(self.connection.execute("DELETE FROM sessions WHERE id = ?1", [session_id])? > 0)
    }

    pub fn exists(&self, session_id: i64) -> Result<bool, HistoryError> {
        let found =
            self.connection.query_row("SELECT 1 FROM sessions WHERE id = ?1", [session_id], |_| Ok(())).optional()?;
        Ok(found.is_some())
    }
}

#[cfg(test)]
mod tests {
    use parlance_domain::{LanguageCode, SegmentId, Stability};

    use super::*;

    fn segment(id: u64, text: &str) -> TranscriptSegment {
        TranscriptSegment {
            id: SegmentId(id),
            revision: 0,
            stable_text: text.into(),
            unstable_text: String::new(),
            language: Some(LanguageCode::new("en").unwrap()),
            start_ms: id * 1000,
            end_ms: id * 1000 + 500,
            stability: Stability::Final,
        }
    }

    fn new_session(store: &HistoryStore) -> i64 {
        store
            .create_session(&NewSession { started_at_ms: 1, target_language: "es".into(), title: "Standup".into() })
            .unwrap()
    }

    #[test]
    fn records_lines_with_translations_in_order() {
        let store = HistoryStore::in_memory().unwrap();
        let id = new_session(&store);
        store.save_segment(id, &segment(2, "second")).unwrap();
        store.save_segment(id, &segment(1, "first")).unwrap();
        store.save_segment(id, &segment(1, "first, revised")).unwrap();
        let translation = Translation {
            segment_id: SegmentId(1),
            source_revision: 0,
            target_language: LanguageCode::new("es").unwrap(),
            text: "primero".into(),
            is_final: true,
        };
        store.save_translation(id, &translation).unwrap();
        store.end_session(id, 99).unwrap();

        let lines = store.transcript(id).unwrap();
        assert_eq!(lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), ["first, revised", "second"]);
        assert_eq!(lines[0].translation.as_deref(), Some("primero"));
        let entries = store.list(10).unwrap();
        assert_eq!((entries[0].line_count, entries[0].ended_at_ms), (2, Some(99)));
    }

    #[test]
    fn deleting_a_session_cascades_to_lines() {
        let store = HistoryStore::in_memory().unwrap();
        let id = new_session(&store);
        store.save_segment(id, &segment(1, "x")).unwrap();
        assert!(store.delete(id).unwrap());
        assert!(!store.exists(id).unwrap());
        assert!(store.transcript(id).unwrap().is_empty());
    }

    #[test]
    fn reopening_keeps_data_and_does_not_rerun_migrations() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.db");
        let id = {
            let store = HistoryStore::open(&path).unwrap();
            new_session(&store)
        };
        let store = HistoryStore::open(&path).unwrap();
        assert!(store.exists(id).unwrap());
    }
}
