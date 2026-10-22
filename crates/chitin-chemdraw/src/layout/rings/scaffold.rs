//! Reduced fragment/port/path embedding before atom-level distance closure.
//!
//! Small rings remain rigid islands; maximal source paths connect their ports
//! and free junctions. Junction positions are optimized together before paths
//! expand. Sketch edges are never exported as chemical bonds or extra vertices.

use super::{Error, Point, Work, constraints::Constraints, folding::Plan};
use chitin_chem::{AtomId, BondOrder, MolecularGraph, perception::RingBlock};
use std::{
  collections::BTreeMap,
  f64::consts::{PI, TAU},
};

mod geometry;

/// One rigid island, or a single freely movable junction.
struct Fragment {
  /// Source-local atom rows and immutable offsets about the fragment center.
  atoms: Vec<(usize, Point)>,
}

/// A source path with independent attachment ports on the reduced graph.
struct Path {
  /// Ordered genuine source rows; endpoints belong to fragments.
  atoms: Vec<usize>,
  /// Fragment identities, not chemical atom IDs.
  ends: [usize; 2],
}

/// Source-complete topology with explicit ownership and short-ring geometry.
struct Skeleton {
  /// Source-local rows of every original edge, in canonical endpoint order.
  edges: Vec<[usize; 2]>,
  /// Rigid local islands and free junctions.
  fragments: Vec<Fragment>,
  /// Maximal flexible paths; parallel paths retain separate identities.
  paths: Vec<Path>,
  /// Rigid distance equations inside short-ring fragments.
  chords: Vec<([usize; 2], f64)>,
  /// Whether each row lies inside a rigid island rather than a flexible path.
  owned: Vec<bool>,
}

/// Complete, closed block candidates; chemical annotations stay on the graph.
pub(super) struct Scaffold {
  /// Dense global rows, independent of sparse source handle serials.
  atoms: Vec<usize>,
  /// Source-complete local edge rows used for attachment headings.
  edges: Vec<[usize; 2]>,
  /// Finite candidates ranked before complete-molecule refinement.
  poses: Vec<Vec<Point>>,
}

impl Skeleton {
  /// Contracts short-ring islands and degree-two paths without losing edges.
  ///
  /// # Returns
  /// A source-complete reduced multigraph. Unsupported fused rigid islands,
  /// closed self-paths and linear centers defer to existing specialized models.
  ///
  /// # Errors
  /// Charges discovery and ownership work before publishing a complete model.
  fn new(edges: Vec<[usize; 2]>, cycles: &[Vec<usize>], n: usize, work: &mut Work) -> Result<Option<Self>, Error> {
    let plan = Plan::new(&edges, cycles, vec![false; n], work)?;
    let mut owner = vec![None; n];
    let mut fragments = Vec::new();
    let mut chords = Vec::new();
    for island in &plan.fragments {
      let mut ring = vec![island[0]];
      let mut previous = usize::MAX;
      let mut current = island[0];
      loop {
        work.charge(edges.len())?;
        let neighbors: Vec<_> = edges
          .iter()
          .filter_map(|&[a, b]| {
            if a == current && island.contains(&b) {
              Some(b)
            } else if b == current && island.contains(&a) {
              Some(a)
            } else {
              None
            }
          })
          .collect();
        if neighbors.len() != 2 {
          return Ok(None);
        }
        let next = neighbors
          .iter()
          .copied()
          .find(|&row| row != previous)
          .ok_or(Error::CoordinateRange)?;
        if next == ring[0] {
          break;
        }
        if ring.contains(&next) {
          return Ok(None);
        }
        ring.push(next);
        previous = current;
        current = next;
      }
      if ring.len() != island.len() {
        return Ok(None);
      }
      let radius = 1. / (2. * (PI / ring.len() as f64).sin());
      let atoms: Vec<_> = ring
        .iter()
        .enumerate()
        .map(|(i, &row)| {
          let angle = TAU * i as f64 / ring.len() as f64;
          (row, Point::new(radius * angle.cos(), radius * angle.sin()))
        })
        .collect();
      for &(row, _) in &atoms {
        owner[row] = Some(fragments.len());
      }
      for (i, &(a, pa)) in atoms.iter().enumerate() {
        for &(b, pb) in &atoms[i + 1..] {
          chords.push(([a, b], pa.distance(pb)));
        }
      }
      fragments.push(Fragment { atoms });
    }
    for path in &plan.paths {
      if path.closed {
        return Ok(None);
      }
      for &row in [path.atoms[0], path.atoms[path.atoms.len() - 1]].iter() {
        if owner[row].is_none() {
          owner[row] = Some(fragments.len());
          fragments.push(Fragment {
            atoms: vec![(row, Point::new(0., 0.))],
          });
        }
      }
    }
    let mut paths = Vec::new();
    for path in plan.paths {
      let ends = [path.atoms[0], path.atoms[path.atoms.len() - 1]].map(|row| owner[row]);
      let [Some(a), Some(b)] = ends else {
        return Err(Error::CoordinateRange);
      };
      if a == b || path.atoms.len() < 3 {
        return Ok(None);
      }
      paths.push(Path {
        atoms: path.atoms,
        ends: [a, b],
      });
    }
    if paths.len() < 3 || fragments.len() > 48 {
      return Ok(None);
    }
    Ok(Some(Self {
      edges,
      fragments,
      paths,
      chords,
      owned: owner.iter().map(Option::is_some).collect(),
    }))
  }
}

