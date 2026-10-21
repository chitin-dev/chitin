//! Whole-fragment composition search after complete coordination closure.
//!
//! Separators are discovered in the hard-distance graph, including covalent
//! angle chords. Reflecting one resulting component preserves every protected
//! distance exactly; only its pose relative to the other components changes.

use super::{Budget, Point, composition, quality};

/// Improves a complete pose without trading resolved contacts for compactness.
///
/// Work exhaustion retains the last fully measured candidate. The declared
/// center never moves, and no individual atom displacement or source edit is
/// used. This stage also operates on clean drawings: no collision is required
/// before an unnecessarily extended fragment can be folded into a better pose.
pub(super) fn improve(
  points: &mut Vec<Point>,
  constraints: &[([usize; 2], f64)],
  edges: &[[usize; 2]],
  center: usize,
  spacing: f64,
  budget: &mut Budget,
) {
  if !budget.charge(points.len() + constraints.len()) {
    return;
  }
  let mut neighbors = vec![Vec::new(); points.len()];
  for &([a, b], _) in constraints {
    neighbors[a].push(b);
    neighbors[b].push(a);
  }
  let Some(mut current) = quality::measure(points, edges, spacing, budget) else {
    return;
  };
  let mut cost = objective(points, &current, edges, center);
  for _ in 0..2 {
    let mut changed = false;
    for a in 0..points.len() {
      if neighbors[a].len() < 2 {
        continue;
      }
      for b in a + 1..points.len() {
        if neighbors[b].len() < 2 {
          continue;
        }
        let length = points[a].distance(points[b]);
        if length < 1e-8 {
          continue;
        }
        let axis = Point::new(
          (points[b].x - points[a].x) / length,
          (points[b].y - points[a].y) / length,
        );
        let Some(parts) = components(&neighbors, [a, b], budget) else {
          return;
        };
        if parts.len() < 2 {
          continue;
        }
        for part in parts {
          if part.contains(&center) {
            continue;
          }
          if !budget.charge(points.len() + edges.len()) {
            return;
          }
          let mut trial = points.clone();
          for row in part {
            let p = Point::new(points[row].x - points[a].x, points[row].y - points[a].y);
            let along = p.x * axis.x + p.y * axis.y;
            trial[row] = points[a].translated(2. * along * axis.x - p.x, 2. * along * axis.y - p.y);
          }
          let Some(q) = quality::measure(&trial, edges, spacing, budget) else {
            return;
          };
          // Contact identity/count cannot be hidden by the aesthetic objective.
          if !budget.charge(q.crowds.len().saturating_mul(current.crowds.len())) {
            return;
          }
          if !q.crossings.is_subset(&current.crossings) || !q.crowds.iter().all(|pair| current.crowds.contains(pair)) {
            continue;
          }
          let next = objective(&trial, &q, edges, center);
          if next + 1e-10 < cost {
            *points = trial;
            current = q;
            cost = next;
            changed = true;
          }
        }
      }
    }
    if !changed {
      break;
    }
  }
}

/// Removes two anchors from the distance graph without changing their constraints.
fn components(neighbors: &[Vec<usize>], anchors: [usize; 2], budget: &mut Budget) -> Option<Vec<Vec<usize>>> {
  if !budget.charge(neighbors.len()) {
    return None;
  }
  let mut seen = vec![false; neighbors.len()];
  for row in anchors {
    seen[row] = true;
  }
  let mut result = Vec::new();
  for root in 0..neighbors.len() {
    if seen[root] {
      continue;
    }
    let mut stack = vec![root];
    let mut part = Vec::new();
    let mut touches = [false; 2];
    seen[root] = true;
    while let Some(row) = stack.pop() {
      if !budget.charge(1 + neighbors[row].len()) {
        return None;
      }
      part.push(row);
      for &other in &neighbors[row] {
        for (i, &anchor) in anchors.iter().enumerate() {
          touches[i] |= other == anchor;
        }
        if !seen[other] {
          seen[other] = true;
          stack.push(other);
        }
      }
    }
    if touches.into_iter().all(|touch| touch) {
      result.push(part);
    }
  }
  Some(result)
}

/// Keeps contact scoring separate from the covalent-angle/composition preference.
fn objective(points: &[Point], q: &quality::Quality, edges: &[[usize; 2]], center: usize) -> f64 {
  q.cost + 100. * q.crossings.len() as f64 + 10. * q.crowds.len() as f64 + composition(points, edges, center)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn separator_poses_should_clear_crossings_and_preserve_all_hard_distances() {
    let mut points = vec![
      Point::new(0., 0.),
      Point::new(1., 0.),
      Point::new(2., 0.),
      Point::new(2., 1.),
      Point::new(1., -0.1),
      Point::new(0., 1.),
    ];
    let edges = [[0, 1], [1, 2], [2, 3], [3, 4], [4, 5], [5, 0]];
    let mut constraints: Vec<_> = edges
      .iter()
      .map(|&[a, b]| ([a, b], points[a].distance(points[b])))
      .collect();
    // The auxiliary chord locks the straight center as well as source lengths.
    constraints.push(([0, 2], 2.));
    let original = points.clone();
    improve(&mut points, &constraints, &edges, 0, 0.2, &mut Budget(200_000));
    assert_eq!(points[0], original[0]);
    assert!(
      constraints
        .iter()
        .all(|&([a, b], length)| (points[a].distance(points[b]) - length).abs() < 1e-10)
    );
    let inspected = quality::measure(&points, &edges, 0.2, &mut Budget(usize::MAX));
    assert!(inspected.is_some_and(|q| q.clear()));
  }

  #[test]
  fn interrupted_pose_search_should_retain_the_complete_input() {
    let mut points = vec![Point::new(0., 0.), Point::new(1., 0.), Point::new(0., 1.)];
    let before = points.clone();
    improve(&mut points, &[([0, 1], 1.)], &[[0, 1]], 0, 0.55, &mut Budget(1));
    assert_eq!(points, before);
  }
}
