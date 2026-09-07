//! CPU tessellation for a rolling-probe solvent-excluded surface (SES).

use std::collections::{HashMap, VecDeque};

use chitin_bio::structure::{ElementCategory, StructureScene};

/// Default water-probe radius used by molecular surfaces.
pub(crate) const SES_PROBE_RADIUS: f32 = 1.4;
/// Default spacing between scalar-field samples.
pub(crate) const SES_GRID_SPACING: f32 = 0.5;
/// Maximum number of scalar-field samples allocated for one surface.
const MAX_GRID_POINTS: usize = 750_000;
/// Number of grid cells over which a truncated distance field is evaluated.
const DISTANCE_FIELD_RANGE: f32 = 2.0;
/// Cell width used by the atom lookup grid. It is larger than the maximum
/// atom-plus-probe influence radius, so a query only needs adjacent cells.
const ATOM_GRID_CELL_SIZE: f32 = 4.0;
/// Number of volume-preserving smoothing pairs applied after contouring.
const SMOOTHING_ITERATIONS: usize = 2;
/// Positive Laplacian coefficient used by the first smoothing pass.
const SMOOTHING_LAMBDA: f32 = 0.28;
/// Negative Laplacian coefficient that approximately restores volume.
const SMOOTHING_MU: f32 = -0.29;

/// Indexed triangle mesh containing source-space positions, normals, and color.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct SesMesh {
  /// Interleaved source position, normal, and linear RGB rows.
  pub(crate) vertices: Vec<[f32; 9]>,
  /// Triangle-list indices into [`Self::vertices`].
  pub(crate) indices: Vec<u32>,
}

/// Atom center and van der Waals radius used by the surface distance field.
#[derive(Clone, Copy)]
struct SesAtom {
  /// Cartesian atom position in ångströms.
  position: glam::Vec3,
  /// Element-dependent van der Waals radius in ångströms.
  radius: f32,
}

