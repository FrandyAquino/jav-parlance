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
}

impl std::fmt::Debug for HistoryRecorder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HistoryRecorder").finish_non_exhaustive()
    }
}

impl HistoryRecorder {
    pub fn start(database: PathBuf, target_language: String) -> Self {
        let (tx, rx) = mpsc::channel::<Write>();
        std::thread::Builder::new()
            .name("parlance-history".into())
            .spawn(move || {
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
            })
            .map_or_else(|e| tracing::error!(%e, "cannot start history writer"), drop);
        Self { writes: Mutex::new(Some(tx)) }
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
