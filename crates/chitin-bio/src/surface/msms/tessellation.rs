//! Display tessellation of analytical MSMS patches.
//!
//! Scientific areas remain properties of the analytical patches. Mesh
//! resolution affects only rendering and exported display geometry.

use std::{
  collections::{BTreeMap, BTreeSet, HashMap},
  f64::consts::{PI, TAU},
};

use lyon::{
  math::point,
  path::{FillRule, Path},
  tessellation::{BuffersBuilder, FillOptions, FillTessellator, FillVertex, VertexBuffers},
};
use thiserror::Error;

use glam::{DVec3, Vec3};

use super::{
  ContactBoundaryArcUse, ContactPatchGeometry, ContactPatchTopology, MsmsPatchGeometryDomain, ReentrantPatch,
  ToroidalPatchGeometry, ToroidalPatchTopology,
};
use crate::surface::SurfaceMesh;

/// Default maximum edge length used by analytical-patch display meshes.
const DEFAULT_MAX_EDGE_LENGTH: f64 = 0.35;

/// Validated resolution parameters for MSMS display tessellation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MsmsTessellationParameters {
  max_edge_length: f64,
}

impl MsmsTessellationParameters {
  /// Creates display tessellation parameters.
  ///
  /// # Parameters
  ///
  /// * `max_edge_length` is the requested upper bound for patch-edge lengths
  ///   in ångströms.
  ///
  /// # Returns
  ///
  /// Validated parameters, or [`MsmsTessellationError::InvalidMaxEdgeLength`]
  /// when the bound is not finite and strictly positive.
  ///
  /// # Examples
  ///
  /// ```
  /// use chitin_bio::surface::msms::MsmsTessellationParameters;
  ///
  /// let parameters = MsmsTessellationParameters::new(0.25)?;
  /// assert_eq!(parameters.max_edge_length(), 0.25);
  /// # Ok::<(), chitin_bio::surface::msms::MsmsTessellationError>(())
  /// ```
  pub fn new(max_edge_length: f64) -> Result<Self, MsmsTessellationError> {
    if !max_edge_length.is_finite() || max_edge_length <= 0.0 {
      return Err(MsmsTessellationError::InvalidMaxEdgeLength(max_edge_length));
    }
    Ok(Self { max_edge_length })
  }

  /// Returns the requested maximum display edge length in ångströms.
  pub const fn max_edge_length(self) -> f64 {
    self.max_edge_length
  }
}

impl Default for MsmsTessellationParameters {
  fn default() -> Self {
    Self {
      max_edge_length: DEFAULT_MAX_EDGE_LENGTH,
    }
  }
}

/// Failure produced while tessellating analytical MSMS patches.
#[derive(Clone, Copy, Debug, Error, PartialEq)]
pub enum MsmsTessellationError {
  /// Display edge lengths must be finite and strictly positive.
  #[error("MSMS tessellation edge length must be finite and positive, got {0}")]
  InvalidMaxEdgeLength(f64),
  /// Singular toroidal geometry must be trimmed before tessellation.
  #[error("toroidal patch {edge_index} is self-intersecting and must be trimmed before tessellation")]
  UntrimmedSingularTorus {
    /// Source reduced-surface edge index.
    edge_index: usize,
  },
  /// Radial singularity splitting produced no retained toric face.
  #[error("toroidal patch {edge_index} has no valid radial-singularity interval")]
  InvalidRadialSingularity {
    /// Source reduced-surface edge index.
    edge_index: usize,
  },
  /// The requested tessellation cannot be represented with `u32` indices.
  #[error("MSMS patch tessellation exceeds the u32 mesh-index limit")]
  MeshTooLarge,
  /// Analytical evaluation produced a coordinate not representable as `f32`.
  #[error("MSMS patch tessellation produced a non-finite display vertex")]
  InvalidDisplayVertex,
  /// Reentrant directions cannot define a stable spherical triangle.
  #[error("reentrant patch {face_index} has invalid spherical geometry")]
  InvalidReentrantPatch {
    /// Source reduced-surface face index.
    face_index: usize,
  },
  /// Contact boundaries cannot define a stable display polygon.
  #[error("contact patch on atom {atom_index} has invalid spherical geometry")]
  InvalidContactPatch {
    /// Source atom index carrying the contact patch.
    atom_index: usize,
  },
  /// Contact-mesh refinement failed to satisfy the requested edge length.
  #[error("contact patch on atom {atom_index} did not converge during interior refinement")]
  ContactRefinementDidNotConverge {
    /// Source atom index carrying the contact patch.
    atom_index: usize,
  },
  /// Reentrant-mesh refinement failed to satisfy the requested edge length.
  #[error("reentrant patch {face_index} did not converge during interior refinement")]
  ReentrantRefinementDidNotConverge {
    /// Source reduced-surface face index.
    face_index: usize,
  },
}

/// Tessellates one exact atom-contact patch into renderer-neutral geometry.
///
/// Bounded contact loops are sampled on their analytical small circles, then
/// mapped through a stereographic chart whose pole lies on the occluded side.
/// Ear clipping operates only in that chart; emitted positions and normals are
/// evaluated on the original atom sphere. This supports exposed regions larger
/// than a hemisphere without introducing a center-fan overlap.
///
/// # Parameters
///
/// * `patch` contains the exact atom sphere, oriented boundary loops, and an
///   occluded projection direction for each boundary arc.
/// * `parameters` controls display resolution without affecting analytical area.
///
/// # Returns
///
/// A spherical contact mesh, or [`MsmsTessellationError`] when its analytical
/// boundary cannot be projected or triangulated safely.
///
/// # Examples
///
/// ```
/// use chitin_bio::surface::msms::{
///   ContactPatchGeometry,
///   ContactPatchTopology,
///   MsmsTessellationParameters,
///   tessellate_contact_patch,
/// };
///
/// let patch = ContactPatchGeometry {
///   atom_index: 0,
///   atom_center: [0.0; 3],
///   atom_radius: 1.7,
///   probe_radius: 1.4,
///   boundary_arcs: Vec::new(),
///   boundary_loops: Vec::new(),
///   topology: ContactPatchTopology::FullSphere,
///   solid_angle: 4.0 * std::f64::consts::PI,
/// };
/// let mesh = tessellate_contact_patch(&patch, MsmsTessellationParameters::default())?;
/// assert!(!mesh.indices.is_empty());
/// # Ok::<(), chitin_bio::surface::msms::MsmsTessellationError>(())
/// ```
pub fn tessellate_contact_patch(
  patch: &ContactPatchGeometry,
  parameters: MsmsTessellationParameters,
) -> Result<SurfaceMesh, MsmsTessellationError> {
  validate_contact_patch(patch)?;
  match patch.topology {
    ContactPatchTopology::FullSphere => tessellate_full_contact_sphere(patch, parameters),
    ContactPatchTopology::Bounded => tessellate_bounded_contact_patch(patch, parameters),
  }
}

/// Tessellates every resolved patch in one analytical MSMS domain.
///
/// Contact, toroidal, and reentrant meshes are appended without changing their
/// analytical coordinates. Patch families use the same boundary subdivision
/// contract, then round-off-scale coincident vertices are welded before the
/// mesh is returned. Radial torus singularities are split into their retained
/// triangular faces before their meshes are appended.
///
/// # Parameters
///
/// * `domain` contains exact patch geometry for one independent atom domain.
/// * `parameters` controls display resolution without affecting analytical area.
///
/// # Returns
///
/// One renderer-neutral triangle mesh, or [`MsmsTessellationError`] when any
/// patch is unresolved or the combined mesh exceeds the index limit.
///
/// # Examples
///
/// ```
/// use chitin_bio::{
///   structure::{PdbParser, StructureScene},
///   surface::msms::{
///     MsmsRequest,
///     MsmsTessellationParameters,
///     build_msms_patch_geometry,
///     tessellate_msms_patch_domain,
///   },
/// };
///
/// let parsed = PdbParser::new().parse_bytes(
///   b"ATOM      1  C   GLY A   1       0.000   0.000   0.000  1.00 10.00           C  \n\
/// END\n",
/// )?;
/// let scene = StructureScene::from_first_model(&parsed.structure)?;
/// let domains = build_msms_patch_geometry(&scene, MsmsRequest::default())?;
/// let mesh = tessellate_msms_patch_domain(
///   &domains[0],
///   MsmsTessellationParameters::default(),
/// )?;
/// assert!(!mesh.indices.is_empty());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn tessellate_msms_patch_domain(
  domain: &MsmsPatchGeometryDomain,
  parameters: MsmsTessellationParameters,
) -> Result<SurfaceMesh, MsmsTessellationError> {
  let mut combined = SurfaceMesh::default();
  for patch in &domain.contact_patches {
    append_mesh(&mut combined, tessellate_contact_patch(patch, parameters)?)?;
  }
  for patch in &domain.toroidal_patches {
    append_mesh(&mut combined, tessellate_toroidal_patch(patch, parameters)?)?;
  }
  append_mesh(
    &mut combined,
    tessellate_resolved_reentrant_patches(domain, parameters)?,
  )?;
  weld_coincident_vertices(&mut combined)?;
  Ok(combined)
}

