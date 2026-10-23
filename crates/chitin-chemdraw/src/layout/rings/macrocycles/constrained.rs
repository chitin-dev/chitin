//! Stereo-directed closed walks instead of fitting constraints to fixed outlines.
//!
//! Search states own genuine source vertices and unit bonds. Local turn parity
//! enforces supplied alkene relations before a walk can consume beam capacity.
//! Complete walks still pass the common closure and intersection validator.

use std::{collections::BTreeMap, f64::consts::TAU};

use chitin_chem::DoubleBondRelation;

use super::{Error, Point, Requirement, Work, caps};

mod closure;

/// Closes a flexible boundary while retaining an exactly rigid shared segment.
///
/// Fixed turns use ring-local vertices and signed exterior radians. The angular
/// solver owns closure; no cap atom or extra chemical constraint is invented.
/// Work exhaustion propagates to the optional whole-block caller.
pub(super) fn fixed_outlines(
  n: usize,
  requirements: &[Requirement],
  fixed: &[(usize, f64)],
  work: &mut Work,
) -> Result<Vec<Vec<Point>>, Error> {
  if !(10..=64).contains(&n) {
    return Ok(Vec::new());
  }
  closure::fixed_outlines(n, requirements, fixed, work)
}

/// Bounded partial source walk with integer lattice position and direction.
#[derive(Clone)]
struct Walk {
  /// Exact axial lattice coordinates, including the most recent source vertex.
  vertices: Vec<(i32, i32)>,
  /// Turns at vertices one onward; vertex zero is determined at closure.
  turns: Vec<i32>,
  /// Most recent directed bond, in sixty-degree increments.
  heading: i32,
  /// Accumulated signed turns, excluding the unknown first vertex.
  winding: i32,
  /// Soft guide deviation; never substitutes for geometric validity.
  cost: f64,
}

/// Converts an axial triangular-lattice vertex to unit Cartesian coordinates.
fn point((x, y): (i32, i32)) -> Point {
  Point::new(f64::from(x) + f64::from(y) * 0.5, f64::from(y) * 3_f64.sqrt() * 0.5)
}

/// Tests a local turn pair against the supplied ring-neighbor relation.
fn compatible(requirements: &[Requirement], n: usize, edge: usize, a: i32, b: i32) -> bool {
  requirements.iter().all(|requirement| {
    let [left, right, _, _] = requirement.slots;
    if (left == edge && right == (edge + 1) % n) || (right == edge && left == (edge + 1) % n) {
      (a == b) == (requirement.expected == DoubleBondRelation::SameSide)
    } else {
      true
    }
  })
}

/// Searches constrained lattice walks, then closes nonlattice turn sequences.
///
/// # Parameters
/// - `n`: source cycle size, between ten and sixty-four atoms.
/// - `requirements`: source-indexed ring-neighbor alkene constraints.
/// - `work`: shared allowance, charged for rejected states and validation.
///
/// # Returns
/// Complete outlines with exact source bonds and validated side relations.
/// Odd sizes use angular closure; even sizes first try exact lattice walks.
///
/// # Errors
/// Work exhaustion propagates so an optional caller can retain its baseline.
pub(super) fn outlines(n: usize, requirements: &[Requirement], work: &mut Work) -> Result<Vec<Vec<Point>>, Error> {
  if !(10..=64).contains(&n) {
    return Ok(Vec::new());
  }
  if n.is_multiple_of(2) {
    let reserve = work.remaining().saturating_sub(500_000);
    if let Some(candidates) = work.optional(reserve, |child| lattice(n, requirements, child))?
      && !candidates.is_empty()
    {
      return Ok(candidates);
    }
  }
  closure::outlines(n, requirements, work)
}

