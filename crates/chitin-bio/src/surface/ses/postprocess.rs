//! Topological orientation, smoothing, and normal reconstruction.

use std::collections::{HashMap, VecDeque};

use super::{SMOOTHING_LAMBDA, SMOOTHING_MU, SurfaceMesh, contour::vertex_position, field::field_gradient};

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

/// Builds the undirected vertex adjacency used by smoothing passes.
fn mesh_neighbors(mesh: &SurfaceMesh) -> Vec<Vec<usize>> {
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
fn laplacian_pass(vertices: &mut [[f32; 6]], neighbors: &[Vec<usize>], factor: f32) {
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

/// Rebuilds smooth vertex normals from the current oriented mesh geometry.
pub(super) fn recompute_surface_normals(mesh: &mut SurfaceMesh) {
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
