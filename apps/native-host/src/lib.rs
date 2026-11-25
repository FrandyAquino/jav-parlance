//! Native messaging host that relays tab audio from the Parlance browser
//! extension to the desktop app's local bridge socket.
//!
//! Stdout is the native messaging channel, so diagnostics go to stderr only.

pub mod bridge;
pub mod install;
pub mod relay;

pub use bridge::{Bridge, BridgeUnavailable, TcpBridge};
pub use relay::{RelayError, RelayOutcome, run_relay};
