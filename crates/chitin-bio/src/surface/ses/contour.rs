//! Shared-edge marching-tetrahedra extraction for sampled scalar fields.

use std::collections::HashMap;

use super::{
  SurfaceMesh,
  field::{field_gradient, grid_index, grid_position},
};

/// Extracts an indexed triangle mesh from the zero isosurface of a field.
///
/// # Parameters
///
/// * `bounds_min`, `spacing`, and `dimensions` describe the source grid.
/// * `field` contains one signed value per grid sample.
///
/// # Returns
///
/// A shared-edge triangle mesh with interpolated positions and field normals.
pub(super) fn contour_field(
  bounds_min: glam::Vec3,
  spacing: f32,
  dimensions: [usize; 3],
  field: &[f32],
) -> SurfaceMesh {
  let mut builder = MeshBuilder {
    bounds_min,
    spacing,
    dimensions,
    field,
    vertices: Vec::new(),
    indices: Vec::new(),
    edge_vertices: HashMap::new(),
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

/// Reads the source-space position from an interleaved mesh vertex.
pub(super) fn vertex_position(vertex: &[f32; 6]) -> glam::Vec3 {
  glam::Vec3::new(vertex[0], vertex[1], vertex[2])
}

/// Incrementally builds a shared-edge triangle mesh from a scalar field.
struct MeshBuilder<'a> {
  /// Source-space origin of the scalar grid.
  bounds_min: glam::Vec3,
  /// Distance between adjacent grid samples.
  spacing: f32,
  /// Number of samples along each axis.
  dimensions: [usize; 3],
  /// Signed scalar values used for interpolation and orientation.
  field: &'a [f32],
  /// Interleaved position and normal vertex records.
  vertices: Vec<[f32; 6]>,
  /// Triangle-list indices into `vertices`.
  indices: Vec<u32>,
  /// Cache sharing an interpolated vertex between adjacent tetrahedra.
  edge_vertices: HashMap<(usize, usize), u32>,
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
    // Global axis ordering gives neighboring cells the same face diagonals.
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
  /// # Parameters
  ///
  /// * `corners` contains the four scalar-field indices forming the tetrahedron.
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
        // This shared diagonal avoids a self-intersecting bow-tie quad.
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
  /// * `a` and `b` identify scalar samples at the edge endpoints.
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
    let denominator = self.field[a] - self.field[b];
    let factor = if denominator.abs() < f32::EPSILON {
      0.5
    } else {
      (self.field[a] / denominator).clamp(0.0, 1.0)
    };
    let position = first + (second - first) * factor;
    let normal =
      field_gradient(position, self.bounds_min, self.spacing, self.dimensions, self.field).normalize_or_zero();
    let index = self.vertices.len() as u32;
    self
      .vertices
      .push([position.x, position.y, position.z, normal.x, normal.y, normal.z]);
    self.edge_vertices.insert(key, index);
    index
  }

  /// Adds an oriented non-degenerate triangle to the mesh.
  fn triangle(&mut self, a: u32, b: u32, c: u32) {
    let pa = vertex_position(&self.vertices[a as usize]);
    let pb = vertex_position(&self.vertices[b as usize]);
    let pc = vertex_position(&self.vertices[c as usize]);
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
  fn finish(self) -> SurfaceMesh {
    SurfaceMesh {
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
