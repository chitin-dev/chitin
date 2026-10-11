//! Geometry-lifetime broad phase for rectangle and visibility queries.

use glam::{Mat4, Vec3, Vec4};
use std::ops::Range;

#[derive(Clone)]
struct Node {
  min: Vec3,
  max: Vec3,
  triangles: Range<usize>,
  children: Option<[usize; 2]>,
}

#[derive(Clone, Default)]
pub(crate) struct BoxSelectionIndex {
  triangles: Vec<[u32; 3]>,
  nodes: Vec<Node>,
}

impl BoxSelectionIndex {
  /// Traverses near nodes first; the visitor returns the updated closest distance.
  pub fn visit_ray(
    &self,
    origin: Vec3,
    direction: Vec3,
    nearest: f32,
    mut visitor: impl FnMut([u32; 3], f32) -> f32,
  ) -> f32 {
    if self.nodes.is_empty() {
      return nearest;
    }
    self.visit_ray_node(0, origin, direction, nearest, &mut visitor)
  }

  fn ray_entry(node: &Node, origin: Vec3, direction: Vec3, nearest: f32) -> Option<f32> {
    let mut entry: f32 = 0.;
    let mut exit = nearest;
    for axis in 0..3 {
      if direction[axis].abs() < 1.0e-12 {
        if origin[axis] < node.min[axis] || origin[axis] > node.max[axis] {
          return None;
        }
      } else {
        let a = (node.min[axis] - origin[axis]) / direction[axis];
        let b = (node.max[axis] - origin[axis]) / direction[axis];
        entry = entry.max(a.min(b));
        exit = exit.min(a.max(b));
      }
      if entry > exit {
        return None;
      }
    }
    Some(entry)
  }

  fn visit_ray_node(
    &self,
    id: usize,
    origin: Vec3,
    direction: Vec3,
    mut nearest: f32,
    visitor: &mut impl FnMut([u32; 3], f32) -> f32,
  ) -> f32 {
    let node = &self.nodes[id];
    if Self::ray_entry(node, origin, direction, nearest).is_none() {
      return nearest;
    }
    if let Some(mut children) = node.children {
      let entry = |id| Self::ray_entry(&self.nodes[id], origin, direction, nearest).unwrap_or(f32::INFINITY);
      if entry(children[1]) < entry(children[0]) {
        children.swap(0, 1);
      }
      for child in children {
        nearest = self.visit_ray_node(child, origin, direction, nearest, visitor);
      }
    } else {
      for &indices in &self.triangles[node.triangles.clone()] {
        nearest = visitor(indices, nearest);
      }
    }
    nearest
  }

  pub fn new(indices: &[u32], vertices: &[[f32; 9]]) -> Self {
    let mut triangles = indices
      .as_chunks::<3>()
      .0
      .iter()
      .copied()
      .filter(|indices| {
        indices.iter().all(|index| {
          vertices
            .get(*index as usize)
            .is_some_and(|vertex| vertex[..3].iter().all(|v| v.is_finite()))
        })
      })
      .collect::<Vec<_>>();
    let mut nodes = Vec::new();
    if !triangles.is_empty() {
      Self::build(
        &mut triangles,
        &|index| Vec3::from_slice(&vertices[index as usize][..3]),
        0,
        &mut nodes,
      );
    }
    Self { triangles, nodes }
  }

  /// Bounds-only primitives use two corner indices; divide a visited index by two for its primitive ID.
  pub fn from_bounds(bounds: impl IntoIterator<Item = [Vec3; 2]>) -> Self {
    let corners = bounds.into_iter().flatten().collect::<Vec<_>>();
    let mut triangles = (0..corners.len() / 2)
      .filter(|&i| corners[2 * i].is_finite() && corners[2 * i + 1].is_finite())
      .map(|i| [2 * i as u32, 2 * i as u32 + 1, 2 * i as u32 + 1])
      .collect::<Vec<_>>();
    let mut nodes = Vec::new();
    if !triangles.is_empty() {
      Self::build(&mut triangles, &|index| corners[index as usize], 0, &mut nodes);
    }
    Self { triangles, nodes }
  }

