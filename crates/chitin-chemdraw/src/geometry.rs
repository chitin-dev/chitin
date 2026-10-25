//! Shared finite geometry; layout units and drawing units are documented by callers.

/// An invalid coordinate or nonrepresentable extent, never silently clamped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum GeometryError {
  /// A coordinate or computed extent contains NaN or infinity.
  #[error("geometry must have finite coordinates and extents")]
  NonFinite,
}

/// Tests a finite segment against an expanded axis-aligned envelope.
pub(crate) fn segment_enters_box(start: Point, end: Point, bounds: [Point; 2], margin: f64) -> bool {
  let [min, max] = bounds;
  let delta = Point::new(end.x - start.x, end.y - start.y);
  let mut interval = [0_f64, 1_f64];
  for (p, q) in [
    (-delta.x, start.x - min.x + margin),
    (delta.x, max.x - start.x + margin),
    (-delta.y, start.y - min.y + margin),
    (delta.y, max.y - start.y + margin),
  ] {
    if p.abs() < 1e-12 {
      if q < 0. {
        return false;
      }
    } else if p < 0. {
      interval[0] = interval[0].max(q / p);
    } else {
      interval[1] = interval[1].min(q / p);
    }
  }
  interval[0] <= interval[1]
}

/// A two-dimensional point; raw construction is validated at stage boundaries.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
  /// Horizontal coordinate, positive to the right.
  pub x: f64,
  /// Vertical coordinate; layout is y-up, drawing/export is y-down.
  pub y: f64,
}

impl Point {
  /// Creates a point without hiding nonfinite input from later validation.
  pub const fn new(x: f64, y: f64) -> Self {
    Self { x, y }
  }

  /// Returns whether both coordinates are finite.
  pub fn is_finite(self) -> bool {
    self.x.is_finite() && self.y.is_finite()
  }

  /// Returns Euclidean distance without squaring large coordinates first.
  pub fn distance(self, other: Self) -> f64 {
    (self.x - other.x).hypot(self.y - other.y)
  }

  /// Translates by a vector; consumers validate the resulting coordinates.
  pub(crate) fn translated(self, x: f64, y: f64) -> Self {
    Self::new(self.x + x, self.y + y)
  }
}

/// Finite axis-aligned bounds, possibly zero-width/height for isolated points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
  /// Componentwise minimum; constructor validates every coordinate and extent.
  min: Point,
  /// Componentwise maximum, never smaller than `min` on either axis.
  max: Point,
}

impl Bounds {
  /// Computes bounds without assigning an arbitrary origin to empty input.
  ///
  /// # Parameters
  /// - `points`: coordinates in one shared unit/axis convention, in any order.
  ///
  /// # Returns
  /// `None` for no points, otherwise extrema with finite width and height.
  ///
  /// # Errors
  /// Rejects nonfinite points or overflowing differences between finite extrema.
  pub fn from_points(points: impl IntoIterator<Item = Point>) -> Result<Option<Self>, GeometryError> {
    let mut bounds: Option<Self> = None;
    for point in points {
      if !point.is_finite() {
        return Err(GeometryError::NonFinite);
      }
      bounds = Some(match bounds {
        None => Self { min: point, max: point },
        Some(previous) => Self {
          min: Point::new(previous.min.x.min(point.x), previous.min.y.min(point.y)),
          max: Point::new(previous.max.x.max(point.x), previous.max.y.max(point.y)),
        },
      });
    }
    if bounds.is_some_and(|b| !b.width().is_finite() || !b.height().is_finite()) {
      return Err(GeometryError::NonFinite);
    }
    Ok(bounds)
  }

  /// Returns the minimum coordinate on both axes.
  pub const fn min(self) -> Point {
    self.min
  }
  /// Returns the maximum coordinate on both axes.
  pub const fn max(self) -> Point {
    self.max
  }
  /// Returns the horizontal extent.
  pub fn width(self) -> f64 {
    self.max.x - self.min.x
  }
  /// Returns the vertical extent.
  pub fn height(self) -> f64 {
    self.max.y - self.min.y
  }
}
