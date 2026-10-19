//! Broad closed outlines with genuine unit bonds and room for inward groups.
//!
//! Auxiliary polygons define equal chords only. Each chord is replaced by a
//! two- or three-bond bent walk; no auxiliary vertex enters the chemical graph.

use std::f64::consts::{PI, TAU};

use super::{Error, Point, Work, caps};

/// Builds a bounded family of open-center outlines for densely substituted rings.
///
/// # Parameters
/// - `n`: source ring size; only 25–96 membered rings use this extra family.
/// - `work`: shared allowance for construction and geometric validation.
///
/// # Returns
/// Simple closed unit-edge walks with visible bends. Odd sizes use one
/// three-bond chord in a two-bond family, preserving exact closure without
/// stretching a bond or flattening a corner. Three-bond periodic walks provide
/// a different placement phase when the ring size permits them.
///
/// # Errors
/// Propagates budget exhaustion before returning an unvalidated outline.
pub(super) fn outlines(n: usize, work: &mut Work) -> Result<Vec<Vec<Point>>, Error> {
  if !(25..=96).contains(&n) {
    return Ok(Vec::new());
  }
  let mut result = Vec::new();
  for stride in [2, 3] {
    if stride == 3 && !n.is_multiple_of(3) {
      continue;
    }
    let sides = n / stride;
    let chord = if stride == 2 { 3_f64.sqrt() } else { 2. };
    let radius = chord / (2. * (PI / sides as f64).sin());
    work.charge(n + sides)?;
    let mut points = Vec::with_capacity(n);
    for side in 0..sides {
      let angle = TAU * side as f64 / sides as f64;
      let next = TAU * (side + 1) as f64 / sides as f64;
      let start = Point::new(radius * angle.cos(), radius * angle.sin());
      let end = Point::new(radius * next.cos(), radius * next.sin());
      let heading = (end.y - start.y).atan2(end.x - start.x);
      let steps = if stride == 2 && !n.is_multiple_of(2) && side == 0 {
        3
      } else {
        stride
      };
      let turn = if steps == 2 {
        (chord / 2.).acos()
      } else {
        ((chord - 1.) / 2.).acos()
      };
      let mut point = start;
      points.push(point);
      for step in 0..steps - 1 {
        let direction = heading + if step == 0 { -turn } else { 0. };
        point = point.translated(direction.cos(), direction.sin());
        points.push(point);
      }
    }
    if points.len() == n && caps::valid(&points, work)? {
      result.push(points);
    }
  }
  Ok(result)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::geometry::Bounds;

  type TestResult = Result<(), Box<dyn std::error::Error>>;

  #[test]
  fn open_walks_should_close_both_parities_without_flat_or_stretched_bonds() -> TestResult {
    for n in 25..=96 {
      let mut work = Work::new(100_000);
      let candidates = outlines(n, &mut work)?;
      assert!(!candidates.is_empty(), "size {n}");
      assert!(candidates.len() <= 2);
      for points in candidates {
        assert_eq!(points.len(), n);
        assert!(caps::valid(&points, &mut work)?, "size {n}");
        let bounds = Bounds::from_points(points.iter().copied())?.ok_or("empty outline")?;
        assert!(bounds.width().min(bounds.height()) > 4., "size {n}");
      }
    }
    Ok(())
  }

  #[test]
  fn interrupted_open_walk_search_should_report_work_exhaustion() {
    assert!(matches!(
      outlines(33, &mut Work::new(1)),
      Err(Error::RingSearchBudgetExceeded)
    ));
  }
}
