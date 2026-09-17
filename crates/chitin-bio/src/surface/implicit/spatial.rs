//! Spatial hashes used by sampled SES distance-field queries.

use std::collections::HashMap;

use super::{ATOM_GRID_CELL_SIZE, ImplicitAtom};

/// Spatial hash used to avoid testing every atom against every SES sample.
pub(super) struct AtomGrid {
  /// Atom records addressed by bucket index.
  pub(super) atoms: Vec<ImplicitAtom>,
  /// Mapping from spatial bucket coordinates to atom indices.
  buckets: HashMap<[i32; 3], Vec<usize>>,
  /// Largest atom radius stored in the grid.
  pub(super) max_radius: f32,
}

impl AtomGrid {
  /// Builds a spatial hash for the supplied atom records.
  pub(super) fn new(atoms: Vec<ImplicitAtom>) -> Self {
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
  pub(super) fn for_each_nearby(&self, position: glam::Vec3, radius: f32, mut visit: impl FnMut(&ImplicitAtom)) {
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
pub(super) struct PointGrid {
  /// Probe-center positions addressed by bucket index.
  points: Vec<glam::Vec3>,
  /// Mapping from spatial bucket coordinates to point indices.
  buckets: HashMap<[i32; 3], Vec<usize>>,
  /// Width of one point-grid bucket.
  cell_size: f32,
}

impl PointGrid {
  /// Builds a spatial hash for probe-center positions.
  pub(super) fn new(points: Vec<glam::Vec3>, cell_size: f32) -> Self {
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
  pub(super) fn for_each_nearby(&self, position: glam::Vec3, mut visit: impl FnMut(glam::Vec3)) {
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
pub(super) fn point_grid_cell(position: glam::Vec3, cell_size: f32) -> [i32; 3] {
  [
    (position.x / cell_size).floor() as i32,
    (position.y / cell_size).floor() as i32,
    (position.z / cell_size).floor() as i32,
  ]
}
