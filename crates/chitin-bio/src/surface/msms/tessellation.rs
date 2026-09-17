//! Display tessellation of analytical MSMS patches.
//!
//! Scientific areas remain properties of the analytical patches. Mesh
//! resolution affects only rendering and exported display geometry.

use thiserror::Error;

use glam::{DVec3, Vec3};

use super::{ReentrantPatch, ToroidalPatchGeometry, ToroidalPatchTopology};
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

  use glam::Vec3;

  use super::*;

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
