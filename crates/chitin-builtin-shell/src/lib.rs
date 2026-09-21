#![forbid(unsafe_code)]
//! Session state and execution routing for Chitin's built-in command shell.
//!
//! This crate is independent of GPUI and terminal emulation. It parses one
//! command line at a time, retains navigation and execution history, forwards
//! portable commands to `chitin-command-runtime`, and returns frontend commands
//! to the host application for execution.

mod event;
mod session;

pub use event::{ShellEvent, ShellEventSink};
pub use session::{
  BuiltinShell, BuiltinShellError, BuiltinShellSnapshot, ShellActiveCommand, ShellBuiltinEffect, ShellCommandId,
  ShellCommandTarget, ShellExecutionRecord, ShellExecutionResult, ShellExecutionStatus, ShellInvocationSource,
  ShellLineSubmission, ShellSubmission, ShellTranscriptContent, ShellTranscriptEntry,
};
