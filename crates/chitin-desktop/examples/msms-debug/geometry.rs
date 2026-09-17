//! Presentation-only geometry for analytical MSMS intermediates.

use std::f64::consts::TAU;

use chitin_bio::{
  structure::StructureScene,
  surface::{
    MolecularSurfaceArtifact, SurfaceDomainArtifact, SurfaceGeometrySource, SurfaceMesh,
    msms::{
      MsmsPatchGeometryDomain, MsmsTessellationParameters, ReducedSurfaceEdge, tessellate_contact_patch,
      tessellate_resolved_reentrant_patches, tessellate_toroidal_patch,
    },
  },
};
use glam::{DVec3, Vec3};

/// Target spacing between diagnostic markers along a rolling-probe arc.
const PROBE_ARC_MARKER_SPACING: f64 = 0.65;
/// Radius of the closed octahedra used to show rolling-probe trajectories.
const PROBE_ARC_MARKER_RADIUS: f32 = 0.10;
/// Radius of fixed-probe-center markers.
const PROBE_CENTER_MARKER_RADIUS: f32 = 0.28;
/// Radius of the tubes outlining reduced-surface faces.
const PROBE_FACE_EDGE_RADIUS: f32 = 0.045;
/// Radial segments used by each reduced-surface face tube.
const PROBE_FACE_EDGE_SIDES: usize = 8;

/// Meshes representing the observable stages of one MSMS domain.
pub(super) struct MsmsDebugMeshes {
  /// Accessible rolling-probe trajectories from reduced-surface edges.
  pub probe_arcs: SurfaceMesh,
  /// Wireframe reduced-surface faces and their fixed probe centers.
  pub probe_faces: SurfaceMesh,
  /// Convex atom-contact patch tessellation.
  pub contact_patches: SurfaceMesh,
  /// Saddle-shaped rolling-probe patch tessellation.
  pub toroidal_patches: SurfaceMesh,
  /// Concave fixed-probe patches after cross-probe clipping.
  pub reentrant_patches: SurfaceMesh,
}

/// Builds every renderable MSMS checkpoint from one analytical domain.
///
/// # Parameters
///
/// * `domain` contains reduced-surface topology and analytical patches.
/// * `scene` supplies atom centers for reduced-surface face visualization.
/// * `parameters` controls display tessellation without changing exact areas.
///
/// # Returns
///
/// Diagnostic meshes for topology and each resolved patch family, or a
/// readable error when a mesh exceeds the index range or an analytical patch
/// cannot be tessellated.
pub(super) fn build_debug_meshes(
  domain: &MsmsPatchGeometryDomain,
  scene: &StructureScene,
  parameters: MsmsTessellationParameters,
) -> Result<MsmsDebugMeshes, String> {
  let probe_arcs = probe_arc_mesh(&domain.topology.edges)?;
  let mut probe_faces = SurfaceMesh::default();
  for face in &domain.topology.faces {
    let atom_positions = face
      .atom_indices
      .map(|atom_index| scene.atoms.get(atom_index).map(|atom| Vec3::from_array(atom.position)));
    if let [Some(first), Some(second), Some(third)] = atom_positions {
      append_wireframe_triangle(&mut probe_faces, [first, second, third])?;
    }
    append_octahedron(
      &mut probe_faces,
      Vec3::from_array(face.probe_center.map(|component| component as f32)),
      PROBE_CENTER_MARKER_RADIUS,
    )?;
  }

  let mut contact_patches = SurfaceMesh::default();
  for patch in &domain.contact_patches {
    append_mesh(
      &mut contact_patches,
      tessellate_contact_patch(patch, parameters).map_err(|error| error.to_string())?,
    )?;
  }
  let mut toroidal_patches = SurfaceMesh::default();
  for patch in &domain.toroidal_patches {
    append_mesh(
      &mut toroidal_patches,
      tessellate_toroidal_patch(patch, parameters).map_err(|error| error.to_string())?,
    )?;
  }
  let reentrant_patches =
    tessellate_resolved_reentrant_patches(domain, parameters).map_err(|error| error.to_string())?;

  Ok(MsmsDebugMeshes {
    probe_arcs,
    probe_faces,
    contact_patches,
    toroidal_patches,
    reentrant_patches,
  })
}

