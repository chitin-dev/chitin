//! Explicit rigid-fragment and flexible-path decomposition of a cyclic block.
//!
//! Every source edge belongs either to a small-ring fragment or to exactly one
//! maximal path. Shared junctions have one coordinate; a fold never fabricates
//! a carbon, removes a closure, or assigns a front/back crossing order.

use std::collections::BTreeMap;

use super::{Error, Point, Work};

use super::super::walks as search;

/// A source-vertex walk, with closure represented once rather than duplicated.
pub(super) struct Path {
  /// Local atom rows in connected traversal order, including both open ends.
  pub(super) atoms: Vec<usize>,
  /// A free simple ring closes from its last vertex back to its first.
  pub(super) closed: bool,
}

/// Immutable ownership and geometry guards for one complete cyclic block.
pub(super) struct Plan {
  /// Small rings sharing atoms form one rigid fragment; distinct rings stay distinct.
  pub(super) fragments: Vec<Vec<usize>>,
  /// Maximal walks connecting rigid fragments and shared branch junctions.
  pub(super) paths: Vec<Path>,
  /// All original source edges, including those not on a selected basis face.
  edges: Vec<[usize; 2]>,
  /// Triple/cumulated-double centers retain linear local geometry.
  linear: Vec<bool>,
}

impl Plan {
  /// Partitions a block independently of source atom numbers or cycle names.
  ///
  /// # Parameters
  /// - `edges`, `cycles`: complete local source edges and a selected cycle basis.
  /// - `linear`: source-derived linear-center flags, one per local vertex.
  /// - `work`: shared allowance for incidence, traversal and coverage checks.
  ///
  /// # Returns
  /// Small cycles of three to six vertices become rigid islands. Remaining
  /// degree-two vertices form paths between islands or degree-three junctions.
  /// Unanchored cycles are retained as closed paths. Every edge is checked for
  /// exactly one owner; cycle-basis overlap cannot duplicate a shared path.
  ///
  /// # Errors
  /// Work exhaustion propagates before partial topology can be published.
  pub(super) fn new(
    edges: &[[usize; 2]],
    cycles: &[Vec<usize>],
    linear: Vec<bool>,
    work: &mut Work,
  ) -> Result<Self, Error> {
    let n = linear.len();
    work.charge(n + edges.len())?;
    let lookup: BTreeMap<_, _> = edges
      .iter()
      .enumerate()
      .map(|(i, &[a, b])| ((a.min(b), a.max(b)), i))
      .collect();
    let mut rows = vec![Vec::new(); n];
    let mut rigid = vec![false; edges.len()];
    for (i, &[a, b]) in edges.iter().enumerate() {
      rows[a].push((b, i));
      rows[b].push((a, i));
    }
    for cycle in cycles.iter().filter(|cycle| cycle.len() <= 6) {
      work.charge(cycle.len())?;
      for (&a, &b) in cycle.iter().zip(cycle.iter().cycle().skip(1)) {
        rigid[lookup[&(a.min(b), a.max(b))]] = true;
      }
    }
    let fixed: Vec<_> = rows
      .iter()
      .map(|row| row.iter().any(|&(_, edge)| rigid[edge]))
      .collect();
    let mut seen = vec![false; n];
    let mut fragments = Vec::new();
    for start in 0..n {
      if !fixed[start] || seen[start] {
        continue;
      }
      let mut pending = vec![start];
      let mut fragment = Vec::new();
      seen[start] = true;
      while let Some(node) = pending.pop() {
        work.charge(1 + rows[node].len())?;
        fragment.push(node);
        for &(other, edge) in &rows[node] {
          if rigid[edge] && !seen[other] {
            seen[other] = true;
            pending.push(other);
          }
        }
      }
      fragment.sort_unstable();
      fragments.push(fragment);
    }
    let junctions: Vec<_> = (0..n).map(|i| fixed[i] || rows[i].len() != 2).collect();
    let mut owned = rigid;
    let mut paths = Vec::new();
    for start in (0..n).filter(|&i| junctions[i]).chain(0..n) {
      for &(next, edge) in &rows[start] {
        if owned[edge] {
          continue;
        }
        let mut atoms = vec![start];
        let mut node = next;
        let mut incoming = edge;
        owned[incoming] = true;
        while node != start {
          work.charge(1 + rows[node].len())?;
          atoms.push(node);
          if junctions[node] {
            break;
          }
          let (other, edge) = rows[node]
            .iter()
            .copied()
            .find(|&(_, edge)| edge != incoming)
            .ok_or(Error::CoordinateRange)?;
          if owned[edge] {
            return Err(Error::CoordinateRange);
          }
          owned[edge] = true;
          incoming = edge;
          node = other;
        }
        paths.push(Path {
          atoms,
          closed: node == start,
        });
      }
    }
    if owned.iter().any(|&flag| !flag) {
      return Err(Error::CoordinateRange);
    }
    Ok(Self {
      fragments,
      paths,
      edges: edges.to_vec(),
      linear,
    })
  }