/// Searches exact sixty-degree walks without spending the nonlattice reserve.
fn lattice(n: usize, requirements: &[Requirement], work: &mut Work) -> Result<Vec<Vec<Point>>, Error> {
  if !(10..=64).contains(&n) || !n.is_multiple_of(2) {
    return Ok(Vec::new());
  }
  let directions = [(1, 0), (0, 1), (-1, 1), (-1, 0), (0, -1), (1, -1)];
  let mut beam = vec![Walk {
    vertices: vec![(0, 0), (1, 0)],
    turns: Vec::new(),
    heading: 0,
    winding: 0,
    cost: 0.,
  }];
  let mut completed = Vec::new();
  for index in 1..n {
    let mut next = Vec::new();
    let angle = TAU * (index + 1) as f64 / n as f64;
    let radius = 0.85 * n as f64 / TAU;
    let guide = Point::new(radius * angle.sin(), radius * (1. - angle.cos()));
    for walk in &beam {
      for turn in [-1, 1] {
        work.charge(index + requirements.len())?;
        if let Some(&previous) = walk.turns.last()
          && !compatible(requirements, n, index - 1, previous, turn)
        {
          continue;
        }
        let heading = (walk.heading + turn).rem_euclid(6);
        let step = directions[heading as usize];
        let last = walk.vertices[index];
        let vertex = (last.0 + step.0, last.1 + step.1);
        let winding = walk.winding + turn;
        if (6_i32 - winding).unsigned_abs() as usize > n - index {
          continue;
        }
        if index == n - 1 {
          let first_turn = (6 - heading).rem_euclid(6);
          let first_turn = if first_turn == 5 { -1 } else { first_turn };
          if vertex != (0, 0)
            || !matches!(first_turn, -1 | 1)
            || winding + first_turn != 6
            || !compatible(requirements, n, n - 1, turn, first_turn)
            || !compatible(requirements, n, 0, first_turn, walk.turns[0])
          {
            continue;
          }
          let points: Vec<_> = walk.vertices.iter().copied().map(point).collect();
          if caps::valid(&points, work)? {
            completed.push((walk.cost, points));
          }
          continue;
        }
        if walk.vertices.contains(&vertex) || point(vertex).distance(Point::new(0., 0.)) > (n - index - 1) as f64 {
          continue;
        }
        let mut candidate = walk.clone();
        candidate.vertices.push(vertex);
        candidate.turns.push(turn);
        candidate.heading = heading;
        candidate.winding = winding;
        candidate.cost += point(vertex).distance(guide).powi(2);
        next.push(candidate);
      }
    }
    next.sort_by(|a, b| a.cost.total_cmp(&b.cost));
    // Preserve path diversity: equal endpoints can hide different obstacles.
    let mut counts = BTreeMap::new();
    next.retain(|walk| {
      let key = (walk.vertices[index + 1], walk.heading, walk.winding, walk.turns[0]);
      let count = counts.entry(key).or_insert(0);
      *count += 1;
      *count <= 4
    });
    next.truncate(256);
    beam = next;
    if beam.is_empty() {
      break;
    }
  }
  completed.sort_by(|a, b| a.0.total_cmp(&b.0));
  completed.truncate(6);
  Ok(completed.into_iter().map(|(_, points)| points).collect())
}

#[cfg(test)]
mod tests {
  use super::*;
  use chitin_chem::io::smiles::read_smiles;

  type TestResult = Result<(), Box<dyn std::error::Error>>;

  #[test]
  fn walk_search_should_return_only_simple_exact_closed_candidates() -> TestResult {
    let mut work = Work::new(750_000);
    let candidates = outlines(12, &[], &mut work)?;
    assert!(!candidates.is_empty());
    for candidate in candidates {
      assert_eq!(candidate.len(), 12);
      assert!(caps::valid(&candidate, &mut work)?);
    }
    Ok(())
  }

  #[test]
  fn parity_checks_should_handle_reversed_axes_and_the_closing_edge() -> TestResult {
    let graph = read_smiles("CC")?.into_graph();
    let bond = graph.bonds().next().ok_or("missing bond")?.0;
    let requirement = Requirement {
      bond,
      slots: [0, 11, 1, 10],
      expected: DoubleBondRelation::OppositeSides,
    };
    assert!(compatible(&[requirement], 12, 11, -1, 1));
    let requirement = Requirement {
      bond,
      slots: [0, 11, 1, 10],
      expected: DoubleBondRelation::OppositeSides,
    };
    assert!(!compatible(&[requirement], 12, 11, 1, 1));
    Ok(())
  }

  #[test]
  fn exhausted_search_should_never_publish_a_partial_walk() {
    assert!(matches!(
      outlines(24, &[], &mut Work::new(1)),
      Err(Error::RingSearchBudgetExceeded)
    ));
  }

  #[test]
  fn unsupported_lattice_parity_should_leave_nonlinear_families_available() -> TestResult {
    assert!(lattice(25, &[], &mut Work::new(0))?.is_empty());
    Ok(())
  }
}