/// Tessellates and clips all reentrant patches in one analytical domain.
///
/// Every fixed-probe spherical triangle is first tessellated independently.
/// The resulting mesh is then clipped against the radical half-planes of all
/// other intersecting probe spheres. Keeping this stage separate allows a
/// renderer or diagnostic tool to color the resolved reentrant family without
/// reconstructing contact or toroidal geometry.
///
/// # Parameters
///
/// * `domain` contains the fixed-probe patches and their shared topology.
/// * `parameters` controls display resolution without changing analytical area.
///
/// # Returns
///
/// One combined reentrant mesh after non-radial singularity clipping, or
/// [`MsmsTessellationError`] if patch tessellation, clipping, or index rebasing
/// fails.
///
/// # Examples
///
/// ```
/// use chitin_bio::{
///   structure::{PdbParser, StructureScene},
///   surface::msms::{
///     MsmsRequest,
///     MsmsTessellationParameters,
///     build_msms_patch_geometry,
///     tessellate_resolved_reentrant_patches,
///   },
/// };
///
/// let parsed = PdbParser::new().parse_bytes(
///   b"ATOM      1  C   GLY A   1       0.000   0.000   0.000  1.00 10.00           C  \n\
/// END\n",
/// )?;
/// let scene = StructureScene::from_first_model(&parsed.structure)?;
/// let domains = build_msms_patch_geometry(&scene, MsmsRequest::default())?;
/// let mesh = tessellate_resolved_reentrant_patches(
///   &domains[0],
///   MsmsTessellationParameters::default(),
/// )?;
/// assert!(mesh.indices.is_empty());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn tessellate_resolved_reentrant_patches(
  domain: &MsmsPatchGeometryDomain,
  parameters: MsmsTessellationParameters,
) -> Result<SurfaceMesh, MsmsTessellationError> {
  let mut combined = SurfaceMesh::default();
  for patch in &domain.reentrant_patches {
    let mut mesh = tessellate_reentrant_patch(patch, parameters)?;
    let occluding_probes = domain
      .reentrant_patches
      .iter()
      .filter(|other| other.face_index != patch.face_index)
      .map(|other| (DVec3::from_array(other.probe_center), other.probe_radius))
      .collect::<Vec<_>>();
    clip_reentrant_mesh_outside_probes(&mut mesh, DVec3::from_array(patch.probe_center), &occluding_probes)?;
    append_mesh(&mut combined, mesh)?;
  }
  Ok(combined)
}

/// Tessellates a regular or radially singular toroidal patch for display.
///
/// A singular spindle patch is first split at every zero of its torus Jacobian.
/// Only non-negative radial intervals are retained, producing the triangular
/// toric faces described by the analytical reduced-surface model. This resolves
/// radial self-intersection only; non-radial probe-probe clipping remains a
/// separate topology stage.
///
/// # Parameters
///
/// * `patch` contains the analytical torus frame and retained angular ranges.
/// * `parameters` controls display resolution without affecting analytical area.
///
/// # Returns
///
/// The combined retained toric mesh, or [`MsmsTessellationError`] if splitting
/// produces no valid face or the result exceeds the display index limit.
pub fn tessellate_toroidal_patch(
  patch: &ToroidalPatchGeometry,
  parameters: MsmsTessellationParameters,
) -> Result<SurfaceMesh, MsmsTessellationError> {
  if patch.topology == ToroidalPatchTopology::Regular {
    return tessellate_regular_toroidal_patch(patch, parameters);
  }
  let retained = patch.split_radial_singularity();
  if retained.is_empty() {
    return Err(MsmsTessellationError::InvalidRadialSingularity {
      edge_index: patch.edge_index,
    });
  }
  let mut mesh = SurfaceMesh::default();
  for regular_patch in retained {
    append_mesh(
      &mut mesh,
      tessellate_regular_toroidal_patch(&regular_patch, parameters)?,
    )?;
  }
  Ok(mesh)
}

/// Appends one indexed mesh while preserving valid global indices.
fn append_mesh(destination: &mut SurfaceMesh, mut source: SurfaceMesh) -> Result<(), MsmsTessellationError> {
  let base_index = u32::try_from(destination.vertices.len()).map_err(|_| MsmsTessellationError::MeshTooLarge)?;
  if source.vertices.len() > (u32::MAX - base_index) as usize {
    return Err(MsmsTessellationError::MeshTooLarge);
  }
  for index in &mut source.indices {
    *index = index
      .checked_add(base_index)
      .ok_or(MsmsTessellationError::MeshTooLarge)?;
  }
  destination.vertices.append(&mut source.vertices);
  destination.indices.append(&mut source.indices);
  Ok(())
}

/// Welds numerically coincident patch vertices into a watertight display mesh.
///
/// Analytical patch families evaluate shared curves independently. Their
/// coordinates are mathematically identical but can differ by a few `f32`
/// units after separate parameterizations and clipping. A tight spatial hash
/// merges only round-off-scale neighbors, remaps triangle indices, removes
/// collapsed triangles, and averages their smooth surface normals.
///
/// # Parameters
///
/// * `mesh` is the combined contact, toroidal, and reentrant mesh to update.
///
/// # Returns
///
/// `Ok(())` after welding, or an error when a vertex is non-finite or the
/// compacted mesh exceeds its index representation.
fn weld_coincident_vertices(mesh: &mut SurfaceMesh) -> Result<(), MsmsTessellationError> {
  const WELD_TOLERANCE: f32 = 1.0e-4;
  const WELD_TOLERANCE_SQUARED: f32 = WELD_TOLERANCE * WELD_TOLERANCE;

  let source_vertices = std::mem::take(&mut mesh.vertices);
  let source_indices = std::mem::take(&mut mesh.indices);
  let mut cells = HashMap::<[i64; 3], Vec<u32>>::new();
  let mut remap = Vec::with_capacity(source_vertices.len());
  let mut normal_sums = Vec::<Vec3>::new();

  for vertex in source_vertices {
    if vertex.iter().any(|component| !component.is_finite()) {
      return Err(MsmsTessellationError::InvalidDisplayVertex);
    }
    let position = Vec3::from_slice(&vertex[..3]);
    let normal = Vec3::from_slice(&vertex[3..]);
    let cell = quantized_vertex_cell(position, WELD_TOLERANCE);
    let mut representative = None;
    'neighbors: for x_offset in -1..=1 {
      for y_offset in -1..=1 {
        for z_offset in -1..=1 {
          let neighbor = [cell[0] + x_offset, cell[1] + y_offset, cell[2] + z_offset];
          let Some(candidates) = cells.get(&neighbor) else {
            continue;
          };
          for &candidate in candidates {
            let candidate_position = Vec3::from_slice(&mesh.vertices[candidate as usize][..3]);
            if position.distance_squared(candidate_position) <= WELD_TOLERANCE_SQUARED {
              representative = Some(candidate);
              break 'neighbors;
            }
          }
        }
      }
    }

    let index = if let Some(index) = representative {
      normal_sums[index as usize] += normal;
      index
    } else {
      let index = u32::try_from(mesh.vertices.len()).map_err(|_| MsmsTessellationError::MeshTooLarge)?;
      mesh.vertices.push(vertex);
      normal_sums.push(normal);
      cells.entry(cell).or_default().push(index);
      index
    };
    remap.push(index);
  }

  for (vertex, normal_sum) in mesh.vertices.iter_mut().zip(normal_sums) {
    let normal = normal_sum.normalize_or_zero();
    if normal.length_squared() <= f32::EPSILON {
      return Err(MsmsTessellationError::InvalidDisplayVertex);
    }
    vertex[3..].copy_from_slice(&normal.to_array());
  }
  for &[first, second, third] in source_indices.as_chunks::<3>().0 {
    let mut triangle = [remap[first as usize], remap[second as usize], remap[third as usize]];
    if triangle[0] == triangle[1] || triangle[1] == triangle[2] || triangle[2] == triangle[0] {
      continue;
    }
    let positions = triangle.map(|index| Vec3::from_slice(&mesh.vertices[index as usize][..3]));
    let geometric_normal = (positions[1] - positions[0]).cross(positions[2] - positions[0]);
    if geometric_normal.length_squared() <= 1.0e-12 {
      continue;
    }
    let desired_normal = triangle
      .map(|index| Vec3::from_slice(&mesh.vertices[index as usize][3..]))
      .into_iter()
      .sum::<Vec3>();
    if geometric_normal.dot(desired_normal) < 0.0 {
      triangle.swap(1, 2);
    }
    mesh.indices.extend(triangle);
  }
  Ok(())
}

/// Maps one finite position to its spatial-welding hash cell.
fn quantized_vertex_cell(position: Vec3, tolerance: f32) -> [i64; 3] {
  [
    (position.x / tolerance).floor() as i64,
    (position.y / tolerance).floor() as i64,
    (position.z / tolerance).floor() as i64,
  ]
}

/// Removes one reentrant display mesh region eaten by other fixed probes.
fn clip_reentrant_mesh_outside_probes(
  mesh: &mut SurfaceMesh,
  source_center: DVec3,
  probes: &[(DVec3, f64)],
) -> Result<(), MsmsTessellationError> {
  if mesh.indices.is_empty() || probes.is_empty() {
    return Ok(());
  }
  let nearby_probes = probes
    .iter()
    .copied()
    .filter(|(center, radius)| sphere_intersects_mesh_bounds(mesh, *center, *radius))
    .collect::<Vec<_>>();
  if nearby_probes.is_empty() {
    return Ok(());
  }
  let original_vertices = std::mem::take(&mut mesh.vertices);
  let original_indices = std::mem::take(&mut mesh.indices);
  for triangle in original_indices.as_chunks::<3>().0 {
    let mut polygon = triangle
      .iter()
      .map(|index| original_vertices[*index as usize])
      .collect::<Vec<_>>();
    for &(center, _) in &nearby_probes {
      polygon = clip_polygon_outside_probe_plane(&polygon, source_center, center)?;
      if polygon.len() < 3 {
        break;
      }
    }
    if polygon.len() < 3 {
      continue;
    }
    let base_index = u32::try_from(mesh.vertices.len()).map_err(|_| MsmsTessellationError::MeshTooLarge)?;
    if polygon.len() > (u32::MAX - base_index) as usize {
      return Err(MsmsTessellationError::MeshTooLarge);
    }
    let polygon_len = polygon.len();
    mesh.vertices.extend(polygon);
    for offset in 1..polygon_len - 1 {
      mesh
        .indices
        .extend([base_index, base_index + offset as u32, base_index + offset as u32 + 1]);
    }
  }
  Ok(())
}

