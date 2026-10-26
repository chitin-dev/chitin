//! Exact circular electron arrows with endpoint clipping along the same circle.

use super::{direction, head};
use crate::{
  diagram::{Diagram, DiagramError, ElectronArrow, Target},
  geometry::Point,
  scene::{CircularArc, Color, Drawing, Group, Primitive, Source},
};

/// Clips the initial connected contact interval, never replacing the circle by
/// its tangent. Bounded subdivision locates the exit; bisection refines it.
fn trim(diagram: &Diagram, arrow: &ElectronArrow, arc: &CircularArc, reverse: bool) -> Result<f64, DiagramError> {
  let anchor = if reverse { arrow.to } else { arrow.from };
  let raw = diagram.resolve(anchor)?;
  let molecule = diagram.molecule(anchor.molecule)?;
  let mut boxes = Vec::new();
  let mut clearance = arrow.style.clearance;
  for primitive in molecule.drawing.leaf_primitives() {
    match (anchor.target, primitive) {
      (Target::Atom(atom), Primitive::Text(t)) if t.source == Some(Source::Atom(atom)) => {}
      (Target::Atom(atom), Primitive::Circle(c)) if c.source == Some(Source::Atom(atom)) => {}
      (Target::Bond(bond), Primitive::Line(line)) if line.source == Some(Source::Bond(bond)) => {
        clearance = clearance.max(arrow.style.clearance + line.width * molecule.placement.scale / 2.);
        continue;
      }
      _ => continue,
    }
    if let Some(bounds) = Drawing::new(vec![primitive.clone()])?.bounds() {
      let min = molecule.placement.point(bounds.min());
      let max = molecule.placement.point(bounds.max());
      boxes.push((min, max));
    }
  }
  // Treat all runs as one label. Inter-glyph whitespace is not an exit through
  // which an arrow may pass before crossing a hydrogen count or charge.
  let label = boxes.into_iter().reduce(|(a, b), (c, d)| {
    (
      Point::new(a.x.min(c.x), a.y.min(c.y)),
      Point::new(b.x.max(d.x), b.y.max(d.y)),
    )
  });
  let inside = |t: f64| {
    let p = arc.point(if reverse { 1. - t } else { t });
    p.distance(raw) <= clearance
      || label.is_some_and(|(min, max)| {
        p.x >= min.x - clearance && p.x <= max.x + clearance && p.y >= min.y - clearance && p.y <= max.y + clearance
      })
  };
  if !inside(0.) {
    return Ok(0.);
  }
  for step in 1..=128 {
    let mut high = f64::from(step) / 128.;
    if inside(high) {
      continue;
    }
    let mut low = f64::from(step - 1) / 128.;
    for _ in 0..40 {
      let mid = (low + high) / 2.;
      if inside(mid) {
        low = mid;
      } else {
        high = mid;
      }
    }
    return Ok(high);
  }
  Err(DiagramError::Geometry)
}

pub(super) fn electron(
  diagram: &Diagram,
  arrow: &ElectronArrow,
  sweep: f64,
  out: &mut Vec<Primitive>,
) -> Result<(), DiagramError> {
  if arrow.controls.is_some()
    || !sweep.is_finite()
    || !(std::f64::consts::PI / 180. ..=std::f64::consts::PI).contains(&sweep.abs())
  {
    return Err(DiagramError::Geometry);
  }
  let start = diagram.resolve(arrow.from)?;
  let end = diagram.resolve(arrow.to)?;
  let unit = direction(start, end)?;
  let chord = start.distance(end);
  let height = chord / (2. * (sweep / 2.).tan());
  let center = Point::new(
    start.x / 2. + end.x / 2. - unit.y * height,
    start.y / 2. + end.y / 2. + unit.x * height,
  );
  let style = super::ArrowStyle {
    color: Color {
      alpha: 255,
      ..arrow.style.color
    },
    ..arrow.style
  };
  let mut arc = CircularArc {
    center,
    radius: chord / (2. * (sweep / 2.).sin().abs()),
    start_angle: (start.y - center.y).atan2(start.x - center.x),
    sweep,
    width: style.width,
    color: style.color,
    source: None,
  };
  // Validate the untrimmed geometry before evaluating it for label clearance.
  Drawing::new(vec![Primitive::CircularArc(arc.clone())])?;
  let from = trim(diagram, arrow, &arc, false)?;
  let to = trim(diagram, arrow, &arc, true)?;
  if from + to >= 1. {
    return Err(DiagramError::Geometry);
  }
  arc.start_angle += arc.sweep * from;
  arc.sweep *= 1. - from - to;
  if arc.radius * arc.sweep.abs() <= style.head_length || arc.point(0.).distance(arc.point(1.)) <= style.head_length {
    return Err(DiagramError::Geometry);
  }
  let arrowhead = head(arc.point(1.), arc.tangent(1.), arrow.head, style);
  out.push(Primitive::Group(Group {
    drawing: Drawing::new(vec![Primitive::CircularArc(arc), arrowhead])?,
    opacity: arrow.style.color.alpha,
  }));
  Ok(())
}
