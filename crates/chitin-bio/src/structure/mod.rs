//! Indexed molecular structure data and PDB parsing.

mod error;
mod mmcif;
mod model;
mod pdb;

pub use error::{Diagnostic, DiagnosticSeverity, MmcifParseError, PdbParseError, StructureParseResult};
pub use mmcif::MmcifParser;
pub use mmcif::cif;
pub use model::{
  AnnotationSource, AssemblyGeneration, AssemblyMetadata, Atom, AtomId, BiologicalAssembly, Bond, BondOrder,
  BondSource, Chain, ChainId, CoordinateSet, CoordinateSetId, Element, MissingPolymerResidue, Model, ModelId,
  PolymerEntity, PolymerSequenceResidue, PolymerType, Residue, ResidueId, ResidueKind, SecondaryRange,
  SecondaryStructure, Structure, StructureMetadata, StructureOperation, Symmetry, UnitCell,
};
pub use pdb::PdbParser;
