//! Errors produced while parsing and executing CLI workflows.

/// Error returned by the Chitin CLI command handlers.
#[derive(Debug, thiserror::Error)]
pub(crate) enum CliError {
  /// Shared command-line arguments could not be converted into a typed command.
  #[error(transparent)]
  CommandLine(#[from] chitin_command::PortableCommandLineError),
  /// A portable typed command could not be executed.
  #[error(transparent)]
  CommandExecution(#[from] chitin_command::CommandExecutionError),
  /// No platform home directory variable was available.
  #[error("could not determine the home directory; set HOME or USERPROFILE")]
  HomeDirectory,
  /// The process working directory could not be read.
  #[error("could not determine the current working directory: {0}")]
  WorkingDirectory(std::io::Error),
  /// Standard input could not be read for a structure command.
  #[error("failed to read structure data from standard input: {0}")]
  StandardInput(std::io::Error),
}
