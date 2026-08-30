//! Molecular structure parsing, semantic projection, and indexed model data.
//!
//! Each source format first parses its own syntax and then projects supported
//! semantics into a shared internal representation. The common builder is the
//! only layer that assigns dense IDs and creates [`Structure`], keeping file
//! syntax independent from macromolecular topology generation.

pub(crate) mod builder;
mod error;
mod mmcif;
mod model;
mod pdb;
pub(crate) mod projection;
mod scene;

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
pub use scene::{
  AtomSceneInstance, BondSceneInstance, ElementCategory, SceneBounds, StructureScene, StructureSceneError,
  StructureSceneOptions,
};
