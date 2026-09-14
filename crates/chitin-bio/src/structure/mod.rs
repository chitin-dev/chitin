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
mod surface;

pub use error::{Diagnostic, DiagnosticSeverity, MmcifParseError, PdbParseError, StructureParseResult};
pub use mmcif::MmcifParser;
pub use mmcif::cif;
pub use model::{
  AnnotationSource, AssemblyGeneration, AssemblyMetadata, Atom, AtomId, BiologicalAssembly, Bond, BondOrder,
  BondSource, Chain, ChainId, CoordinateSet, CoordinateSetId, Element, MissingPolymerResidue, Model, ModelId,
  PolymerEntity, PolymerSequenceResidue, PolymerType, Residue, ResidueId, ResidueKind, SecondaryRange,
  SecondaryStructure, Structure, StructureInvariantError, StructureMetadata, StructureOperation, Symmetry, UnitCell,
};
pub use pdb::PdbParser;
pub use scene::{
  AtomSceneInstance, BondSceneInstance, ElementCategory, PolymerTrace, PolymerTraceKind, PolymerTracePoint,
  SceneBounds, StructureScene, StructureSceneError, StructureSceneOptions,
};
pub use surface::{
  MolecularSurfaceArtifact, MolecularSurfaceParameterError, MolecularSurfaceRequest, SesParameters, SurfaceAtomScope,
  SurfaceDomainArtifact, SurfaceMesh, SurfacePartition, generate_molecular_surface,
};
#[cfg(feature = "surface-profiling")]
pub use surface::{MolecularSurfaceProfile, MolecularSurfaceTimings, profile_molecular_surface};
