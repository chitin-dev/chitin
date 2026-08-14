use std::io;

/// Severity assigned to a recoverable parser diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
  /// The input was accepted, but a record or field could not be interpreted.
  Warning,
  /// Additional information useful to callers inspecting a parse.
  Info,
}

/// A recoverable issue encountered while reading a PDB stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
  /// Stable machine-readable diagnostic code.
  pub code: &'static str,
  /// One-based source line, when the issue belongs to a record.
  pub line: usize,
  /// Severity of the diagnostic.
  pub severity: DiagnosticSeverity,
  /// Human-readable explanation.
  pub message: String,
}

/// A fatal error that prevents a valid structure snapshot from being built.
#[derive(Debug, thiserror::Error)]
pub enum PdbParseError {
  /// The input stream could not be read.
  #[error("failed to read PDB input: {0}")]
  Io(#[from] io::Error),
  /// A PDB record was not valid UTF-8.
  #[error("PDB line {line} is not valid UTF-8")]
  InvalidUtf8 {
    /// One-based source line.
    line: usize,
  },
  /// A required fixed-column field could not be parsed.
  #[error("PDB line {line}: invalid {field}: {value:?}")]
  InvalidField {
    /// One-based source line.
    line: usize,
    /// Name of the failing field.
    field: &'static str,
    /// Original field text.
    value: String,
  },
  /// A record violates the structure builder's indexing invariant.
  #[error("PDB line {line}: {message}")]
  InvalidStructure {
    /// One-based source line.
    line: usize,
    /// Explanation of the violated invariant.
    message: String,
  },
}

/// A fatal error that prevents a valid mmCIF structure snapshot from being built.
#[derive(Debug, thiserror::Error)]
pub enum MmcifParseError {
  /// The input stream could not be read.
  #[error("failed to read mmCIF input: {0}")]
  Io(#[from] io::Error),
  /// The input bytes are not valid UTF-8.
  #[error("mmCIF input is not valid UTF-8")]
  InvalidUtf8,
  /// Tokenization or loop structure is invalid.
  #[error("mmCIF line {line}: invalid token stream: {message}")]
  InvalidToken {
    /// One-based source line where tokenization failed.
    line: usize,
    /// Explanation of the tokenization failure.
    message: String,
  },
  /// A required atom-site field is absent or malformed.
  #[error("mmCIF atom_site row {row}: invalid {field}: {value:?}")]
  InvalidField {
    /// One-based atom-site row.
    row: usize,
    /// Name of the failing field.
    field: &'static str,
    /// Original field text.
    value: String,
  },
  /// The shared structure builder rejected a semantic event.
  #[error("mmCIF structure error: {0}")]
  InvalidStructure(String),
}

/// The result of a successful PDB parse, including recoverable diagnostics.
#[derive(Debug, Clone, PartialEq)]
pub struct PdbParseResult {
  /// Immutable structure data assembled from the input.
  pub structure: crate::structure::Structure,
  /// Non-fatal issues encountered while reading the input.
  pub diagnostics: Vec<Diagnostic>,
}

/// The result shape shared by PDB and mmCIF readers.
pub type MmcifParseResult = PdbParseResult;
