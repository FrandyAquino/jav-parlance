use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::mpsc::{self, Sender};
use std::time::{SystemTime, UNIX_EPOCH};

use parlance_domain::{TranscriptSegment, Translation};
use parlance_session::SegmentListener;

use crate::store::{HistoryStore, NewSession};

enum Write {
    Segment(TranscriptSegment),
    Translation(Translation),
    End,
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

/// Persists one session's final lines and translations on its own thread.
pub struct HistoryRecorder {
    writes: Mutex<Option<Sender<Write>>>,
    completion: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl std::fmt::Debug for HistoryRecorder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HistoryRecorder").finish_non_exhaustive()
    }
}

impl HistoryRecorder {
    pub fn start(database: PathBuf, target_language: String) -> Self {
        let (tx, rx) = mpsc::channel::<Write>();
        let completion = std::thread::Builder::new().name("parlance-history".into()).spawn(move || {
            let result = (|| {
                let store = HistoryStore::open(&database)?;
                let title = format!("Meeting {}", now_ms());
                let id = store.create_session(&NewSession { started_at_ms: now_ms(), target_language, title })?;
                for write in rx {
                    match write {
                        Write::Segment(segment) => store.save_segment(id, &segment)?,
                        Write::Translation(translation) => store.save_translation(id, &translation)?,
                        Write::End => break,
                    }
                }
                store.end_session(id, now_ms())
            })();
            if let Err(error) = result {
                tracing::error!(%error, "meeting history writer stopped");
            }
        });
        let completion = match completion {
            Ok(thread) => Some(thread),
            Err(error) => {
                tracing::error!(%error, "cannot start history writer");
                None
            }
        };
        Self { writes: Mutex::new(Some(tx)), completion: Mutex::new(completion) }
    }

    /// Transfer the completion handle to the application so shutdown can
    /// join it off the real-time threads, after all listeners are released.
    pub fn take_completion(&self) -> Option<std::thread::JoinHandle<()>> {
        self.completion.lock().unwrap_or_else(|p| p.into_inner()).take()
    }

    fn send(&self, write: Write) {
        if let Some(sender) = self.writes.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
            let _ = sender.send(write);
        }
    }

    pub fn record_translation(&self, translation: &Translation) {
        if translation.is_final {
            self.send(Write::Translation(translation.clone()));
        }
    }
}

impl SegmentListener for HistoryRecorder {
    fn on_segment(&self, segment: &TranscriptSegment) {
        if segment.is_final() && !segment.stable_text.trim().is_empty() {
            self.send(Write::Segment(segment.clone()));
        }
    }

    /// Translations may still arrive after the transcript ends, so the
    /// writer stays open until the recorder itself is dropped.
    fn on_session_end(&self) {}
}

impl Drop for HistoryRecorder {
    fn drop(&mut self) {
        self.send(Write::End);
        self.writes.lock().unwrap_or_else(|p| p.into_inner()).take();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parlance_domain::{LanguageCode, SegmentId, Stability};

    #[test]
    fn completion_flushes_transcript_and_late_translation_before_exit() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("history.db");
        let recorder = HistoryRecorder::start(database.clone(), "es".into());
        let completion = recorder.take_completion().unwrap();
        assert!(recorder.take_completion().is_none());
        recorder.on_segment(&TranscriptSegment {
            id: SegmentId(1),
            revision: 1,
            stable_text: "Hello".into(),
            unstable_text: String::new(),
            language: Some(LanguageCode::new("en").unwrap()),
            start_ms: 0,
            end_ms: 1000,
            stability: Stability::Final,
        });
        recorder.on_session_end();
        recorder.record_translation(&Translation {
            segment_id: SegmentId(1),
            source_revision: 1,
            target_language: LanguageCode::new("es").unwrap(),
            text: "Hola".into(),
            is_final: true,
        });
        drop(recorder);
        completion.join().unwrap();

        let store = HistoryStore::open(&database).unwrap();
        let sessions = store.list(10).unwrap();
        assert_eq!(sessions.len(), 1);
        assert!(sessions[0].ended_at_ms.is_some());
        let lines = store.transcript(sessions[0].id).unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "Hello");
        assert_eq!(lines[0].translation.as_deref(), Some("Hola"));
    }
}
