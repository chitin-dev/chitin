//! Indexed molecular structure data and PDB parsing.

mod error;
mod mmcif;
mod model;
mod pdb;

pub use error::{Diagnostic, DiagnosticSeverity, MmcifParseError, MmcifParseResult, PdbParseError, PdbParseResult};
pub use mmcif::MmcifParser;
pub use mmcif::cif;
pub use model::{
  AnnotationSource, Atom, AtomId, AtomName, Bond, BondOrder, BondSource, Chain, ChainId, CoordinateSet,
  CoordinateSetId, Element, MissingPolymerResidue, Model, ModelId, PolymerEntity, PolymerSequenceResidue, PolymerType,
  Residue, ResidueId, ResidueKind, SecondaryRange, SecondaryStructure, Structure, StructureMetadata,
};
pub use pdb::PdbParser;
