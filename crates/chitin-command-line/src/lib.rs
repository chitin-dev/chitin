#![forbid(unsafe_code)]
//! Shared command-line grammar for Chitin's textual frontends.
//!
//! This crate owns the portable `db` and `structure` command fragments used by
//! both the process CLI and the built-in shell. Each frontend composes these
//! fragments into its own root grammar, so frontend-specific commands do not
//! need to appear in every textual interface.

mod portable;
mod shell;

pub use portable::{PortableCommandArgs, PortableCommandLineError};
pub use shell::{
  BuiltinCommandLine, BuiltinShellGrammar, CommandLineParseError, ShellBuiltin, complete_builtin_shell_line,
  parse_builtin_command_line,
};
