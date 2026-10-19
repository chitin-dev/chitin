//! Reflection-invariant relative-side tests shared by layout and depiction.

use crate::geometry::Point;
use chitin_chem::DoubleBondRelation;

/// Classifies `[axis start, axis end, start reference, end reference]`.
///
/// Returns `None` for nonfinite, coincident or nearly collinear directions.
/// Normalized cross products make the tolerance independent of export scaling
/// and ensure the same result in y-up layout and y-down scene coordinates.
pub(crate) fn relation([a, b, ra, rb]: [Point; 4]) -> Option<DoubleBondRelation> {
  let axis = Point::new(b.x - a.x, b.y - a.y);
  let length = a.distance(b);
  let side = |origin: Point, reference: Point| {
    (axis.x * (reference.y - origin.y) - axis.y * (reference.x - origin.x)) / (length * origin.distance(reference))
  };
  let left = side(a, ra);
  let right = side(b, rb);
  if !left.is_finite() || !right.is_finite() || left.abs() < 1e-8 || right.abs() < 1e-8 {
    None
  } else if left.is_sign_positive() == right.is_sign_positive() {
    Some(DoubleBondRelation::SameSide)
  } else {
    Some(DoubleBondRelation::OppositeSides)
  }
}
