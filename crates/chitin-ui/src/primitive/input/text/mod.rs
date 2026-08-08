//! A reusable single-line text input.
//!
//! [`TextInputState`] owns editing and focus state. Consumers subscribe to its
//! [`TextInputEvent`] values rather than attaching component-specific callbacks.

mod event;
mod render;
mod state;

pub use event::{TextInputEvent, TextSelection};
pub(crate) use render::TextInputColors;
pub use render::{TextInput, TextInputSize, TextInputStyle, TextInputVariant};
pub use state::TextInputState;
