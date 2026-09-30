//! Monospace terminal content and a tail-following scroll viewport.
//!
//! This primitive has no shell or command semantics. Callers provide styled
//! lines and may attach one interactive tail element, such as a live prompt.

mod emulator;
mod model;
mod render;
mod state;

pub use emulator::{TerminalEmulator, TerminalEmulatorEvent, TerminalEmulatorState};
pub use model::{TerminalLine, TerminalSpan, TerminalTone};
pub use render::TerminalViewport;
pub use state::TerminalViewportState;
