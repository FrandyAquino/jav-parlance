#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("capture is not supported: {0}")]
    Unsupported(String),
    #[error("audio source `{0}` was not found")]
    SourceNotFound(String),
    #[error("permission to capture audio was denied: {0}")]
    PermissionDenied(String),
    #[error("audio backend error: {0}")]
    Backend(String),
}
