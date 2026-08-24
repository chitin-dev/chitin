//! Chemistry algorithms derived from parsed molecular structure data.
//!
//! Parsers preserve source facts in [`crate::structure::Structure`]. This
//! module computes derived chemical relationships without modifying that
//! source topology, so callers can distinguish file-provided data from
//! heuristic results.

mod bond_inference;

pub use bond_inference::{BondInferenceConfig, BondInferenceError, InferredBond, infer_bonds};
