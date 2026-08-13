//! Errors produced while parsing and executing CLI workflows.

/// Error returned by the Chitin CLI command handlers.
#[derive(Debug, thiserror::Error)]
pub(crate) enum CliError {
  /// The supplied PDB identifier failed provider validation.
  #[error("invalid PDB ID: {0}")]
  InvalidPdbId(#[from] chitin_databases::providers::rcsb::PdbIdError),
  /// The RCSB provider or local persistence step failed.
  #[error("RCSB download failed: {0}")]
  Rcsb(#[from] chitin_databases::providers::rcsb::RcsbDownloadError),
  /// No platform home directory variable was available.
  #[error("could not determine the home directory; set HOME or USERPROFILE")]
  HomeDirectory,
  /// A command was not implemented by this CLI entry point.
  #[error("unsupported CLI command: {0}")]
  UnsupportedCommand(&'static str),
}
