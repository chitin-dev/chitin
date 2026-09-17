//! Requests, parameters, artifacts, grids, and diagnostic trace types.

use thiserror::Error;

use crate::structure::ChainId;

use super::super::{SurfaceAtomScope, SurfacePartition};
use super::{
  DEFAULT_SES_GRID_MEMORY_LIMIT_BYTES, DEFAULT_SES_GRID_SPACING, DEFAULT_SES_PROBE_RADIUS,
  ESTIMATED_SES_BYTES_PER_GRID_POINT, MIN_SES_GRID_MEMORY_LIMIT_BYTES, MIN_SES_MAX_GRID_POINTS,
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

/// Validated parameters for the sampled-grid SES approximation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SesParameters {
  probe_radius: f32,
  grid_spacing: f32,
  grid_budget: SesGridBudget,
}

/// Resource policy used to resolve the scalar grid for one SES domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SesGridBudget {
  /// Preserve the preferred spacing while the estimated peak fits in memory.
  Automatic { memory_limit_bytes: usize },
  /// Enforce an explicit upper bound on samples in each scalar grid.
  Fixed { max_grid_points: usize },
}

impl Default for SesGridBudget {
  fn default() -> Self {
    Self::Automatic {
      memory_limit_bytes: DEFAULT_SES_GRID_MEMORY_LIMIT_BYTES,
    }
  }
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
      grid_budget: SesGridBudget::default(),
    })
  }

  /// Uses a fixed maximum number of samples in each calculation-domain grid.
  pub fn with_max_grid_points(mut self, max_grid_points: usize) -> Result<Self, MolecularSurfaceParameterError> {
    self.grid_budget = validate_grid_budget(SesGridBudget::Fixed { max_grid_points })?;
    Ok(self)
  }

  /// Uses automatic grid sizing under a peak-memory safety limit.
  pub fn with_automatic_grid_budget(
    mut self,
    memory_limit_bytes: usize,
  ) -> Result<Self, MolecularSurfaceParameterError> {
    self.grid_budget = validate_grid_budget(SesGridBudget::Automatic { memory_limit_bytes })?;
    Ok(self)
  }

  /// Returns the rolling-probe radius in ångströms.
  pub const fn probe_radius(self) -> f32 {
    self.probe_radius
  }

  /// Returns the preferred scalar-grid spacing in ångströms.
  pub const fn grid_spacing(self) -> f32 {
    self.grid_spacing
  }

  /// Returns the configured automatic or fixed resource policy.
  pub const fn grid_budget(self) -> SesGridBudget {
    self.grid_budget
  }

  /// Returns the effective hard sample limit for one scalar grid.
  pub const fn max_grid_points(self) -> usize {
    match self.grid_budget {
      SesGridBudget::Automatic { memory_limit_bytes } => memory_limit_bytes / ESTIMATED_SES_BYTES_PER_GRID_POINT,
      SesGridBudget::Fixed { max_grid_points } => max_grid_points,
    }
  }
}

impl Default for SesParameters {
  fn default() -> Self {
    Self {
      probe_radius: DEFAULT_SES_PROBE_RADIUS,
      grid_spacing: DEFAULT_SES_GRID_SPACING,
      grid_budget: SesGridBudget::default(),
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
  /// A three-dimensional scalar grid requires at least one cell.
  #[error("molecular-surface grid point budget must be at least {minimum}, got {value}")]
  InvalidMaxGridPoints { value: usize, minimum: usize },
  /// Automatic sizing needs enough memory for at least one grid cell.
  #[error("molecular-surface grid memory limit must be at least {minimum} bytes, got {value}")]
  InvalidGridMemoryLimit { value: usize, minimum: usize },
}

/// Validates a scalar-grid resource policy before storing it in SES parameters.
fn validate_grid_budget(budget: SesGridBudget) -> Result<SesGridBudget, MolecularSurfaceParameterError> {
  match budget {
    SesGridBudget::Automatic { memory_limit_bytes } if memory_limit_bytes < MIN_SES_GRID_MEMORY_LIMIT_BYTES => {
      Err(MolecularSurfaceParameterError::InvalidGridMemoryLimit {
        value: memory_limit_bytes,
        minimum: MIN_SES_GRID_MEMORY_LIMIT_BYTES,
      })
    }
    SesGridBudget::Fixed { max_grid_points } if max_grid_points < MIN_SES_MAX_GRID_POINTS => {
      Err(MolecularSurfaceParameterError::InvalidMaxGridPoints {
        value: max_grid_points,
        minimum: MIN_SES_MAX_GRID_POINTS,
      })
    }
    _ => Ok(budget),
  }
}

/// Request describing an implicit-grid rendering surface.
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

/// Algorithm provenance attached to renderer-neutral molecular-surface geometry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SurfaceGeometrySource {
  /// Surface extracted from Chitin's sampled scalar-field pipeline.
  ImplicitGrid(MolecularSurfaceRequest),
  /// Analytical MSMS patches tessellated at the given vertex density.
  Msms {
    /// Rolling solvent-probe radius in ångströms.
    probe_radius: f64,
    /// Requested tessellation vertices per square ångström.
    vertex_density: f64,
  },
}

/// Renderer-neutral molecular-surface geometry and its algorithm provenance.
#[derive(Debug, PartialEq)]
pub struct MolecularSurfaceArtifact {
  /// Algorithm and parameters that produced this display geometry.
  pub source: SurfaceGeometrySource,
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
  /// Molecular-side contour of the composite inner field, before smoothing.
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
