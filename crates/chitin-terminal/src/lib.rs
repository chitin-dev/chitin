#![forbid(unsafe_code)]
//! Frontend-independent PTY sessions and VT terminal state.

mod session;
mod size;
mod snapshot;

pub use session::{TerminalEvent, TerminalSession, TerminalSessionError};
pub use size::TerminalSize;
pub use snapshot::{TerminalCell, TerminalCellAttributes, TerminalColor, TerminalNamedColor, TerminalSnapshot};