  fn build(triangles: &mut [[u32; 3]], position: &impl Fn(u32) -> Vec3, start: usize, nodes: &mut Vec<Node>) -> usize {
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for indices in triangles.iter() {
      for &index in indices {
        let p = position(index);
        min = min.min(p);
        max = max.max(p);
      }
    }
    let id = nodes.len();
    nodes.push(Node {
      min,
      max,
      triangles: start..start + triangles.len(),
      children: None,
    });
    if triangles.len() > 32 {
      let extent = max - min;
      let axis = if extent.x >= extent.y && extent.x >= extent.z {
        0
      } else if extent.y >= extent.z {
        1
      } else {
        2
      };
      let middle = triangles.len() / 2;
      let center = |indices: &[u32; 3]| indices.iter().map(|index| position(*index)[axis]).sum::<f32>() / 3.;
      triangles.select_nth_unstable_by(middle, |a, b| center(a).total_cmp(&center(b)));
      let (left, right) = triangles.split_at_mut(middle);
      nodes[id].children = Some([
        Self::build(left, position, start, nodes),
        Self::build(right, position, start + middle, nodes),
      ]);
    }
    id
  }

  /// Visits only candidate triangles; fully contained nodes bypass exact clipping.
  pub fn visit(&self, mvp: Mat4, min: [f32; 2], max: [f32; 2], mut visitor: impl FnMut([u32; 3], bool)) {
    if self.nodes.is_empty() || !mvp.is_finite() {
      return;
    }
    let clip_planes = [
      Vec4::new(1., 0., 0., -min[0]),
      Vec4::new(-1., 0., 0., max[0]),
      Vec4::new(0., 1., 0., -min[1]),
      Vec4::new(0., -1., 0., max[1]),
      Vec4::Z,
      Vec4::new(0., 0., -1., 1.),
      Vec4::W,
    ];
    let planes = clip_planes.map(|plane| mvp.transpose() * plane);
    self.visit_node(0, &planes, false, &mut visitor);
  }

