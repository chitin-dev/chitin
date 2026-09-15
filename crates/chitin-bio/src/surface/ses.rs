//! Renderer-neutral molecular-surface planning and CPU tessellation.

mod contour;
mod field;
mod postprocess;
#[cfg(feature = "surface-profiling")]
mod profiling;
mod spatial;
mod types;

use std::collections::{BTreeMap, HashMap};

use crate::structure::{ChainId, ElementCategory, ResidueKind, StructureScene};
use rayon::prelude::*;

use self::{
  contour::{contour_field, vertex_position},
  field::{atom_surface_distance, compose_inner_surface_field, grid_dimensions, probe_surface_distance, sample_field},
  postprocess::{orient_inner_surface, recompute_inner_surface_normals, recompute_surface_normals, smooth_mesh},
  spatial::{AtomGrid, PointGrid, point_grid_cell},
};

#[cfg(feature = "surface-profiling")]
pub use self::profiling::{MolecularSurfaceProfile, MolecularSurfaceTimings, profile_molecular_surface};
pub use self::types::{
  MolecularSurfaceArtifact, MolecularSurfaceParameterError, MolecularSurfaceRequest, MolecularSurfaceTrace,
  ScalarFieldGrid, SesDomainTrace, SesParameters, SurfaceAtomScope, SurfaceDomainArtifact, SurfaceMesh,
  SurfacePartition,
};

/// Default water-probe radius used by molecular surfaces.
const DEFAULT_SES_PROBE_RADIUS: f32 = 1.4;
/// Default spacing between scalar-field samples.
const DEFAULT_SES_GRID_SPACING: f32 = 0.5;
/// Default maximum number of samples in each domain's scalar-grid layout.
const DEFAULT_SES_MAX_GRID_POINTS: usize = 750_000;
/// Smallest budget capable of describing one three-dimensional grid cell.
const MIN_SES_MAX_GRID_POINTS: usize = 8;
/// Number of grid cells over which a truncated distance field is evaluated.
const DISTANCE_FIELD_RANGE: f32 = 2.0;
/// Cell width tuned so the default atom-plus-probe query visits adjacent cells.
/// Larger custom probes or grid spacings expand the lookup range dynamically.
const ATOM_GRID_CELL_SIZE: f32 = 4.25;
/// Number of volume-preserving smoothing pairs applied after contouring.
const SMOOTHING_ITERATIONS: usize = 2;
/// Positive Laplacian coefficient used by the first smoothing pass.
const SMOOTHING_LAMBDA: f32 = 0.28;
/// Negative Laplacian coefficient that approximately restores volume.
const SMOOTHING_MU: f32 = -0.29;

/// Atom center and van der Waals radius used by the surface distance field.
#[derive(Clone, Copy)]
struct SurfaceAtom {
  /// Cartesian atom position in ångströms.
  position: glam::Vec3,
  /// Element-dependent van der Waals radius in ångströms.
  radius: f32,
}