/// Outlines one reduced-surface face without filling its interior.
fn append_wireframe_triangle(mesh: &mut SurfaceMesh, points: [Vec3; 3]) -> Result<(), String> {
  for [start, end] in [[points[0], points[1]], [points[1], points[2]], [points[2], points[0]]] {
    append_tube(mesh, start, end, PROBE_FACE_EDGE_RADIUS, PROBE_FACE_EDGE_SIDES)?;
  }
  Ok(())
}

/// Appends the side wall of one round tube between arbitrary 3-D points.
///
/// # Parameters
///
/// * `mesh` receives generated vertices and consistently oriented indices.
/// * `start` and `end` define the tube axis in molecular coordinates.
/// * `radius` is the tube radius in ångströms.
/// * `side_count` controls the radial tessellation and must be at least three.
///
/// # Returns
///
/// `Ok(())` after appending the tube, or an error when the segment is
/// degenerate, the radial tessellation is invalid, or indices overflow `u32`.
fn append_tube(mesh: &mut SurfaceMesh, start: Vec3, end: Vec3, radius: f32, side_count: usize) -> Result<(), String> {
  let delta = end - start;
  let axis = delta.normalize_or_zero();
  if axis.length_squared() <= f32::EPSILON {
    return Err("reduced-surface face contains a degenerate edge".to_string());
  }
  if side_count < 3 {
    return Err("reduced-surface face tube requires at least three sides".to_string());
  }
  let vertex_count = side_count
    .checked_mul(2)
    .ok_or_else(|| "MSMS debug mesh exceeds u32 indices".to_string())?;
  let base = u32::try_from(mesh.vertices.len()).map_err(|_| "MSMS debug mesh exceeds u32 indices".to_string())?;
  if vertex_count > (u32::MAX - base) as usize {
    return Err("MSMS debug mesh exceeds u32 indices".to_string());
  }

  let reference = if axis.x.abs() < 0.9 { Vec3::X } else { Vec3::Y };
  let horizontal = axis.cross(reference).normalize_or_zero();
  let vertical = axis.cross(horizontal).normalize_or_zero();
  for position in [start, end] {
    for side in 0..side_count {
      let angle = TAU as f32 * side as f32 / side_count as f32;
      let normal = angle.cos() * horizontal + angle.sin() * vertical;
      let vertex_position = position + radius * normal;
      mesh.vertices.push([
        vertex_position.x,
        vertex_position.y,
        vertex_position.z,
        normal.x,
        normal.y,
        normal.z,
      ]);
    }
  }
  for side in 0..side_count {
    let next = (side + 1) % side_count;
    let start_current = base + side as u32;
    let start_next = base + next as u32;
    let end_current = base + side_count as u32 + side as u32;
    let end_next = base + side_count as u32 + next as u32;
    push_triangle_by_vertex_normals(mesh, [start_current, end_current, end_next]);
    push_triangle_by_vertex_normals(mesh, [start_current, end_next, start_next]);
  }
  Ok(())
}

/// Wraps one diagnostic mesh in the shared renderer-neutral surface artifact.
pub(super) fn single_surface(mesh: SurfaceMesh, probe_radius: f64, max_edge_length: f64) -> MolecularSurfaceArtifact {
  MolecularSurfaceArtifact {
    source: SurfaceGeometrySource::Msms {
      probe_radius,
      max_edge_length,
    },
    domains: vec![SurfaceDomainArtifact { chain_id: None, mesh }],
  }
}

/// Samples accessible probe-center arcs into closed diagnostic markers.
fn probe_arc_mesh(edges: &[ReducedSurfaceEdge]) -> Result<SurfaceMesh, String> {
  let mut mesh = SurfaceMesh::default();
  for edge in edges {
    let arc_length = edge.probe_circle_radius * edge.sweep_angle;
    let segment_count = (arc_length / PROBE_ARC_MARKER_SPACING).ceil().max(4.0) as usize;
    let center = DVec3::from_array(edge.probe_circle_center);
    let axis = DVec3::from_array(edge.probe_circle_axis);
    let basis = DVec3::from_array(edge.probe_circle_basis);
    let perpendicular = axis.cross(basis);
    let includes_duplicate_endpoint = (edge.sweep_angle - TAU).abs() > 1.0e-10;
    let sample_count = segment_count + usize::from(includes_duplicate_endpoint);
    for sample_index in 0..sample_count {
      let fraction = sample_index as f64 / segment_count as f64;
      let angle = edge.start_angle + edge.sweep_angle * fraction;
      let point = center + edge.probe_circle_radius * (angle.cos() * basis + angle.sin() * perpendicular);
      append_octahedron(&mut mesh, point.as_vec3(), PROBE_ARC_MARKER_RADIUS)?;
    }
  }
  Ok(mesh)
}

