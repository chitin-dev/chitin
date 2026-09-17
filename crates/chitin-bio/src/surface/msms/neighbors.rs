//! Conservative spatial candidates for analytical reduced-surface construction.
//!
//! The graph in this module is deliberately broader than a weighted Delaunay
//! or regular triangulation. Every real three-sphere intersection requires all
//! three expanded sphere pairs to intersect, so graph triangles form a safe
//! candidate set. The analytical probe solver remains responsible for removing
//! false-positive triplets.

use std::collections::HashMap;

use thiserror::Error;

use super::geometry::MsmsAtom;

/// Relative tolerance used by expanded-sphere intersection predicates.
const RELATIVE_NEIGHBOR_TOLERANCE: f64 = 128.0 * f64::EPSILON;

/// Spatial candidate graph for probe-expanded atom spheres.
///
/// Adjacency rows are sorted, symmetric, and contain no duplicate or self
/// indices. An edge means only that the two expanded sphere surfaces may
/// intersect; it does not by itself imply solvent accessibility.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpandedSphereNeighborGraph {
  adjacency: Vec<Vec<usize>>,
}

impl ExpandedSphereNeighborGraph {
  /// Builds a conservative expanded-sphere neighbor graph using a spatial hash.
  ///
  /// # Parameters
  ///
  /// * `atoms` contains atom centers and van der Waals radii.
  /// * `probe_radius` is added to every atom radius before pair testing.
  ///
  /// # Returns
  ///
  /// A deterministic graph, or [`NeighborGraphError`] when the input contains
  /// invalid coordinates or radii.
  ///
  /// # Examples
  ///
  /// ```
  /// use chitin_bio::surface::msms::{
  ///   geometry::MsmsAtom,
  ///   neighbors::ExpandedSphereNeighborGraph,
  /// };
  /// use glam::DVec3;
  ///
  /// let atoms = vec![
  ///   MsmsAtom { atom_index: 0, center: DVec3::ZERO, radius: 1.0 },
  ///   MsmsAtom { atom_index: 1, center: DVec3::new(2.0, 0.0, 0.0), radius: 1.0 },
  ///   MsmsAtom {
  ///     atom_index: 2,
  ///     center: DVec3::new(1.0, 3.0_f64.sqrt(), 0.0),
  ///     radius: 1.0,
  ///   },
  /// ];
  /// let graph = ExpandedSphereNeighborGraph::new(&atoms, 1.0)?;
  /// assert_eq!(graph.candidate_triplets(), vec![[0, 1, 2]]);
  /// # Ok::<(), chitin_bio::surface::msms::neighbors::NeighborGraphError>(())
  /// ```
  pub fn new(atoms: &[MsmsAtom], probe_radius: f64) -> Result<Self, NeighborGraphError> {
    let maximum_expanded_radius = validate_geometry(atoms, probe_radius)?;
    if atoms.is_empty() {
      return Ok(Self { adjacency: Vec::new() });
    }

    // With a cell width equal to the largest possible pair reach, every
    // intersecting pair lies either in one bucket or in adjacent buckets.
    let cell_size = 2.0 * maximum_expanded_radius;
    let mut buckets: HashMap<[i64; 3], Vec<usize>> = HashMap::new();
    let mut adjacency = vec![Vec::new(); atoms.len()];
    for (atom_index, atom) in atoms.iter().enumerate() {
      let cell = spatial_cell(atom.center, cell_size);
      for z_offset in -1_i64..=1 {
        for y_offset in -1_i64..=1 {
          for x_offset in -1_i64..=1 {
            let neighbor_cell = [
              cell[0].saturating_add(x_offset),
              cell[1].saturating_add(y_offset),
              cell[2].saturating_add(z_offset),
            ];
            let Some(candidate_indices) = buckets.get(&neighbor_cell) else {
              continue;
            };
            for &candidate_index in candidate_indices {
              if expanded_sphere_surfaces_may_intersect(atom, &atoms[candidate_index], probe_radius) {
                adjacency[atom_index].push(candidate_index);
                adjacency[candidate_index].push(atom_index);
              }
            }
          }
        }
      }
      buckets.entry(cell).or_default().push(atom_index);
    }
    for neighbors in &mut adjacency {
      neighbors.sort_unstable();
    }
    Ok(Self { adjacency })
  }

  /// Returns the sorted neighbor row for one local atom index.
  pub fn neighbors(&self, atom_index: usize) -> Option<&[usize]> {
    self.adjacency.get(atom_index).map(Vec::as_slice)
  }

