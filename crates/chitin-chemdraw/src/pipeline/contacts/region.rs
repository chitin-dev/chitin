//! Measured label footprints, shared by contact acceptance and repair envelopes.

use crate::{
  depict::DepictError,
  geometry::{Point, segment_enters_box},
  scene::{Line, LineCap, Primitive, Source},
};

/// Text reserves its measured box; a charge marker reserves its enclosed disk.
#[derive(Clone, Copy)]
pub(in crate::pipeline) enum LabelRegion {
  /// Baseline-independent glyph envelope.
  Box([Point; 2]),
  /// Outer circle radius, including half its stroke width.
  Circle { center: Point, radius: f64 },
}

/// Extracts label geometry without treating aromatic bond circles as labels.
/// Charge marks use their owner atom as a counting key only; the original
/// scene provenance is never rewritten or used to classify ring strokes.
pub(in crate::pipeline) fn label_region(primitive: &Primitive) -> Result<Option<(Source, LabelRegion)>, DepictError> {
  match primitive {
    Primitive::Text(text) => Ok(match text.source {
      Some(Source::Charge(atom)) => Some((Source::Atom(atom), LabelRegion::Box(text.envelope()?))),
      Some(source @ (Source::Atom(_) | Source::AttachedHydrogen(_))) => {
        Some((source, LabelRegion::Box(text.envelope()?)))
      }
      _ => None,
    }),
    Primitive::Circle(circle) => Ok(match circle.source {
      Some(Source::Charge(atom)) => Some((
        Source::Atom(atom),
        LabelRegion::Circle {
          center: circle.center,
          radius: circle.radius + circle.width / 2.,
        },
      )),
      _ => None,
    }),
    _ => Ok(None),
  }
}

impl LabelRegion {
  /// Returns a conservative repair proxy, not the exact contact-test shape.
  pub(in crate::pipeline) fn bounds(self) -> [Point; 2] {
    match self {
      Self::Box(bounds) => bounds,
      Self::Circle { center, radius } => [center.translated(-radius, -radius), center.translated(radius, radius)],
    }
  }

  /// Compares label footprints without counting empty circle-box corners.
  pub(super) fn overlaps(self, other: Self) -> bool {
    match (self, other) {
      (Self::Box(bounds), Self::Box(region)) => {
        bounds[0].x < region[1].x && bounds[1].x > region[0].x && bounds[0].y < region[1].y && bounds[1].y > region[0].y
      }
      (Self::Circle { center, radius }, Self::Box(bounds)) | (Self::Box(bounds), Self::Circle { center, radius }) => {
        center.distance(Point::new(
          center.x.clamp(bounds[0].x, bounds[1].x),
          center.y.clamp(bounds[0].y, bounds[1].y),
        )) < radius
      }
      (Self::Circle { center: a, radius: r }, Self::Circle { center: b, radius: s }) => a.distance(b) < r + s,
    }
  }

  /// Tests actual line caps and filled polygon area against a label footprint.
  pub(super) fn touches(self, primitive: &Primitive) -> bool {
    match (self, primitive) {
      (Self::Box(bounds), Primitive::Line(line)) => segment_enters_box(line.start, line.end, bounds, line.width / 2.),
      (Self::Circle { center, radius }, Primitive::Line(line)) => circle_line(center, radius, line),
      (Self::Box(bounds), Primitive::Polygon(polygon)) => {
        let corners = [
          bounds[0],
          Point::new(bounds[1].x, bounds[0].y),
          bounds[1],
          Point::new(bounds[0].x, bounds[1].y),
        ];
        polygon
          .points
          .iter()
          .zip(polygon.points.iter().cycle().skip(1))
          .any(|(&a, &b)| segment_enters_box(a, b, bounds, 0.))
          || corners.iter().any(|&point| inside(point, &polygon.points))
      }
      (Self::Circle { center, radius }, Primitive::Polygon(polygon)) => {
        inside(center, &polygon.points)
          || polygon
            .points
            .iter()
            .zip(polygon.points.iter().cycle().skip(1))
            .any(|(&a, &b)| segment_distance(center, a, b) <= radius)
      }
      _ => false,
    }
  }
}

/// A finite stroke is its oriented rectangle plus any explicitly round caps.
fn circle_line(center: Point, radius: f64, line: &Line) -> bool {
  let half = line.width / 2.;
  if line.caps[0] == LineCap::Round && center.distance(line.start) <= radius + half
    || line.caps[1] == LineCap::Round && center.distance(line.end) <= radius + half
  {
    return true;
  }
  let length = line.start.distance(line.end);
  if length == 0. {
    return false;
  }
  let unit = Point::new(
    (line.end.x - line.start.x) / length,
    (line.end.y - line.start.y) / length,
  );
  let delta = Point::new(center.x - line.start.x, center.y - line.start.y);
  let along = delta.x * unit.x + delta.y * unit.y;
  let across = (delta.x * unit.y - delta.y * unit.x).abs();
  (along - along.clamp(0., length)).hypot((across - half).max(0.)) <= radius
}

/// Stable Euclidean point/segment distance without squaring drawing coordinates.
fn segment_distance(point: Point, a: Point, b: Point) -> f64 {
  let length = a.distance(b);
  if length == 0. {
    return point.distance(a);
  }
  let unit = Point::new((b.x - a.x) / length, (b.y - a.y) / length);
  let along = ((point.x - a.x) * unit.x + (point.y - a.y) * unit.y).clamp(0., length);
  point.distance(a.translated(unit.x * along, unit.y * along))
}

/// Ray parity also catches a label completely enclosed by a filled wedge.
fn inside(point: Point, polygon: &[Point]) -> bool {
  let mut inside = false;
  for (&a, &b) in polygon.iter().zip(polygon.iter().cycle().skip(1)) {
    if (a.y > point.y) != (b.y > point.y) && point.x < a.x + (b.x - a.x) * (point.y - a.y) / (b.y - a.y) {
      inside = !inside;
    }
  }
  inside
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::scene::{Circle, Color};
  use chitin_chem::io::smiles::read_smiles;

  #[test]
  fn repair_bounds_should_include_the_full_charge_outline() -> Result<(), Box<dyn std::error::Error>> {
    let graph = read_smiles("[O-]")?.into_graph();
    let atom = graph.atoms().next().ok_or("atom")?.0;
    let circle = Primitive::Circle(Circle {
      center: Point::new(10., 20.),
      radius: 5.,
      width: 2.,
      color: Color::BLACK,
      source: Some(Source::Charge(atom)),
    });
    let (source, region) = label_region(&circle)?.ok_or("circle label")?;
    assert_eq!(source, Source::Atom(atom));
    assert_eq!(region.bounds(), [Point::new(4., 14.), Point::new(16., 26.)]);
    Ok(())
  }
}
