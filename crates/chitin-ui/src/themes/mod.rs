//! Theme data structures and built-in Chitin palettes.
//!
//! The public theme type groups semantic UI colors. Built-in themes provide
//! sensible defaults, while applications can construct their own `UIThemes`
//! values for custom styling.

/// Built-in theme palettes.
pub mod builtins;
mod ui;

pub use ui::{UIAccentColors, UIBackgroundColors, UIBorderColors, UITextColors, UIThemes};