  /// Returns all unique candidate atom pairs in lexicographic order.
  pub fn candidate_pairs(&self) -> Vec<[usize; 2]> {
    self
      .adjacency
      .iter()
      .enumerate()
      .flat_map(|(first, neighbors)| {
        neighbors
          .iter()
          .copied()
          .filter(move |&second| second > first)
          .map(move |second| [first, second])
      })
      .collect()
  }

  /// Returns graph triangles as unique candidate atom triplets in lexicographic order.
  pub fn candidate_triplets(&self) -> Vec<[usize; 3]> {
    let mut triplets = Vec::new();
    for (first, first_neighbors) in self.adjacency.iter().enumerate() {
      for &second in first_neighbors.iter().filter(|&&index| index > first) {
        let second_neighbors = &self.adjacency[second];
        append_common_larger_neighbors(&mut triplets, first, second, first_neighbors, second_neighbors);
      }
    }
    triplets
  }
}

/// Invalid molecular geometry supplied to neighbor-graph construction.
#[derive(Clone, Copy, Debug, Error, PartialEq)]
pub enum NeighborGraphError {
  /// The probe radius must be finite and strictly positive.
  #[error("neighbor graph probe radius must be finite and positive, got {0}")]
  InvalidProbeRadius(f64),
  /// The indexed atom has a non-finite center or invalid radius.
  #[error("neighbor graph atom {atom_index} has an invalid center or radius")]
  InvalidAtom {
    /// Local index of the invalid atom.
    atom_index: usize,
  },
  /// Adding the probe radius overflowed the expanded sphere radius.
  #[error("neighbor graph atom {atom_index} has a non-finite expanded radius")]
  ExpandedRadiusOverflow {
    /// Local index whose expanded radius overflowed.
    atom_index: usize,
  },
}

/// Validates input geometry and returns the largest expanded sphere radius.
fn validate_geometry(atoms: &[MsmsAtom], probe_radius: f64) -> Result<f64, NeighborGraphError> {
  if !probe_radius.is_finite() || probe_radius <= 0.0 {
    return Err(NeighborGraphError::InvalidProbeRadius(probe_radius));
  }
  let mut maximum_expanded_radius: f64 = 0.0;
  for (atom_index, atom) in atoms.iter().enumerate() {
    if !atom.center.is_finite() || !atom.radius.is_finite() || atom.radius <= 0.0 {
      return Err(NeighborGraphError::InvalidAtom { atom_index });
    }
    let expanded_radius = atom.radius + probe_radius;
    if !expanded_radius.is_finite() {
      return Err(NeighborGraphError::ExpandedRadiusOverflow { atom_index });
    }
    maximum_expanded_radius = maximum_expanded_radius.max(expanded_radius);
  }
  Ok(maximum_expanded_radius)
}

/// Returns the integer spatial-hash cell containing an atom center.
fn spatial_cell(center: glam::DVec3, cell_size: f64) -> [i64; 3] {
  [
    (center.x / cell_size).floor() as i64,
    (center.y / cell_size).floor() as i64,
    (center.z / cell_size).floor() as i64,
  ]
}

/// Tests the necessary pair condition for a common tangent-probe center.
fn expanded_sphere_surfaces_may_intersect(first: &MsmsAtom, second: &MsmsAtom, probe_radius: f64) -> bool {
  let first_radius = first.radius + probe_radius;
  let second_radius = second.radius + probe_radius;
  let distance = first.center.distance(second.center);
  let upper_bound = first_radius + second_radius;
  let lower_bound = (first_radius - second_radius).abs();
  let scale = distance.max(upper_bound).max(1.0);
  let tolerance = RELATIVE_NEIGHBOR_TOLERANCE * scale;
  distance <= upper_bound + tolerance && distance + tolerance >= lower_bound
}