/// Generates a rolling-probe solvent-excluded surface.
///
/// The algorithm first contours the solvent-accessible surface formed by atoms
/// expanded by the probe radius. Those contour vertices become probe centers
/// for a second distance field. Its inward connected components form the SES;
/// the outward offset components are discarded before conservative smoothing.
///
/// # Parameters
///
/// * `scene` supplies atom coordinates, element categories, and solvent flags.
/// * `color` is the linear RGB color written into every generated vertex.
///
/// # Returns
///
/// An indexed SES triangle mesh. An empty scene or a scene containing only
/// solvent atoms produces an empty mesh.
pub(crate) fn ses_mesh(scene: &StructureScene, color: [f32; 3]) -> SesMesh {
  let atoms = scene
    .atoms
    .iter()
    .filter(|atom| !atom.is_solvent)
    .map(|atom| SesAtom {
      position: glam::Vec3::from_array(atom.position),
      radius: vdw_radius(atom.element),
    })
    .collect::<Vec<_>>();
  if atoms.is_empty() {
    return SesMesh::default();
  }
  let atom_grid = AtomGrid::new(atoms);

  let (atom_bounds_min, atom_bounds_max) = atom_bounds(&atom_grid.atoms);
  let margin = atom_grid.max_radius + 2.0 * SES_PROBE_RADIUS + SES_GRID_SPACING;
  let bounds_min = atom_bounds_min - glam::Vec3::splat(margin);
  let bounds_max = atom_bounds_max + glam::Vec3::splat(margin);
  let extent = bounds_max - bounds_min;
  let mut spacing = SES_GRID_SPACING;
  let mut dimensions = grid_dimensions(extent, spacing);
  let point_count = dimensions[0] * dimensions[1] * dimensions[2];
  if point_count > MAX_GRID_POINTS {
    spacing *= (point_count as f32 / MAX_GRID_POINTS as f32).cbrt();
    dimensions = grid_dimensions(extent, spacing);
  }

  let sas_field = sample_field(bounds_min, spacing, dimensions, |position| {
    atom_surface_distance(position, &atom_grid, SES_PROBE_RADIUS, spacing)
  });
  let sas_mesh = contour_field(bounds_min, spacing, dimensions, &sas_field, color);
  if sas_mesh.vertices.is_empty() {
    return SesMesh::default();
  }

  let probe_centers = merge_close_probe_centers(&sas_mesh, 0.35 * spacing);
  let probe_grid = PointGrid::new(probe_centers, SES_PROBE_RADIUS + DISTANCE_FIELD_RANGE * spacing);
  let ses_field = sample_field(bounds_min, spacing, dimensions, |position| {
    probe_surface_distance(position, &probe_grid, spacing)
  });
  let mesh = contour_field(bounds_min, spacing, dimensions, &ses_field, color);
  let mut mesh = retain_inner_components(mesh, &atom_grid);
  orient_inner_surface(&mut mesh, bounds_min, spacing, dimensions, &ses_field);
  smooth_mesh(&mut mesh, SMOOTHING_ITERATIONS);
  recompute_inner_surface_normals(&mut mesh, bounds_min, spacing, dimensions, &ses_field);
  mesh
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
fn merge_close_probe_centers(mesh: &SesMesh, minimum_separation: f32) -> Vec<glam::Vec3> {
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
fn atom_bounds(atoms: &[SesAtom]) -> (glam::Vec3, glam::Vec3) {
  atoms.iter().fold(
    (glam::Vec3::splat(f32::INFINITY), glam::Vec3::splat(f32::NEG_INFINITY)),
    |(minimum, maximum), atom| (minimum.min(atom.position), maximum.max(atom.position)),
  )
}

/// Samples a scalar field at every point of the regular Cartesian grid.
///
/// # Parameters
///
/// * `bounds_min`, `spacing`, and `dimensions` define the sampled grid.
/// * `sample` computes the signed value at one source-space position.
///
/// # Returns
///
/// Row-major scalar samples indexed with [`grid_index`].
fn sample_field(
  bounds_min: glam::Vec3,
  spacing: f32,
  dimensions: [usize; 3],
  mut sample: impl FnMut(glam::Vec3) -> f32,
) -> Vec<f32> {
  let mut field = vec![0.0; dimensions[0] * dimensions[1] * dimensions[2]];
  for z in 0..dimensions[2] {
    for y in 0..dimensions[1] {
      for x in 0..dimensions[0] {
        let index = grid_index(x, y, z, dimensions);
        field[index] = sample(grid_position(x, y, z, bounds_min, spacing));
      }
    }
  }
  field
}

/// Extracts an indexed triangle mesh from the zero isosurface of a field.
///
/// # Parameters
///
/// * `bounds_min`, `spacing`, and `dimensions` describe the source grid.
/// * `field` contains one signed value per grid sample.
/// * `color` is copied into generated vertices.
///
/// # Returns
///
/// A shared-edge triangle mesh with interpolated positions and field normals.
fn contour_field(
  bounds_min: glam::Vec3,
  spacing: f32,
  dimensions: [usize; 3],
  field: &[f32],
  color: [f32; 3],
) -> SesMesh {
  let mut builder = MeshBuilder {
    bounds_min,
    spacing,
    dimensions,
    field,
    vertices: Vec::new(),
    indices: Vec::new(),
    edge_vertices: HashMap::new(),
    color,
  };
  for z in 0..dimensions[2] - 1 {
    for y in 0..dimensions[1] - 1 {
      for x in 0..dimensions[0] - 1 {
        builder.cell(x, y, z);
      }
    }
  }
  builder.finish()
}

/// Converts a world-space extent and sample spacing into grid dimensions.
fn grid_dimensions(extent: glam::Vec3, spacing: f32) -> [usize; 3] {
  [
    (extent.x / spacing).ceil() as usize + 1,
    (extent.y / spacing).ceil() as usize + 1,
    (extent.z / spacing).ceil() as usize + 1,
  ]
}

/// Converts a three-dimensional grid coordinate into a row-major index.
fn grid_index(x: usize, y: usize, z: usize, dimensions: [usize; 3]) -> usize {
  (z * dimensions[1] + y) * dimensions[0] + x
}

/// Converts a grid coordinate into its source-space position.
fn grid_position(x: usize, y: usize, z: usize, bounds_min: glam::Vec3, spacing: f32) -> glam::Vec3 {
  bounds_min + glam::Vec3::new(x as f32 * spacing, y as f32 * spacing, z as f32 * spacing)
}

/// Returns the truncated signed distance to the expanded atom union.
fn atom_surface_distance(position: glam::Vec3, atom_grid: &AtomGrid, expansion: f32, spacing: f32) -> f32 {
  let limit = DISTANCE_FIELD_RANGE * spacing;
  let mut distance = limit;
  atom_grid.for_each_nearby(position, atom_grid.max_radius + expansion + limit, |atom| {
    distance = distance.min(position.distance(atom.position) - atom.radius - expansion);
  });
  distance
}

/// Returns the truncated signed distance to the union of probe spheres.
fn probe_surface_distance(position: glam::Vec3, probe_grid: &PointGrid, spacing: f32) -> f32 {
  let limit = DISTANCE_FIELD_RANGE * spacing;
  let mut distance = limit;
  probe_grid.for_each_nearby(position, |probe_center| {
    distance = distance.min(position.distance(probe_center) - SES_PROBE_RADIUS);
  });
  distance
}

/// Spatial hash used to avoid testing every atom against every SES sample.
struct AtomGrid {
  /// Atom records addressed by bucket index.
  atoms: Vec<SesAtom>,
  /// Mapping from spatial bucket coordinates to atom indices.
  buckets: HashMap<[i32; 3], Vec<usize>>,
  /// Largest atom radius stored in the grid.
  max_radius: f32,
}

impl AtomGrid {
  /// Builds a spatial hash for the supplied atom records.
  fn new(atoms: Vec<SesAtom>) -> Self {
    let max_radius = atoms.iter().map(|atom| atom.radius).fold(0.0, f32::max);
    let mut buckets = HashMap::new();
    for (index, atom) in atoms.iter().enumerate() {
      buckets
        .entry(Self::cell(atom.position))
        .or_insert_with(Vec::new)
        .push(index);
    }
    Self {
      atoms,
      buckets,
      max_radius,
    }
  }

  /// Returns the fixed-size bucket containing a position.
  fn cell(position: glam::Vec3) -> [i32; 3] {
    [
      (position.x / ATOM_GRID_CELL_SIZE).floor() as i32,
      (position.y / ATOM_GRID_CELL_SIZE).floor() as i32,
      (position.z / ATOM_GRID_CELL_SIZE).floor() as i32,
    ]
  }

  /// Visits atoms in every bucket intersecting a spherical query neighborhood.
  fn for_each_nearby(&self, position: glam::Vec3, radius: f32, mut visit: impl FnMut(&SesAtom)) {
    let center = Self::cell(position);
    let range = (radius / ATOM_GRID_CELL_SIZE).ceil() as i32;
    for z in center[2] - range..=center[2] + range {
      for y in center[1] - range..=center[1] + range {
        for x in center[0] - range..=center[0] + range {
          let Some(indices) = self.buckets.get(&[x, y, z]) else {
            continue;
          };
          for index in indices {
            visit(&self.atoms[*index]);
          }
        }
      }
    }
  }
}

/// Spatial hash for the probe-center samples produced by the first contour.
struct PointGrid {
  /// Probe-center positions addressed by bucket index.
  points: Vec<glam::Vec3>,
  /// Mapping from spatial bucket coordinates to point indices.
  buckets: HashMap<[i32; 3], Vec<usize>>,
  /// Width of one point-grid bucket.
  cell_size: f32,
}

impl PointGrid {
  /// Builds a spatial hash for probe-center positions.
  fn new(points: Vec<glam::Vec3>, cell_size: f32) -> Self {
    let mut buckets = HashMap::new();
    for (index, point) in points.iter().enumerate() {
      let cell = point_grid_cell(*point, cell_size);
      buckets.entry(cell).or_insert_with(Vec::new).push(index);
    }
    Self {
      points,
      buckets,
      cell_size,
    }
  }

  /// Visits probe centers in the current and adjacent spatial buckets.
  fn for_each_nearby(&self, position: glam::Vec3, mut visit: impl FnMut(glam::Vec3)) {
    let center = point_grid_cell(position, self.cell_size);
    for z in center[2] - 1..=center[2] + 1 {
      for y in center[1] - 1..=center[1] + 1 {
        for x in center[0] - 1..=center[0] + 1 {
          let Some(indices) = self.buckets.get(&[x, y, z]) else {
            continue;
          };
          for index in indices {
            visit(self.points[*index]);
          }
        }
      }
    }
  }
}

/// Returns the point-grid bucket containing a position.
fn point_grid_cell(position: glam::Vec3, cell_size: f32) -> [i32; 3] {
  [
    (position.x / cell_size).floor() as i32,
    (position.y / cell_size).floor() as i32,
    (position.z / cell_size).floor() as i32,
  ]
}

/// Incrementally builds a shared-edge triangle mesh from a scalar field.
struct MeshBuilder<'a> {
  /// World-space origin of the scalar grid.
  bounds_min: glam::Vec3,
  /// Distance between adjacent grid samples.
  spacing: f32,
  /// Number of samples along each axis.
  dimensions: [usize; 3],
  /// Signed scalar values used for interpolation and orientation.
  field: &'a [f32],
  /// Interleaved position, normal, and color vertex records.
  vertices: Vec<[f32; 9]>,
  /// Triangle-list indices into [`Self::vertices`].
  indices: Vec<u32>,
  /// Cache that shares an interpolated vertex between adjacent tetrahedra.
  edge_vertices: HashMap<(usize, usize), u32>,
  /// Linear RGB color copied into every generated vertex.
  color: [f32; 3],
}

impl MeshBuilder<'_> {
  /// Contours one grid cube using six globally compatible tetrahedra.
  ///
  /// The fixed decomposition is shared by every cell, which makes contour
  /// vertices on common cube faces use the same diagonal and prevents seams.
  ///
  /// # Parameters
  ///
  /// * `x`, `y`, and `z` identify the lower grid corner of the cell.
  fn cell(&mut self, x: usize, y: usize, z: usize) {
    let corners = [
      self.index(x, y, z),
      self.index(x + 1, y, z),
      self.index(x + 1, y + 1, z),
      self.index(x, y + 1, z),
      self.index(x, y, z + 1),
      self.index(x + 1, y, z + 1),
      self.index(x + 1, y + 1, z + 1),
      self.index(x, y + 1, z + 1),
    ];
    // This Freudenthal decomposition uses face diagonals selected by global
    // axis order, so adjacent cells triangulate every shared face identically.
    const TETRAHEDRA: [[usize; 4]; 6] = [
      [0, 1, 2, 6],
      [0, 1, 5, 6],
      [0, 3, 2, 6],
      [0, 3, 7, 6],
      [0, 4, 5, 6],
      [0, 4, 7, 6],
    ];
    for tetra in TETRAHEDRA {
      self.tetra([
        corners[tetra[0]],
        corners[tetra[1]],
        corners[tetra[2]],
        corners[tetra[3]],
      ]);
    }
  }

  /// Emits the marching-tetrahedra triangles for one tetrahedron.
  ///
  /// The two-inside/two-outside case is triangulated with a deterministic
  /// shared diagonal so its quad cannot become self-intersecting.
  ///
  /// # Parameters
  ///
  /// * `corners` contains the four scalar-field indices that form the tetrahedron.
  fn tetra(&mut self, corners: [usize; 4]) {
    let mut inside = [0; 4];
    let mut outside = [0; 4];
    let mut inside_count = 0;
    let mut outside_count = 0;
    for (local_index, corner) in corners.iter().enumerate() {
      if self.field[*corner] < 0.0 {
        inside[inside_count] = local_index;
        inside_count += 1;
      } else {
        outside[outside_count] = local_index;
        outside_count += 1;
      }
    }

    match inside_count {
      1 => {
        let center = corners[inside[0]];
        let a = self.edge_vertex(center, corners[outside[0]]);
        let b = self.edge_vertex(center, corners[outside[1]]);
        let c = self.edge_vertex(center, corners[outside[2]]);
        self.triangle(a, b, c);
      }
      2 => {
        let i0 = corners[inside[0]];
        let i1 = corners[inside[1]];
        let o0 = corners[outside[0]];
        let o1 = corners[outside[1]];
        let a = self.edge_vertex(i0, o0);
        let b = self.edge_vertex(i0, o1);
        let c = self.edge_vertex(i1, o0);
        let d = self.edge_vertex(i1, o1);
        // The two triangles share the diagonal between opposite crossing
        // edges. Raw tetrahedron-edge order can instead form a bow tie.
        self.triangle(a, b, c);
        self.triangle(b, d, c);
      }
      3 => {
        let center = corners[outside[0]];
        let a = self.edge_vertex(center, corners[inside[0]]);
        let b = self.edge_vertex(center, corners[inside[1]]);
        let c = self.edge_vertex(center, corners[inside[2]]);
        self.triangle(a, b, c);
      }
      _ => {}
    }
  }

  /// Returns or creates the interpolated vertex on a scalar-field edge.
  ///
  /// # Parameters
  ///
  /// * `a` and `b` identify the scalar-field samples at the edge endpoints.
  ///
  /// # Returns
  ///
  /// The index of the shared interpolated mesh vertex.
  fn edge_vertex(&mut self, a: usize, b: usize) -> u32 {
    let key = if a < b { (a, b) } else { (b, a) };
    if let Some(index) = self.edge_vertices.get(&key) {
      return *index;
    }
    let first = self.grid_point(a);
    let second = self.grid_point(b);
    let first_value = self.field[a];
    let second_value = self.field[b];
    let denominator = first_value - second_value;
    let factor = if denominator.abs() < f32::EPSILON {
      0.5
    } else {
      (first_value / denominator).clamp(0.0, 1.0)
    };
    let position = first + (second - first) * factor;
    let normal =
      field_gradient(position, self.bounds_min, self.spacing, self.dimensions, self.field).normalize_or_zero();
    let index = self.vertices.len() as u32;
    self.vertices.push([
      position.x,
      position.y,
      position.z,
      normal.x,
      normal.y,
      normal.z,
      self.color[0],
      self.color[1],
      self.color[2],
    ]);
    self.edge_vertices.insert(key, index);
    index
  }

  /// Adds an oriented non-degenerate triangle to the mesh.
  ///
  /// The field gradient determines the winding direction, so generated
  /// triangles initially face toward increasing scalar-field values.
  ///
  /// # Parameters
  ///
  /// * `a`, `b`, and `c` are indices of the triangle vertices.
  fn triangle(&mut self, a: u32, b: u32, c: u32) {
    let pa = glam::Vec3::from_array([
      self.vertices[a as usize][0],
      self.vertices[a as usize][1],
      self.vertices[a as usize][2],
    ]);
    let pb = glam::Vec3::from_array([
      self.vertices[b as usize][0],
      self.vertices[b as usize][1],
      self.vertices[b as usize][2],
    ]);
    let pc = glam::Vec3::from_array([
      self.vertices[c as usize][0],
      self.vertices[c as usize][1],
      self.vertices[c as usize][2],
    ]);
    let normal = (pb - pa).cross(pc - pa);
    if normal.length_squared() < f32::EPSILON {
      return;
    }
    let gradient = field_gradient(
      (pa + pb + pc) / 3.0,
      self.bounds_min,
      self.spacing,
      self.dimensions,
      self.field,
    );
    if normal.dot(gradient) < 0.0 {
      self.indices.extend([a, c, b]);
    } else {
      self.indices.extend([a, b, c]);
    }
  }

  /// Finalizes the accumulated vertex and index arrays.
  fn finish(self) -> SesMesh {
    SesMesh {
      vertices: self.vertices,
      indices: self.indices,
    }
  }

  /// Converts a grid coordinate into a scalar-field index.
  fn index(&self, x: usize, y: usize, z: usize) -> usize {
    grid_index(x, y, z, self.dimensions)
  }

  /// Returns the source-space position of a scalar-field index.
  fn grid_point(&self, index: usize) -> glam::Vec3 {
    let x = index % self.dimensions[0];
    let yz = index / self.dimensions[0];
    let y = yz % self.dimensions[1];
    let z = yz / self.dimensions[1];
    grid_position(x, y, z, self.bounds_min, self.spacing)
  }
}

