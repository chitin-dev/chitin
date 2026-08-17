//! Indexed molecular structure data and PDB parsing.

mod error;
mod mmcif;
mod model;
mod pdb;

pub use error::{Diagnostic, DiagnosticSeverity, MmcifParseError, PdbParseError, StructureParseResult};
pub use mmcif::MmcifParser;
pub use mmcif::cif;
pub use model::{
  AnnotationSource, Atom, AtomId, Bond, BondOrder, BondSource, Chain, ChainId, CoordinateSet, CoordinateSetId, Element,
  MissingPolymerResidue, Model, ModelId, PolymerEntity, PolymerSequenceResidue, PolymerType, Residue, ResidueId,
  ResidueKind, SecondaryRange, SecondaryStructure, Structure, StructureMetadata, Symmetry, UnitCell,
};
pub use pdb::PdbParser;