  fn visit_node(&self, id: usize, planes: &[Vec4; 7], inside: bool, visitor: &mut impl FnMut([u32; 3], bool)) {
    let node = &self.nodes[id];
    let mut contained = inside;
    if !inside {
      contained = true;
      for plane in planes {
        let normal = plane.truncate();
        let lo = Vec3::new(
          if normal.x >= 0. { node.min.x } else { node.max.x },
          if normal.y >= 0. { node.min.y } else { node.max.y },
          if normal.z >= 0. { node.min.z } else { node.max.z },
        );
        let hi = node.min + node.max - lo;
        // Conservative tolerance prevents broad-phase rounding from rejecting
        // an edge that the exact homogeneous clipper considers inside.
        let tolerance = 1.0e-5 * (normal.abs().dot(node.min.abs().max(node.max.abs())) + plane.w.abs() + 1.);
        if normal.dot(hi) + plane.w < -tolerance {
          return;
        }
        contained &= normal.dot(lo) + plane.w > tolerance;
      }
    }
    if contained || node.children.is_none() {
      for &indices in &self.triangles[node.triangles.clone()] {
        visitor(indices, contained);
      }
    } else if let Some(children) = node.children {
      for child in children {
        self.visit_node(child, planes, false, visitor);
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  #[test]
  fn ray_queries_should_prune_and_visit_the_nearest_bounds_first() {
    let tree = BoxSelectionIndex::from_bounds((0..100_000).map(|i| {
      let z = i as f32 + 1.;
      [Vec3::new(-1., -1., z), Vec3::new(1., 1., z + 0.5)]
    }));
    let mut visited = 0;
    let nearest = tree.visit_ray(Vec3::ZERO, Vec3::Z, 200_000., |indices, nearest| {
      visited += 1;
      nearest.min(indices[0] as f32 / 2. + 1.)
    });
    assert_eq!(nearest, 1.);
    assert!(visited <= 32);
    let mut visited = 0;
    tree.visit_ray(Vec3::new(2., 0., 0.), Vec3::Z, 200_000., |_, nearest| {
      visited += 1;
      nearest
    });
    assert_eq!(visited, 0);
  }
  fn mesh(count: usize) -> (Vec<[f32; 9]>, Vec<u32>) {
    let vertices = (0..count)
      .flat_map(|i| {
        let x = (i % 500) as f32 * 0.004 - 1.;
        let y = (i / 500) as f32 * 0.004 - 1.;
        let z = ((i % 13) as f32 - 6.) * 0.04;
        [
          [x, y, z, 0., 0., 1., 1., 1., 1.],
          [x + 0.003, y, z, 0., 0., 1., 1., 1., 1.],
          [x, y + 0.003, z, 0., 0., 1., 1., 1., 1.],
        ]
      })
      .collect::<Vec<_>>();
    let indices = (0..vertices.len() as u32).collect();
    (vertices, indices)
  }

  fn exact(indices: [u32; 3], vertices: &[[f32; 9]], mvp: Mat4, min: [f32; 2], max: [f32; 2]) -> bool {
    crate::picking::triangle_in_rect(
      indices.map(|index| Vec3::from_slice(&vertices[index as usize][..3])),
      mvp,
      min,
      max,
    )
  }

  #[test]
  fn indexed_queries_should_match_exhaustive_clipping_including_near_plane_crossings() {
    let (vertices, indices) = mesh(3000);
    let tree = BoxSelectionIndex::new(&indices, &vertices);
    for mvp in [
      Mat4::IDENTITY,
      Mat4::ZERO,
      glam::camera::rh::proj::directx::perspective(1., 1.2, 0.1, 10.)
        * glam::camera::rh::view::look_at_mat4(Vec3::new(0., 0., 0.2), Vec3::new(0., 0., -1.), Vec3::Y),
    ] {
      for i in 0..12 {
        let min = [-1. + i as f32 * 0.1, -1.];
        let max = [min[0] + 0.2, -0.85];
        let reference = indices
          .as_chunks::<3>()
          .0
          .iter()
          .filter(|&&triangle| exact(triangle, &vertices, mvp, min, max))
          .map(|triangle| triangle[0])
          .collect::<Vec<_>>();
        let mut actual = Vec::new();
        tree.visit(mvp, min, max, |triangle, inside| {
          if inside || exact(triangle, &vertices, mvp, min, max) {
            actual.push(triangle[0]);
          }
        });
        actual.sort_unstable();
        assert_eq!(actual, reference);
      }
    }
  }

  #[test]
  #[ignore = "release-mode CPU benchmark; no GPU required"]
  fn box_selection_benchmark() {
    use std::{hint::black_box, time::Instant};
    let (vertices, indices) = mesh(250_000);
    let mvp = Mat4::from_translation(Vec3::new(0., 0., 0.5));
    let started = Instant::now();
    let tree = BoxSelectionIndex::new(&indices, &vertices);
    let build = started.elapsed();
    let queries = (0..100)
      .map(|i| {
        let min = [-0.9 + i as f32 * 0.016, -0.5];
        (min, [min[0] + 0.02, min[1] + 0.02])
      })
      .collect::<Vec<_>>();
    let started = Instant::now();
    let reference = queries
      .iter()
      .map(|&(min, max)| {
        indices
          .as_chunks::<3>()
          .0
          .iter()
          .filter(|&&triangle| exact(triangle, &vertices, black_box(mvp), min, max))
          .count()
      })
      .collect::<Vec<_>>();
    let exhaustive = started.elapsed();
    let mut candidates = 0;
    let started = Instant::now();
    let actual = queries
      .iter()
      .map(|&(min, max)| {
        let mut hits = 0;
        tree.visit(black_box(mvp), min, max, |triangle, inside| {
          candidates += 1;
          if inside || exact(triangle, &vertices, mvp, min, max) {
            hits += 1;
          }
        });
        hits
      })
      .collect::<Vec<_>>();
    let indexed = started.elapsed();
    assert_eq!(actual, reference);
    eprintln!(
      "triangles=250000 queries=100 build={build:?} exhaustive={exhaustive:?} indexed={indexed:?} speedup={:.1} candidates={candidates}/25000000",
      exhaustive.as_secs_f64() / indexed.as_secs_f64()
    );
  }
  #[test]
  fn small_boxes_should_prune_distant_triangles() {
    let vertices = (0..100_000)
      .flat_map(|i| {
        let x = i as f32 * 0.01;
        [
          [x, 0., 0.5, 0., 0., 1., 1., 1., 1.],
          [x + 0.004, 0., 0.5, 0., 0., 1., 1., 1., 1.],
          [x, 0.004, 0.5, 0., 0., 1., 1., 1., 1.],
        ]
      })
      .collect::<Vec<_>>();
    let indices = (0..vertices.len() as u32).collect::<Vec<_>>();
    let tree = BoxSelectionIndex::new(&indices, &vertices);
    let mut candidates = 0;
    tree.visit(Mat4::IDENTITY, [0., 0.], [0.02, 0.02], |_, _| candidates += 1);
    assert!(candidates < 100, "visited {candidates} of 100000 triangles");
  }
}
