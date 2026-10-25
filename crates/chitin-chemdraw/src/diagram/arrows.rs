//! Authored arrow geometry and measured endpoint clearance, without route inference.

use super::{
  Anchor, ArrowStyle, Diagram, DiagramError, ElectronArrow, ElectronHead, ReactionArrow, ReactionArrowKind, Target,
};
use crate::{
  geometry::Point,
  scene::{Color, CubicBezier, Drawing, DrawingError, Group, Line, LineCap, Polygon, Primitive, Source, positive},
};

fn validate(style: ArrowStyle) -> Result<(), DiagramError> {
  if ![style.width, style.head_length, style.head_width]
    .into_iter()
    .all(positive)
    || !style.clearance.is_finite()
    || style.clearance < 0.
  {
    return Err(DiagramError::Geometry);
  }
  Ok(())
}

fn direction(from: Point, to: Point) -> Result<Point, DiagramError> {
  let length = from.distance(to);
  if !positive(length) {
    return Err(DiagramError::Geometry);
  }
  Ok(Point::new((to.x - from.x) / length, (to.y - from.y) / length))
}

fn head(tip: Point, tangent: Point, kind: ElectronHead, style: ArrowStyle) -> Primitive {
  let base = tip.translated(-tangent.x * style.head_length, -tangent.y * style.head_length);
  let normal = Point::new(-tangent.y * style.head_width / 2., tangent.x * style.head_width / 2.);
  let left = base.translated(-normal.x, -normal.y);
  let right = base.translated(normal.x, normal.y);
  let points = match kind {
    ElectronHead::Pair => vec![tip, left, right],
    ElectronHead::FishhookLeft => vec![tip, left, base],
    ElectronHead::FishhookRight => vec![tip, base, right],
  };
  Primitive::Polygon(Polygon {
    points,
    color: style.color,
    source: None,
  })
}

/// Resolves a ray against all label fragments of the selected atom, including H,
/// isotope and charge runs. Explicit offsets outside the label remain usable.
fn endpoint(diagram: &Diagram, anchor: Anchor, toward: Point, style: ArrowStyle) -> Result<Point, DiagramError> {
  let point = diagram.resolve(anchor)?;
  let unit = direction(point, toward)?;
  let molecule = diagram.molecule(anchor.molecule)?;
  let mut distance = style.clearance;
  match anchor.target {
    Target::Atom(id) => {
      for primitive in molecule.drawing.primitives() {
        let Primitive::Text(text) = primitive else {
          continue;
        };
        if text.source != Some(Source::Atom(id)) {
          continue;
        }
        let [min, max] = text
          .envelope()
          .map_err(DrawingError::from)?
          .map(|p| molecule.placement.point(p));
        if let Some(exit) = ray_exit(point, unit, min, max, style.clearance) {
          distance = distance.max(exit);
        }
      }
    }
    Target::Bond(id) => {
      let width = molecule
        .drawing
        .primitives()
        .iter()
        .filter_map(|p| match p {
          Primitive::Line(line) if line.source == Some(Source::Bond(id)) => Some(line.width),
          _ => None,
        })
        .fold(0_f64, f64::max)
        * molecule.placement.scale;
      distance += width / 2.;
    }
  }
  if !distance.is_finite() || distance >= point.distance(toward) {
    return Err(DiagramError::Geometry);
  }
  Ok(point.translated(unit.x * distance, unit.y * distance))
}

/// Slab intersection returns the far boundary, not the near edge of a label.
fn ray_exit(origin: Point, unit: Point, min: Point, max: Point, margin: f64) -> Option<f64> {
  let mut enter = 0_f64;
  let mut exit = f64::INFINITY;
  for (o, u, low, high) in [
    (origin.x, unit.x, min.x - margin, max.x + margin),
    (origin.y, unit.y, min.y - margin, max.y + margin),
  ] {
    if u.abs() < 1e-12 {
      if o < low || o > high {
        return None;
      }
    } else {
      let a = (low - o) / u;
      let b = (high - o) / u;
      enter = enter.max(a.min(b));
      exit = exit.min(a.max(b));
    }
  }
  (exit >= enter).then_some(exit)
}

