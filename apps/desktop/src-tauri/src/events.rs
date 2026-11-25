use std::sync::{Arc, Mutex};

use parlance_protocol::SessionEvent;
use parlance_session::EventSink;
use tauri::ipc::Channel;

/// Forwards session events to the webview's subscription channel. Events
/// emitted while no window is subscribed are dropped: the UI re-syncs state
/// on (re)subscribe, and raw audio never crosses this boundary.
#[derive(Default)]
pub struct EventHub {
    channel: Mutex<Option<Channel<SessionEvent>>>,
}

impl EventHub {
    pub fn subscribe(&self, channel: Channel<SessionEvent>) {
        *self.lock() = Some(channel);
    }

    pub fn emit(&self, event: SessionEvent) {
        let mut guard = self.lock();
        if let Some(channel) = guard.as_ref()
            && channel.send(event).is_err()
        {
            *guard = None;
        }
    }

    pub fn sink(self: &Arc<Self>) -> EventSink {
        let hub = Arc::clone(self);
        Arc::new(move |event| hub.emit(event))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Channel<SessionEvent>>> {
        self.channel.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}
