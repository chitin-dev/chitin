//! Topological orientation, smoothing, and normal reconstruction.

use std::collections::VecDeque;

use super::{SMOOTHING_LAMBDA, SMOOTHING_MU, SurfaceMesh, contour::vertex_position, field::field_gradient};

/// One triangle's directed use of an undirected mesh edge.
#[derive(Clone, Copy)]
struct EdgeRecord {
  first: u32,
  second: u32,
  /// Triangle index in the high bits and forward-direction flag in bit zero.
  directed_triangle: usize,
}

impl EdgeRecord {
  /// Creates a canonically ordered edge record from one directed triangle edge.
  fn new(first: u32, second: u32, triangle: usize) -> Self {
    let (first, second, forward) = if first < second {
      (first, second, true)
    } else {
      (second, first, false)
    };
    Self {
      first,
      second,
      directed_triangle: (triangle << 1) | usize::from(forward),
    }
  }

  /// Returns the canonical undirected edge key.
  fn edge(self) -> (u32, u32) {
    (self.first, self.second)
  }

  /// Returns the triangle index and its direction along the canonical edge.
  fn use_data(self) -> (usize, bool) {
    (self.directed_triangle >> 1, self.directed_triangle & 1 != 0)
  }
}

/// Fixed-degree triangle adjacency encoded without per-triangle allocations.
#[derive(Clone, Copy, Default)]
struct TriangleAdjacency {
  /// Neighbor triangle index in the high bits and relative-flip flag in bit zero.
  encoded: [usize; 3],
  len: u8,
}

impl TriangleAdjacency {
  /// Adds the neighbor contributed by one of the triangle's three edges.
  fn push(&mut self, triangle: usize, relative_flip: bool) {
    let Some(slot) = self.encoded.get_mut(self.len as usize) else {
      return;
    };
    *slot = (triangle << 1) | relative_flip as usize;
    self.len += 1;
  }

  /// Iterates over decoded neighbor indices and winding relationships.
  fn iter(&self) -> impl Iterator<Item = (usize, bool)> + '_ {
    self.encoded[..self.len as usize]
      .iter()
      .map(|encoded| (encoded >> 1, encoded & 1 != 0))
  }
}

/// Compressed sparse-row vertex adjacency used by smoothing and normal passes.
struct MeshNeighbors {
  offsets: Vec<usize>,
  entries: Vec<usize>,
}

impl MeshNeighbors {
  /// Returns the sorted unique neighbors of one vertex.
  fn get(&self, vertex: usize) -> &[usize] {
    &self.entries[self.offsets[vertex]..self.offsets[vertex + 1]]
  }
}

