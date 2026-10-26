//! Exact circular geometry shared by rendering, bounds and arrow routing.

use super::{Color, Source};
use crate::geometry::Point;

/// A constant-radius circular stroke in y-down drawing coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct CircularArc {
  /// Circle center.
  pub center: Point,
  /// Positive centerline radius.
  pub radius: f64,
  /// Initial polar angle in radians.
  pub start_angle: f64,
  /// Signed sweep in radians; positive is clockwise in y-down coordinates.
  /// Must be nonzero and strictly smaller than a complete revolution.
  pub sweep: f64,
  /// Positive stroke width.
  pub width: f64,
  /// Stroke paint.
  pub color: Color,
  /// Optional chemical origin.
  pub source: Option<Source>,
}

impl CircularArc {
  /// Evaluates a fraction of the sweep; zero and one are the endpoints.
  pub fn point(&self, fraction: f64) -> Point {
    let angle = self.start_angle + self.sweep * fraction;
    self
      .center
      .translated(self.radius * angle.cos(), self.radius * angle.sin())
  }

  /// Returns the unit forward tangent at a sweep fraction.
  pub fn tangent(&self, fraction: f64) -> Point {
    let angle = self.start_angle + self.sweep * fraction;
    let sign = self.sweep.signum();
    Point::new(-angle.sin() * sign, angle.cos() * sign)
  }

  /// Tests a polar angle against the directed sweep, including endpoints.
  pub(crate) fn contains_angle(&self, angle: f64) -> bool {
    ((angle - self.start_angle) * self.sweep.signum()).rem_euclid(std::f64::consts::TAU) <= self.sweep.abs()
  }
}
