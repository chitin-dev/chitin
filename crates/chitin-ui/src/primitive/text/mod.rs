//! A text element with optional clipboard support.
//!
//! [`TextState`] owns the displayed value and copyability. Consumers subscribe
//! to [`TextEvent`] values instead of attaching component-specific callbacks.

mod event;
mod render;
mod state;

pub use event::TextEvent;
pub use render::{Text, TextSize};
pub use state::TextState;
