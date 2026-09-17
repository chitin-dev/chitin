//! Renderer-neutral molecular-surface algorithms and intermediate data.
//!
//! Surface construction is independent from structure parsing and rendering.
//! Rendering geometry and scientific surface measurements deliberately use
//! separate algorithms. [`implicit`] provides the sampled scalar-field mesh
//! used by the interactive renderer. [`msms`] defines the analytical
//! reduced-surface model used for SAS/SES measurements and optional analytical
//! tessellation.

mod atoms;
pub mod implicit;
pub mod msms;

pub use atoms::{SurfaceAtomScope, SurfacePartition};
pub use implicit::{
  MolecularSurfaceArtifact, MolecularSurfaceParameterError, MolecularSurfaceRequest, MolecularSurfaceTrace,
  ScalarFieldGrid, SesDomainTrace, SesGridBudget, SesParameters, SurfaceDomainArtifact, SurfaceGeometrySource,
  SurfaceMesh, generate_implicit_surface, trace_molecular_surface,
};
#[cfg(feature = "surface-profiling")]
pub use implicit::{MolecularSurfaceProfile, MolecularSurfaceTimings, profile_molecular_surface};