/// Reads the source-space position from an interleaved mesh vertex.
fn vertex_position(vertex: &[f32; 9]) -> glam::Vec3 {
  glam::Vec3::new(vertex[0], vertex[1], vertex[2])
}

/// Evaluates the gradient of a trilinearly interpolated scalar field.
///
/// Coordinates are clamped to the final grid cell so positions moved slightly
/// by mesh smoothing still receive a stable outward normal.
///
/// # Parameters
///
/// * `position` is the source-space point at which the gradient is evaluated.
/// * `bounds_min`, `spacing`, and `dimensions` describe the scalar grid.
/// * `field` contains the signed grid samples.
///
/// # Returns
///
/// The source-space gradient, pointing toward increasing field values.
fn field_gradient(
  position: glam::Vec3,
  bounds_min: glam::Vec3,
  spacing: f32,
  dimensions: [usize; 3],
  field: &[f32],
) -> glam::Vec3 {
  let maximum = glam::Vec3::new(
    dimensions[0].saturating_sub(1) as f32,
    dimensions[1].saturating_sub(1) as f32,
    dimensions[2].saturating_sub(1) as f32,
  );
  let coordinate = ((position - bounds_min) / spacing).clamp(glam::Vec3::ZERO, maximum);
  let x0 = (coordinate.x.floor() as usize).min(dimensions[0] - 2);
  let y0 = (coordinate.y.floor() as usize).min(dimensions[1] - 2);
  let z0 = (coordinate.z.floor() as usize).min(dimensions[2] - 2);
  let fraction = coordinate - glam::Vec3::new(x0 as f32, y0 as f32, z0 as f32);
  let value = |x: usize, y: usize, z: usize| field[grid_index(x, y, z, dimensions)];
  let interpolate = |a: f32, b: f32, factor: f32| a + (b - a) * factor;

  let dx0 = interpolate(
    value(x0 + 1, y0, z0) - value(x0, y0, z0),
    value(x0 + 1, y0 + 1, z0) - value(x0, y0 + 1, z0),
    fraction.y,
  );
  let dx1 = interpolate(
    value(x0 + 1, y0, z0 + 1) - value(x0, y0, z0 + 1),
    value(x0 + 1, y0 + 1, z0 + 1) - value(x0, y0 + 1, z0 + 1),
    fraction.y,
  );
  let dy0 = interpolate(
    value(x0, y0 + 1, z0) - value(x0, y0, z0),
    value(x0 + 1, y0 + 1, z0) - value(x0 + 1, y0, z0),
    fraction.x,
  );
  let dy1 = interpolate(
    value(x0, y0 + 1, z0 + 1) - value(x0, y0, z0 + 1),
    value(x0 + 1, y0 + 1, z0 + 1) - value(x0 + 1, y0, z0 + 1),
    fraction.x,
  );
  let dz0 = interpolate(
    value(x0, y0, z0 + 1) - value(x0, y0, z0),
    value(x0 + 1, y0, z0 + 1) - value(x0 + 1, y0, z0),
    fraction.x,
  );
  let dz1 = interpolate(
    value(x0, y0 + 1, z0 + 1) - value(x0, y0 + 1, z0),
    value(x0 + 1, y0 + 1, z0 + 1) - value(x0 + 1, y0 + 1, z0),
    fraction.x,
  );

  glam::Vec3::new(
    interpolate(dx0, dx1, fraction.z),
    interpolate(dy0, dy1, fraction.z),
    interpolate(dz0, dz1, fraction.y),
  ) / spacing
}

