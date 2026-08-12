//! A reusable single-choice selector input.
//!
//! [`SelectInputState`] owns selection, highlighted-option, open, and focus
//! state. Consumers subscribe to [`SelectInputEvent`] rather than attaching
//! raw pointer or keyboard handlers.

mod event;
mod render;
mod state;

pub use event::SelectInputEvent;
pub use render::{
  Select, SelectContent, SelectContentPosition, SelectGroup, SelectInputSize, SelectInputStyle, SelectInputVariant,
  SelectItem, SelectLabel, SelectSeparator, SelectTrigger, SelectValue,
};
pub use state::{SelectInputState, SelectOption};
