#![forbid(unsafe_code)]
//! Typed commands, textual grammar, portable execution, and shared output.
//!
//! Chitin frontends construct the same typed commands, execute portable work
//! through [`execution`], and render canonical reports from [`output`]. The
//! crate deliberately contains no process-terminal, GPUI, or shell-session
//! state.

pub mod execution;
pub mod grammar;
pub mod model;
pub mod output;

pub use execution::{
  CommandEventSink, CommandExecutionContext, CommandExecutionError, CommandExecutionEvent, CommandExecutor,
  CommandMessage, CommandMessageLevel, CommandOutcome, CommandProgress, StructureInspection, StructureValidation,
  resolve_rcsb_download_paths,
};
pub use grammar::{PortableCommandArgs, PortableCommandLineError};
pub use model::*;
pub use output::{
  CommandOutputKind, CommandOutputLine, CommandOutputSpan, CommandOutputTone, CommandReport, CommandReportStatus,
  render_outcome,
};