/// Removes the outward probe offset and isolated contour fragments.
///
/// The second distance map contains both sides of the probe spheres. A
/// connected component is retained only when at least one of its vertices lies
/// less than one and a half probe radii from an atom's van der Waals surface.
///
/// # Parameters
///
/// * `mesh` is the contour of the probe-center distance field.
/// * `atom_grid` provides the atom-surface proximity test.
///
/// # Returns
///
/// A compact mesh containing only components classified as the molecular SES.
fn retain_inner_components(mesh: SesMesh, atom_grid: &AtomGrid) -> SesMesh {
  if mesh.indices.is_empty() {
    return mesh;
  }

  let mut components = DisjointSet::new(mesh.vertices.len());
  for triangle in mesh.indices.chunks_exact(3) {
    components.union(triangle[0] as usize, triangle[1] as usize);
    components.union(triangle[1] as usize, triangle[2] as usize);
  }

  let threshold = 1.5 * SES_PROBE_RADIUS;
  let mut retain = vec![false; mesh.vertices.len()];
  for (index, vertex) in mesh.vertices.iter().enumerate() {
    let position = vertex_position(vertex);
    let mut clearance = threshold;
    atom_grid.for_each_nearby(position, atom_grid.max_radius + threshold, |atom| {
      clearance = clearance.min(position.distance(atom.position) - atom.radius);
    });
    if clearance < threshold {
      let root = components.find(index);
      retain[root] = true;
    }
  }

  let mut indices = Vec::with_capacity(mesh.indices.len());
  for triangle in mesh.indices.chunks_exact(3) {
    if retain[components.find(triangle[0] as usize)] {
      indices.extend_from_slice(triangle);
    }
  }
  compact_mesh(mesh.vertices, indices)
}

