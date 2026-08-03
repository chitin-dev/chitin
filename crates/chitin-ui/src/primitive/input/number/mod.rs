//! A reusable single-line numeric input.
//!
//! [`NumberInputState`] owns the editing draft, parsed value, committed value,
//! and numeric interaction. Consumers subscribe to [`NumberInputEvent`] rather
//! than interpreting the nested text-input events.

mod event;
mod render;
mod state;

pub use event::NumberInputEvent;
pub use render::{NumberInput, NumberInputSize};
pub use state::NumberInputState;
