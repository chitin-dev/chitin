//! Requests, parameters, artifacts, grids, and diagnostic trace types.

use thiserror::Error;

use crate::structure::ChainId;

use super::{
  DEFAULT_SES_GRID_SPACING, DEFAULT_SES_PROBE_RADIUS,
  field::{grid_index, grid_position},
};

/// Renderer-neutral indexed triangle mesh in source-space ångström coordinates.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct SurfaceMesh {
  /// Interleaved source position and unit-normal rows.
  pub vertices: Vec<[f32; 6]>,
  /// Triangle-list indices into [`Self::vertices`].
  pub indices: Vec<u32>,
}

/// Atom-selection policy used before molecular-surface domains are partitioned.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceAtomScope {
  /// Use biopolymer atoms, falling back to non-solvent atoms for ligand-only scenes.
  #[default]
  BiopolymerOrNonSolvent,
  /// Use every non-solvent atom in the scene, including ligands and cofactors.
  AllNonSolvent,
}

/// Partition applied to selected atoms before independent surface calculations.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum SurfacePartition {
  /// Calculate one envelope around all selected atoms.
  Unified,
  /// Calculate one independent surface for each chain.
  #[default]
  ByChain,
}

/// Validated parameters for the sampled-grid SES approximation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SesParameters {
  probe_radius: f32,
  grid_spacing: f32,
}

impl SesParameters {
  /// Creates validated SES parameters in ångströms.
  pub fn new(probe_radius: f32, grid_spacing: f32) -> Result<Self, MolecularSurfaceParameterError> {
    if !probe_radius.is_finite() || probe_radius <= 0.0 {
      return Err(MolecularSurfaceParameterError::InvalidProbeRadius(probe_radius));
    }
    if !grid_spacing.is_finite() || grid_spacing <= 0.0 {
      return Err(MolecularSurfaceParameterError::InvalidGridSpacing(grid_spacing));
    }
    Ok(Self {
      probe_radius,
      grid_spacing,
    })
  }

  /// Returns the rolling-probe radius in ångströms.
  pub const fn probe_radius(self) -> f32 {
    self.probe_radius
  }

  /// Returns the preferred scalar-grid spacing in ångströms.
  pub const fn grid_spacing(self) -> f32 {
    self.grid_spacing
  }
}

impl Default for SesParameters {
  fn default() -> Self {
    Self {
      probe_radius: DEFAULT_SES_PROBE_RADIUS,
      grid_spacing: DEFAULT_SES_GRID_SPACING,
    }
  }
}

/// Invalid numeric parameter supplied for molecular-surface generation.
#[derive(Debug, Error, Clone, Copy, PartialEq)]
pub enum MolecularSurfaceParameterError {
  /// Probe radii must be finite and positive.
  #[error("molecular-surface probe radius must be finite and positive, got {0}")]
  InvalidProbeRadius(f32),
  /// Grid spacing must be finite and positive.
  #[error("molecular-surface grid spacing must be finite and positive, got {0}")]
  InvalidGridSpacing(f32),
}

/// Scientific request describing atom scope and calculation-domain partitioning.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct MolecularSurfaceRequest {
  /// Atoms eligible to participate in surface generation.
  pub atom_scope: SurfaceAtomScope,
  /// How eligible atoms are split into independent calculations.
  pub partition: SurfacePartition,
  /// Parameters of the sampled-grid solvent-excluded surface calculation.
  pub ses: SesParameters,
}

/// One resolved calculation domain and its independently generated mesh.
#[derive(Debug, PartialEq)]
pub struct SurfaceDomainArtifact {
  /// Chain identity for a per-chain domain, or `None` for a unified domain.
  pub chain_id: Option<ChainId>,
  /// Renderer-neutral geometry generated for this domain.
  pub mesh: SurfaceMesh,
}

/// Molecular-surface result retaining the request and resolved domain boundaries.
#[derive(Debug, PartialEq)]
pub struct MolecularSurfaceArtifact {
  /// Reproducible scientific request that produced this artifact.
  pub request: MolecularSurfaceRequest,
  /// Independently calculated surface domains in deterministic order.
  pub domains: Vec<SurfaceDomainArtifact>,
}

/// Regular scalar grid captured from one SES construction stage.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct ScalarFieldGrid {
  /// Molecular-space position of sample `(0, 0, 0)`.
  pub bounds_min: [f32; 3],
  /// Uniform distance between adjacent samples, in ångströms.
  pub spacing: f32,
  /// Number of samples along X, Y, and Z.
  pub dimensions: [usize; 3],
  /// X-major scalar samples, with Z as the outermost coordinate.
  pub values: Vec<f32>,
}

impl ScalarFieldGrid {
  /// Returns the maximum molecular-space corner covered by the grid.
  pub fn bounds_max(&self) -> [f32; 3] {
    let extent = glam::Vec3::new(
      self.dimensions[0].saturating_sub(1) as f32,
      self.dimensions[1].saturating_sub(1) as f32,
      self.dimensions[2].saturating_sub(1) as f32,
    ) * self.spacing;
    (glam::Vec3::from_array(self.bounds_min) + extent).to_array()
  }

  /// Returns the flat sample index for one grid coordinate.
  pub fn index(&self, x: usize, y: usize, z: usize) -> usize {
    grid_index(x, y, z, self.dimensions)
  }

  /// Returns the molecular-space position represented by one flat sample index.
  pub fn sample_position(&self, index: usize) -> [f32; 3] {
    let x = index % self.dimensions[0];
    let yz = index / self.dimensions[0];
    let y = yz % self.dimensions[1];
    let z = yz / self.dimensions[1];
    grid_position(x, y, z, glam::Vec3::from_array(self.bounds_min), self.spacing).to_array()
  }
}

/// Intermediate products for one independently calculated SES domain.
///
/// This type is intentionally owned and potentially large. Applications should
/// request it only for diagnostics, visualization, or numerical validation;
/// normal surface generation retains only the final mesh.
#[derive(Debug, Default, PartialEq)]
pub struct SesDomainTrace {
  /// Chain identity for a per-chain domain, or `None` for a unified domain.
  pub chain_id: Option<ChainId>,
  /// Expanded-atom distance field whose zero contour is the SAS.
  pub sas_field: ScalarFieldGrid,
  /// Zero contour extracted from [`Self::sas_field`].
  pub sas_surface: SurfaceMesh,
  /// Merged positions sampled from the SAS and used as rolling-probe centers.
  pub probe_centers: Vec<[f32; 3]>,
  /// Distance field generated by the union of probe spheres.
  pub probe_field: ScalarFieldGrid,
  /// Complete probe-field zero contour containing inner and outer sheets.
  pub raw_probe_surface: SurfaceMesh,
  /// Atom-facing components after filtering and orientation, before smoothing.
  pub inner_probe_surface: SurfaceMesh,
  /// Inner surface after positional smoothing and before field-guided normals.
  pub smoothed_inner_surface: SurfaceMesh,
  /// Final smoothed SES mesh produced by the same kernel.
  pub final_surface: SurfaceMesh,
}

/// Complete opt-in trace of molecular-surface construction.
#[derive(Debug, PartialEq)]
pub struct MolecularSurfaceTrace {
  /// Scientific request used for every traced domain.
  pub request: MolecularSurfaceRequest,
  /// Intermediate products in the same deterministic domain order as normal generation.
  pub domains: Vec<SesDomainTrace>,
}
