//! Feasibility checks for shortened, noncoincident interior bridge projections.
//!
//! Three two-edge paths cannot all be unit length in a noncoincident plane:
//! their internal vertices would need three intersections of two unit circles.
//! A bounded interior shortening is a depiction convention, not bond chemistry.

use std::collections::BTreeMap;

use super::{Error, Point, Work};
use crate::layout::quality::{crosses, segment_distance};

/// Rejects coincident vertices, hidden vertices on bonds and crossing skeletons.
///
/// # Parameters
/// - `paths`: three source paths with identical endpoints and disjoint interiors.
/// - `points`: complete candidate coordinates before rigid placement.
/// - `work`: shared ring allowance, charged before each geometric scan.
///
/// # Returns
/// Whether the projection preserves distinct visible atom locations and edges.
/// Pair tests are bounded by the ring budget, not an unbounded relaxation.
///
/// # Errors
/// Exhausted work aborts the block; no partial projection is returned.
pub(super) fn separated(
  paths: &[Vec<usize>; 3],
  points: &BTreeMap<usize, Point>,
  work: &mut Work,
) -> Result<bool, Error> {
  work.charge(points.len() + paths.iter().map(|path| path.len() - 1).sum::<usize>())?;
  let nodes: Vec<_> = points.iter().collect();
  for (i, (_, a)) in nodes.iter().enumerate() {
    work.charge(nodes.len() - i - 1)?;
    if nodes[i + 1..].iter().any(|(_, b)| a.distance(**b) < 0.55) {
      return Ok(false);
    }
  }
  let edges: Vec<_> = paths.iter().flat_map(|path| path.windows(2)).collect();
  for (i, edge) in edges.iter().enumerate() {
    let (a, b) = (points[&edge[0]], points[&edge[1]]);
    work.charge(nodes.len() + edges.len() - i - 1)?;
    for &(&node, &point) in &nodes {
      if !edge.contains(&node) && segment_distance(point, a, b) < 0.08 {
        return Ok(false);
      }
    }
    for other in &edges[i + 1..] {
      if !edge.iter().any(|node| other.contains(node)) && crosses(a, b, points[&other[0]], points[&other[1]]) {
        return Ok(false);
      }
    }
  }
  Ok(true)
}

#[cfg(test)]
mod tests {
  use super::*;

  /// A separated three-path projection with a visible interior corner.
  fn seed() -> ([Vec<usize>; 3], BTreeMap<usize, Point>) {
    (
      [vec![0, 1, 4], vec![0, 2, 4], vec![0, 3, 4]],
      BTreeMap::from([
        (0, Point::new(0., 0.)),
        (1, Point::new(0.7, 0.14)),
        (2, Point::new(0.7, 0.714)),
        (3, Point::new(0.7, -0.714)),
        (4, Point::new(1.4, 0.)),
      ]),
    )
  }

  #[test]
  fn separated_projection_should_be_accepted_but_coincident_atoms_rejected() -> Result<(), Error> {
    let (paths, mut points) = seed();
    assert!(separated(&paths, &points, &mut Work::new(1000))?);
    points.insert(1, points[&2]);
    assert!(!separated(&paths, &points, &mut Work::new(1000))?);
    Ok(())
  }

  #[test]
  fn crossing_edges_should_be_rejected_even_when_all_vertices_are_separated() -> Result<(), Error> {
    let (paths, mut points) = seed();
    points.insert(1, Point::new(2., 1.));
    assert!(!separated(&paths, &points, &mut Work::new(1000))?);
    Ok(())
  }

  #[test]
  fn geometry_validation_should_charge_before_work_and_fail_explicitly() {
    let (paths, points) = seed();
    assert!(matches!(
      separated(&paths, &points, &mut Work::new(0)),
      Err(Error::RingSearchBudgetExceeded)
    ));
  }
}
