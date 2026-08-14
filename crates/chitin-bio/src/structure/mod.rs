//! Indexed molecular structure data and PDB parsing.

mod error;
mod model;
mod pdb;

pub use error::{Diagnostic, DiagnosticSeverity, PdbParseError, PdbParseResult};
pub use model::{
  AnnotationSource, Atom, AtomId, AtomName, Bond, BondOrder, BondSource, Chain, ChainId, CoordinateSet,
  CoordinateSetId, Element, Model, ModelId, Residue, ResidueId, ResidueKind, SecondaryRange, SecondaryStructure,
  Structure, StructureMetadata,
};
pub use pdb::PdbParser;
