//! Display tessellation of analytical MSMS patches.
//!
//! Scientific areas remain properties of the analytical patches. Mesh
//! resolution affects only rendering and exported display geometry.

use std::f64::consts::{PI, TAU};

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
/// analytical coordinates. Coincident boundary samples remain duplicated until
/// the later welding stage, but they use the same subdivision contract and
/// therefore occupy identical positions. A singular torus stops the entire
/// operation so an incomplete surface cannot be mistaken for a valid result.
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
    append_mesh(&mut combined, tessellate_regular_toroidal_patch(patch, parameters)?)?;
  }
  for patch in &domain.reentrant_patches {
    append_mesh(&mut combined, tessellate_reentrant_patch(patch, parameters)?)?;
  }
  Ok(combined)
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
  tessellate_reentrant_fan(patch, center_direction, &boundary)
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

/// Triangulates every disconnected exposed loop through a bounded chart.
fn tessellate_bounded_contact_patch(
  patch: &ContactPatchGeometry,
  parameters: MsmsTessellationParameters,
) -> Result<SurfaceMesh, MsmsTessellationError> {
  let center = DVec3::from_array(patch.atom_center);
  let mut mesh = SurfaceMesh::default();
  for boundary_loop in &patch.boundary_loops {
    let directions = sample_contact_loop(patch, &boundary_loop.arcs, parameters)?;
    if directions.len() < 3 {
      return Err(MsmsTessellationError::InvalidContactPatch {
        atom_index: patch.atom_index,
      });
    }
    let first_arc = &patch.boundary_arcs[boundary_loop.arcs[0].arc_index];
    let pole = DVec3::from_array(first_arc.occluded_direction).normalize_or_zero();
    let projected = stereographic_project_loop(patch.atom_index, &directions, pole)?;
    let triangles = ear_clip_polygon(patch.atom_index, &projected)?;
    let base_index = u32::try_from(mesh.vertices.len()).map_err(|_| MsmsTessellationError::MeshTooLarge)?;
    for direction in directions {
      mesh
        .vertices
        .push(contact_vertex(center, patch.atom_radius, direction)?);
    }
    for triangle in triangles {
      let triangle = triangle.map(|index| base_index + index as u32);
      push_outward_triangle(&mut mesh, triangle);
    }
  }
  Ok(mesh)
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
) -> Result<Vec<[f64; 2]>, MsmsTessellationError> {
  if !pole.is_finite() || pole.length_squared() <= f64::EPSILON {
    return Err(MsmsTessellationError::InvalidContactPatch { atom_index });
  }
  let reference = if pole.z.abs() < 0.9 { DVec3::Z } else { DVec3::Y };
  let horizontal = pole.cross(reference).normalize_or_zero();
  let vertical = pole.cross(horizontal).normalize_or_zero();
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

/// Ear-clips one simple projected polygon into local vertex indices.
fn ear_clip_polygon(atom_index: usize, polygon: &[[f64; 2]]) -> Result<Vec<[usize; 3]>, MsmsTessellationError> {
  if polygon.len() < 3 {
    return Err(MsmsTessellationError::InvalidContactPatch { atom_index });
  }
  let signed_area = polygon
    .iter()
    .enumerate()
    .map(|(index, point)| {
      let next = polygon[(index + 1) % polygon.len()];
      point[0] * next[1] - next[0] * point[1]
    })
    .sum::<f64>();
  if !signed_area.is_finite() || signed_area.abs() <= 1.0e-14 {
    return Err(MsmsTessellationError::InvalidContactPatch { atom_index });
  }
  let mut remaining = if signed_area > 0.0 {
    (0..polygon.len()).collect::<Vec<_>>()
  } else {
    (0..polygon.len()).rev().collect::<Vec<_>>()
  };
  let mut triangles = Vec::with_capacity(polygon.len() - 2);
  while remaining.len() > 3 {
    let mut clipped = false;
    for cursor in 0..remaining.len() {
      let previous = remaining[(cursor + remaining.len() - 1) % remaining.len()];
      let current = remaining[cursor];
      let next = remaining[(cursor + 1) % remaining.len()];
      if orient_2d(polygon[previous], polygon[current], polygon[next]) <= 1.0e-14 {
        continue;
      }
      let contains_vertex = remaining.iter().copied().any(|candidate| {
        candidate != previous
          && candidate != current
          && candidate != next
          && point_in_triangle(polygon[candidate], polygon[previous], polygon[current], polygon[next])
      });
      if contains_vertex {
        continue;
      }
      triangles.push([previous, current, next]);
      remaining.remove(cursor);
      clipped = true;
      break;
    }
    if !clipped {
      return Err(MsmsTessellationError::InvalidContactPatch { atom_index });
    }
  }
  triangles.push([remaining[0], remaining[1], remaining[2]]);
  Ok(triangles)
}

/// Returns the signed twice-area of an oriented 2-D triangle.
fn orient_2d(first: [f64; 2], second: [f64; 2], third: [f64; 2]) -> f64 {
  (second[0] - first[0]) * (third[1] - first[1]) - (second[1] - first[1]) * (third[0] - first[0])
}

/// Tests whether a point lies inside or on a counter-clockwise triangle.
fn point_in_triangle(point: [f64; 2], first: [f64; 2], second: [f64; 2], third: [f64; 2]) -> bool {
  orient_2d(first, second, point) >= -1.0e-14
    && orient_2d(second, third, point) >= -1.0e-14
    && orient_2d(third, first, point) >= -1.0e-14
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
  Ok(mesh)
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
  fn regular_torus_should_produce_finite_indexed_geometry() {
    let mesh = tessellate_regular_toroidal_patch(&regular_patch(), MsmsTessellationParameters::default())
      .unwrap_or_else(|error| panic!("regular torus should tessellate: {error}"));

    assert!(!mesh.vertices.is_empty());
    assert!(!mesh.indices.is_empty());
    assert!(mesh.indices.iter().all(|index| (*index as usize) < mesh.vertices.len()));
    assert!(mesh.vertices.iter().flatten().all(|component| component.is_finite()));
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