/// Tests an occluding sphere against one mesh axis-aligned bounding box.
fn sphere_intersects_mesh_bounds(mesh: &SurfaceMesh, center: DVec3, radius: f64) -> bool {
  let mut minimum = DVec3::splat(f64::INFINITY);
  let mut maximum = DVec3::splat(f64::NEG_INFINITY);
  for vertex in &mesh.vertices {
    let position = DVec3::new(vertex[0] as f64, vertex[1] as f64, vertex[2] as f64);
    minimum = minimum.min(position);
    maximum = maximum.max(position);
  }
  let closest = center.clamp(minimum, maximum);
  center.distance_squared(closest) <= radius * radius
}

/// Clips a reentrant polygon by the radical plane of two equal-radius probes.
fn clip_polygon_outside_probe_plane(
  polygon: &[[f32; 6]],
  source_center: DVec3,
  occluding_center: DVec3,
) -> Result<Vec<[f32; 6]>, MsmsTessellationError> {
  if polygon.is_empty() {
    return Ok(Vec::new());
  }
  let mut clipped = Vec::with_capacity(polygon.len() + 2);
  let plane_normal = source_center - occluding_center;
  let plane_offset = 0.5 * (source_center.length_squared() - occluding_center.length_squared());
  let mut previous = polygon[polygon.len() - 1];
  let mut previous_distance = vertex_plane_distance(previous, plane_normal, plane_offset);
  for &current in polygon {
    let current_distance = vertex_plane_distance(current, plane_normal, plane_offset);
    match (previous_distance >= -1.0e-10, current_distance >= -1.0e-10) {
      (true, true) => clipped.push(current),
      (true, false) => clipped.push(plane_edge_intersection(
        previous,
        current,
        previous_distance,
        current_distance,
      )?),
      (false, true) => {
        clipped.push(plane_edge_intersection(
          previous,
          current,
          previous_distance,
          current_distance,
        )?);
        clipped.push(current);
      }
      (false, false) => {}
    }
    previous = current;
    previous_distance = current_distance;
  }
  Ok(clipped)
}

/// Returns one display vertex's signed radical-plane distance.
fn vertex_plane_distance(vertex: [f32; 6], plane_normal: DVec3, plane_offset: f64) -> f64 {
  let position = DVec3::new(vertex[0] as f64, vertex[1] as f64, vertex[2] as f64);
  position.dot(plane_normal) - plane_offset
}

/// Intersects a display edge with one probe radical plane.
fn plane_edge_intersection(
  start: [f32; 6],
  end: [f32; 6],
  start_distance: f64,
  end_distance: f64,
) -> Result<[f32; 6], MsmsTessellationError> {
  let denominator = start_distance - end_distance;
  if denominator.abs() <= f64::EPSILON {
    return Err(MsmsTessellationError::InvalidDisplayVertex);
  }
  let fraction = (start_distance / denominator).clamp(0.0, 1.0);
  let mut vertex = [0.0; 6];
  for component in 0..6 {
    vertex[component] = start[component] + (end[component] - start[component]) * fraction as f32;
  }
  let normal = Vec3::from_slice(&vertex[3..]).normalize_or_zero();
  if normal.length_squared() <= f32::EPSILON {
    return Err(MsmsTessellationError::InvalidDisplayVertex);
  }
  vertex[3..].copy_from_slice(&normal.to_array());
  Ok(vertex)
}

/// Tessellates one reentrant spherical triangle into renderer-neutral geometry.
///
/// Every boundary is sampled by spherical interpolation using the same
/// arc-length subdivision rule as toroidal meridians. Boundary vertices are
/// therefore coincident with the neighboring toroidal patch at equal
/// resolution. The current interior uses a solvent-facing center fan; later
/// adaptive refinement may add interior vertices without changing the shared
/// boundary contract.
///
/// # Parameters
///
/// * `patch` contains the fixed probe center and three oriented contact directions.
/// * `parameters` controls display resolution without affecting analytical area.
///
/// # Returns
///
/// A spherical triangle mesh, or [`MsmsTessellationError`] when the patch is
/// degenerate or cannot be represented by the shared display mesh.
///
/// # Examples
///
/// ```
/// use chitin_bio::surface::msms::{
///   MsmsTessellationParameters,
///   ReentrantPatch,
///   tessellate_reentrant_patch,
/// };
///
/// let patch = ReentrantPatch {
///   face_index: 0,
///   probe_center: [0.0, 0.0, 0.0],
///   probe_radius: 1.4,
///   contact_directions: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
///   solid_angle: std::f64::consts::FRAC_PI_2,
/// };
/// let mesh = tessellate_reentrant_patch(&patch, MsmsTessellationParameters::default())?;
/// assert!(!mesh.indices.is_empty());
/// # Ok::<(), chitin_bio::surface::msms::MsmsTessellationError>(())
/// ```
pub fn tessellate_reentrant_patch(
  patch: &ReentrantPatch,
  parameters: MsmsTessellationParameters,
) -> Result<SurfaceMesh, MsmsTessellationError> {
  let directions = patch.contact_directions.map(DVec3::from_array);
  if !patch.probe_radius.is_finite()
    || patch.probe_radius <= 0.0
    || directions
      .iter()
      .any(|direction| !direction.is_finite() || (direction.length_squared() - 1.0).abs() > 1.0e-10)
  {
    return Err(MsmsTessellationError::InvalidReentrantPatch {
      face_index: patch.face_index,
    });
  }
  let center_direction = (directions[0] + directions[1] + directions[2]).normalize_or_zero();
  if center_direction.length_squared() <= f64::EPSILON {
    return Err(MsmsTessellationError::InvalidReentrantPatch {
      face_index: patch.face_index,
    });
  }

  let mut boundary = Vec::new();
  for edge_index in 0..3 {
    let start = directions[edge_index];
    let end = directions[(edge_index + 1) % 3];
    let angle = start.dot(end).clamp(-1.0, 1.0).acos();
    let segments = segment_count(patch.probe_radius * angle, parameters.max_edge_length)?;
    for segment_index in 0..segments {
      boundary.push(spherical_interpolate(
        start,
        end,
        segment_index as f64 / segments as f64,
      )?);
    }
  }
  if boundary.len() < 3 {
    return Err(MsmsTessellationError::InvalidReentrantPatch {
      face_index: patch.face_index,
    });
  }
  let mut mesh = tessellate_reentrant_fan(patch, center_direction, &boundary)?;
  refine_reentrant_mesh(&mut mesh, patch, parameters.max_edge_length)?;
  Ok(mesh)
}

/// Tessellates one regular toroidal patch into renderer-neutral triangles.
///
/// Azimuth and meridian counts are derived independently from conservative
/// arc-length bounds. Vertex normals point from the solvent-excluded volume
/// into the rolling probe. Singular patches are rejected until analytical
/// trimming has split away their self-intersecting parameter region.
///
/// # Parameters
///
/// * `patch` contains one untrimmed toroidal parameter rectangle.
/// * `parameters` controls display resolution without affecting patch area.
///
/// # Returns
///
/// A triangle mesh for a regular patch, or [`MsmsTessellationError`] when the
/// patch still requires singularity trimming or exceeds mesh limits.
pub fn tessellate_regular_toroidal_patch(
  patch: &ToroidalPatchGeometry,
  parameters: MsmsTessellationParameters,
) -> Result<SurfaceMesh, MsmsTessellationError> {
  if patch.topology != ToroidalPatchTopology::Regular {
    return Err(MsmsTessellationError::UntrimmedSingularTorus {
      edge_index: patch.edge_index,
    });
  }
  let azimuth_length_bound = (patch.major_radius + patch.probe_radius) * patch.azimuth_sweep;
  let meridian_length = patch.probe_radius * patch.polar_sweep;
  let azimuth_segments = segment_count(azimuth_length_bound, parameters.max_edge_length)?;
  let polar_segments = segment_count(meridian_length, parameters.max_edge_length)?;
  tessellate_toroidal_grid(patch, azimuth_segments, polar_segments)
}

/// Validates the sphere and boundary data used by contact tessellation.
fn validate_contact_patch(patch: &ContactPatchGeometry) -> Result<(), MsmsTessellationError> {
  let center = DVec3::from_array(patch.atom_center);
  let valid_topology = match patch.topology {
    ContactPatchTopology::FullSphere => patch.boundary_arcs.is_empty() && patch.boundary_loops.is_empty(),
    ContactPatchTopology::Bounded => !patch.boundary_arcs.is_empty() && !patch.boundary_loops.is_empty(),
  };
  if !center.is_finite()
    || !patch.atom_radius.is_finite()
    || patch.atom_radius <= 0.0
    || !patch.probe_radius.is_finite()
    || patch.probe_radius <= 0.0
    || !valid_topology
  {
    return Err(MsmsTessellationError::InvalidContactPatch {
      atom_index: patch.atom_index,
    });
  }
  Ok(())
}

