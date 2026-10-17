//! Input facts remain authoritative; analyses return detached, noncached results.

mod valence;

pub use valence::{
  AtomValence, AtomValenceAnalysis, ValenceDiagnostic, ValenceModel, ValenceReport, ValenceStatus, analyze_valence,
};
