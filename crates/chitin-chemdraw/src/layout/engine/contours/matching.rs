//! Bounded cyclic assignment of detached chemistry rules to integer contours.
//! Source attribution and redistribution terms are in licenses/coordinate-generation.txt.

use super::{
  Error, Point, Work,
  lattice::{Cell, Shape, Vertex},
  shapes,
};
use std::collections::{BTreeMap, BTreeSet};

/// Ring-slot constraints contain no molecular handles or mutable coordinates.
#[derive(Default)]
pub(crate) struct Rules {
  pub hetero: Vec<bool>,
  pub attachments: Vec<(usize, usize)>,
  /// Four cyclic slots and whether their outer neighbors must lie on one side.
  pub sides: Vec<([usize; 4], bool)>,
  /// Source slot, shared small-face identity and mandatory outward orientation.
  pub fusions: Vec<(usize, usize, bool)>,
}

/// Matches initial and successively peeled cohorts with stable first-best ties.
///
/// # Parameters
/// - `n`, `rules`: perimeter and detached slot constraints in cyclic order.
/// - `work`: shared construction, matching and coordinate-copy allowance.
///
/// # Returns
/// The highest-scoring complete assignment, or `None` when no candidate exceeds
/// the rejection threshold. Odd paths contain one collapsed corner and must be
/// relaxed after assembly. No partial atom assignment is published.
///
/// # Errors
/// Invalid slot references and exhausted work reject the whole proposal.
pub(crate) fn select(n: usize, rules: &Rules, work: &mut Work) -> Result<Option<Vec<Point>>, Error> {
  if rules.hetero.len() != n
    || rules.attachments.iter().any(|&(i, _)| i >= n)
    || rules.sides.iter().any(|(slots, _)| slots.iter().any(|&i| i >= n))
    || rules.fusions.iter().any(|&(i, _, _)| i >= n)
  {
    return Err(Error::CoordinateRange);
  }
  let mut cohort = shapes::initial_family(n, work)?;
  let mut best_score = -1000_i64;
  let mut best = None;
  let mut checked = 0;
  while !cohort.is_empty() {
    let mut unique = BTreeSet::new();
    let mut current = Vec::new();
    for shape in cohort {
      if !unique.insert(shape.key(work)?) {
        continue;
      }
      let Some(path) = shape.contour(work)? else {
        continue;
      };
      if path.len() != n {
        continue;
      }
      let multiplicities: Vec<_> = path.iter().map(|&vertex| shape.multiplicity(vertex)).collect();
      let period = (1..n)
        .find(|&period| (0..n).all(|i| multiplicities[i] == multiplicities[(i + period) % n]))
        .unwrap_or(n);
      for offset in 0..period {
        work.charge(n + rules.sides.len() + rules.fusions.len())?;
        if !allowed(&shape, &path, &multiplicities, offset, rules) {
          continue;
        }
        let score = preference(&shape, &path, &multiplicities, offset, rules);
        if score > best_score {
          best_score = score;
          best = Some((0..n).map(|i| path[(i + offset) % n].point()).collect());
          if score == 0 {
            return Ok(best);
          }
        }
      }
      checked += 1;
      if checked > 41 {
        return Ok(best);
      }
      current.push(shape);
    }
    cohort = Vec::new();
    for shape in current {
      cohort.extend(shape.equivalents(work)?);
    }
  }
  Ok(best)
}

/// Intersects permitted empty cells for every shared face independently.
fn allowed(shape: &Shape, path: &[Vertex], multiplicities: &[usize], offset: usize, rules: &Rules) -> bool {
  let n = path.len();
  let mut faces: BTreeMap<usize, Vec<Cell>> = BTreeMap::new();
  for &(slot, face, outward) in &rules.fusions {
    let i = (slot + offset) % n;
    if outward && multiplicities[i] != 1 {
      return false;
    }
    let free = shape.free_cells(path[i]);
    let next = if let Some(previous) = faces.get(&face) {
      free.into_iter().filter(|cell| previous.contains(cell)).collect()
    } else {
      free
    };
    if next.is_empty() {
      return false;
    }
    faces.insert(face, next);
  }
  for &(slots, same) in &rules.sides {
    let [p, a, b, q] = slots.map(|i| path[(i + offset) % n].point());
    let axis = (b.x - a.x, b.y - a.y);
    let side = |p: Point| axis.0 * (p.y - a.y) - axis.1 * (p.x - a.x);
    if (side(p) * side(q) > 0.) != same {
      return false;
    }
  }
  true
}

/// Negative costs favor inward heteroatoms and outward, noncolliding attachments.
fn preference(shape: &Shape, path: &[Vertex], multiplicities: &[usize], offset: usize, rules: &Rules) -> i64 {
  let n = path.len();
  let mut score = 0;
  for (slot, &hetero) in rules.hetero.iter().enumerate() {
    if hetero && multiplicities[(slot + offset) % n] == 1 {
      score -= 1;
    }
  }
  let mut occupied = BTreeSet::new();
  for &(slot, weight) in &rules.attachments {
    let i = (slot + offset) % n;
    if multiplicities[i] != 2 {
      continue;
    }
    score -= 10 * weight as i64;
    // Attachment collision follows the assigned slot, not its sparse-list index.
    if let Some(vertex) = shape.substituent(path[i]) {
      if !occupied.insert(vertex) {
        score -= 200;
      }
      if path.contains(&vertex) {
        score -= 400;
      }
    }
  }
  score
}

#[cfg(test)]
mod tests {
  use super::*;
  type TestResult = Result<(), Box<dyn std::error::Error>>;

  #[test]
  fn odd_contours_should_collapse_one_corner_without_losing_atoms() -> TestResult {
    for n in (9..=39).step_by(2) {
      let rules = Rules {
        hetero: vec![false; n],
        ..Default::default()
      };
      let points = select(n, &rules, &mut Work::new(1_000_000))?.ok_or("no odd contour")?;
      assert_eq!(points.len(), n);
      let lengths: Vec<_> = (0..n).map(|i| points[i].distance(points[(i + 1) % n])).collect();
      assert_eq!(lengths.iter().filter(|&&length| length > 1.01).count(), 1);
      assert!(lengths.iter().all(|&length| (1. ..1.74).contains(&(length + 1e-9))));
    }
    Ok(())
  }

  #[test]
  fn incompatible_fused_face_slots_should_not_publish_an_assignment() -> TestResult {
    let rules = Rules {
      hetero: vec![false; 10],
      fusions: vec![(0, 0, true), (5, 0, true)],
      ..Default::default()
    };
    assert!(select(10, &rules, &mut Work::new(1_000_000))?.is_none());
    Ok(())
  }
}
