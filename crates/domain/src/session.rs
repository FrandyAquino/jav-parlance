use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export, type = "number")]
pub struct SessionId(pub u64);

/// Lifecycle of a capture session.
///
/// ```text
///  Idle ──Start──▶ Starting ──MarkRunning──▶ Running ◀──Resume── Paused
///   ▲                 │                       │  └──Pause──────────▲
///   │                 └──Fail──▶ Failed ◀─Fail┤
///   └──MarkStopped── Stopping ◀────Stop───────┘ (also from Paused/Starting)
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum SessionState {
    Idle,
    Starting,
    Running,
    Paused,
    Stopping,
    Failed { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionCommand {
    Start,
    MarkRunning,
    Pause,
    Resume,
    Stop,
    MarkStopped,
    Fail(String),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("cannot apply {command:?} while {state:?}")]
pub struct TransitionError {
    pub state: SessionState,
    pub command: SessionCommand,
}

impl SessionState {
    pub fn apply(&self, command: SessionCommand) -> Result<SessionState, TransitionError> {
        use SessionCommand as C;
        use SessionState as S;

        let next = match (self, &command) {
            (S::Idle | S::Failed { .. }, C::Start) => S::Starting,
            (S::Starting, C::MarkRunning) | (S::Paused, C::Resume) => S::Running,
            (S::Running, C::Pause) => S::Paused,
            (S::Starting | S::Running | S::Paused, C::Stop) => S::Stopping,
            (S::Stopping, C::MarkStopped) => S::Idle,
            (S::Starting | S::Running | S::Paused | S::Stopping, C::Fail(reason)) => {
                S::Failed { reason: reason.clone() }
            }
            _ => return Err(TransitionError { state: self.clone(), command }),
        };
        Ok(next)
    }

    /// Whether audio should currently flow through the pipeline.
    pub fn is_capturing(&self) -> bool {
        matches!(self, SessionState::Running)
    }

    pub fn is_active(&self) -> bool {
        matches!(self, SessionState::Starting | SessionState::Running | SessionState::Paused)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use SessionCommand as C;

    fn run(commands: impl IntoIterator<Item = SessionCommand>) -> Result<SessionState, TransitionError> {
        commands.into_iter().try_fold(SessionState::Idle, |state, cmd| state.apply(cmd))
    }

    #[test]
    fn full_lifecycle_returns_to_idle() {
        let end = run([C::Start, C::MarkRunning, C::Pause, C::Resume, C::Stop, C::MarkStopped]);
        assert_eq!(end, Ok(SessionState::Idle));
    }

    #[test]
    fn failed_session_can_restart() {
        let failed = run([C::Start, C::Fail("device lost".into())]).unwrap();
        assert!(matches!(failed, SessionState::Failed { .. }));
        assert_eq!(failed.apply(C::Start), Ok(SessionState::Starting));
    }

    #[test]
    fn rejects_invalid_transitions() {
        assert!(SessionState::Idle.apply(C::Stop).is_err());
        assert!(SessionState::Idle.apply(C::Pause).is_err());
        assert!(SessionState::Running.apply(C::Start).is_err());
        assert!(SessionState::Stopping.apply(C::Resume).is_err());
    }

    #[test]
    fn only_running_captures() {
        assert!(SessionState::Running.is_capturing());
        assert!(!SessionState::Paused.is_capturing());
        assert!(SessionState::Paused.is_active());
    }
}