impl Scaffold {
  /// Generates reduced-graph candidates only for large, multiply connected blocks.
  ///
  /// # Parameters
  /// - `graph`, `block`, `indices`: immutable integer-bond source topology.
  /// - `work`: shared bounded allowance; caller reserves ordinary fallback work.
  ///
  /// # Returns
  /// Complete unit-edge poses with rigid small rings. No pose is accepted merely
  /// because the sketch closes: all real source edges are checked after expansion.
  ///
  /// # Errors
  /// Graph errors and work exhaustion propagate; unsupported models return `None`.
  pub(super) fn new(
    graph: &MolecularGraph,
    block: &RingBlock,
    indices: &BTreeMap<AtomId, usize>,
    work: &mut Work,
  ) -> Result<Option<Self>, Error> {
    // Compact constrained polycycles retain the established embedding models;
    // the reduced sketch is intended for large blocks with long flexible paths.
    if block.atoms().len() < 64 || block.cycle_rank() < 3 || block.cycle_rank() > 12 {
      return Ok(None);
    }
    let atoms: Vec<_> = block.atoms().iter().map(|id| indices[id]).collect();
    let local: BTreeMap<_, _> = block.atoms().iter().enumerate().map(|(i, &id)| (id, i)).collect();
    let mut edges = Vec::new();
    for &id in block.bonds() {
      let bond = graph.bond(id).ok_or(chitin_chem::GraphError::MissingBond(id))?;
      if bond.order() == BondOrder::Triple {
        return Ok(None);
      }
      let [a, b] = bond.atoms().map(|id| local[&id]);
      edges.push([a.min(b), a.max(b)]);
    }
    for &id in block.atoms() {
      let neighbors = graph.neighbors(id)?;
      work.charge(1 + neighbors.len())?;
      let orders: Vec<_> = neighbors
        .map(|(_, edge)| graph.bond(edge).map(|bond| bond.order()))
        .collect();
      if orders.len() == 2 && orders.iter().all(|&order| order == Some(BondOrder::Double)) {
        return Ok(None);
      }
    }
    edges.sort_unstable();
    let cycles = Constraints::cycles(graph, block, &local, work)?;
    let Some(skeleton) = Skeleton::new(edges, &cycles, atoms.len(), work)? else {
      return Ok(None);
    };
    if skeleton.paths.iter().filter(|path| path.atoms.len() >= 8).count() < 3 {
      return Ok(None);
    }
    let poses = geometry::candidates(&skeleton, work)?;
    if poses.is_empty() {
      return Ok(None);
    }
    Ok(Some(Self {
      atoms,
      edges: skeleton.edges,
      poses,
    }))
  }