/// Makes every connected component consistently wound and outward-facing.
///
/// Adjacent triangles first receive opposite directions along each shared
/// edge. Each resulting component is then compared with the negated gradient
/// of the probe field, which is the outward direction on the inner boundary.
///
/// # Parameters
///
/// * `mesh` is mutated in place.
/// * `bounds_min`, `spacing`, and `dimensions` describe `field`.
/// * `field` is the signed probe-center distance field.
///
/// # Returns
///
/// This function returns `()` after updating the triangle winding in `mesh`.
fn orient_inner_surface(
  mesh: &mut SesMesh,
  bounds_min: glam::Vec3,
  spacing: f32,
  dimensions: [usize; 3],
  field: &[f32],
) {
  let triangle_count = mesh.indices.len() / 3;
  let mut edge_uses: HashMap<(u32, u32), Vec<(usize, bool)>> = HashMap::new();
  for (triangle_index, triangle) in mesh.indices.chunks_exact(3).enumerate() {
    for [first, second] in [
      [triangle[0], triangle[1]],
      [triangle[1], triangle[2]],
      [triangle[2], triangle[0]],
    ] {
      let edge = if first < second {
        (first, second)
      } else {
        (second, first)
      };
      edge_uses
        .entry(edge)
        .or_default()
        .push((triangle_index, first < second));
    }
  }

  let mut adjacency = vec![Vec::new(); triangle_count];
  for uses in edge_uses.values() {
    let [(first_triangle, first_direction), (second_triangle, second_direction)] = uses.as_slice() else {
      continue;
    };
    let relative_flip = first_direction == second_direction;
    adjacency[*first_triangle].push((*second_triangle, relative_flip));
    adjacency[*second_triangle].push((*first_triangle, relative_flip));
  }

  let mut flips = vec![None; triangle_count];
  for root in 0..triangle_count {
    if flips[root].is_some() {
      continue;
    }
    flips[root] = Some(false);
    let mut component = Vec::new();
    let mut pending = VecDeque::from([root]);
    while let Some(triangle_index) = pending.pop_front() {
      component.push(triangle_index);
      let current_flip = flips[triangle_index].unwrap_or(false);
      for (neighbor, relative_flip) in &adjacency[triangle_index] {
        if flips[*neighbor].is_none() {
          flips[*neighbor] = Some(current_flip ^ relative_flip);
          pending.push_back(*neighbor);
        }
      }
    }

    let alignment = component.iter().fold(0.0, |sum, triangle_index| {
      let triangle = &mesh.indices[3 * triangle_index..3 * triangle_index + 3];
      let [a, b, c] = [triangle[0] as usize, triangle[1] as usize, triangle[2] as usize];
      let pa = vertex_position(&mesh.vertices[a]);
      let pb = vertex_position(&mesh.vertices[b]);
      let pc = vertex_position(&mesh.vertices[c]);
      let mut face_normal = (pb - pa).cross(pc - pa);
      if flips[*triangle_index].unwrap_or(false) {
        face_normal = -face_normal;
      }
      let outward = -field_gradient((pa + pb + pc) / 3.0, bounds_min, spacing, dimensions, field);
      sum + face_normal.dot(outward)
    });
    if alignment < 0.0 {
      for triangle_index in component {
        flips[triangle_index] = Some(!flips[triangle_index].unwrap_or(false));
      }
    }
  }

  for (triangle_index, triangle) in mesh.indices.chunks_exact_mut(3).enumerate() {
    if flips[triangle_index].unwrap_or(false) {
      triangle.swap(1, 2);
    }
  }
}

