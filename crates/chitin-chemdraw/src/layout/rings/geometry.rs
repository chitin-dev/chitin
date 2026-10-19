//! Local planar predicates shared by macrocycle construction and fused assembly.

use super::{Point, side};

/// Computes point-to-segment clearance without assuming a unit-length edge.
pub(super) fn clearance(point: Point, a: Point, b: Point) -> f64 {
  let (dx, dy) = (b.x - a.x, b.y - a.y);
  let projection = ((point.x - a.x) * dx + (point.y - a.y) * dy) / (dx * dx + dy * dy);
  point.distance(a.translated(dx * projection.clamp(0., 1.), dy * projection.clamp(0., 1.)))
}

/// Tests nonincident unit edges for crossings, overlap, tangency and clearance.
pub(super) fn separated(a: Point, b: Point, c: Point, d: Point) -> bool {
  if side(a, b, c) * side(a, b, d) < -1e-12 && side(c, d, a) * side(c, d, b) < -1e-12 {
    return false;
  }
  clearance(a, c, d)
    .min(clearance(b, c, d))
    .min(clearance(c, a, b))
    .min(clearance(d, a, b))
    >= 0.25
}

/// Tests strict polygon interior with a half-open horizontal ray.
///
/// Callers handle shared boundary vertices separately. Concave polygons require
/// the actual walk; the centroid's half-plane is not an interior oracle.
pub(super) fn contains(polygon: &[Point], point: Point) -> bool {
  let mut inside = false;
  for (a, b) in polygon.iter().zip(polygon.iter().cycle().skip(1)).take(polygon.len()) {
    if (a.y > point.y) != (b.y > point.y) && point.x < a.x + (point.y - a.y) * (b.x - a.x) / (b.y - a.y) {
      inside = !inside;
    }
  }
  inside
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn edge_separation_should_reject_crossings_tangencies_and_collinear_overlap() {
    let (a, b) = (Point::new(-1., 0.), Point::new(1., 0.));
    for (c, d) in [
      (Point::new(0., -1.), Point::new(0., 1.)),
      (Point::new(0., 0.), Point::new(0., 1.)),
      (Point::new(-0.5, 0.), Point::new(1.5, 0.)),
    ] {
      assert!(!separated(a, b, c, d));
    }
    assert!(separated(a, b, Point::new(-1., 1.), Point::new(1., 1.)));
  }

  #[test]
  fn interior_test_should_handle_concave_notches_and_reversed_winding() {
    let mut polygon = vec![
      Point::new(0., 0.),
      Point::new(3., 0.),
      Point::new(3., 3.),
      Point::new(2., 3.),
      Point::new(2., 1.),
      Point::new(1., 1.),
      Point::new(1., 3.),
      Point::new(0., 3.),
    ];
    for _ in 0..2 {
      assert!(contains(&polygon, Point::new(0.5, 2.)));
      assert!(!contains(&polygon, Point::new(1.5, 2.)));
      polygon.reverse();
    }
  }
}
