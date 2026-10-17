//! Rigid regular-ring seeds assembled across shared vertices/edges.
//!
//! The domain decomposition determines block boundaries. This initial embedding
//! supports simple cycle blocks and edge-fused bicyclic blocks, not arbitrary
//! bridged cages. The selected polygons are layout constraints, not perceived SSSR.

use super::{LayoutError as Error, placement::Topology};
use crate::geometry::Point;
use chitin_chem::{MolecularGraph, perception::analyze_rings};
use std::{
  collections::BTreeMap,
  f64::consts::{PI, TAU},
};

/// Polygon constraints plus an atom-to-polygon incidence index.
pub(super) struct Rings {
  /// Each polygon visits consecutive bonded vertices; closure is implicit.
  cycles: Vec<Vec<usize>>,
  /// Polygon indices sharing each dense atom, including fused/spiro junctions.
  pub(super) membership: Vec<Vec<usize>>,
}

impl Rings {
  /// Converts topological blocks into the supported rigid polygon constraints.
  ///
  /// # Parameters
  /// - `graph`, `topology`: the same graph namespace and dense adjacency snapshot.
  ///
  /// # Returns
  /// Independent rings, spiro connections, and two polygons per simple fused
  /// bicyclic block. Connected rings need not share a block to share a molecule.
  ///
  /// # Errors
  /// Rejects bridged/higher-rank blocks rather than breaking a ring-closure bond.
  pub(super) fn new(graph: &MolecularGraph, topology: &Topology) -> Result<Self, Error> {
    let report = analyze_rings(graph)?;
    let indices: BTreeMap<_, _> = topology.ids.iter().enumerate().map(|(i, &id)| (id, i)).collect();
    let mut cycles = Vec::new();
    for block in report.blocks().iter().filter(|block| block.cycle_rank() > 0) {
      let nodes: Vec<_> = block.atoms().iter().map(|id| indices[id]).collect();
      let mut rows = BTreeMap::<usize, Vec<usize>>::new();
      for &edge in block.bonds() {
        let [a, b] = graph
          .bond(edge)
          .ok_or(chitin_chem::GraphError::MissingBond(edge))?
          .atoms();
        rows.entry(indices[&a]).or_default().push(indices[&b]);
        rows.entry(indices[&b]).or_default().push(indices[&a]);
      }
      for row in rows.values_mut() {
        row.sort_unstable();
      }
      let unsupported = || Error::RingSystem {
        atom: block.atoms()[0],
        cycles: block.cycle_rank(),
      };
      if block.cycle_rank() == 1 {
        cycles.push(walk(&rows, nodes[0], rows[&nodes[0]][0], nodes[0]).ok_or_else(unsupported)?);
      } else if block.cycle_rank() == 2 {
        let junctions: Vec<_> = nodes.iter().copied().filter(|node| rows[node].len() == 3).collect();
        if junctions.len() != 2
          || rows.values().any(|row| !matches!(row.len(), 2 | 3))
          || !rows[&junctions[0]].contains(&junctions[1])
        {
          return Err(unsupported());
        }
        let [a, b] = [junctions[0], junctions[1]];
        for &next in rows[&a].iter().filter(|&&next| next != b) {
          // Two non-shared paths close through the direct fusion edge. A bridged
          // theta graph without that edge cannot satisfy this polygon model.
          let mut polygon = walk(&rows, a, next, b).ok_or_else(unsupported)?;
          polygon.push(b);
          cycles.push(polygon);
        }
      } else {
        return Err(unsupported());
      }
    }
    // Large ring seeds reduce later attachment constraints; ties are stable IDs.
    cycles.sort_by(|a, b| b.len().cmp(&a.len()).then(a.cmp(b)));
    let mut membership = vec![Vec::new(); topology.ids.len()];
    for (index, cycle) in cycles.iter().enumerate() {
      for &node in cycle {
        membership[node].push(index);
      }
    }
    Ok(Self { cycles, membership })
  }

  /// Returns the largest polygon in a component, if it contains a ring.
  pub(super) fn seed(&self, component: &[usize]) -> Option<usize> {
    component
      .iter()
      .flat_map(|&node| self.membership[node].iter())
      .copied()
      .min()
  }

  /// Returns the number of rigid polygons.
  pub(super) fn len(&self) -> usize {
    self.cycles.len()
  }

