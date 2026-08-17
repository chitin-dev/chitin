//! Errors produced while parsing and executing CLI workflows.

use chitin_databases::providers::rcsb::{PdbIdError, PdbIdListError, RcsbDownloadError};
use std::path::PathBuf;

/// Error returned by the Chitin CLI command handlers.
#[derive(Debug, thiserror::Error)]
pub(crate) enum CliError {
  /// A comma-separated identifier list contains an invalid element.
  #[error("{0}")]
  InvalidPdbIdList(#[from] PdbIdListError),
  /// The supplied PDB identifier failed provider validation.
  #[error("invalid PDB ID: {0}")]
  InvalidPdbId(#[from] PdbIdError),
  /// The RCSB provider or local persistence step failed.
  #[error("RCSB download failed: {0}")]
  Rcsb(#[from] RcsbDownloadError),
  /// No platform home directory variable was available.
  #[error("could not determine the home directory; set HOME or USERPROFILE")]
  HomeDirectory,
  /// Multiple downloads were given one explicit file path.
  #[error("--output must be a directory when downloading multiple PDB IDs: {0}")]
  MultipleOutputFile(std::path::PathBuf),
  /// A command was not implemented by this CLI entry point.
  #[error("unsupported CLI command: {0}")]
  UnsupportedCommand(&'static str),
  /// The structure input could not be read.
  #[error("failed to read structure `{path}`: {source}")]
  StructureRead { path: PathBuf, source: std::io::Error },
  /// The structure format could not be determined from the input.
  #[error("could not determine structure format for `{0}`; use --format pdb or --format mmcif")]
  StructureFormat(PathBuf),
  /// The structure parser rejected the input.
  #[error("failed to parse structure `{path}`: {message}")]
  StructureParse { path: PathBuf, message: String },
  /// The parsed structure violated a model invariant.
  #[error("structure validation failed for `{path}`: {message}")]
  StructureValidation { path: PathBuf, message: String },
  /// JSON output could not be serialized.
  #[error("failed to serialize structure output: {0}")]
  Json(#[from] serde_json::Error),
}