/// Applies paired Laplacian passes to suppress grid-scale high-curvature noise.
///
/// The negative second pass approximately restores the volume lost by the
/// first averaging pass. This keeps the operation conservative while removing
/// short spikes left by sampled contouring.
///
/// # Parameters
///
/// * `mesh` is mutated in place.
/// * `iterations` controls the number of positive/negative pass pairs.
///
/// # Returns
///
/// This function returns `()` after updating the vertex positions in `mesh`.
fn smooth_mesh(mesh: &mut SesMesh, iterations: usize) {
  if mesh.indices.is_empty() {
    return;
  }
  let neighbors = mesh_neighbors(mesh);
  for _ in 0..iterations {
    laplacian_pass(&mut mesh.vertices, &neighbors, SMOOTHING_LAMBDA);
    laplacian_pass(&mut mesh.vertices, &neighbors, SMOOTHING_MU);
  }
}

/// Builds the undirected vertex adjacency used by smoothing passes.
fn mesh_neighbors(mesh: &SesMesh) -> Vec<Vec<usize>> {
  let mut neighbors = vec![Vec::new(); mesh.vertices.len()];
  for triangle in mesh.indices.chunks_exact(3) {
    let [a, b, c] = [triangle[0] as usize, triangle[1] as usize, triangle[2] as usize];
    neighbors[a].extend([b, c]);
    neighbors[b].extend([a, c]);
    neighbors[c].extend([a, b]);
  }
  for adjacent in &mut neighbors {
    adjacent.sort_unstable();
    adjacent.dedup();
  }
  neighbors
}