  /// Embeds a polygon anchored by zero, one, or two adjacent existing vertices.
  ///
  /// # Parameters
  /// - `index`: unprocessed polygon constraint.
  /// - `positions`: previously fixed centers; shared vertices must not move.
  /// - `component`: rows eligible for scoring, excluding disconnected fragments.
  /// - `heading`: outward attachment direction when only one vertex is known.
  ///
  /// # Returns
  /// New centers and radial headings for branch expansion. Fused polygons try
  /// both sides of the shared edge; spiro/attached polygons try six orientations.
  /// Lowest atom-overlap cost wins deterministically, preserving unit edges.
  ///
  /// # Errors
  /// Rejects incompatible anchors instead of stretching a ring closure.
  pub(super) fn embed(
    &self,
    index: usize,
    positions: &[Option<Point>],
    component: &[usize],
    heading: f64,
  ) -> Result<Vec<(usize, Point, f64)>, Error> {
    let ring = &self.cycles[index];
    let known: Vec<_> = ring
      .iter()
      .enumerate()
      .filter(|&(_, &node)| positions[node].is_some())
      .map(|(i, _)| i)
      .collect();
    let radius = 1. / (2. * (PI / ring.len() as f64).sin());
    let mut candidates = Vec::new();
    match known.as_slice() {
      [] => candidates.push((Point::new(0., 0.), 0., 1.)),
      &[anchor] => {
        let point = positions[ring[anchor]].ok_or(Error::CoordinateRange)?;
        for rotation in [0., PI / 3., -PI / 3., 2. * PI / 3., -2. * PI / 3., PI] {
          let direction = heading + rotation;
          let center = point.translated(radius * direction.cos(), radius * direction.sin());
          candidates.push((center, direction + PI - TAU * anchor as f64 / ring.len() as f64, 1.));
        }
      }
      &[a, b] if b == a + 1 || (a == 0 && b == ring.len() - 1) => {
        let start = positions[ring[a]].ok_or(Error::CoordinateRange)?;
        let end = positions[ring[b]].ok_or(Error::CoordinateRange)?;
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let length = start.distance(end);
        if (length - 1.).abs() > 1e-8 {
          return Err(Error::CoordinateRange);
        }
        let apothem = 0.5 / (PI / ring.len() as f64).tan();
        for side in [-1., 1.] {
          let center = Point::new(
            (start.x + end.x) / 2. - side * dy * apothem,
            (start.y + end.y) / 2. + side * dx * apothem,
          );
          let angle = (start.y - center.y).atan2(start.x - center.x);
          let sign = if b == a + 1 { side } else { -side };
          candidates.push((center, angle - sign * TAU * a as f64 / ring.len() as f64, sign));
        }
      }
      _ => return Err(Error::CoordinateRange),
    }
    let mut best = None;
    let mut best_cost = f64::INFINITY;
    for (center, angle, sign) in candidates {
      let points: Vec<_> = ring
        .iter()
        .enumerate()
        .map(|(i, &node)| {
          let radial = angle + sign * TAU * i as f64 / ring.len() as f64;
          (
            node,
            center.translated(radius * radial.cos(), radius * radial.sin()),
            radial,
          )
        })
        .collect();
      if points
        .iter()
        .any(|&(node, point, _)| positions[node].is_some_and(|old| old.distance(point) > 1e-8))
      {
        continue;
      }
      let cost: f64 = points
        .iter()
        .filter(|&&(node, _, _)| positions[node].is_none())
        .map(|&(_, point, _)| {
          component
            .iter()
            .filter_map(|&node| positions[node])
            .map(|other| (1. - point.distance(other)).max(0.).powi(2))
            .sum::<f64>()
        })
        .sum();
      if cost < best_cost {
        best_cost = cost;
        best = Some(
          points
            .into_iter()
            .filter(|&(node, _, _)| positions[node].is_none())
            .collect(),
        );
      }
    }
    best.ok_or(Error::CoordinateRange)
  }
}

/// Walks a degree-two path, excluding its target and retaining its starting atom.
///
/// Returns `None` for a branch, premature loop, or path that cannot close within
/// the block's size. The traversal is iterative and independent of numeric IDs.
fn walk(rows: &BTreeMap<usize, Vec<usize>>, start: usize, next: usize, target: usize) -> Option<Vec<usize>> {
  let mut path = vec![start];
  let mut previous = start;
  let mut current = next;
  while current != target {
    if path.len() >= rows.len() || rows.get(&current)?.len() != 2 {
      return None;
    }
    path.push(current);
    let other = rows[&current].iter().copied().find(|&node| node != previous)?;
    previous = current;
    current = other;
  }
  Some(path)
}
