//! Budgeted identity and duplicate rejection for one rigid branch search.
//!
//! Different reflection/rotation choices can leave a terminal vertex fixed or
//! produce the same coordinates. Only moved source rows participate in identity
//! checks; visually similar poses with permuted atoms remain distinct.

use super::{Budget, Point};

/// Previously admitted poses for one cut, bounded by its seven transforms.
#[derive(Default)]
pub(super) struct Trials {
  /// Coordinates follow the caller's unchanged movable-row order.
  poses: Vec<Vec<Point>>,
}

impl Trials {
  /// Admits a new pose only after charging comparison and snapshot work.
  ///
  /// # Parameters
  /// - `rows`: the same movable source rows for every call on this cut.
  /// - `original`, `candidate`: complete finite coordinates in one unit frame.
  /// - `budget`: shared allowance, including comparisons against earlier poses.
  ///
  /// # Returns
  /// `Some(true)` for a stored new pose, `Some(false)` for identity/repetition,
  /// or `None` without storing anything when work is exhausted. A 1e-12 unit
  /// tolerance absorbs rotation roundoff; no rigid alignment or graph-identity
  /// substitution is performed. Callers still enforce angle/stereo constraints.
  pub(super) fn insert(
    &mut self,
    rows: &[usize],
    original: &[Point],
    candidate: &[Point],
    budget: &mut Budget,
  ) -> Option<bool> {
    if !budget.charge(rows.len().saturating_mul(self.poses.len() + 2)) {
      return None;
    }
    let same = |a: Point, b: Point| (a.x - b.x).abs() <= 1e-12 && (a.y - b.y).abs() <= 1e-12;
    if rows.iter().all(|&row| same(original[row], candidate[row]))
      || self
        .poses
        .iter()
        .any(|pose| rows.iter().zip(pose).all(|(&row, &point)| same(point, candidate[row])))
    {
      return Some(false);
    }
    self.poses.push(rows.iter().map(|&row| candidate[row]).collect());
    Some(true)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn identity_and_repeated_coordinates_should_not_require_another_measurement() {
    let original = [Point::new(0., 0.), Point::new(1., 0.)];
    let moved = [original[0], Point::new(-1., 0.)];
    let mut tried = Trials::default();
    let mut work = Budget(100);
    assert_eq!(tried.insert(&[1], &original, &original, &mut work), Some(false));
    assert_eq!(tried.insert(&[1], &original, &moved, &mut work), Some(true));
    assert_eq!(tried.insert(&[1], &original, &moved, &mut work), Some(false));
  }

  #[test]
  fn exchanging_atom_coordinates_should_remain_a_distinct_pose() {
    let original = [Point::new(0., 0.), Point::new(1., 0.)];
    let a = [Point::new(0., 1.), Point::new(1., 1.)];
    let b = [a[1], a[0]];
    let mut tried = Trials::default();
    let mut work = Budget(100);
    assert_eq!(tried.insert(&[0, 1], &original, &a, &mut work), Some(true));
    assert_eq!(tried.insert(&[0, 1], &original, &b, &mut work), Some(true));
  }

  #[test]
  fn exhausted_comparisons_should_not_store_a_pose() {
    let original = [Point::new(0., 0.)];
    let moved = [Point::new(1., 0.)];
    let mut tried = Trials::default();
    assert_eq!(tried.insert(&[0], &original, &moved, &mut Budget(1)), None);
    assert!(tried.poses.is_empty());
  }
}
