//! A reusable button that renders arbitrary GPUI child elements.
//!
//! [`ButtonState`] owns interaction state. Consumers subscribe to
//! [`ButtonEvent`] values rather than attaching component-specific callbacks.

mod event;
mod render;
mod state;

pub use event::ButtonEvent;
pub use render::{Button, ButtonSize, ButtonStyle, ButtonVariant};
pub use state::ButtonState;
