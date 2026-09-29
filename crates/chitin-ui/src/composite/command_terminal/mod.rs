//! Structured command terminal built from terminal and text-input primitives.

mod model;
mod render;

pub use model::{CommandTerminalBlock, CommandTerminalId, CommandTerminalState, CommandTerminalStatus};
pub use render::CommandTerminal;
