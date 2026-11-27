//! Wire contracts. Every type here is exported to TypeScript by `cargo test`
//! (see `TS_RS_EXPORT_DIR` in `.cargo/config.toml`), so the frontend never
//! hand-writes a contract that could drift from Rust.

pub mod app;
pub mod copilot;
pub mod error;
pub mod events;
pub mod models;
pub mod native_messaging;
pub mod settings;

pub use app::{AppInfo, CaptureSupport};
pub use copilot::{AskCopilotRequest, ProjectProfiles, SaveProjectProfileRequest};
pub use error::{ErrorCode, IpcError};
pub use events::{Notice, NoticeLevel, SessionEvent, StartSessionRequest};
pub use models::{ModelDescriptor, ModelKind, ModelProgress, ModelStatus};
pub use settings::{AppSettings, Theme};
