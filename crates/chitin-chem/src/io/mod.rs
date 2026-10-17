//! Chemical format boundaries; reading does not silently sanitize chemistry.
//!
//! Each codec owns its grammar and source provenance. Shared molecular facts
//! belong to the domain model, not to a universal format-specific property bag.

pub mod smiles;
