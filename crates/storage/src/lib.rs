//! Opt-in meeting history. Only text is stored (never audio), in a local
//! SQLite database. Writes happen on a dedicated thread so persistence can
//! never stall transcription.

mod profiles;
mod recorder;
mod store;

pub use profiles::{JsonProfileRepository, ProfileError, ProfileRepository};
pub use recorder::HistoryRecorder;
pub use store::{HistoryEntry, HistoryError, HistoryLine, HistoryStore, NewSession};
