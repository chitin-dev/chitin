//! Errors produced while parsing and executing CLI workflows.

use chitin_databases::providers::rcsb::PdbIdListError;
use std::path::PathBuf;

/// Error returned by the Chitin CLI command handlers.
#[derive(Debug, thiserror::Error)]
pub(crate) enum CliError {
  /// A comma-separated identifier list contains an invalid element.
  #[error("{0}")]
  InvalidPdbIdList(#[from] PdbIdListError),
  /// A portable typed command could not be executed.
  #[error(transparent)]
  CommandExecution(#[from] chitin_command_runtime::CommandExecutionError),
  /// No platform home directory variable was available.
  #[error("could not determine the home directory; set HOME or USERPROFILE")]
  HomeDirectory,
  /// The process working directory could not be read.
  #[error("could not determine the current working directory: {0}")]
  WorkingDirectory(std::io::Error),
  /// Standard input could not be read for a structure command.
  #[error("failed to read structure data from standard input: {0}")]
  StandardInput(std::io::Error),
  /// The parsed structure violated a model invariant.
  #[error("structure validation failed for `{path}`: {message}")]
  StructureValidation { path: PathBuf, message: String },
  /// JSON output could not be serialized.
  #[error("failed to serialize structure output: {0}")]
  Json(#[from] serde_json::Error),
}
