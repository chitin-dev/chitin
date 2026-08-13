//! RCSB provider error types.

use crate::{DecodeError, TransportError};
use std::path::PathBuf;

/// Validation error for an RCSB PDB identifier.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum PdbIdError {
  /// PDB identifiers must be exactly four ASCII alphanumeric characters.
  #[error("PDB ID must be 4 characters, got {actual}")]
  InvalidLength {
    /// Number of characters found in the input.
    actual: usize,
  },
  /// PDB identifiers may only contain ASCII letters and digits.
  #[error("PDB ID contains invalid character '{character}'")]
  InvalidCharacter {
    /// Invalid character found in the input.
    character: char,
  },
}

/// RCSB provider error.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum RcsbError {
  /// Invalid PDB identifier.
  #[error(transparent)]
  InvalidPdbId(#[from] PdbIdError),
  /// RCSB returned 404 for the entry.
  #[error("RCSB entry not found: {id}")]
  EntryNotFound {
    /// Requested PDB identifier.
    id: String,
  },
  /// RCSB returned 429 for the request.
  #[error("RCSB rate limited request")]
  RateLimited,
  /// Requested structure format is not supported by this provider.
  #[error("unsupported RCSB structure format")]
  UnsupportedStructureFormat,
  /// RCSB response was incompatible with the expected DTO.
  #[error("malformed RCSB response: {message}")]
  MalformedResponse {
    /// Sanitized parse or validation message.
    message: String,
  },
  /// Shared transport failed before a usable provider response was available.
  #[error(transparent)]
  Transport(#[from] TransportError),
  /// Provider response could not be decoded.
  #[error(transparent)]
  Decode(#[from] DecodeError),
}

/// Failure while downloading an RCSB artifact to a local file.
#[derive(Debug, thiserror::Error)]
pub enum RcsbDownloadError {
  /// The provider request failed.
  #[error(transparent)]
  Provider(#[from] RcsbError),
  /// The destination directory could not be created.
  #[error("failed to create '{path}': {source}")]
  CreateDirectory { path: PathBuf, source: std::io::Error },
  /// The downloaded bytes could not be written.
  #[error("failed to write '{path}': {source}")]
  WriteFile { path: PathBuf, source: std::io::Error },
}