/// Builds a latitude-longitude sphere with non-degenerate polar fans.
fn tessellate_full_contact_sphere(
  patch: &ContactPatchGeometry,
  parameters: MsmsTessellationParameters,
) -> Result<SurfaceMesh, MsmsTessellationError> {
  let latitude_segments = segment_count(PI * patch.atom_radius, parameters.max_edge_length)?.max(2);
  let longitude_segments = latitude_segments
    .checked_mul(2)
    .ok_or(MsmsTessellationError::MeshTooLarge)?;
  let ring_count = latitude_segments - 1;
  let vertex_count = ring_count
    .checked_mul(longitude_segments)
    .and_then(|count| count.checked_add(2))
    .ok_or(MsmsTessellationError::MeshTooLarge)?;
  if vertex_count > u32::MAX as usize {
    return Err(MsmsTessellationError::MeshTooLarge);
  }
  let triangle_count = longitude_segments
    .checked_mul(latitude_segments - 1)
    .and_then(|count| count.checked_mul(2))
    .ok_or(MsmsTessellationError::MeshTooLarge)?;
  let mut mesh = SurfaceMesh {
    vertices: Vec::with_capacity(vertex_count),
    indices: Vec::with_capacity(triangle_count * 3),
  };
  let center = DVec3::from_array(patch.atom_center);
  mesh.vertices.push(contact_vertex(center, patch.atom_radius, DVec3::Z)?);
  for latitude_index in 1..latitude_segments {
    let polar = PI * latitude_index as f64 / latitude_segments as f64;
    for longitude_index in 0..longitude_segments {
      let azimuth = TAU * longitude_index as f64 / longitude_segments as f64;
      let direction = DVec3::new(polar.sin() * azimuth.cos(), polar.sin() * azimuth.sin(), polar.cos());
      mesh
        .vertices
        .push(contact_vertex(center, patch.atom_radius, direction)?);
    }
  }
  let south_index = mesh.vertices.len() as u32;
  mesh
    .vertices
    .push(contact_vertex(center, patch.atom_radius, -DVec3::Z)?);

  for longitude_index in 0..longitude_segments {
    let current = 1 + longitude_index;
    let next = 1 + (longitude_index + 1) % longitude_segments;
    push_outward_triangle(&mut mesh, [0, current as u32, next as u32]);
  }
  for ring_index in 0..ring_count.saturating_sub(1) {
    let current_ring = 1 + ring_index * longitude_segments;
    let next_ring = current_ring + longitude_segments;
    for longitude_index in 0..longitude_segments {
      let next_longitude = (longitude_index + 1) % longitude_segments;
      let vertices = [
        current_ring + longitude_index,
        current_ring + next_longitude,
        next_ring + next_longitude,
        next_ring + longitude_index,
      ];
      push_outward_triangle(&mut mesh, [vertices[0] as u32, vertices[2] as u32, vertices[1] as u32]);
      push_outward_triangle(&mut mesh, [vertices[0] as u32, vertices[3] as u32, vertices[2] as u32]);
    }
  }
  let last_ring = 1 + (ring_count - 1) * longitude_segments;
  for longitude_index in 0..longitude_segments {
    let current = last_ring + longitude_index;
    let next = last_ring + (longitude_index + 1) % longitude_segments;
    push_outward_triangle(&mut mesh, [south_index, next as u32, current as u32]);
  }
  Ok(mesh)
}

/// Triangulates all exposed loops and holes through one bounded chart.
fn tessellate_bounded_contact_patch(
  patch: &ContactPatchGeometry,
  parameters: MsmsTessellationParameters,
) -> Result<SurfaceMesh, MsmsTessellationError> {
  let center = DVec3::from_array(patch.atom_center);
  let first_arc_use = patch
    .boundary_loops
    .first()
    .and_then(|boundary_loop| boundary_loop.arcs.first())
    .ok_or(MsmsTessellationError::InvalidContactPatch {
      atom_index: patch.atom_index,
    })?;
  let first_arc =
    patch
      .boundary_arcs
      .get(first_arc_use.arc_index)
      .ok_or(MsmsTessellationError::InvalidContactPatch {
        atom_index: patch.atom_index,
      })?;
  let pole = DVec3::from_array(first_arc.occluded_direction).normalize_or_zero();
  let (horizontal, vertical) = stereographic_basis(patch.atom_index, pole)?;
  let mut projected_loops = Vec::with_capacity(patch.boundary_loops.len());
  for boundary_loop in &patch.boundary_loops {
    let directions = sample_contact_loop(patch, &boundary_loop.arcs, parameters)?;
    if directions.len() < 3 {
      return Err(MsmsTessellationError::InvalidContactPatch {
        atom_index: patch.atom_index,
      });
    }
    let projected = stereographic_project_loop(patch.atom_index, &directions, pole, horizontal, vertical)?;
    projected_loops.push(projected);
  }
  let mut planar = tessellate_projected_contact_loops(patch.atom_index, &projected_loops)?;
  refine_projected_contact_mesh(
    patch.atom_index,
    &mut planar,
    pole,
    horizontal,
    vertical,
    patch.atom_radius,
    parameters.max_edge_length,
  )?;
  let mut mesh = SurfaceMesh {
    vertices: Vec::with_capacity(planar.vertices.len()),
    indices: Vec::with_capacity(planar.indices.len()),
  };
  for point_2d in planar.vertices {
    let direction = inverse_stereographic_project(point_2d, pole, horizontal, vertical)?;
    mesh
      .vertices
      .push(contact_vertex(center, patch.atom_radius, direction)?);
  }
  for triangle in planar.indices.as_chunks::<3>().0 {
    push_outward_triangle(&mut mesh, *triangle);
  }
  if mesh.indices.is_empty() {
    return Err(MsmsTessellationError::InvalidContactPatch {
      atom_index: patch.atom_index,
    });
  }
  Ok(mesh)
}

/// Tessellates projected contact boundaries while preserving nested holes.
///
/// All loops share one affine normalization so Lyon receives scale-stable
/// coordinates while retaining the relative placement of outer boundaries and
/// holes. Generated vertices are restored to the original stereographic chart
/// in double precision before spherical refinement.
///
/// # Parameters
///
/// * `atom_index` identifies the source atom for structured errors.
/// * `projected_loops` contains outer boundaries and nested holes in one
///   stereographic chart.
///
/// # Returns
///
/// A planar indexed mesh in the original chart coordinates, or an error when
/// the compound polygon is degenerate or cannot be tessellated.
fn tessellate_projected_contact_loops(
  atom_index: usize,
  projected_loops: &[Vec<[f64; 2]>],
) -> Result<VertexBuffers<[f64; 2], u32>, MsmsTessellationError> {
  let normalization = ProjectedContactNormalization::from_loops(atom_index, projected_loops)?;
  let mut path_builder = Path::builder();
  for projected in projected_loops {
    if projected.len() < 3 {
      return Err(MsmsTessellationError::InvalidContactPatch { atom_index });
    }
    let first = normalization.normalize(projected[0]);
    path_builder.begin(point(first[0], first[1]));
    for point_2d in &projected[1..] {
      let normalized = normalization.normalize(*point_2d);
      path_builder.line_to(point(normalized[0], normalized[1]));
    }
    path_builder.close();
  }
  let mut planar = VertexBuffers::<[f64; 2], u32>::new();
  FillTessellator::new()
    .tessellate_path(
      &path_builder.build(),
      &FillOptions::default()
        .with_fill_rule(FillRule::EvenOdd)
        .with_tolerance(ProjectedContactNormalization::TESSELLATION_TOLERANCE),
      &mut BuffersBuilder::new(&mut planar, |vertex: FillVertex<'_>| {
        normalization.denormalize(vertex.position().to_array())
      }),
    )
    .map_err(|_| MsmsTessellationError::InvalidContactPatch { atom_index })?;
  Ok(planar)
}

/// Affine normalization used to give Lyon scale-independent input.
///
/// Stereographic coordinates have no fixed numerical scale: a valid patch may
/// occupy either a tiny region or a very large region depending on its chosen
/// projection pole. Lyon's absolute flattening tolerance is intended for
/// display coordinates, so raw projected input can cause a small polygon to be
/// accepted while producing no triangles. Centering and scaling the complete
/// compound polygon keeps the tessellator in a predictable coordinate range.
#[derive(Clone, Copy, Debug)]
struct ProjectedContactNormalization {
  center: [f64; 2],
  scale: f64,
}

impl ProjectedContactNormalization {
  /// Lyon operates on straight boundary segments here, so this tolerance only
  /// governs its numerical merging and intersection machinery.
  const TESSELLATION_TOLERANCE: f32 = 1.0e-5;

  /// Builds one normalization shared by every outer loop and nested hole.
  ///
  /// # Parameters
  ///
  /// * `atom_index` identifies the source atom for structured errors.
  /// * `projected_loops` contains every boundary participating in the compound
  ///   contact polygon.
  ///
  /// # Returns
  ///
  /// A finite normalization with nonzero scale, or an invalid-contact error
  /// when the projected geometry is non-finite or degenerate.
  fn from_loops(atom_index: usize, projected_loops: &[Vec<[f64; 2]>]) -> Result<Self, MsmsTessellationError> {
    let mut minimum = [f64::INFINITY; 2];
    let mut maximum = [f64::NEG_INFINITY; 2];
    for point in projected_loops.iter().flatten() {
      if point.iter().any(|component| !component.is_finite()) {
        return Err(MsmsTessellationError::InvalidContactPatch { atom_index });
      }
      for axis in 0..2 {
        minimum[axis] = minimum[axis].min(point[axis]);
        maximum[axis] = maximum[axis].max(point[axis]);
      }
    }
    let scale = (maximum[0] - minimum[0]).max(maximum[1] - minimum[1]);
    if !scale.is_finite() || scale <= f64::EPSILON {
      return Err(MsmsTessellationError::InvalidContactPatch { atom_index });
    }
    Ok(Self {
      center: [0.5 * (minimum[0] + maximum[0]), 0.5 * (minimum[1] + maximum[1])],
      scale,
    })
  }

  /// Maps one raw stereographic point into the canonical Lyon chart.
  fn normalize(self, point: [f64; 2]) -> [f32; 2] {
    [
      ((point[0] - self.center[0]) / self.scale) as f32,
      ((point[1] - self.center[1]) / self.scale) as f32,
    ]
  }

  /// Restores one Lyon vertex to its original stereographic coordinates.
  fn denormalize(self, point: [f32; 2]) -> [f64; 2] {
    [
      (point[0] as f64).mul_add(self.scale, self.center[0]),
      (point[1] as f64).mul_add(self.scale, self.center[1]),
    ]
  }
}

