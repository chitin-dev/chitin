//! Input facts remain authoritative; analyses return detached, noncached results.
//!
//! The current valence model is deliberately restricted, not a general chemical
//! sanitization pipeline. Missing facts, unsupported chemistry, and supported
//! but invalid valence have distinct outcomes. A report belongs to its input
//! snapshot and must be recomputed after edits; successful analysis never
//! installs inferred H or other derived properties back into atomic input facts.

mod valence;

pub use valence::{
  AtomValence, AtomValenceAnalysis, ValenceDiagnostic, ValenceModel, ValenceReport, ValenceStatus, analyze_valence,
};