  /// Returns block size for stable seed priority.
  pub(super) fn len(&self) -> usize {
    self.atoms.len()
  }
  /// Returns the bounded number of complete candidates.
  pub(super) fn variants(&self) -> usize {
    self.poses.len()
  }

  /// Rigidly attaches one complete candidate to at most one existing source atom.
  ///
  /// # Errors
  /// Multiple existing anchors defer to another embedding rather than stretching
  /// an island. Exterior headings come from actual local source-neighbor rays.
  pub(super) fn embed(&self, old: &[Option<Point>], variant: usize) -> Result<Vec<(usize, Point, f64)>, Error> {
    let anchors: Vec<_> = self
      .atoms
      .iter()
      .enumerate()
      .filter_map(|(i, &row)| old[row].map(|p| (i, p)))
      .collect();
    if anchors.len() > 1 {
      return Err(Error::CoordinateRange);
    }
    let mut points = self.poses[variant % self.poses.len()].clone();
    if let Some(&(row, target)) = anchors.first() {
      let offset = Point::new(target.x - points[row].x, target.y - points[row].y);
      for point in &mut points {
        *point = point.translated(offset.x, offset.y);
      }
    }
    Ok(
      self
        .atoms
        .iter()
        .enumerate()
        .map(|(i, &row)| {
          let mut angles: Vec<_> = self
            .edges
            .iter()
            .filter_map(|&[a, b]| {
              let other = if a == i {
                b
              } else if b == i {
                a
              } else {
                return None;
              };
              Some((points[other].y - points[i].y).atan2(points[other].x - points[i].x))
            })
            .collect();
          angles.sort_by(f64::total_cmp);
          let (mut gap, mut heading) = (0., 0.);
          for j in 0..angles.len() {
            let delta = (angles[(j + 1) % angles.len()] - angles[j]).rem_euclid(TAU);
            if delta > gap {
              gap = delta;
              heading = angles[j] + delta / 2.;
            }
          }
          (row, points[i], heading)
        })
        .collect(),
    )
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn reduced_paths_should_own_each_source_edge_once_and_keep_distinct_island_ports() -> Result<(), Error> {
    let rings = vec![vec![0, 1, 2, 3, 4, 5], vec![6, 7, 8, 9, 10, 11]];
    let mut edges = Vec::new();
    for cycle in &rings {
      for (&a, &b) in cycle.iter().zip(cycle.iter().cycle().skip(1)) {
        edges.push([a, b]);
      }
    }
    for path in [[0, 12, 13, 6], [3, 14, 15, 9], [1, 16, 17, 7]] {
      edges.extend(path.windows(2).map(|pair| [pair[0], pair[1]]));
    }
    let Some(model) = Skeleton::new(edges, &rings, 18, &mut Work::new(100_000))? else {
      return Err(Error::CoordinateRange);
    };
    assert_eq!(model.fragments.len(), 2);
    assert_eq!(model.paths.len(), 3);
    for &[a, b] in &model.edges {
      let rigid = model
        .fragments
        .iter()
        .filter(|fragment| {
          fragment.atoms.iter().any(|&(row, _)| row == a) && fragment.atoms.iter().any(|&(row, _)| row == b)
        })
        .count();
      let flexible = model
        .paths
        .iter()
        .flat_map(|path| path.atoms.windows(2))
        .filter(|pair| (pair[0] == a && pair[1] == b) || (pair[0] == b && pair[1] == a))
        .count();
      assert_eq!(rigid + flexible, 1);
    }
    assert!(
      model
        .paths
        .iter()
        .all(|path| path.ends == [0, 1] || path.ends == [1, 0])
    );
    Ok(())
  }
}
