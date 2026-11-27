//! Conversation intelligence for the reply copilot.
//!
//! Final segments feed a rolling [`ConversationContext`]; when one reads as a
//! question worth answering, [`CopilotWorker`] streams a suggested reply in the
//! speaker's language and then translates it into the user's language.

mod context;
mod detector;
mod policy;
mod prompt;
mod service;
mod worker;
pub use service::{ActiveProfile, CopilotError, CopilotPreferences, CopilotService};

pub use context::ConversationContext;
pub use detector::is_question;
pub use policy::{SuggestionPolicy, should_suggest};
pub use worker::CopilotWorker;
