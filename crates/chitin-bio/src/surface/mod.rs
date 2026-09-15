//! Renderer-neutral molecular-surface algorithms and intermediate data.
//!
//! Surface construction is independent from structure parsing and rendering.
//! Each algorithm lives in its own submodule; the sampled-grid rolling-probe
//! SES implementation is currently available through [`ses`].

pub mod ses;

pub use ses::{
  MolecularSurfaceArtifact, MolecularSurfaceParameterError, MolecularSurfaceRequest, MolecularSurfaceTrace,
  ScalarFieldGrid, SesDomainTrace, SesGridBudget, SesParameters, SurfaceAtomScope, SurfaceDomainArtifact, SurfaceMesh,
  SurfacePartition, generate_molecular_surface, trace_molecular_surface,
};
#[cfg(feature = "surface-profiling")]
pub use ses::{MolecularSurfaceProfile, MolecularSurfaceTimings, profile_molecular_surface};
