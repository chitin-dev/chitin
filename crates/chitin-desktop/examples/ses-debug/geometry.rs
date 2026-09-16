//! Presentation-only geometry for visualizing scientific SES intermediates.

use chitin_bio::{
  structure::StructureScene,
  surface::{
    MolecularSurfaceArtifact, MolecularSurfaceRequest, ScalarFieldGrid, SurfaceDomainArtifact, SurfaceGeometrySource,
    SurfaceMesh, SurfacePartition,
  },
};

/// Maximum number of displayed lattice coordinates on any one axis.
const MAX_VISIBLE_GRID_STEPS: usize = 9;
/// Half-width of the solid bars used to visualize lattice lines.
const GRID_LINE_HALF_WIDTH: f32 = 0.06;

/// Wraps one diagnostic mesh in the production surface artifact type.
pub(super) fn single_surface(mesh: SurfaceMesh) -> MolecularSurfaceArtifact {
  MolecularSurfaceArtifact {
    source: SurfaceGeometrySource::ImplicitGrid(MolecularSurfaceRequest {
      partition: SurfacePartition::Unified,
      ..MolecularSurfaceRequest::default()
    }),
    domains: vec![SurfaceDomainArtifact { chain_id: None, mesh }],
  }
}

/// Appends one position/normal vertex and returns its mesh index.
fn vertex(mesh: &mut SurfaceMesh, position: [f32; 3], normal: [f32; 3]) -> u32 {
  let index = mesh.vertices.len() as u32;
  mesh
    .vertices
    .push([position[0], position[1], position[2], normal[0], normal[1], normal[2]]);
  index
}

/// Appends a double-sided quad with opposite normals on each side.
///
/// The production surface pipeline culls back faces because an SES is a closed,
/// consistently oriented manifold. Diagnostic grids and scalar slices are open
/// sheets, so they need explicit front and back faces to remain visible while
/// the camera rotates around them.
fn quad(mesh: &mut SurfaceMesh, a: [f32; 3], b: [f32; 3], c: [f32; 3], d: [f32; 3], normal: [f32; 3]) {
  let front = vertex(mesh, a, normal);
  vertex(mesh, b, normal);
  vertex(mesh, c, normal);
  vertex(mesh, d, normal);
  mesh
    .indices
    .extend_from_slice(&[front, front + 1, front + 2, front, front + 2, front + 3]);

  let back_normal = [-normal[0], -normal[1], -normal[2]];
  let back = vertex(mesh, a, back_normal);
  vertex(mesh, d, back_normal);
  vertex(mesh, c, back_normal);
  vertex(mesh, b, back_normal);
  mesh
    .indices
    .extend_from_slice(&[back, back + 1, back + 2, back, back + 2, back + 3]);
}

/// Builds a sparse three-dimensional Cartesian sampling lattice.
///
/// The full SES grid can contain hundreds of thousands of points, so the
/// example preserves its bounds and Cartesian topology while displaying at
/// most [`MAX_VISIBLE_GRID_STEPS`] coordinates per axis. Each line is a solid
/// rectangular prism rather than a zero-thickness sheet. That gives depth
/// testing stable geometry and prevents view-dependent disappearance or
/// flickering when the camera rotates.
///
/// # Parameters
///
/// * `volume` supplies the exact bounds and spacing of the sampled scalar field.
///
/// # Returns
///
/// A closed triangle mesh containing lines parallel to all three axes.
pub(super) fn grid_mesh(grid: &ScalarFieldGrid) -> SurfaceMesh {
  let mut mesh = SurfaceMesh::default();
  let min = glam::Vec3::from_array(grid.bounds_min);
  let max = glam::Vec3::from_array(grid.bounds_max());
  let coordinates = [
    visible_grid_coordinates(min.x, max.x, grid.spacing),
    visible_grid_coordinates(min.y, max.y, grid.spacing),
    visible_grid_coordinates(min.z, max.z, grid.spacing),
  ];

  // Lines parallel to X are repeated over the sampled YZ lattice, and the
  // other two passes apply the same construction cyclically.
  for axis in 0..3 {
    let other = (axis + 1) % 3;
    let third = (axis + 2) % 3;
    for &other_coordinate in &coordinates[other] {
      for &third_coordinate in &coordinates[third] {
        let mut start = min;
        let mut end = min;
        start[other] = other_coordinate;
        end[other] = other_coordinate;
        start[third] = third_coordinate;
        end[third] = third_coordinate;
        end[axis] = max[axis];
        append_axis_bar(&mut mesh, start, end, axis, GRID_LINE_HALF_WIDTH);
      }
    }
  }
  mesh
}

/// Selects evenly distributed coordinates from one sampled volume axis.
pub(super) fn visible_grid_coordinates(minimum: f32, maximum: f32, spacing: f32) -> Vec<f32> {
  let interval_count = ((maximum - minimum) / spacing).round().max(1.0) as usize;
  let stride = interval_count.div_ceil(MAX_VISIBLE_GRID_STEPS - 1).max(1);
  let mut coordinates = (0..=interval_count)
    .step_by(stride)
    .map(|index| (minimum + index as f32 * spacing).min(maximum))
    .collect::<Vec<_>>();
  if coordinates.last().is_none_or(|coordinate| *coordinate < maximum) {
    coordinates.push(maximum);
  }
  coordinates
}

/// Appends a closed axis-aligned prism between two lattice coordinates.
fn append_axis_bar(mesh: &mut SurfaceMesh, start: glam::Vec3, end: glam::Vec3, axis: usize, half_width: f32) {
  let mut minimum = start.min(end);
  let mut maximum = start.max(end);
  for component in 0..3 {
    if component != axis {
      minimum[component] -= half_width;
      maximum[component] += half_width;
    }
  }
  append_box(mesh, minimum, maximum);
}