/// Appends the sorted intersection of two adjacency rows above `second`.
fn append_common_larger_neighbors(
  triplets: &mut Vec<[usize; 3]>,
  first: usize,
  second: usize,
  first_neighbors: &[usize],
  second_neighbors: &[usize],
) {
  let mut first_cursor = first_neighbors.partition_point(|&index| index <= second);
  let mut second_cursor = second_neighbors.partition_point(|&index| index <= second);
  while first_cursor < first_neighbors.len() && second_cursor < second_neighbors.len() {
    match first_neighbors[first_cursor].cmp(&second_neighbors[second_cursor]) {
      std::cmp::Ordering::Less => first_cursor += 1,
      std::cmp::Ordering::Greater => second_cursor += 1,
      std::cmp::Ordering::Equal => {
        triplets.push([first, second, first_neighbors[first_cursor]]);
        first_cursor += 1;
        second_cursor += 1;
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use glam::DVec3;

  use super::*;

  fn atom(atom_index: usize, center: DVec3, radius: f64) -> MsmsAtom {
    MsmsAtom {
      atom_index,
      center,
      radius,
    }
  }

  #[test]
  fn equilateral_neighbors_should_form_one_candidate_triplet() {
    let atoms = vec![
      atom(0, DVec3::ZERO, 1.0),
      atom(1, DVec3::new(2.0, 0.0, 0.0), 1.0),
      atom(2, DVec3::new(1.0, 3.0_f64.sqrt(), 0.0), 1.0),
    ];
    let graph = ExpandedSphereNeighborGraph::new(&atoms, 1.0)
      .unwrap_or_else(|error| panic!("equilateral atoms should build a graph: {error}"));

    assert_eq!(graph.candidate_triplets(), vec![[0, 1, 2]]);
  }

  #[test]
  fn separated_atoms_should_not_form_candidate_pairs() {
    let atoms = vec![atom(0, DVec3::ZERO, 1.0), atom(1, DVec3::new(5.0, 0.0, 0.0), 1.0)];
    let graph = ExpandedSphereNeighborGraph::new(&atoms, 1.0)
      .unwrap_or_else(|error| panic!("separated atoms should build a graph: {error}"));

    assert!(graph.candidate_pairs().is_empty());
  }

  #[test]
  fn contained_expanded_sphere_should_not_form_candidate_pair() {
    let atoms = vec![atom(0, DVec3::ZERO, 3.0), atom(1, DVec3::new(0.5, 0.0, 0.0), 1.0)];
    let graph = ExpandedSphereNeighborGraph::new(&atoms, 1.0)
      .unwrap_or_else(|error| panic!("contained atoms should build a graph: {error}"));

    assert!(graph.candidate_pairs().is_empty());
  }

  #[test]
  fn tetrahedral_clique_should_emit_each_pair_and_triplet_once() {
    let atoms = vec![
      atom(0, DVec3::ZERO, 1.0),
      atom(1, DVec3::X, 1.0),
      atom(2, DVec3::Y, 1.0),
      atom(3, DVec3::Z, 1.0),
    ];
    let graph = ExpandedSphereNeighborGraph::new(&atoms, 1.0)
      .unwrap_or_else(|error| panic!("tetrahedral atoms should build a graph: {error}"));

    assert_eq!(graph.candidate_pairs().len(), 6);
    assert_eq!(
      graph.candidate_triplets(),
      vec![[0, 1, 2], [0, 1, 3], [0, 2, 3], [1, 2, 3]]
    );
  }

  #[test]
  fn spatial_hash_should_match_brute_force_pair_predicate() {
    let atoms = (0..64)
      .map(|index| {
        let x = (index % 4) as f64 * 2.7 - 4.0;
        let y = ((index / 4) % 4) as f64 * 2.3 - 3.0;
        let z = (index / 16) as f64 * 2.9 - 5.0;
        atom(index, DVec3::new(x, y, z), 1.0 + (index % 5) as f64 * 0.1)
      })
      .collect::<Vec<_>>();
    let probe_radius = 1.4;
    let graph = ExpandedSphereNeighborGraph::new(&atoms, probe_radius)
      .unwrap_or_else(|error| panic!("finite atom lattice should build a graph: {error}"));
    let brute_force = (0..atoms.len())
      .flat_map(|first| ((first + 1)..atoms.len()).map(move |second| [first, second]))
      .filter(|&[first, second]| expanded_sphere_surfaces_may_intersect(&atoms[first], &atoms[second], probe_radius))
      .collect::<Vec<_>>();

    assert_eq!(graph.candidate_pairs(), brute_force);
  }

  #[test]
  fn invalid_atom_should_return_its_local_index() {
    let atoms = vec![atom(10, DVec3::ZERO, 1.0), atom(20, DVec3::X, f64::NAN)];

    assert_eq!(
      ExpandedSphereNeighborGraph::new(&atoms, 1.4),
      Err(NeighborGraphError::InvalidAtom { atom_index: 1 })
    );
  }
}
