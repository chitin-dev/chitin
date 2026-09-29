#![forbid(unsafe_code)]
//! Frontend-independent terminal profiles, PTY and in-process sessions, transcript state, and VT snapshots.

mod model;
mod session;
mod shell;
mod size;
mod snapshot;

pub use model::{
  TerminalBlock, TerminalBlockId, TerminalBlockStatus, TerminalBuffer, TerminalLine, TerminalProfile, TerminalSpan,
  TerminalTone,
};
pub use session::{
  TerminalEvent, TerminalProgram, TerminalProgramInput, TerminalProgramOutput, TerminalScroll, TerminalScrollState,
  TerminalSession, TerminalSessionError,
};
pub use shell::{ShellCatalog, ShellDefinition};
pub use size::TerminalSize;
pub use snapshot::{TerminalCell, TerminalCellAttributes, TerminalColor, TerminalNamedColor, TerminalSnapshot};