/// Moves one indexed mesh into another while preserving global indices.
fn append_mesh(target: &mut SurfaceMesh, source: SurfaceMesh) -> Result<(), String> {
  let offset = u32::try_from(target.vertices.len()).map_err(|_| "MSMS debug mesh exceeds u32 indices".to_string())?;
  if source.vertices.len() > (u32::MAX - offset) as usize {
    return Err("MSMS debug mesh exceeds u32 indices".to_string());
  }
  target.vertices.extend(source.vertices);
  target
    .indices
    .extend(source.indices.into_iter().map(|index| index + offset));
  Ok(())
}

/// Appends one closed octahedron used as a depth-stable point marker.
fn append_octahedron(mesh: &mut SurfaceMesh, center: Vec3, radius: f32) -> Result<(), String> {
  let top = center + Vec3::Y * radius;
  let bottom = center - Vec3::Y * radius;
  let equator = [
    center + Vec3::X * radius,
    center + Vec3::Z * radius,
    center - Vec3::X * radius,
    center - Vec3::Z * radius,
  ];
  for index in 0..equator.len() {
    let next = (index + 1) % equator.len();
    append_outward_triangle(mesh, center, top, equator[index], equator[next])?;
    append_outward_triangle(mesh, center, bottom, equator[next], equator[index])?;
  }
  Ok(())
}

/// Appends one outward-facing diagnostic triangle.
fn append_outward_triangle(
  mesh: &mut SurfaceMesh,
  center: Vec3,
  first: Vec3,
  mut second: Vec3,
  mut third: Vec3,
) -> Result<(), String> {
  let start = u32::try_from(mesh.vertices.len()).map_err(|_| "MSMS debug mesh exceeds u32 indices".to_string())?;
  if start > u32::MAX - 3 {
    return Err("MSMS debug mesh exceeds u32 indices".to_string());
  }
  let mut normal = (second - first).cross(third - first).normalize_or_zero();
  if normal.dot((first + second + third) / 3.0 - center) < 0.0 {
    std::mem::swap(&mut second, &mut third);
    normal = -normal;
  }
  for position in [first, second, third] {
    mesh
      .vertices
      .push([position.x, position.y, position.z, normal.x, normal.y, normal.z]);
  }
  mesh.indices.extend([start, start + 1, start + 2]);
  Ok(())
}

/// Appends existing vertex indices after matching winding to their normals.
fn push_triangle_by_vertex_normals(mesh: &mut SurfaceMesh, mut triangle: [u32; 3]) {
  let positions = triangle.map(|index| Vec3::from_slice(&mesh.vertices[index as usize][..3]));
  let desired_normal = triangle
    .map(|index| Vec3::from_slice(&mesh.vertices[index as usize][3..]))
    .into_iter()
    .sum::<Vec3>();
  if (positions[1] - positions[0])
    .cross(positions[2] - positions[0])
    .dot(desired_normal)
    < 0.0
  {
    triangle.swap(1, 2);
  }
  mesh.indices.extend(triangle);
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn wireframe_triangle_should_not_fill_its_interior() {
    let points = [Vec3::ZERO, 2.0 * Vec3::X, 2.0 * Vec3::Y];
    let center = points.into_iter().sum::<Vec3>() / 3.0;
    let mut mesh = SurfaceMesh::default();
    append_wireframe_triangle(&mut mesh, points)
      .unwrap_or_else(|error| panic!("valid reduced-surface face should produce wireframe geometry: {error}"));

    assert!(
      mesh
        .vertices
        .iter()
        .all(|vertex| Vec3::from_slice(&vertex[..3]).distance(center) > 0.35)
    );
  }
}