/// Refines long interior diagonals without changing analytical boundaries.
///
/// Lyon triangulates the projected contact polygon but does not constrain the
/// length of diagonals inside it. A large spherical contact region can
/// therefore contain a few almost atom-wide planar triangles even though its
/// boundary arcs were sampled densely. Each pass identifies shared mesh edges
/// (and therefore excludes one-sided boundary edges), inserts one shared
/// midpoint per long edge, and conformingly retriangulates both incident
/// triangles. Midpoints remain in the stereographic chart until the final mesh
/// is lifted onto the atom sphere.
///
/// # Parameters
///
/// * `atom_index` identifies the source atom for structured errors.
/// * `planar` is the projected contact mesh to refine in place.
/// * `pole`, `horizontal`, and `vertical` define the stereographic chart.
/// * `atom_radius` converts unit-sphere chords to ångströms.
/// * `max_edge_length` is the requested upper bound for interior mesh edges.
///
/// # Returns
///
/// `Ok(())` when every shared edge satisfies the bound, or an error when the
/// mesh cannot be represented safely or refinement fails to converge.
fn refine_projected_contact_mesh(
  atom_index: usize,
  planar: &mut VertexBuffers<[f64; 2], u32>,
  pole: DVec3,
  horizontal: DVec3,
  vertical: DVec3,
  atom_radius: f64,
  max_edge_length: f64,
) -> Result<(), MsmsTessellationError> {
  const MAX_REFINEMENT_PASSES: usize = 32;
  const EDGE_LENGTH_TOLERANCE: f64 = 1.0 + 1.0e-6;

  for _ in 0..MAX_REFINEMENT_PASSES {
    let edge_uses = mesh_edge_use_counts(&planar.indices);
    let mut edges_to_split = BTreeSet::new();
    for (&edge, &uses) in &edge_uses {
      // One-sided edges lie on an analytical contact boundary shared with a
      // toroidal patch. Splitting them here would create a surface seam.
      if uses < 2 {
        continue;
      }
      let [start, end] = contact_edge_directions(planar, edge, pole, horizontal, vertical)?;
      if atom_radius * start.distance(end) > max_edge_length * EDGE_LENGTH_TOLERANCE {
        edges_to_split.insert(edge);
      }
    }
    if edges_to_split.is_empty() {
      return Ok(());
    }

    let midpoint_indices = insert_contact_edge_midpoints(planar, &edges_to_split)?;
    planar.indices = refine_triangles(&planar.indices, &midpoint_indices)?;
  }

  Err(MsmsTessellationError::ContactRefinementDidNotConverge { atom_index })
}

/// Counts how many triangles use each undirected edge.
fn mesh_edge_use_counts(indices: &[u32]) -> BTreeMap<(u32, u32), usize> {
  let mut uses = BTreeMap::new();
  for &[a, b, c] in indices.as_chunks::<3>().0 {
    for edge in [mesh_edge_key(a, b), mesh_edge_key(b, c), mesh_edge_key(c, a)] {
      *uses.entry(edge).or_default() += 1;
    }
  }
  uses
}

/// Returns the unit-sphere endpoints represented by one projected edge.
fn contact_edge_directions(
  planar: &VertexBuffers<[f64; 2], u32>,
  edge: (u32, u32),
  pole: DVec3,
  horizontal: DVec3,
  vertical: DVec3,
) -> Result<[DVec3; 2], MsmsTessellationError> {
  let start = planar
    .vertices
    .get(edge.0 as usize)
    .copied()
    .ok_or(MsmsTessellationError::InvalidDisplayVertex)?;
  let end = planar
    .vertices
    .get(edge.1 as usize)
    .copied()
    .ok_or(MsmsTessellationError::InvalidDisplayVertex)?;
  Ok([
    inverse_stereographic_project(start, pole, horizontal, vertical)?,
    inverse_stereographic_project(end, pole, horizontal, vertical)?,
  ])
}

/// Inserts one deterministic projected midpoint for every selected edge.
fn insert_contact_edge_midpoints(
  planar: &mut VertexBuffers<[f64; 2], u32>,
  edges: &BTreeSet<(u32, u32)>,
) -> Result<BTreeMap<(u32, u32), u32>, MsmsTessellationError> {
  let mut midpoint_indices = BTreeMap::new();
  for &edge in edges {
    let start = planar
      .vertices
      .get(edge.0 as usize)
      .copied()
      .ok_or(MsmsTessellationError::InvalidDisplayVertex)?;
    let end = planar
      .vertices
      .get(edge.1 as usize)
      .copied()
      .ok_or(MsmsTessellationError::InvalidDisplayVertex)?;
    let midpoint = [0.5 * (start[0] + end[0]), 0.5 * (start[1] + end[1])];
    if midpoint.iter().any(|component| !component.is_finite()) {
      return Err(MsmsTessellationError::InvalidDisplayVertex);
    }
    let midpoint_index = u32::try_from(planar.vertices.len()).map_err(|_| MsmsTessellationError::MeshTooLarge)?;
    planar.vertices.push(midpoint);
    midpoint_indices.insert(edge, midpoint_index);
  }
  Ok(midpoint_indices)
}

/// Conformingly subdivides triangles whose shared edges received midpoints.
fn refine_triangles(
  indices: &[u32],
  midpoint_indices: &BTreeMap<(u32, u32), u32>,
) -> Result<Vec<u32>, MsmsTessellationError> {
  let mut refined = Vec::with_capacity(indices.len().saturating_mul(2));
  for &[a, b, c] in indices.as_chunks::<3>().0 {
    let midpoints = [
      midpoint_indices.get(&mesh_edge_key(a, b)).copied(),
      midpoint_indices.get(&mesh_edge_key(b, c)).copied(),
      midpoint_indices.get(&mesh_edge_key(c, a)).copied(),
    ];
    let mask = u8::from(midpoints[0].is_some())
      | (u8::from(midpoints[1].is_some()) << 1)
      | (u8::from(midpoints[2].is_some()) << 2);
    match (mask, midpoints) {
      (0, _) => append_triangles(&mut refined, &[[a, b, c]]),
      (1, [Some(ab), _, _]) => append_triangles(&mut refined, &[[a, ab, c], [ab, b, c]]),
      (2, [_, Some(bc), _]) => append_triangles(&mut refined, &[[b, bc, a], [bc, c, a]]),
      (4, [_, _, Some(ca)]) => append_triangles(&mut refined, &[[c, ca, b], [ca, a, b]]),
      (3, [Some(ab), Some(bc), _]) => {
        append_triangles(&mut refined, &[[a, ab, c], [ab, bc, c], [ab, b, bc]]);
      }
      (6, [_, Some(bc), Some(ca)]) => {
        append_triangles(&mut refined, &[[b, bc, a], [bc, ca, a], [bc, c, ca]]);
      }
      (5, [Some(ab), _, Some(ca)]) => {
        append_triangles(&mut refined, &[[c, ca, b], [ca, ab, b], [ca, a, ab]]);
      }
      (7, [Some(ab), Some(bc), Some(ca)]) => {
        append_triangles(&mut refined, &[[a, ab, ca], [ab, b, bc], [ca, bc, c], [ab, bc, ca]]);
      }
      _ => return Err(MsmsTessellationError::InvalidDisplayVertex),
    }
  }
  Ok(refined)
}

/// Appends oriented triangles to a flat index buffer.
fn append_triangles(indices: &mut Vec<u32>, triangles: &[[u32; 3]]) {
  indices.extend(triangles.iter().flatten().copied());
}

/// Canonicalizes an undirected mesh edge for deterministic lookup.
const fn mesh_edge_key(start: u32, end: u32) -> (u32, u32) {
  if start < end { (start, end) } else { (end, start) }
}

/// Samples one oriented analytical contact loop without duplicating corners.
fn sample_contact_loop(
  patch: &ContactPatchGeometry,
  arc_uses: &[ContactBoundaryArcUse],
  parameters: MsmsTessellationParameters,
) -> Result<Vec<DVec3>, MsmsTessellationError> {
  let mut directions = Vec::new();
  for arc_use in arc_uses {
    let arc = patch
      .boundary_arcs
      .get(arc_use.arc_index)
      .ok_or(MsmsTessellationError::InvalidContactPatch {
        atom_index: patch.atom_index,
      })?;
    let probe_circle_radius = arc.direction_circle_radius * (patch.atom_radius + patch.probe_radius);
    let length_bound = (probe_circle_radius + patch.probe_radius) * arc.sweep_angle;
    let segments = segment_count(length_bound, parameters.max_edge_length)?;
    for segment_index in 0..segments {
      let fraction = segment_index as f64 / segments as f64;
      let offset = if arc_use.reversed {
        arc.sweep_angle * (1.0 - fraction)
      } else {
        arc.sweep_angle * fraction
      };
      directions.push(DVec3::from_array(arc.direction(offset)));
    }
  }
  Ok(directions)
}

/// Maps a spherical loop away from an occluded pole into a finite 2-D chart.
fn stereographic_project_loop(
  atom_index: usize,
  directions: &[DVec3],
  pole: DVec3,
  horizontal: DVec3,
  vertical: DVec3,
) -> Result<Vec<[f64; 2]>, MsmsTessellationError> {
  let mut projected = Vec::with_capacity(directions.len());
  for &direction in directions {
    let denominator = 1.0 - pole.dot(direction);
    if !direction.is_finite() || denominator <= 1.0e-10 {
      return Err(MsmsTessellationError::InvalidContactPatch { atom_index });
    }
    let point = [
      direction.dot(horizontal) / denominator,
      direction.dot(vertical) / denominator,
    ];
    if point.iter().any(|component| !component.is_finite()) {
      return Err(MsmsTessellationError::InvalidContactPatch { atom_index });
    }
    projected.push(point);
  }
  Ok(projected)
}

