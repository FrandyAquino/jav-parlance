use std::sync::{Arc, Mutex};

use parlance_protocol::SessionEvent;
use parlance_session::EventSink;
use tauri::ipc::Channel;

/// Forwards session events to the webview's subscription channel. Events
/// emitted while no window is subscribed are dropped: the UI re-syncs state
/// on (re)subscribe, and raw audio never crosses this boundary.
#[derive(Default)]
pub struct EventHub {
    subscription: Mutex<Subscription>,
}

#[derive(Default)]
struct Subscription {
    channel: Option<Channel<SessionEvent>>,
    generation: u64,
}

impl EventHub {
    pub fn subscribe(&self, channel: Channel<SessionEvent>) {
        self.lock().channel = Some(channel);
    }

    pub fn emit(&self, event: SessionEvent) {
        Self::send(&mut self.lock(), event);
    }

    fn send(subscription: &mut Subscription, event: SessionEvent) {
        if let Some(channel) = subscription.channel.as_ref()
            && channel.send(event).is_err()
        {
            subscription.channel = None;
        }
    }

    pub fn sink(self: &Arc<Self>) -> EventSink {
        let hub = Arc::clone(self);
        Arc::new(move |event| hub.emit(event))
    }

    pub fn session_sink(self: &Arc<Self>) -> EventSink {
        let generation = {
            let mut subscription = self.lock();
            subscription.generation = subscription.generation.wrapping_add(1);
            subscription.generation
        };
        let hub = Arc::clone(self);
        Arc::new(move |event| {
            let mut subscription = hub.lock();
            if subscription.generation == generation {
                Self::send(&mut subscription, event);
            }
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Subscription> {
        self.subscription.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parlance_domain::{LanguageCode, SegmentId, Translation};
    use parlance_protocol::Notice;
    use tauri::ipc::InvokeResponseBody;

    type Received = Arc<Mutex<Vec<SessionEvent>>>;

    fn subscribed(hub: &EventHub) -> Received {
        let received = Arc::new(Mutex::new(Vec::new()));
        let output = Arc::clone(&received);
        hub.subscribe(Channel::new(move |body| {
            let InvokeResponseBody::Json(json) = body else { panic!("expected JSON events") };
            output.lock().unwrap().push(serde_json::from_str(&json).unwrap());
            Ok(())
        }));
        received
    }

    fn translation(text: &str) -> SessionEvent {
        SessionEvent::Translation {
            translation: Translation {
                segment_id: SegmentId(1),
                source_revision: 1,
                target_language: LanguageCode::new("es").unwrap(),
                text: text.into(),
                is_final: true,
            },
        }
    }

    #[test]
    fn a_new_session_rejects_previous_results_with_the_same_segment_id() {
        let hub = Arc::new(EventHub::default());
        let received = subscribed(&hub);
        let old = hub.session_sink();
        old(translation("A final before next session"));
        let current = hub.session_sink();
        current(translation("B final"));
        old(translation("A late final"));
        assert_eq!(*received.lock().unwrap(), vec![translation("A final before next session"), translation("B final")]);
    }

    #[test]
    fn resubscription_preserves_session_filter_and_application_events() {
        let hub = Arc::new(EventHub::default());
        let old = hub.session_sink();
        let current = hub.session_sink();
        let received = subscribed(&hub);
        old(translation("stale"));
        current(translation("current"));
        hub.sink()(SessionEvent::Notice { notice: Notice::info("application event") });
        assert_eq!(
            *received.lock().unwrap(),
            vec![translation("current"), SessionEvent::Notice { notice: Notice::info("application event") }]
        );
    }
}