/// Borrowed intermediate emitted by the SES kernel to an optional recorder.
enum SesKernelStage<'a> {
  SasField {
    bounds_min: glam::Vec3,
    spacing: f32,
    dimensions: [usize; 3],
    values: &'a [f32],
  },
  SasSurface(&'a SurfaceMesh),
  ProbeCenters(&'a [glam::Vec3]),
  ProbeField {
    bounds_min: glam::Vec3,
    spacing: f32,
    dimensions: [usize; 3],
    values: &'a [f32],
  },
  InnerProbeSurface(&'a SurfaceMesh),
  SmoothedInnerSurface(&'a SurfaceMesh),
}

/// Owned collector used only by the explicit trace API.
#[derive(Default)]
struct SesTraceRecorder {
  sas_field: ScalarFieldGrid,
  sas_surface: SurfaceMesh,
  probe_centers: Vec<[f32; 3]>,
  probe_field: ScalarFieldGrid,
  raw_probe_surface: SurfaceMesh,
  inner_probe_surface: SurfaceMesh,
  smoothed_inner_surface: SurfaceMesh,
}

impl SesTraceRecorder {
  /// Copies one borrowed kernel stage into the owned diagnostic trace.
  fn record(&mut self, stage: SesKernelStage<'_>) {
    match stage {
      SesKernelStage::SasField {
        bounds_min,
        spacing,
        dimensions,
        values,
      } => {
        self.sas_field = ScalarFieldGrid {
          bounds_min: bounds_min.to_array(),
          spacing,
          dimensions,
          values: values.to_vec(),
        };
      }
      SesKernelStage::SasSurface(mesh) => self.sas_surface = mesh.clone(),
      SesKernelStage::ProbeCenters(centers) => {
        self.probe_centers = centers.iter().map(|center| center.to_array()).collect();
      }
      SesKernelStage::ProbeField {
        bounds_min,
        spacing,
        dimensions,
        values,
      } => {
        self.raw_probe_surface = contour_field(bounds_min, spacing, dimensions, values);
        self.probe_field = ScalarFieldGrid {
          bounds_min: bounds_min.to_array(),
          spacing,
          dimensions,
          values: values.to_vec(),
        };
      }
      SesKernelStage::InnerProbeSurface(mesh) => {
        self.inner_probe_surface = mesh.clone();
        recompute_surface_normals(&mut self.inner_probe_surface);
      }
      SesKernelStage::SmoothedInnerSurface(mesh) => {
        self.smoothed_inner_surface = mesh.clone();
        recompute_surface_normals(&mut self.smoothed_inner_surface);
      }
    }
  }

  /// Completes one domain trace with the final surface returned by the kernel.
  fn finish(self, chain_id: Option<ChainId>, final_surface: SurfaceMesh) -> SesDomainTrace {
    SesDomainTrace {
      chain_id,
      sas_field: self.sas_field,
      sas_surface: self.sas_surface,
      probe_centers: self.probe_centers,
      probe_field: self.probe_field,
      raw_probe_surface: self.raw_probe_surface,
      inner_probe_surface: self.inner_probe_surface,
      smoothed_inner_surface: self.smoothed_inner_surface,
      final_surface,
    }
  }
}

/// Generates rolling-probe solvent-excluded surfaces for a scientific request.
///
/// Atom scope and domain partitioning are resolved before each domain is sent
/// independently to the numerical SES kernel. The default request uses a
/// 1.4 Å probe; the initial implementation is a sampled-grid approximation.
///
/// # Parameters
///
/// * `scene` supplies renderer-neutral atom coordinates and structure identity.
/// * `request` selects atoms and partitions them into calculation domains.
///
/// # Returns
///
/// A surface artifact retaining independently calculated domain meshes.
pub fn generate_molecular_surface(
  scene: &StructureScene,
  request: MolecularSurfaceRequest,
) -> MolecularSurfaceArtifact {
  let atom_groups = surface_atom_groups(scene, request).into_iter().collect::<Vec<_>>();
  let domains = atom_groups
    .into_par_iter()
    .map(|(chain_id, atoms)| SurfaceDomainArtifact {
      chain_id,
      mesh: ses_mesh_for_atoms(atoms, request.ses),
    })
    .collect();
  MolecularSurfaceArtifact { request, domains }
}

/// Generates an owned trace of every numerical SES construction stage.
///
/// Unlike [`generate_molecular_surface`], this diagnostic entry point retains
/// two dense scalar grids and several intermediate meshes per domain. It is
/// intended for algorithm visualization and validation rather than routine
/// molecular rendering.
///
/// # Parameters
///
/// * `scene` supplies renderer-neutral atom coordinates and structure identity.
/// * `request` selects atoms and partitions them into calculation domains.
///
/// # Returns
///
/// An ordered trace containing the real intermediates produced by the SES kernel.
pub fn trace_molecular_surface(scene: &StructureScene, request: MolecularSurfaceRequest) -> MolecularSurfaceTrace {
  let atom_groups = surface_atom_groups(scene, request).into_iter().collect::<Vec<_>>();
  let domains = atom_groups
    .into_par_iter()
    .map(|(chain_id, atoms)| {
      let mut recorder = SesTraceRecorder::default();
      let final_surface = ses_kernel_for_atoms(atoms, request.ses, |stage| recorder.record(stage));
      recorder.finish(chain_id, final_surface)
    })
    .collect();
  MolecularSurfaceTrace { request, domains }
}

/// Selects surface atoms and partitions them into requested calculation domains.
///
/// When a scene contains polymer atoms, hetero residues are omitted to match
/// chain-oriented molecular surfaces. If no polymer exists, non-solvent hetero
/// atoms are retained as a useful fallback for ligand-only structures.
///
/// # Parameters
///
/// * `scene` supplies atom chain membership and residue classifications.
///
/// # Returns
///
/// Deterministically ordered atom groups keyed by optional chain identifier.
fn surface_atom_groups(
  scene: &StructureScene,
  request: MolecularSurfaceRequest,
) -> BTreeMap<Option<ChainId>, Vec<SurfaceAtom>> {
  let contains_polymer = scene.atoms.iter().any(|atom| atom.residue_kind == ResidueKind::Polymer);
  let mut groups = BTreeMap::new();
  for atom in scene.atoms.iter().filter(|atom| {
    !atom.is_solvent
      && match request.atom_scope {
        SurfaceAtomScope::BiopolymerOrNonSolvent => !contains_polymer || atom.residue_kind == ResidueKind::Polymer,
        SurfaceAtomScope::AllNonSolvent => true,
      }
  }) {
    let domain = match request.partition {
      SurfacePartition::Unified => None,
      SurfacePartition::ByChain => Some(atom.chain_id),
    };
    groups.entry(domain).or_insert_with(Vec::new).push(SurfaceAtom {
      position: glam::Vec3::from_array(atom.position),
      radius: vdw_radius(atom.element),
    });
  }
  groups
}

/// Tessellates a rolling-probe SES for one atom group.
///
/// The algorithm first contours the solvent-accessible surface formed by atoms
/// expanded by the probe radius. Those contour vertices become probe centers
/// for a second distance field. The SAS exterior is filled into that field
/// before contouring, leaving only the molecular-side boundary for conservative
/// smoothing and normal reconstruction.
///
/// # Parameters
///
/// * `atoms` contains one calculation domain's atom centers and van der Waals radii.
///
/// # Returns
///
/// An indexed SES triangle mesh, or an empty mesh when `atoms` is empty.
fn ses_mesh_for_atoms(atoms: Vec<SurfaceAtom>, parameters: SesParameters) -> SurfaceMesh {
  ses_kernel_for_atoms(atoms, parameters, |_| {})
}

/// Runs the numerical SES pipeline and exposes borrowed stages to a recorder.
///
/// # Parameters
///
/// * `atoms` contains one calculation domain's atom centers and van der Waals radii.
/// * `parameters` configures probe radius and preferred grid spacing.
/// * `capture` receives each intermediate while its backing allocation is alive.
///
/// # Returns
///
/// The final oriented, smoothed, and normal-recomputed SES mesh.
fn ses_kernel_for_atoms(
  atoms: Vec<SurfaceAtom>,
  parameters: SesParameters,
  mut capture: impl FnMut(SesKernelStage<'_>),
) -> SurfaceMesh {
  if atoms.is_empty() {
    return SurfaceMesh::default();
  }
  let atom_grid = AtomGrid::new(atoms);

  let (atom_bounds_min, atom_bounds_max) = atom_bounds(&atom_grid.atoms);
  let probe_radius = parameters.probe_radius();
  let preferred_spacing = parameters.grid_spacing();
  let margin = atom_grid.max_radius + 2.0 * probe_radius + preferred_spacing;
  let bounds_min = atom_bounds_min - glam::Vec3::splat(margin);
  let bounds_max = atom_bounds_max + glam::Vec3::splat(margin);
  let extent = bounds_max - bounds_min;
  let (spacing, dimensions) = budgeted_grid_layout(extent, preferred_spacing, parameters.max_grid_points());

  let sas_field = sample_field(bounds_min, spacing, dimensions, |position| {
    atom_surface_distance(position, &atom_grid, probe_radius, spacing)
  });
  capture(SesKernelStage::SasField {
    bounds_min,
    spacing,
    dimensions,
    values: &sas_field,
  });
  let sas_mesh = contour_field(bounds_min, spacing, dimensions, &sas_field);
  capture(SesKernelStage::SasSurface(&sas_mesh));
  if sas_mesh.vertices.is_empty() {
    return SurfaceMesh::default();
  }

  let probe_centers = merge_close_probe_centers(&sas_mesh, 0.35 * spacing);
  capture(SesKernelStage::ProbeCenters(&probe_centers));
  let probe_grid = PointGrid::new(probe_centers, probe_radius + DISTANCE_FIELD_RANGE * spacing);
  let ses_field = sample_field(bounds_min, spacing, dimensions, |position| {
    probe_surface_distance(position, &probe_grid, probe_radius, spacing)
  });
  capture(SesKernelStage::ProbeField {
    bounds_min,
    spacing,
    dimensions,
    values: &ses_field,
  });
  let inner_surface_field = compose_inner_surface_field(&ses_field, &sas_field);
  let mut mesh = contour_field(bounds_min, spacing, dimensions, &inner_surface_field);
  orient_inner_surface(&mut mesh, bounds_min, spacing, dimensions, &inner_surface_field);
  capture(SesKernelStage::InnerProbeSurface(&mesh));
  smooth_mesh(&mut mesh, SMOOTHING_ITERATIONS);
  capture(SesKernelStage::SmoothedInnerSurface(&mesh));
  recompute_inner_surface_normals(&mut mesh, bounds_min, spacing, dimensions, &inner_surface_field);
  mesh
}

/// Resolves a regular-grid layout that does not exceed a point budget.
///
/// The preferred spacing is retained whenever its grid fits. Otherwise the
/// spacing grows approximately by the cube root of the excess sample ratio;
/// the small safety factor accounts for dimension rounding at cell boundaries.
///
/// # Parameters
///
/// * `extent` is the molecular-space size covered by the scalar grid.
/// * `preferred_spacing` is the finest spacing requested by the caller.
/// * `max_grid_points` is the maximum number of samples in each domain grid.
///
/// # Returns
///
/// The effective spacing and corresponding grid dimensions.
///
/// # Examples
///
/// For an extent whose preferred grid contains 200,000 samples and a budget
/// of 100,000, the returned spacing is greater than the preferred spacing and
/// the product of the returned dimensions is at most 100,000.
fn budgeted_grid_layout(extent: glam::Vec3, preferred_spacing: f32, max_grid_points: usize) -> (f32, [usize; 3]) {
  let mut spacing = preferred_spacing;
  let mut dimensions = grid_dimensions(extent, spacing);
  let mut point_count = grid_point_count(dimensions);
  while point_count > max_grid_points {
    let excess_ratio = point_count as f32 / max_grid_points as f32;
    spacing *= excess_ratio.cbrt().max(1.01);
    dimensions = grid_dimensions(extent, spacing);
    point_count = grid_point_count(dimensions);
  }
  (spacing, dimensions)
}

/// Returns the saturating number of samples in a three-dimensional grid.
fn grid_point_count(dimensions: [usize; 3]) -> usize {
  dimensions.into_iter().fold(1, usize::saturating_mul)
}

/// Accumulated probe-center position and averaged surface normal.
#[derive(Clone, Copy)]
struct ProbeSample {
  /// Merged source-space probe-center position.
  position: glam::Vec3,
  /// Average normal used to decide whether another point may be merged.
  normal: glam::Vec3,
  /// Number of contour vertices represented by this sample.
  count: u32,
}

/// Merges near-coincident probe centers whose surface normals agree.
///
/// Contouring can place several points almost at the same grid corner. Keeping
/// all of them adds high-frequency extrema to the second distance map. Points
/// on opposing sides of a narrow channel are deliberately kept separate.
///
/// # Parameters
///
/// * `mesh` is the first-pass solvent-accessible contour.
/// * `minimum_separation` is the distance below which similarly oriented
///   samples are merged.
///
/// # Returns
///
/// Probe-center positions suitable for the second distance field.
fn merge_close_probe_centers(mesh: &SurfaceMesh, minimum_separation: f32) -> Vec<glam::Vec3> {
  let mut samples: Vec<ProbeSample> = Vec::new();
  let mut buckets: HashMap<[i32; 3], Vec<usize>> = HashMap::new();
  for vertex in &mesh.vertices {
    let position = vertex_position(vertex);
    let normal = glam::Vec3::new(vertex[3], vertex[4], vertex[5]);
    let cell = point_grid_cell(position, minimum_separation);
    let mut nearest = None;
    let mut nearest_distance = minimum_separation;
    for z in cell[2] - 1..=cell[2] + 1 {
      for y in cell[1] - 1..=cell[1] + 1 {
        for x in cell[0] - 1..=cell[0] + 1 {
          let Some(indices) = buckets.get(&[x, y, z]) else {
            continue;
          };
          for index in indices {
            let sample = samples[*index];
            let distance = position.distance(sample.position);
            if distance < nearest_distance && normal.dot(sample.normal) > 0.1 {
              nearest = Some(*index);
              nearest_distance = distance;
            }
          }
        }
      }
    }
    if let Some(index) = nearest {
      let sample = &mut samples[index];
      let count = sample.count as f32;
      sample.position = (sample.position * count + position) / (count + 1.0);
      sample.normal = (sample.normal * count + normal).normalize_or_zero();
      sample.count += 1;
    } else {
      let index = samples.len();
      samples.push(ProbeSample {
        position,
        normal,
        count: 1,
      });
      buckets.entry(cell).or_default().push(index);
    }
  }
  samples.into_iter().map(|sample| sample.position).collect()
}

/// Computes the axis-aligned bounds of the non-solvent atom centers.
fn atom_bounds(atoms: &[SurfaceAtom]) -> (glam::Vec3, glam::Vec3) {
  atoms.iter().fold(
    (glam::Vec3::splat(f32::INFINITY), glam::Vec3::splat(f32::NEG_INFINITY)),
    |(minimum, maximum), atom| (minimum.min(atom.position), maximum.max(atom.position)),
  )
}

/// Returns the van der Waals radius used for one element category.
fn vdw_radius(element: ElementCategory) -> f32 {
  match element {
    ElementCategory::Hydrogen => 1.20,
    ElementCategory::Carbon => 1.70,
    ElementCategory::Nitrogen => 1.55,
    ElementCategory::Oxygen => 1.52,
    ElementCategory::Phosphorus | ElementCategory::Sulfur => 1.80,
    ElementCategory::Halogen => 1.75,
    ElementCategory::Metal => 1.70,
    ElementCategory::Other => 1.50,
  }
}

#[cfg(test)]
mod tests;