/// Builds an orthonormal plane basis for stereographic projection.
fn stereographic_basis(atom_index: usize, pole: DVec3) -> Result<(DVec3, DVec3), MsmsTessellationError> {
  if !pole.is_finite() || pole.length_squared() <= f64::EPSILON {
    return Err(MsmsTessellationError::InvalidContactPatch { atom_index });
  }
  let reference = if pole.z.abs() < 0.9 { DVec3::Z } else { DVec3::Y };
  let horizontal = pole.cross(reference).normalize_or_zero();
  let vertical = pole.cross(horizontal).normalize_or_zero();
  Ok((horizontal, vertical))
}

/// Maps one planar stereographic point back onto the atom unit sphere.
fn inverse_stereographic_project(
  point_2d: [f64; 2],
  pole: DVec3,
  horizontal: DVec3,
  vertical: DVec3,
) -> Result<DVec3, MsmsTessellationError> {
  let x = point_2d[0];
  let y = point_2d[1];
  let radius_squared = x.mul_add(x, y * y);
  let denominator = radius_squared + 1.0;
  let direction = ((radius_squared - 1.0) / denominator) * pole + (2.0 / denominator) * (x * horizontal + y * vertical);
  if !direction.is_finite() || direction.length_squared() <= f64::EPSILON {
    return Err(MsmsTessellationError::InvalidDisplayVertex);
  }
  Ok(direction.normalize())
}

/// Converts one atom-relative direction into a contact display vertex.
fn contact_vertex(atom_center: DVec3, atom_radius: f64, direction: DVec3) -> Result<[f32; 6], MsmsTessellationError> {
  let direction = direction.normalize_or_zero();
  let position = atom_center + atom_radius * direction;
  let vertex = [
    position.x as f32,
    position.y as f32,
    position.z as f32,
    direction.x as f32,
    direction.y as f32,
    direction.z as f32,
  ];
  if direction.length_squared() <= f64::EPSILON || vertex.iter().any(|component| !component.is_finite()) {
    return Err(MsmsTessellationError::InvalidDisplayVertex);
  }
  Ok(vertex)
}

/// Appends one triangle after aligning its winding with averaged vertex normals.
fn push_outward_triangle(mesh: &mut SurfaceMesh, triangle: [u32; 3]) {
  let positions = triangle.map(|index| Vec3::from_slice(&mesh.vertices[index as usize][..3]));
  let geometric_normal = (positions[1] - positions[0]).cross(positions[2] - positions[0]);
  let desired_normal = triangle
    .map(|index| Vec3::from_slice(&mesh.vertices[index as usize][3..]))
    .into_iter()
    .sum::<Vec3>();
  if geometric_normal.dot(desired_normal) >= 0.0 {
    mesh.indices.extend(triangle);
  } else {
    mesh.indices.extend([triangle[0], triangle[2], triangle[1]]);
  }
}

/// Converts an arc-length bound into a non-zero segment count.
fn segment_count(length: f64, max_edge_length: f64) -> Result<usize, MsmsTessellationError> {
  let segments = (length / max_edge_length).ceil().max(1.0);
  if !segments.is_finite() || segments > u32::MAX as f64 {
    return Err(MsmsTessellationError::MeshTooLarge);
  }
  Ok(segments as usize)
}

/// Interpolates the minor great-circle arc between two unit directions.
fn spherical_interpolate(start: DVec3, end: DVec3, fraction: f64) -> Result<DVec3, MsmsTessellationError> {
  let angle = start.dot(end).clamp(-1.0, 1.0).acos();
  if angle <= 1024.0 * f64::EPSILON {
    return Ok(start);
  }
  let sine = angle.sin();
  if sine.abs() <= 1024.0 * f64::EPSILON {
    return Err(MsmsTessellationError::InvalidDisplayVertex);
  }
  let direction = (((1.0 - fraction) * angle).sin() * start + (fraction * angle).sin() * end) / sine;
  let direction = direction.normalize_or_zero();
  if !direction.is_finite() || direction.length_squared() <= f64::EPSILON {
    return Err(MsmsTessellationError::InvalidDisplayVertex);
  }
  Ok(direction)
}

/// Builds a consistently oriented fan inside one spherical boundary loop.
fn tessellate_reentrant_fan(
  patch: &ReentrantPatch,
  center_direction: DVec3,
  boundary: &[DVec3],
) -> Result<SurfaceMesh, MsmsTessellationError> {
  let vertex_count = boundary
    .len()
    .checked_add(1)
    .ok_or(MsmsTessellationError::MeshTooLarge)?;
  if vertex_count > u32::MAX as usize {
    return Err(MsmsTessellationError::MeshTooLarge);
  }
  let center = DVec3::from_array(patch.probe_center);
  if !center.is_finite() {
    return Err(MsmsTessellationError::InvalidReentrantPatch {
      face_index: patch.face_index,
    });
  }
  let mut mesh = SurfaceMesh {
    vertices: Vec::with_capacity(vertex_count),
    indices: Vec::with_capacity(boundary.len() * 3),
  };
  mesh
    .vertices
    .push(reentrant_vertex(center, patch.probe_radius, center_direction)?);
  for &direction in boundary {
    mesh
      .vertices
      .push(reentrant_vertex(center, patch.probe_radius, direction)?);
  }

  for boundary_index in 0..boundary.len() {
    let current = boundary_index + 1;
    let next = (boundary_index + 1) % boundary.len() + 1;
    let center_position = Vec3::from_slice(&mesh.vertices[0][..3]);
    let current_position = Vec3::from_slice(&mesh.vertices[current][..3]);
    let next_position = Vec3::from_slice(&mesh.vertices[next][..3]);
    let geometric_normal = (current_position - center_position).cross(next_position - center_position);
    let desired_normal = Vec3::from_slice(&mesh.vertices[0][3..])
      + Vec3::from_slice(&mesh.vertices[current][3..])
      + Vec3::from_slice(&mesh.vertices[next][3..]);
    let triangle = if geometric_normal.dot(desired_normal) >= 0.0 {
      [0, current as u32, next as u32]
    } else {
      [0, next as u32, current as u32]
    };
    mesh.indices.extend(triangle);
  }
  Ok(mesh)
}

/// Refines long reentrant fan edges while preserving analytical boundaries.
///
/// Boundary arcs already share their subdivision contract with neighboring
/// toroidal patches and must remain untouched. Internal fan edges, however,
/// can span most of the probe sphere and form visibly planar sheets. This
/// routine repeatedly splits only shared edges and projects each shared
/// midpoint back onto the analytical probe sphere.
///
/// # Parameters
///
/// * `mesh` is the initial center-fan tessellation to refine in place.
/// * `patch` supplies the probe sphere and source face identity.
/// * `max_edge_length` bounds every interior chord in ångströms.
///
/// # Returns
///
/// `Ok(())` when all internal edges satisfy the bound, or an error when a
/// midpoint is undefined, the index range is exceeded, or refinement does not
/// converge.
fn refine_reentrant_mesh(
  mesh: &mut SurfaceMesh,
  patch: &ReentrantPatch,
  max_edge_length: f64,
) -> Result<(), MsmsTessellationError> {
  const MAX_REFINEMENT_PASSES: usize = 32;
  const EDGE_LENGTH_TOLERANCE: f64 = 1.0 + 1.0e-6;

  for _ in 0..MAX_REFINEMENT_PASSES {
    let edge_uses = mesh_edge_use_counts(&mesh.indices);
    let mut edges_to_split = BTreeSet::new();
    for (&edge, &uses) in &edge_uses {
      // A one-sided edge belongs to a reentrant–toroidal analytical boundary.
      if uses < 2 {
        continue;
      }
      let start = mesh
        .vertices
        .get(edge.0 as usize)
        .ok_or(MsmsTessellationError::InvalidDisplayVertex)?;
      let end = mesh
        .vertices
        .get(edge.1 as usize)
        .ok_or(MsmsTessellationError::InvalidDisplayVertex)?;
      let length = Vec3::from_slice(&start[..3]).distance(Vec3::from_slice(&end[..3])) as f64;
      if length > max_edge_length * EDGE_LENGTH_TOLERANCE {
        edges_to_split.insert(edge);
      }
    }
    if edges_to_split.is_empty() {
      return Ok(());
    }

    let midpoint_indices = insert_reentrant_edge_midpoints(mesh, patch, &edges_to_split)?;
    mesh.indices = refine_triangles(&mesh.indices, &midpoint_indices)?;
  }

  Err(MsmsTessellationError::ReentrantRefinementDidNotConverge {
    face_index: patch.face_index,
  })
}

/// Inserts one shared spherical midpoint for every selected reentrant edge.
fn insert_reentrant_edge_midpoints(
  mesh: &mut SurfaceMesh,
  patch: &ReentrantPatch,
  edges: &BTreeSet<(u32, u32)>,
) -> Result<BTreeMap<(u32, u32), u32>, MsmsTessellationError> {
  let center = DVec3::from_array(patch.probe_center);
  let mut midpoint_indices = BTreeMap::new();
  for &edge in edges {
    let start = mesh
      .vertices
      .get(edge.0 as usize)
      .ok_or(MsmsTessellationError::InvalidDisplayVertex)?;
    let end = mesh
      .vertices
      .get(edge.1 as usize)
      .ok_or(MsmsTessellationError::InvalidDisplayVertex)?;
    let start_direction = (DVec3::new(start[0] as f64, start[1] as f64, start[2] as f64) - center).normalize_or_zero();
    let end_direction = (DVec3::new(end[0] as f64, end[1] as f64, end[2] as f64) - center).normalize_or_zero();
    let midpoint_direction = (start_direction + end_direction).normalize_or_zero();
    if !midpoint_direction.is_finite() || midpoint_direction.length_squared() <= f64::EPSILON {
      return Err(MsmsTessellationError::InvalidReentrantPatch {
        face_index: patch.face_index,
      });
    }
    let midpoint_index = u32::try_from(mesh.vertices.len()).map_err(|_| MsmsTessellationError::MeshTooLarge)?;
    mesh
      .vertices
      .push(reentrant_vertex(center, patch.probe_radius, midpoint_direction)?);
    midpoint_indices.insert(edge, midpoint_index);
  }
  Ok(midpoint_indices)
}

