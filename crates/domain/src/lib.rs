//! Platform-independent domain model shared by every Parlance crate.
//!
//! This crate performs no I/O. Types that cross the UI boundary derive `TS`
//! so the frontend contract is generated, never hand-written.

pub mod audio;
pub mod copilot;
pub mod language;
pub mod session;
pub mod transcript;

pub use audio::{AudioLevel, SourceDescriptor, SourceId, SourceKind};
pub use copilot::{Suggestion, SuggestionId};
pub use language::{LanguageCode, LanguageError};
pub use session::{SessionCommand, SessionId, SessionState, TransitionError};
pub use transcript::{SegmentId, Stability, TranscriptSegment, Translation};
