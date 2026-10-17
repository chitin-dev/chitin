//! Chemical identities and editable topology shared by future Chitin features.
//!
//! This crate has no dependency on GPUI, biological hierarchies, drawing
//! documents, coordinates, or quantum chemistry engines. Unknown chemical facts
//! remain unknown: adding a bond does not infer valence, hydrogens, or aromaticity.
//! [`MolecularGraph`] enforces topology, not chemical plausibility, so incomplete
//! structures can remain editable without being mistaken for validated molecules.
//!
//! # Examples
//!
//! ```
//! use chitin_chem::{Atom, BondOrder, Element, MolecularGraph};
//!
//! let mut graph = MolecularGraph::new();
//! let carbon = graph.add_atom(Atom::new(Element::Carbon))?;
//! let oxygen = graph.add_atom(Atom::new(Element::Oxygen))?;
//! let bond = graph.add_bond(carbon, oxygen, BondOrder::Single)?;
//! assert_eq!(graph.neighbors(carbon)?.next(), Some((oxygen, bond)));
//! assert_eq!(graph.atom(carbon).and_then(Atom::formal_charge), None);
//! # Ok::<(), chitin_chem::GraphError>(())
//! ```

#![deny(missing_docs)]

mod atom;
mod bond;
mod element;
mod graph;
mod hydrogen;
/// Format-specific readers and source diagnostics, independent of perception.
pub mod io;
/// Read-only chemistry analysis with explicit model boundaries and diagnostics.
pub mod perception;

pub use atom::Atom;
pub use bond::{Bond, BondOrder};
pub use element::{Element, ElementError, Isotope, IsotopeError};
pub use graph::{AtomId, BondId, GraphError, MolecularGraph};
pub use hydrogen::{HydrogenCounts, ImplicitHydrogenPolicy};
