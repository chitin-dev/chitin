#![forbid(unsafe_code)]
//! Session state and execution routing for Chitin's built-in command shell.
//!
//! This crate is independent of GPUI. It parses one command line at a time,
//! adapts terminal byte streams, retains navigation and execution history, forwards
//! portable commands to `chitin-command::execution`, and returns frontend commands
//! to the host application for execution.
//!
//! Rendering context is local to each shell session. `panel list` asks the host
//! for open molecular views, `panel enter <ID>` selects one, and `panel leave`
//! clears the selection without closing it. The host reconciles closed views
//! and updated titles; submissions retain their original target for audit.

mod event;
mod grammar;
mod panel;
mod session;
mod terminal;

pub use event::{ShellEvent, ShellEventSink};
pub use grammar::{
  BuiltinCommandLine, BuiltinShellGrammar, CommandLineParseError, ShellBuiltin, complete_builtin_shell_line,
  parse_builtin_command_line,
};
pub use panel::{RenderingPanel, RenderingPanelCommand};
pub use session::{
  BuiltinShell, BuiltinShellError, BuiltinShellSnapshot, ShellActiveCommand, ShellBuiltinEffect, ShellCommandId,
  ShellCommandTarget, ShellExecutionRecord, ShellExecutionResult, ShellExecutionStatus, ShellInvocationSource,
  ShellLineSubmission, ShellSubmission, ShellTranscriptContent, ShellTranscriptEntry,
};
pub use terminal::{BuiltinTerminalEvent, BuiltinTerminalProgram};