/// Converts one probe-relative direction into a reentrant display vertex.
fn reentrant_vertex(
  probe_center: DVec3,
  probe_radius: f64,
  direction: DVec3,
) -> Result<[f32; 6], MsmsTessellationError> {
  let position = probe_center + probe_radius * direction;
  let normal = -direction;
  let vertex = [
    position.x as f32,
    position.y as f32,
    position.z as f32,
    normal.x as f32,
    normal.y as f32,
    normal.z as f32,
  ];
  if vertex.iter().any(|component| !component.is_finite()) {
    return Err(MsmsTessellationError::InvalidDisplayVertex);
  }
  Ok(vertex)
}

/// Samples and indexes one rectangular torus parameter domain.
fn tessellate_toroidal_grid(
  patch: &ToroidalPatchGeometry,
  azimuth_segments: usize,
  polar_segments: usize,
) -> Result<SurfaceMesh, MsmsTessellationError> {
  let row_width = polar_segments
    .checked_add(1)
    .ok_or(MsmsTessellationError::MeshTooLarge)?;
  let row_count = azimuth_segments
    .checked_add(1)
    .ok_or(MsmsTessellationError::MeshTooLarge)?;
  let vertex_count = row_width
    .checked_mul(row_count)
    .ok_or(MsmsTessellationError::MeshTooLarge)?;
  if vertex_count > u32::MAX as usize {
    return Err(MsmsTessellationError::MeshTooLarge);
  }
  let index_count = azimuth_segments
    .checked_mul(polar_segments)
    .and_then(|quad_count| quad_count.checked_mul(6))
    .ok_or(MsmsTessellationError::MeshTooLarge)?;
  let mut mesh = SurfaceMesh {
    vertices: Vec::with_capacity(vertex_count),
    indices: Vec::with_capacity(index_count),
  };

  for azimuth_index in 0..=azimuth_segments {
    let azimuth_offset = patch.azimuth_sweep * azimuth_index as f64 / azimuth_segments as f64;
    for polar_index in 0..=polar_segments {
      let polar_offset = patch.polar_sweep * polar_index as f64 / polar_segments as f64;
      let position = patch.position(azimuth_offset, polar_offset);
      let normal = patch.outward_normal(azimuth_offset, polar_offset);
      let vertex = [
        position[0] as f32,
        position[1] as f32,
        position[2] as f32,
        normal[0] as f32,
        normal[1] as f32,
        normal[2] as f32,
      ];
      if vertex.iter().any(|component| !component.is_finite()) {
        return Err(MsmsTessellationError::InvalidDisplayVertex);
      }
      mesh.vertices.push(vertex);
    }
  }

  for azimuth_index in 0..azimuth_segments {
    for polar_index in 0..polar_segments {
      let first = azimuth_index * row_width + polar_index;
      let next_azimuth = first + row_width;
      let indices = [first, first + 1, next_azimuth + 1, next_azimuth];
      // Polar-major winding points back toward the rolling-probe center and
      // therefore agrees with the solvent-facing SES normals.
      mesh
        .indices
        .extend([indices[0], indices[1], indices[2], indices[0], indices[2], indices[3]].map(|index| index as u32));
    }
  }
  collapse_singular_torus_rows(&mut mesh, patch, row_width, row_count);
  Ok(mesh)
}

/// Collapses Jacobian-zero parameter rows into single singular vertices.
fn collapse_singular_torus_rows(
  mesh: &mut SurfaceMesh,
  patch: &ToroidalPatchGeometry,
  row_width: usize,
  row_count: usize,
) {
  let polar_end = patch.polar_start + patch.polar_sweep;
  let scale = patch.major_radius.max(patch.probe_radius).max(1.0);
  let tolerance = 4096.0 * f64::EPSILON * scale;
  let collapse_start = (patch.major_radius + patch.probe_radius * patch.polar_start.cos()).abs() <= tolerance;
  let collapse_end = (patch.major_radius + patch.probe_radius * polar_end.cos()).abs() <= tolerance;
  if !collapse_start && !collapse_end {
    return;
  }
  let end_column = row_width - 1;
  for index in &mut mesh.indices {
    let local = *index as usize;
    let column = local % row_width;
    if collapse_start && column == 0 {
      *index = 0;
    } else if collapse_end && column == end_column {
      *index = end_column as u32;
    }
  }
  mesh.indices = mesh
    .indices
    .as_chunks::<3>()
    .0
    .iter()
    .copied()
    .filter(|triangle| triangle[0] != triangle[1] && triangle[1] != triangle[2] && triangle[2] != triangle[0])
    .flatten()
    .collect();
  debug_assert_eq!(mesh.vertices.len(), row_width * row_count);
}

#[cfg(test)]
mod tests {
  use std::f64::consts::{PI, TAU};

  use glam::{DVec3, Vec3};

  use super::*;
  use crate::surface::msms::{
    MsmsParameters, build_accessible_probe_edges, build_contact_patch_geometry, geometry::MsmsAtom,
  };

  fn regular_patch() -> ToroidalPatchGeometry {
    ToroidalPatchGeometry {
      edge_index: 0,
      atom_indices: [0, 1],
      probe_circle_center: [0.0, 0.0, 0.0],
      probe_circle_axis: [1.0, 0.0, 0.0],
      probe_circle_basis: [0.0, 1.0, 0.0],
      major_radius: 2.0,
      probe_radius: 1.0,
      azimuth_start: 0.0,
      azimuth_sweep: TAU,
      polar_start: 0.75 * PI,
      polar_sweep: 0.5 * PI,
      topology: ToroidalPatchTopology::Regular,
    }
  }

  fn reentrant_patch() -> ReentrantPatch {
    ReentrantPatch {
      face_index: 0,
      probe_center: [0.0, 0.0, 0.0],
      probe_radius: 1.0,
      contact_directions: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
      solid_angle: 0.5 * PI,
    }
  }

  fn full_contact_patch() -> ContactPatchGeometry {
    ContactPatchGeometry {
      atom_index: 0,
      atom_center: [1.0, 2.0, 3.0],
      atom_radius: 1.7,
      probe_radius: 1.4,
      boundary_arcs: Vec::new(),
      boundary_loops: Vec::new(),
      topology: ContactPatchTopology::FullSphere,
      solid_angle: 4.0 * PI,
    }
  }

  #[test]
  fn full_contact_sphere_should_produce_outward_finite_geometry() {
    let patch = full_contact_patch();
    let mesh = tessellate_contact_patch(&patch, MsmsTessellationParameters::default())
      .unwrap_or_else(|error| panic!("full contact sphere should tessellate: {error}"));
    let center = Vec3::from_array(patch.atom_center.map(|component| component as f32));

    assert!(!mesh.indices.is_empty());
    assert!(mesh.vertices.iter().all(|vertex| {
      let position = Vec3::from_slice(&vertex[..3]);
      let normal = Vec3::from_slice(&vertex[3..]);
      ((position - center).length() - patch.atom_radius as f32).abs() < 1.0e-5 && (normal.length() - 1.0).abs() < 1.0e-6
    }));
    assert!(mesh.indices.as_chunks::<3>().0.iter().all(|triangle| {
      let positions = triangle.map(|index| Vec3::from_slice(&mesh.vertices[index as usize][..3]));
      let normal = Vec3::from_slice(&mesh.vertices[triangle[0] as usize][3..]);
      (positions[1] - positions[0])
        .cross(positions[2] - positions[0])
        .dot(normal)
        > 0.0
    }));
  }

  #[test]
  fn greater_than_hemisphere_contact_patch_should_tessellate_without_center_fan() {
    let atoms = vec![
      MsmsAtom {
        atom_index: 0,
        center: DVec3::ZERO,
        radius: 1.0,
      },
      MsmsAtom {
        atom_index: 1,
        center: 2.0 * DVec3::X,
        radius: 1.0,
      },
    ];
    let parameters =
      MsmsParameters::new(1.0).unwrap_or_else(|error| panic!("unit probe radius should be valid: {error}"));
    let edges = build_accessible_probe_edges(&atoms, parameters)
      .unwrap_or_else(|error| panic!("intersecting pair should produce a probe edge: {error}"));
    let patches = build_contact_patch_geometry(&atoms, &edges, parameters)
      .unwrap_or_else(|error| panic!("free edge should produce contact patches: {error}"));
    assert!(patches[0].solid_angle > 2.0 * PI);

    let mesh = tessellate_contact_patch(&patches[0], MsmsTessellationParameters::default())
      .unwrap_or_else(|error| panic!("large contact region should tessellate through an occluded chart: {error}"));

    assert!(!mesh.indices.is_empty());
    assert!(mesh.indices.iter().all(|index| (*index as usize) < mesh.vertices.len()));
  }

  #[test]
  fn nested_contact_boundaries_should_leave_the_inner_region_empty() {
    let loops = vec![
      vec![[-2.0, -2.0], [2.0, -2.0], [2.0, 2.0], [-2.0, 2.0]],
      vec![[-1.0, -1.0], [-1.0, 1.0], [1.0, 1.0], [1.0, -1.0]],
    ];

    let mesh = tessellate_projected_contact_loops(0, &loops)
      .unwrap_or_else(|error| panic!("nested contact boundaries should tessellate: {error}"));

    assert!(!mesh.indices.is_empty());
    assert!(mesh.indices.as_chunks::<3>().0.iter().all(|triangle| {
      let centroid = triangle
        .iter()
        .map(|index| mesh.vertices[*index as usize])
        .fold([0.0_f64; 2], |sum, point| [sum[0] + point[0], sum[1] + point[1]])
        .map(|component| component / 3.0);
      centroid[0].abs() >= 1.0 || centroid[1].abs() >= 1.0
    }));
  }

