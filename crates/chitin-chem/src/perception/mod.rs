//! Input facts remain authoritative; analyses return detached, noncached results.
//!
//! The current valence model is deliberately restricted, not a general chemical
//! sanitization pipeline. Missing facts, unsupported chemistry, and supported
//! but invalid valence have distinct outcomes. A report belongs to its input
//! snapshot and must be recomputed after edits; successful analysis never
//! installs inferred H or other derived properties back into atomic input facts.
//!
//! Ring analysis is instead graph-theoretic: bridges, articulation atoms, and
//! biconnected edge blocks. It accepts unknown chemical facts and assigns neither
//! aromaticity nor an SSSR/all-rings interpretation to a block's cycle rank.

mod kekule;
mod rings;
mod valence;

pub use kekule::{KekuleError, KekuleOptions, kekulize};
pub use rings::{RingBlock, RingTopology, analyze_rings};

pub use valence::{
  AtomValence, AtomValenceAnalysis, ValenceDiagnostic, ValenceModel, ValenceReport, ValenceStatus, analyze_valence,
};