/// Applies one synchronous Laplacian displacement pass to mesh positions.
fn laplacian_pass(vertices: &mut [[f32; 9]], neighbors: &[Vec<usize>], factor: f32) {
  let positions = vertices.iter().map(vertex_position).collect::<Vec<_>>();
  for (index, vertex) in vertices.iter_mut().enumerate() {
    let adjacent = &neighbors[index];
    if adjacent.is_empty() {
      continue;
    }
    let average = adjacent
      .iter()
      .map(|neighbor| positions[*neighbor])
      .fold(glam::Vec3::ZERO, |sum, position| sum + position)
      / adjacent.len() as f32;
    let position = positions[index] + (average - positions[index]) * factor;
    vertex[0] = position.x;
    vertex[1] = position.y;
    vertex[2] = position.z;
  }
}

/// Rebuilds outward normals for the inner boundary of the probe distance map.
///
/// The probe-sphere field increases toward the atom side of its inner boundary,
/// opposite to the outward direction of the molecular surface.
///
/// # Parameters
///
/// * `mesh` is mutated in place.
/// * `bounds_min`, `spacing`, and `dimensions` describe `field`.
/// * `field` is the signed probe-center distance field.
///
/// # Returns
///
/// This function returns `()` after updating the vertex normals in `mesh`.
fn recompute_inner_surface_normals(
  mesh: &mut SesMesh,
  bounds_min: glam::Vec3,
  spacing: f32,
  dimensions: [usize; 3],
  field: &[f32],
) {
  let mut normals = vec![glam::Vec3::ZERO; mesh.vertices.len()];
  for triangle in mesh.indices.chunks_exact(3) {
    let [a, b, c] = [triangle[0] as usize, triangle[1] as usize, triangle[2] as usize];
    let face_normal = (vertex_position(&mesh.vertices[b]) - vertex_position(&mesh.vertices[a]))
      .cross(vertex_position(&mesh.vertices[c]) - vertex_position(&mesh.vertices[a]));
    for index in [a, b, c] {
      normals[index] += face_normal;
    }
  }
  for normal in &mut normals {
    *normal = normal.normalize_or_zero();
  }

  let neighbors = mesh_neighbors(mesh);
  for _ in 0..2 {
    let previous = normals;
    normals = previous
      .iter()
      .enumerate()
      .map(|(index, normal)| {
        let sum = neighbors[index]
          .iter()
          .map(|neighbor| previous[*neighbor])
          .fold(*normal * 2.0, |sum, adjacent| sum + adjacent);
        (sum / (neighbors[index].len() as f32 + 2.0)).normalize_or_zero()
      })
      .collect();
  }

  for (vertex, mut normal) in mesh.vertices.iter_mut().zip(normals) {
    let outward = -field_gradient(vertex_position(vertex), bounds_min, spacing, dimensions, field).normalize_or_zero();
    if normal.dot(outward) < 0.0 {
      normal = -normal;
    }
    normal = (normal * 0.85 + outward * 0.15).normalize_or_zero();
    vertex[3] = normal.x;
    vertex[4] = normal.y;
    vertex[5] = normal.z;
  }
}

/// Removes vertices that are no longer referenced after component filtering.
///
/// # Parameters
///
/// * `vertices` contains the source vertex records.
/// * `indices` contains the retained triangle-list indices.
///
/// # Returns
///
/// A mesh whose vertex array contains only vertices referenced by `indices`.
fn compact_mesh(vertices: Vec<[f32; 9]>, indices: Vec<u32>) -> SesMesh {
  let mut remap = vec![usize::MAX; vertices.len()];
  let mut compact_vertices = Vec::new();
  let mut compact_indices = Vec::with_capacity(indices.len());
  for index in indices {
    let source = index as usize;
    let target = if remap[source] == usize::MAX {
      let target = compact_vertices.len();
      compact_vertices.push(vertices[source]);
      remap[source] = target;
      target
    } else {
      remap[source]
    };
    compact_indices.push(target as u32);
  }
  SesMesh {
    vertices: compact_vertices,
    indices: compact_indices,
  }
}

/// Union-find structure used to label connected triangle components.
struct DisjointSet {
  /// Parent pointer for each set element.
  parents: Vec<usize>,
}

impl DisjointSet {
  /// Creates one singleton set for every mesh vertex.
  fn new(length: usize) -> Self {
    Self {
      parents: (0..length).collect(),
    }
  }

  /// Returns the representative of a set and compresses its path.
  fn find(&mut self, index: usize) -> usize {
    let mut root = index;
    while self.parents[root] != root {
      root = self.parents[root];
    }
    let mut current = index;
    while self.parents[current] != root {
      let parent = self.parents[current];
      self.parents[current] = root;
      current = parent;
    }
    root
  }

  /// Merges the sets containing two vertex indices.
  fn union(&mut self, first: usize, second: usize) {
    let first_root = self.find(first);
    let second_root = self.find(second);
    if first_root != second_root {
      self.parents[second_root] = first_root;
    }
  }
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
mod tests {
  use super::*;
  use chitin_bio::structure::{PdbParser, StructureScene};