  #[test]
  fn small_projected_contact_patch_should_survive_scale_normalization() {
    // This is the valid atom-127 contact triangle from 1Y7Q. Its complete
    // projected extent is smaller than Lyon's default absolute tolerance.
    let boundary = vec![
      [-1.882_734_310_366_298_8, -0.100_714_723_050_975_09],
      [-1.883_959_983_151_179_7, -0.074_341_917_884_194_17],
      [-1.855_686_055_337_787_9, -0.106_516_549_641_048_95],
    ];

    let mesh = tessellate_projected_contact_loops(127, &[boundary])
      .unwrap_or_else(|error| panic!("the small valid contact patch should tessellate: {error}"));

    assert_eq!(mesh.indices.len(), 3);
  }

  #[test]
  fn contact_refinement_should_bound_shared_spherical_edges() {
    let boundary = (0..64)
      .map(|index| {
        let angle = TAU * index as f64 / 64.0;
        [0.8 * angle.cos(), 0.8 * angle.sin()]
      })
      .collect::<Vec<_>>();
    let mut planar = tessellate_projected_contact_loops(0, &[boundary])
      .unwrap_or_else(|error| panic!("the circular boundary should tessellate: {error}"));
    let pole = DVec3::Z;
    let (horizontal, vertical) = stereographic_basis(0, pole)
      .unwrap_or_else(|error| panic!("a unit projection pole should define a chart: {error}"));
    let original_boundary = mesh_edge_use_counts(&planar.indices)
      .into_iter()
      .filter_map(|(edge, uses)| (uses == 1).then_some(edge))
      .collect::<BTreeSet<_>>();

    refine_projected_contact_mesh(0, &mut planar, pole, horizontal, vertical, 1.7, 0.35)
      .unwrap_or_else(|error| panic!("the circular interior should refine conformingly: {error}"));

    let refined_edge_uses = mesh_edge_use_counts(&planar.indices);
    assert!(
      original_boundary
        .iter()
        .all(|edge| refined_edge_uses.get(edge) == Some(&1))
    );
    assert!(refined_edge_uses.iter().all(|(&edge, &uses)| {
      if uses < 2 {
        return true;
      }
      let directions = contact_edge_directions(&planar, edge, pole, horizontal, vertical)
        .unwrap_or_else(|error| panic!("refined edge should map to the sphere: {error}"));
      1.7 * directions[0].distance(directions[1]) <= 0.35 * (1.0 + 1.0e-6)
    }));
  }

  #[test]
  fn regular_torus_should_produce_finite_indexed_geometry() {
    let mesh = tessellate_regular_toroidal_patch(&regular_patch(), MsmsTessellationParameters::default())
      .unwrap_or_else(|error| panic!("regular torus should tessellate: {error}"));

    assert!(!mesh.vertices.is_empty());
    assert!(!mesh.indices.is_empty());
    assert!(mesh.indices.iter().all(|index| (*index as usize) < mesh.vertices.len()));
    assert!(mesh.vertices.iter().flatten().all(|component| component.is_finite()));
    assert!(mesh.indices.as_chunks::<3>().0.iter().all(|triangle| {
      let positions = triangle.map(|index| Vec3::from_slice(&mesh.vertices[index as usize][..3]));
      (positions[1] - positions[0])
        .cross(positions[2] - positions[0])
        .length_squared()
        > 1.0e-12
    }));
  }

  #[test]
  fn torus_triangle_winding_should_agree_with_vertex_normals() {
    let mesh = tessellate_regular_toroidal_patch(&regular_patch(), MsmsTessellationParameters::default())
      .unwrap_or_else(|error| panic!("regular torus should tessellate: {error}"));
    let triangle = &mesh.indices[..3];
    let positions = [
      Vec3::from_slice(&mesh.vertices[triangle[0] as usize][..3]),
      Vec3::from_slice(&mesh.vertices[triangle[1] as usize][..3]),
      Vec3::from_slice(&mesh.vertices[triangle[2] as usize][..3]),
    ];
    let geometric_normal = (positions[1] - positions[0])
      .cross(positions[2] - positions[0])
      .normalize();
    let vertex_normal = Vec3::from_slice(&mesh.vertices[triangle[0] as usize][3..]);

    assert!(geometric_normal.dot(vertex_normal) > 0.0);
  }

  #[test]
  fn singular_torus_should_require_trimming_before_tessellation() {
    let mut patch = regular_patch();
    patch.topology = ToroidalPatchTopology::SelfIntersecting;

    assert_eq!(
      tessellate_regular_toroidal_patch(&patch, MsmsTessellationParameters::default()),
      Err(MsmsTessellationError::UntrimmedSingularTorus { edge_index: 0 })
    );
  }

  #[test]
  fn radial_singularity_should_split_into_renderable_toric_faces() {
    let mut patch = regular_patch();
    patch.major_radius = 0.5;
    patch.probe_radius = 1.0;
    patch.polar_start = 0.0;
    patch.polar_sweep = TAU;
    patch.topology = ToroidalPatchTopology::SelfIntersecting;

    let mesh = tessellate_toroidal_patch(&patch, MsmsTessellationParameters::default())
      .unwrap_or_else(|error| panic!("radial singularity should split before tessellation: {error}"));

    assert!(!mesh.indices.is_empty());
    assert!(mesh.indices.iter().all(|index| (*index as usize) < mesh.vertices.len()));
    assert!(mesh.vertices.iter().flatten().all(|component| component.is_finite()));
    assert!(mesh.indices.as_chunks::<3>().0.iter().all(|triangle| {
      let positions = triangle.map(|index| Vec3::from_slice(&mesh.vertices[index as usize][..3]));
      (positions[1] - positions[0])
        .cross(positions[2] - positions[0])
        .length_squared()
        > 1.0e-12
    }));
  }

  #[test]
  fn overlapping_fixed_probe_should_clip_reentrant_display_mesh() {
    let mut mesh = SurfaceMesh {
      vertices: vec![
        [1.0, 0.0, 0.0, -1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0, -1.0, 0.0],
      ],
      indices: vec![0, 1, 2],
    };

    clip_reentrant_mesh_outside_probes(&mut mesh, DVec3::ZERO, &[(DVec3::X, 1.0)])
      .unwrap_or_else(|error| panic!("overlapping fixed probe should clip display triangles: {error}"));

    assert_eq!(mesh.indices.len(), 6);
    assert!(mesh.vertices.iter().all(|vertex| vertex[0] <= 0.500_001));
  }

  #[test]
  fn coincident_patch_boundaries_should_weld_into_one_shared_edge() {
    let normal = [0.0, 0.0, 1.0];
    let mut mesh = SurfaceMesh {
      vertices: vec![
        [0.0, 0.0, 0.0, normal[0], normal[1], normal[2]],
        [1.0, 0.0, 0.0, normal[0], normal[1], normal[2]],
        [0.0, 1.0, 0.0, normal[0], normal[1], normal[2]],
        [1.0 + 5.0e-5, 0.0, 0.0, normal[0], normal[1], normal[2]],
        [1.0, 1.0, 0.0, normal[0], normal[1], normal[2]],
        [0.0, 1.0 + 5.0e-5, 0.0, normal[0], normal[1], normal[2]],
      ],
      indices: vec![0, 1, 2, 3, 4, 5],
    };

    weld_coincident_vertices(&mut mesh)
      .unwrap_or_else(|error| panic!("round-off-scale boundary differences should weld: {error}"));

    let edge_uses = mesh_edge_use_counts(&mesh.indices);
    assert_eq!(mesh.vertices.len(), 4);
    assert_eq!(edge_uses.values().filter(|&&uses| uses == 2).count(), 1);
  }

  #[test]
  fn reentrant_triangle_should_produce_probe_sphere_vertices() {
    let patch = reentrant_patch();
    let mesh = tessellate_reentrant_patch(&patch, MsmsTessellationParameters::default())
      .unwrap_or_else(|error| panic!("reentrant triangle should tessellate: {error}"));

    assert!(mesh.vertices.iter().all(|vertex| {
      let position = Vec3::from_slice(&vertex[..3]);
      (position.length() - patch.probe_radius as f32).abs() < 1.0e-6
    }));
  }

  #[test]
  fn reentrant_triangle_should_bound_every_display_edge() {
    let parameters = MsmsTessellationParameters::default();
    let mesh = tessellate_reentrant_patch(&reentrant_patch(), parameters)
      .unwrap_or_else(|error| panic!("reentrant triangle should refine: {error}"));

    assert!(mesh_edge_use_counts(&mesh.indices).keys().all(|&(start, end)| {
      let start = Vec3::from_slice(&mesh.vertices[start as usize][..3]);
      let end = Vec3::from_slice(&mesh.vertices[end as usize][..3]);
      start.distance(end) <= parameters.max_edge_length() as f32 * 1.000_01
    }));
  }

  #[test]
  fn reentrant_triangle_winding_should_agree_with_vertex_normals() {
    let mesh = tessellate_reentrant_patch(&reentrant_patch(), MsmsTessellationParameters::default())
      .unwrap_or_else(|error| panic!("reentrant triangle should tessellate: {error}"));

    assert!(mesh.indices.as_chunks::<3>().0.iter().all(|triangle| {
      let positions = [
        Vec3::from_slice(&mesh.vertices[triangle[0] as usize][..3]),
        Vec3::from_slice(&mesh.vertices[triangle[1] as usize][..3]),
        Vec3::from_slice(&mesh.vertices[triangle[2] as usize][..3]),
      ];
      let geometric_normal = (positions[1] - positions[0]).cross(positions[2] - positions[0]);
      let vertex_normal = Vec3::from_slice(&mesh.vertices[triangle[0] as usize][3..]);
      geometric_normal.dot(vertex_normal) > 0.0
    }));
  }
}