  /// Improves path turns against complete-block geometry with fixed shared ends.
  ///
  /// A bounded beam searches unit-edge zigzags and analytically closes the last
  /// two bonds. Complete accepted snapshots preserve all rigid islands and all
  /// junction positions bit-for-bit. No new crowd or crossing pair is allowed.
  /// An infeasible path retains its original pose; budget exhaustion is an error.
  pub(super) fn fold(&self, initial: &[Point], work: &mut Work) -> Result<Vec<Point>, Error> {
    work.charge(initial.len())?;
    let mut points = initial.to_vec();
    for path in self.paths.iter().filter(|path| !path.closed && path.atoms.len() >= 3) {
      // Long paths are handled in overlapping bounded windows. Each window's
      // two endpoints are real, shared source vertices, not synthetic anchors.
      for start in (0..path.atoms.len() - 2).step_by(8) {
        let end = (start + 10).min(path.atoms.len() - 1);
        let window = &path.atoms[start..=end];
        let before = work.measure(&points, &self.edges)?;
        let mut best_cost = before.cost + search::turn_cost(window, &points, &self.linear);
        let mut best = None;
        for candidate in search::candidates(window, &points, &self.edges, &self.linear, work)? {
          let quality = work.measure(&candidate, &self.edges)?;
          if !quality.crossings.is_subset(&before.crossings)
            || quality.crowds.iter().any(|pair| !before.crowds.contains(pair))
          {
            continue;
          }
          let cost = quality.cost + search::turn_cost(window, &candidate, &self.linear);
          if cost + 1e-9 < best_cost {
            best_cost = cost;
            best = Some(candidate);
          }
        }
        if let Some(candidate) = best {
          points = candidate;
        }
      }
    }
    // These assertions are checked in release too: decomposition is an
    // ownership contract, not permission to deform already-drawn small rings.
    if self.fragments.iter().flatten().any(|&i| points[i] != initial[i]) {
      return Err(Error::CoordinateRange);
    }
    Ok(points)
  }