/// Appends the six faces of an axis-aligned box to a diagnostic mesh.
fn append_box(mesh: &mut SurfaceMesh, minimum: glam::Vec3, maximum: glam::Vec3) {
  let [x0, y0, z0] = minimum.to_array();
  let [x1, y1, z1] = maximum.to_array();
  quad(
    mesh,
    [x0, y0, z0],
    [x0, y1, z0],
    [x0, y1, z1],
    [x0, y0, z1],
    [-1.0, 0.0, 0.0],
  );
  quad(
    mesh,
    [x1, y0, z0],
    [x1, y0, z1],
    [x1, y1, z1],
    [x1, y1, z0],
    [1.0, 0.0, 0.0],
  );
  quad(
    mesh,
    [x0, y0, z0],
    [x0, y0, z1],
    [x1, y0, z1],
    [x1, y0, z0],
    [0.0, -1.0, 0.0],
  );
  quad(
    mesh,
    [x0, y1, z0],
    [x1, y1, z0],
    [x1, y1, z1],
    [x0, y1, z1],
    [0.0, 1.0, 0.0],
  );
  quad(
    mesh,
    [x0, y0, z0],
    [x1, y0, z0],
    [x1, y1, z0],
    [x0, y1, z0],
    [0.0, 0.0, -1.0],
  );
  quad(
    mesh,
    [x0, y0, z1],
    [x0, y1, z1],
    [x1, y1, z1],
    [x1, y0, z1],
    [0.0, 0.0, 1.0],
  );
}

/// Reproduces the molecule renderer's source-to-fitted transform for overlays.
pub(super) fn scene_fit_transform(scene: &StructureScene) -> glam::Mat4 {
  let center = glam::Vec3::from_array(scene.bounds.center());
  let scale = 0.90 / scene.bounds.radius().max(1.0);
  glam::Mat4::from_scale(glam::Vec3::splat(scale)) * glam::Mat4::from_translation(-center)
}

/// Builds closed octahedral markers at the extracted rolling-probe centers.
///
/// # Parameters
///
/// * `grid` supplies the spacing used to size markers.
/// * `centers` contains the merged rolling-probe positions.
///
/// # Returns
///
/// A closed triangle mesh that remains visible from every camera direction.
pub(super) fn probe_center_mesh(grid: &ScalarFieldGrid, centers: &[[f32; 3]]) -> SurfaceMesh {
  let mut mesh = SurfaceMesh::default();
  let radius = (0.32 * grid.spacing).clamp(0.12, 0.35);
  for center in centers {
    append_octahedron(&mut mesh, glam::Vec3::from_array(*center), radius);
  }
  mesh
}

/// Combines the preceding sampling grid with the probe-center markers.
pub(super) fn probe_center_stage_mesh(grid: &ScalarFieldGrid, centers: &[[f32; 3]]) -> SurfaceMesh {
  let mut mesh = grid_mesh(grid);
  append_mesh(&mut mesh, probe_center_mesh(grid, centers));
  mesh
}

/// Combines the sampling grid with one extracted probe-field surface.
///
/// # Parameters
///
/// * `grid` supplies the preceding sampling-grid geometry.
/// * `surface` is the raw or inner second-field isosurface to overlay.
///
/// # Returns
///
/// One diagnostic mesh containing the grid and extracted surface, without the
/// probe-center markers used by the preceding stages.
pub(super) fn probe_surface_stage_mesh(grid: &ScalarFieldGrid, surface: SurfaceMesh) -> SurfaceMesh {
  let mut mesh = grid_mesh(grid);
  append_mesh(&mut mesh, surface);
  mesh
}

/// Moves one mesh into another while rebasing its triangle indices.
fn append_mesh(target: &mut SurfaceMesh, source: SurfaceMesh) {
  let vertex_offset = target.vertices.len() as u32;
  target.vertices.extend(source.vertices);
  target
    .indices
    .extend(source.indices.into_iter().map(|index| index + vertex_offset));
}

/// Appends one closed octahedral point marker.
fn append_octahedron(mesh: &mut SurfaceMesh, center: glam::Vec3, radius: f32) {
  let top = center + glam::Vec3::Y * radius;
  let bottom = center - glam::Vec3::Y * radius;
  let equator = [
    center + glam::Vec3::X * radius,
    center + glam::Vec3::Z * radius,
    center - glam::Vec3::X * radius,
    center - glam::Vec3::Z * radius,
  ];
  for index in 0..equator.len() {
    let next = (index + 1) % equator.len();
    append_outward_triangle(mesh, center, top, equator[index], equator[next]);
    append_outward_triangle(mesh, center, bottom, equator[next], equator[index]);
  }
}

/// Appends one consistently outward-facing triangle to a closed marker.
fn append_outward_triangle(
  mesh: &mut SurfaceMesh,
  center: glam::Vec3,
  a: glam::Vec3,
  mut b: glam::Vec3,
  mut c: glam::Vec3,
) {
  let mut normal = (b - a).cross(c - a).normalize_or_zero();
  if normal.dot((a + b + c) / 3.0 - center) < 0.0 {
    std::mem::swap(&mut b, &mut c);
    normal = -normal;
  }
  let start = vertex(mesh, a.to_array(), normal.to_array());
  vertex(mesh, b.to_array(), normal.to_array());
  vertex(mesh, c.to_array(), normal.to_array());
  mesh.indices.extend_from_slice(&[start, start + 1, start + 2]);
}