/// Builds fixed-degree triangle adjacency through shared manifold edges.
///
/// All directed triangle edges are stored in one contiguous array, sorted by
/// their canonical vertex pair, and scanned in equal-key runs. This avoids the
/// bucket and control-table overhead of a hash map while retaining the rule
/// that only edges with exactly two uses create adjacency.
///
/// # Parameters
///
/// * `indices` contains triangle-list vertex indices.
///
/// # Returns
///
/// One fixed-capacity adjacency record per complete input triangle.
fn triangle_adjacency(indices: &[u32]) -> Vec<TriangleAdjacency> {
  let triangle_count = indices.len() / 3;
  let mut edge_records = Vec::with_capacity(triangle_count.saturating_mul(3));
  for (triangle_index, triangle) in indices.as_chunks::<3>().0.iter().enumerate() {
    edge_records.extend([
      EdgeRecord::new(triangle[0], triangle[1], triangle_index),
      EdgeRecord::new(triangle[1], triangle[2], triangle_index),
      EdgeRecord::new(triangle[2], triangle[0], triangle_index),
    ]);
  }
  edge_records.sort_unstable_by_key(|record| record.edge());

  let mut adjacency = vec![TriangleAdjacency::default(); triangle_count];
  let mut run_start = 0;
  while run_start < edge_records.len() {
    let edge = edge_records[run_start].edge();
    let mut run_end = run_start + 1;
    while run_end < edge_records.len() && edge_records[run_end].edge() == edge {
      run_end += 1;
    }

    if let [first, second] = &edge_records[run_start..run_end] {
      let (first_triangle, first_forward) = first.use_data();
      let (second_triangle, second_forward) = second.use_data();
      let relative_flip = first_forward == second_forward;
      adjacency[first_triangle].push(second_triangle, relative_flip);
      adjacency[second_triangle].push(first_triangle, relative_flip);
    }
    run_start = run_end;
  }
  adjacency
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
pub(super) fn orient_inner_surface(
  mesh: &mut SurfaceMesh,
  bounds_min: glam::Vec3,
  spacing: f32,
  dimensions: [usize; 3],
  field: &[f32],
) {
  let triangle_count = mesh.indices.len() / 3;
  let adjacency = triangle_adjacency(&mesh.indices);

  let mut flips = vec![None; triangle_count];
  let mut component = Vec::new();
  let mut pending = VecDeque::new();
  for root in 0..triangle_count {
    if flips[root].is_some() {
      continue;
    }
    flips[root] = Some(false);
    component.clear();
    pending.clear();
    pending.push_back(root);
    while let Some(triangle_index) = pending.pop_front() {
      component.push(triangle_index);
      let current_flip = flips[triangle_index].unwrap_or(false);
      for (neighbor, relative_flip) in adjacency[triangle_index].iter() {
        if flips[neighbor].is_none() {
          flips[neighbor] = Some(current_flip ^ relative_flip);
          pending.push_back(neighbor);
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
      for triangle_index in &component {
        flips[*triangle_index] = Some(!flips[*triangle_index].unwrap_or(false));
      }
    }
  }

  for (triangle_index, triangle) in mesh.indices.as_chunks_mut::<3>().0.iter_mut().enumerate() {
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
pub(super) fn smooth_mesh(mesh: &mut SurfaceMesh, iterations: usize) {
  if mesh.indices.is_empty() {
    return;
  }
  let neighbors = mesh_neighbors(mesh);
  for _ in 0..iterations {
    laplacian_pass(&mut mesh.vertices, &neighbors, SMOOTHING_LAMBDA);
    laplacian_pass(&mut mesh.vertices, &neighbors, SMOOTHING_MU);
  }
}

/// Builds compact undirected vertex adjacency used by smoothing passes.
///
/// Duplicate neighbors contributed by adjacent triangles are sorted and
/// removed in place, leaving one contiguous CSR row for each vertex.
///
/// # Parameters
///
/// * `mesh` supplies the indexed triangles whose edges define adjacency.
///
/// # Returns
///
/// Sorted unique vertex neighbors stored in two contiguous allocations.
fn mesh_neighbors(mesh: &SurfaceMesh) -> MeshNeighbors {
  let vertex_count = mesh.vertices.len();
  let mut degrees = vec![0_usize; vertex_count];
  for triangle in mesh.indices.as_chunks::<3>().0 {
    let [a, b, c] = [triangle[0] as usize, triangle[1] as usize, triangle[2] as usize];
    degrees[a] += 2;
    degrees[b] += 2;
    degrees[c] += 2;
  }

  let mut offsets = Vec::with_capacity(vertex_count + 1);
  offsets.push(0);
  for degree in &degrees {
    offsets.push(offsets.last().copied().unwrap_or(0) + degree);
  }
  let mut entries = vec![0_usize; offsets.last().copied().unwrap_or(0)];
  degrees.fill(0);
  for triangle in mesh.indices.as_chunks::<3>().0 {
    let [a, b, c] = [triangle[0] as usize, triangle[1] as usize, triangle[2] as usize];
    for (vertex, adjacent) in [(a, [b, c]), (b, [a, c]), (c, [a, b])] {
      for neighbor in adjacent {
        entries[offsets[vertex] + degrees[vertex]] = neighbor;
        degrees[vertex] += 1;
      }
    }
  }

  // Sort and compact each row in place. The write cursor never advances past
  // unread rows, so this removes duplicate edge contributions without another
  // allocation proportional to the mesh size.
  let mut source_start = 0;
  let mut write = 0;
  for (vertex, degree) in degrees.into_iter().enumerate() {
    let source_end = source_start + degree;
    entries[source_start..source_end].sort_unstable();
    offsets[vertex] = write;
    let mut previous = None;
    for read in source_start..source_end {
      let neighbor = entries[read];
      if previous != Some(neighbor) {
        entries[write] = neighbor;
        write += 1;
        previous = Some(neighbor);
      }
    }
    source_start = source_end;
  }
  offsets[vertex_count] = write;
  entries.truncate(write);
  MeshNeighbors { offsets, entries }
}

/// Applies one synchronous Laplacian displacement pass to mesh positions.
///
/// # Parameters
///
/// * `vertices` contains positions updated after all source positions are copied.
/// * `neighbors` supplies the unique adjacent vertices used by each average.
/// * `factor` controls the signed displacement toward the neighbor average.
///
/// # Returns
///
/// This function returns `()` after updating every non-isolated vertex.
fn laplacian_pass(vertices: &mut [[f32; 6]], neighbors: &MeshNeighbors, factor: f32) {
  let positions = vertices.iter().map(vertex_position).collect::<Vec<_>>();
  for (index, vertex) in vertices.iter_mut().enumerate() {
    let adjacent = neighbors.get(index);
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

/// Rebuilds smooth vertex normals from the current oriented mesh geometry.
pub(super) fn recompute_surface_normals(mesh: &mut SurfaceMesh) {
  let mut normals = vec![glam::Vec3::ZERO; mesh.vertices.len()];
  for triangle in mesh.indices.as_chunks::<3>().0 {
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
        let adjacent = neighbors.get(index);
        let sum = adjacent
          .iter()
          .map(|neighbor| previous[*neighbor])
          .fold(*normal * 2.0, |sum, adjacent| sum + adjacent);
        (sum / (adjacent.len() as f32 + 2.0)).normalize_or_zero()
      })
      .collect();
  }

  for (vertex, normal) in mesh.vertices.iter_mut().zip(normals) {
    vertex[3] = normal.x;
    vertex[4] = normal.y;
    vertex[5] = normal.z;
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
pub(super) fn recompute_inner_surface_normals(
  mesh: &mut SurfaceMesh,
  bounds_min: glam::Vec3,
  spacing: f32,
  dimensions: [usize; 3],
  field: &[f32],
) {
  recompute_surface_normals(mesh);
  for vertex in &mut mesh.vertices {
    let mut normal = glam::Vec3::from_slice(&vertex[3..6]);
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

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn triangle_adjacency_should_connect_manifold_edge_uses() {
    let adjacency = triangle_adjacency(&[0, 1, 2, 1, 0, 3]);

    assert_eq!(adjacency[0].iter().collect::<Vec<_>>(), vec![(1, false)]);
    assert_eq!(adjacency[1].iter().collect::<Vec<_>>(), vec![(0, false)]);
  }

  #[test]
  fn triangle_adjacency_should_reject_non_manifold_edge_uses() {
    let adjacency = triangle_adjacency(&[0, 1, 2, 1, 0, 3, 0, 1, 4]);

    assert!(adjacency.iter().all(|neighbors| neighbors.iter().next().is_none()));
  }

  #[test]
  fn mesh_neighbors_should_store_sorted_unique_csr_rows() {
    let mesh = SurfaceMesh {
      vertices: vec![[0.0; 6]; 4],
      indices: vec![0, 1, 2, 0, 2, 3],
    };

    let neighbors = mesh_neighbors(&mesh);
    let rows = (0..4).map(|vertex| neighbors.get(vertex).to_vec()).collect::<Vec<_>>();

    assert_eq!(rows, vec![vec![1, 2, 3], vec![0, 2], vec![0, 1, 3], vec![0, 2]]);
  }
}