pub(super) fn electron(diagram: &Diagram, arrow: &ElectronArrow, out: &mut Vec<Primitive>) -> Result<(), DiagramError> {
  validate(arrow.style)?;
  if !arrow.bend.is_finite() {
    return Err(DiagramError::Geometry);
  }
  let raw_start = diagram.resolve(arrow.from)?;
  let raw_end = diagram.resolve(arrow.to)?;
  let unit = direction(raw_start, raw_end)?;
  let dx = raw_end.x - raw_start.x;
  let dy = raw_end.y - raw_start.y;
  let control1 = raw_start.translated(dx / 3. - unit.y * arrow.bend, dy / 3. + unit.x * arrow.bend);
  let control2 = raw_start.translated(2. * dx / 3. - unit.y * arrow.bend, 2. * dy / 3. + unit.x * arrow.bend);
  if !control1.is_finite() || !control2.is_finite() {
    return Err(DiagramError::Geometry);
  }
  let start = endpoint(diagram, arrow.from, control1, arrow.style)?;
  let end = endpoint(diagram, arrow.to, control2, arrow.style)?;
  if end.distance(control2) <= arrow.style.head_length || start.distance(end) <= arrow.style.head_length {
    return Err(DiagramError::Geometry);
  }
  let tangent = direction(control2, end)?;
  let style = ArrowStyle {
    color: Color {
      alpha: 255,
      ..arrow.style.color
    },
    ..arrow.style
  };
  // Isolate paint so a translucent shaft and its head do not darken their seam.
  let pieces = vec![
    Primitive::CubicBezier(CubicBezier {
      start,
      control1,
      control2,
      end,
      width: style.width,
      color: style.color,
      source: None,
    }),
    head(end, tangent, arrow.head, style),
  ];
  out.push(Primitive::Group(Group {
    drawing: Drawing::new(pieces)?,
    opacity: arrow.style.color.alpha,
  }));
  Ok(())
}

pub(super) fn reaction(arrow: &ReactionArrow, out: &mut Vec<Primitive>) -> Result<(), DiagramError> {
  validate(arrow.style)?;
  if !arrow.start.is_finite() || !arrow.end.is_finite() {
    return Err(DiagramError::Geometry);
  }
  let unit = direction(arrow.start, arrow.end)?;
  if arrow.start.distance(arrow.end) <= 2. * arrow.style.head_length {
    return Err(DiagramError::Geometry);
  }
  let style = ArrowStyle {
    color: Color {
      alpha: 255,
      ..arrow.style.color
    },
    ..arrow.style
  };
  let mut pieces = Vec::new();
  let mut shaft = |a: Point, b: Point, kind| -> Result<(), DiagramError> {
    pieces.push(Primitive::Line(Line {
      start: a,
      end: b,
      width: style.width,
      color: style.color,
      caps: [LineCap::Butt; 2],
      source: None,
    }));
    pieces.push(head(b, direction(a, b)?, kind, style));
    Ok(())
  };
  match arrow.kind {
    ReactionArrowKind::Forward => shaft(arrow.start, arrow.end, ElectronHead::Pair)?,
    ReactionArrowKind::Equilibrium => {
      let offset = style.head_width / 2. + style.width;
      let shift = |p: Point, sign: f64| p.translated(-unit.y * offset * sign, unit.x * offset * sign);
      shaft(
        shift(arrow.start, -1.),
        shift(arrow.end, -1.),
        ElectronHead::FishhookLeft,
      )?;
      shaft(shift(arrow.end, 1.), shift(arrow.start, 1.), ElectronHead::FishhookLeft)?;
    }
  }
  out.push(Primitive::Group(Group {
    drawing: Drawing::new(pieces)?,
    opacity: arrow.style.color.alpha,
  }));
  Ok(())
}
