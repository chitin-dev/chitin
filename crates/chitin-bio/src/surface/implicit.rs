//! Sampled scalar-field molecular surfaces for interactive rendering.
//!
//! This backend produces a renderer-neutral triangle mesh through two distance
//! fields and marching tetrahedra. Its geometry is intentionally approximate:
//! mesh triangle areas are not exposed as scientific SAS/SES measurements.

pub use super::ses::{
  MolecularSurfaceArtifact, MolecularSurfaceParameterError, MolecularSurfaceRequest, MolecularSurfaceTrace,
  ScalarFieldGrid, SesDomainTrace, SesGridBudget, SesParameters, SurfaceAtomScope, SurfaceDomainArtifact,
  SurfaceGeometrySource, SurfaceMesh, SurfacePartition, generate_implicit_surface, trace_molecular_surface,
};
#[cfg(feature = "surface-profiling")]
pub use super::ses::{MolecularSurfaceProfile, MolecularSurfaceTimings, profile_molecular_surface};