  fn one_atom_scene() -> StructureScene {
    let parsed = PdbParser::new()
      .parse_bytes(b"ATOM      1  C   GLY A   1       0.000   0.000   0.000  1.00 10.00           C  \nEND\n")
      .unwrap_or_else(|error| panic!("SES fixture should parse: {error}"));
    StructureScene::from_first_model(&parsed.structure)
      .unwrap_or_else(|error| panic!("SES fixture should produce a scene: {error}"))
  }

  #[test]
  fn ses_mesh_should_generate_triangles_for_one_atom() {
    let mesh = ses_mesh(&one_atom_scene(), [0.8, 0.8, 0.8]);
    assert!(!mesh.indices.is_empty());
  }

  #[test]
  fn ses_mesh_should_contain_only_finite_vertices() {
    let mesh = ses_mesh(&one_atom_scene(), [0.8, 0.8, 0.8]);
    assert!(
      mesh
        .vertices
        .iter()
        .all(|vertex| vertex.iter().all(|value| value.is_finite()))
    );
  }

  #[test]
  fn ses_mesh_should_not_contain_grid_scale_spikes() {
    let mesh = ses_mesh(&one_atom_scene(), [0.8, 0.8, 0.8]);
    let longest_edge = mesh
      .indices
      .chunks_exact(3)
      .flat_map(|triangle| {
        [
          [triangle[0], triangle[1]],
          [triangle[1], triangle[2]],
          [triangle[2], triangle[0]],
        ]
      })
      .map(|edge| {
        vertex_position(&mesh.vertices[edge[0] as usize]).distance(vertex_position(&mesh.vertices[edge[1] as usize]))
      })
      .fold(0.0, f32::max);
    assert!(
      longest_edge <= 3.0 * SES_GRID_SPACING,
      "surface edge {longest_edge} exceeds the contour-cell scale"
    );
  }

  #[test]
  fn ses_mesh_should_be_watertight_for_one_atom() {
    let mesh = ses_mesh(&one_atom_scene(), [0.8, 0.8, 0.8]);
    let mut edge_use_counts = HashMap::new();
    for triangle in mesh.indices.chunks_exact(3) {
      for [first, second] in [
        [triangle[0], triangle[1]],
        [triangle[1], triangle[2]],
        [triangle[2], triangle[0]],
      ] {
        let edge = if first < second {
          (first, second)
        } else {
          (second, first)
        };
        *edge_use_counts.entry(edge).or_insert(0_u8) += 1;
      }
    }
    let invalid_edge_count = edge_use_counts.values().filter(|count| **count != 2).count();
    assert_eq!(invalid_edge_count, 0, "surface contains open or non-manifold edges");
  }

  #[test]
  fn ses_mesh_should_use_outward_distance_field_normals() {
    let mesh = ses_mesh(&one_atom_scene(), [0.8, 0.8, 0.8]);
    let minimum_alignment = mesh
      .vertices
      .iter()
      .map(|vertex| {
        let position = vertex_position(vertex).normalize_or_zero();
        let normal = glam::Vec3::new(vertex[3], vertex[4], vertex[5]);
        position.dot(normal)
      })
      .fold(1.0, f32::min);
    assert!(minimum_alignment > 0.0, "surface contains inward-facing normals");
  }

  #[test]
  fn ses_mesh_winding_should_agree_with_vertex_normals() {
    let mesh = ses_mesh(&one_atom_scene(), [0.8, 0.8, 0.8]);
    let inconsistent_triangle_count = mesh
      .indices
      .chunks_exact(3)
      .filter(|triangle| {
        let [a, b, c] = [triangle[0] as usize, triangle[1] as usize, triangle[2] as usize];
        let face_normal = (vertex_position(&mesh.vertices[b]) - vertex_position(&mesh.vertices[a]))
          .cross(vertex_position(&mesh.vertices[c]) - vertex_position(&mesh.vertices[a]));
        let vertex_normal = [a, b, c]
          .into_iter()
          .map(|index| glam::Vec3::from_slice(&mesh.vertices[index][3..6]))
          .fold(glam::Vec3::ZERO, |sum, normal| sum + normal);
        face_normal.dot(vertex_normal) <= 0.0
      })
      .count();
    assert_eq!(
      inconsistent_triangle_count, 0,
      "surface contains triangles opposite to their outward normals"
    );
  }

  #[test]
  fn ses_mesh_should_ignore_solvent_atoms() {
    let parsed = PdbParser::new()
      .parse_bytes(b"HETATM    1  O   HOH A   1       0.000   0.000   0.000  1.00 10.00           O  \nEND\n")
      .unwrap_or_else(|error| panic!("solvent fixture should parse: {error}"));
    let scene = StructureScene::from_first_model(&parsed.structure)
      .unwrap_or_else(|error| panic!("solvent fixture should produce a scene: {error}"));
    let mesh = ses_mesh(&scene, [0.8, 0.8, 0.8]);
    assert!(mesh.vertices.is_empty());
  }
}
