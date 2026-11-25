//! Maps internal errors onto the stable `IpcError` contract.

use parlance_capture::CaptureError;
use parlance_models::DownloadError;
use parlance_protocol::{ErrorCode, IpcError};
use parlance_session::SessionError;

pub type CommandResult<T> = Result<T, IpcError>;

pub fn from_capture(error: CaptureError) -> IpcError {
    let code = match error {
        CaptureError::PermissionDenied(_) => ErrorCode::PermissionDenied,
        CaptureError::SourceNotFound(_) => ErrorCode::InvalidRequest,
        CaptureError::Unsupported(_) | CaptureError::Backend(_) => ErrorCode::CaptureUnavailable,
    };
    IpcError::new(code, error.to_string())
}

pub fn from_session(error: SessionError) -> IpcError {
    match error {
        SessionError::InvalidTransition(e) => IpcError::new(ErrorCode::InvalidState, e.to_string()),
        SessionError::Capture(e) => from_capture(e),
        SessionError::Speech(e) => IpcError::new(ErrorCode::InferenceFailed, e.to_string()),
        SessionError::Audio(e) => IpcError::new(ErrorCode::Internal, e.to_string()),
        SessionError::Internal(message) => IpcError::new(ErrorCode::Internal, message),
    }
}

pub fn from_download(error: DownloadError) -> IpcError {
    let code = match error {
        DownloadError::Network(_) => ErrorCode::Network,
        DownloadError::ChecksumMismatch { .. } | DownloadError::SizeMismatch { .. } => ErrorCode::ModelCorrupt,
        DownloadError::Io(_) | DownloadError::Extract(_) => ErrorCode::Storage,
        DownloadError::Cancelled => ErrorCode::Cancelled,
    };
    IpcError::new(code, error.to_string())
}

pub fn invalid(message: impl Into<String>) -> IpcError {
    IpcError::new(ErrorCode::InvalidRequest, message)
}