  /// Returns the soft turn preference for all open paths of a complete pose.
  pub(super) fn turn_cost(&self, points: &[Point]) -> f64 {
    self
      .paths
      .iter()
      .filter(|path| !path.closed)
      .map(|path| search::turn_cost(&path.atoms, points, &self.linear))
      .sum()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::collections::BTreeSet;
  type TestResult = Result<(), Box<dyn std::error::Error>>;

  #[test]
  fn two_rigid_islands_should_share_only_their_real_connecting_paths() -> TestResult {
    let cycles = vec![(0..6).collect::<Vec<_>>(), (6..12).collect()];
    let mut edges: Vec<_> = cycles
      .iter()
      .flat_map(|cycle| cycle.iter().zip(cycle.iter().cycle().skip(1)).map(|(&a, &b)| [a, b]))
      .collect();
    edges.extend([[0, 12], [12, 13], [13, 6], [3, 14], [14, 15], [15, 9]]);
    let plan = Plan::new(&edges, &cycles, vec![false; 16], &mut Work::new(10_000))?;
    assert_eq!(plan.fragments, cycles);
    assert_eq!(plan.paths.len(), 2);
    let path_edges: BTreeSet<_> = plan
      .paths
      .iter()
      .flat_map(|path| path.atoms.windows(2).map(|p| (p[0].min(p[1]), p[0].max(p[1]))))
      .collect();
    assert_eq!(path_edges.len(), 6);
    assert!(
      plan
        .paths
        .iter()
        .all(|path| !path.closed && path.atoms[1..path.atoms.len() - 1].iter().all(|&i| i >= 12))
    );
    Ok(())
  }

  #[test]
  fn shared_degree_three_junctions_should_not_duplicate_bridge_paths() -> TestResult {
    let walks = [[0, 2, 3, 4, 5, 1], [0, 6, 7, 8, 9, 1], [0, 10, 11, 12, 13, 1]];
    let edges: Vec<_> = walks
      .iter()
      .flat_map(|walk| walk.windows(2).map(|p| [p[0], p[1]]))
      .collect();
    let plan = Plan::new(&edges, &[], vec![false; 14], &mut Work::new(10_000))?;
    assert!(plan.fragments.is_empty());
    assert_eq!(plan.paths.len(), 3);
    assert!(
      plan
        .paths
        .iter()
        .all(|path| path.atoms.first() == Some(&0) && path.atoms.last() == Some(&1))
    );
    assert_eq!(
      plan.paths.iter().map(|path| path.atoms.len() - 1).sum::<usize>(),
      edges.len()
    );
    Ok(())
  }

  #[test]
  fn unanchored_cycles_should_remain_closed_source_walks() -> TestResult {
    for n in [9, 12, 15] {
      let edges: Vec<_> = (0..n).map(|i| [i, (i + 1) % n]).collect();
      let plan = Plan::new(&edges, &[], vec![false; n], &mut Work::new(10_000))?;
      assert_eq!(plan.paths.len(), 1);
      assert!(plan.paths[0].closed);
      assert_eq!(plan.paths[0].atoms.len(), n);
    }
    Ok(())
  }

  #[test]
  fn folding_should_restore_zigzags_without_moving_either_anchor_or_rigid_ring() -> TestResult {
    let root = 3_f64.sqrt() / 2.;
    let points = vec![
      Point::new(0., 0.),
      Point::new(root, 0.5),
      Point::new(2. * root, 1.),
      Point::new(3. * root, 0.5),
      Point::new(4. * root, 0.),
      Point::new(-0.5, root),
      Point::new(-1.5, root),
      Point::new(-2., 0.),
      Point::new(-1.5, -root),
      Point::new(-0.5, -root),
    ];
    let cycle = vec![0, 5, 6, 7, 8, 9];
    let mut edges: Vec<_> = cycle
      .iter()
      .zip(cycle.iter().cycle().skip(1))
      .map(|(&a, &b)| [a, b])
      .collect();
    edges.extend([[0, 1], [1, 2], [2, 3], [3, 4]]);
    let mut work = Work::new(100_000);
    let plan = Plan::new(&edges, &[cycle], vec![false; 10], &mut work)?;
    let folded = plan.fold(&points, &mut work)?;
    assert!(plan.turn_cost(&folded) + 1e-9 < plan.turn_cost(&points));
    for &i in &[0, 4, 5, 6, 7, 8, 9] {
      assert_eq!(folded[i], points[i]);
    }
    assert!(
      edges
        .iter()
        .all(|&[a, b]| (folded[a].distance(folded[b]) - 1.).abs() < 1e-9)
    );
    assert_eq!(folded.len(), points.len());
    Ok(())
  }

  #[test]
  fn exhausted_path_discovery_should_not_publish_partial_ownership() {
    assert!(matches!(
      Plan::new(&[[0, 1]], &[], vec![false; 2], &mut Work::new(0)),
      Err(Error::RingSearchBudgetExceeded)
    ));
  }
}
