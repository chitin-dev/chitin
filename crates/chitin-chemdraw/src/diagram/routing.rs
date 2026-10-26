//! Bounded deterministic curve search without changing chemical attachments.

use super::{Annotation, ArcPath, Diagram, DiagramError, ElectronArrow, ElectronCurve, Target, arrows, transform};
use crate::{
  geometry::{Bounds, Point},
  scene::{Drawing, Primitive, Source},
};

/// Resource limits and spacing for opt-in electron-arrow routing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoutingOptions {
  /// Additional clearance around obstacles, in diagram drawing units.
  pub margin: f64,
  /// Largest absolute perpendicular control displacement searched for Bezier curves.
  pub max_bend: f64,
  /// Spacing of Bezier bend candidates on either side of the endpoint axis.
  pub bend_step: f64,
  /// Maximum complete candidates; must be in 1..=129.
  pub max_candidates: usize,
  /// Maximum segment/obstacle comparisons, shared across all candidates.
  pub work_budget: usize,
}

impl Default for RoutingOptions {
  fn default() -> Self {
    Self {
      margin: 2.,
      max_bend: 192.,
      bend_step: 12.,
      max_candidates: 65,
      work_budget: 1_000_000,
    }
  }
}

/// Search result. Failure leaves the original diagram unchanged.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoutingReport {
  /// Selected arrow, retaining the authored source, destination, head and paint.
  pub arrow: ElectronArrow,
  /// Whether a complete candidate cleared every obstacle.
  pub clear: bool,
  /// Number of intersecting candidate-segment/obstacle pairs in the best candidate.
  pub collisions: usize,
  /// Number of completely evaluated candidates.
  pub evaluated: usize,
  /// Number of segment/obstacle comparisons performed.
  pub work_used: usize,
  /// Search stopped because of a work or candidate limit.
  pub limited: bool,
}

#[derive(Clone, Copy)]
struct Obstacle {
  shape: Shape,
  start_attachment: bool,
  end_attachment: bool,
  label: bool,
}

#[derive(Clone, Copy)]
enum Shape {
  Rect(Point, Point),
  Segment(Point, Point, f64),
}

const FLATNESS: f64 = 0.2;

impl Diagram {
  /// Searches bend candidates against molecular ink, existing annotations and host boxes.
  ///
  /// Source/destination identities never change. Authored bends are tried first,
  /// then straight and progressively larger bends on both sides. Measured labels,
  /// bond strokes, prior arrows and externally measured labels are obstacles;
  /// highlights are decorative and are excluded. Attachment ink may be touched
  /// only near the original chemical endpoint. A conservative flattened-curve
  /// envelope includes shaft and head width. No complete clear candidate is
  /// claimed when limits stop evaluation. The method does not insert an arrow.
  /// Explicit control offsets are checked as a single fixed candidate; no
  /// alternative path replaces the caller's authored controls.
  /// Arc mode tries the authored signed sweep, its reverse, then sweeps in
  /// 30-degree steps up to a semicircle. Explicit paths remain in their family;
  /// automatic policies additionally try complementary paths in preference order.
  /// Clear, readable candidates outrank clipped slivers. Automatic major paths
  /// cannot exceed 300 degrees before clipping. Every candidate is an exact arc.
  ///
  /// # Errors
  /// Rejects invalid limits, attachments, existing annotations and obstacle boxes.
  pub fn route_electron_arrow(
    &self,
    arrow: ElectronArrow,
    options: &RoutingOptions,
    boxes: &[Bounds],
  ) -> Result<RoutingReport, DiagramError> {
    if !options.margin.is_finite()
      || !(0. ..=1_000_000.).contains(&options.margin)
      || !options.max_bend.is_finite()
      || !(0. ..=1_000_000.).contains(&options.max_bend)
      || !options.bend_step.is_finite()
      || options.bend_step <= 0.
      || !(1..=129).contains(&options.max_candidates)
      || options.work_budget == 0
      || !arrow.bend.is_finite()
      || arrow.controls.is_some_and(|points| {
        points
          .iter()
          .any(|p| !p.is_finite() || p.x.abs() > 1e6 || p.y.abs() > 1e6)
      })
    {
      return Err(DiagramError::Geometry);
    }
    let raw_start = self.resolve(arrow.from)?;
    let raw_end = self.resolve(arrow.to)?;
    if [raw_start.x, raw_start.y, raw_end.x, raw_end.y]
      .iter()
      .any(|x| x.abs() > 1e9)
      || [
        arrow.bend,
        arrow.style.width,
        arrow.style.head_width,
        arrow.style.head_length,
        arrow.style.clearance,
      ]
      .iter()
      .any(|x| x.abs() > 1e6)
    {
      return Err(DiagramError::Geometry);
    }
    // Validate the authored dimensions even if its preferred curve is degenerate.
    arrows::validate(arrow.style)?;
    let mut obstacles = Vec::new();
    for molecule in &self.molecules {
      let drawing = transform::drawing(&molecule.drawing, molecule.placement)?;
      for primitive in drawing.leaf_primitives() {
        let attached = |anchor: super::Anchor, source: Option<Source>| {
          molecule.id == anchor.molecule
            && match (anchor.target, source) {
              (Target::Atom(a), Some(Source::Atom(b))) => a == b,
              (Target::Atom(a), Some(Source::AttachedHydrogen(b))) => a == b,
              (Target::Atom(a), Some(Source::Bond(b))) => {
                molecule.bonds.get(&b).is_some_and(|atoms| atoms.contains(&a))
              }
              (Target::Bond(a), Some(Source::Bond(b))) => a == b,
              _ => false,
            }
        };
        let source = source(primitive);
        collect(
          primitive,
          attached(arrow.from, source),
          attached(arrow.to, source),
          &mut obstacles,
        )?;
      }
    }
    for annotation in &self.annotations {
      let mut primitives = Vec::new();
      let mut start = false;
      let mut end = false;
      match annotation {
        Annotation::Highlight(_) => continue,
        Annotation::ElectronArrow(a) => arrows::electron(self, a, &mut primitives)?,
        Annotation::ReactionArrow(a) => arrows::reaction(a, &mut primitives)?,
        Annotation::LonePair(a) => {
          start = a.anchor == arrow.from;
          end = a.anchor == arrow.to;
          self.lone_pair(a, &mut primitives)?;
        }
        Annotation::ElectronDot(a) => {
          start = a.anchor == arrow.from;
          end = a.anchor == arrow.to;
          self.electron_dot(a, &mut primitives)?;
        }
      }
      let drawing = Drawing::new(primitives)?;
      for p in drawing.leaf_primitives() {
        collect(p, start, end, &mut obstacles)?;
      }
    }
    for bounds in boxes {
      if [bounds.min().x, bounds.min().y, bounds.max().x, bounds.max().y]
        .iter()
        .any(|x| x.abs() > 1e9)
      {
        return Err(DiagramError::Geometry);
      }
      obstacles.push(Obstacle {
        shape: Shape::Rect(bounds.min(), bounds.max()),
        start_attachment: false,
        end_attachment: false,
        label: false,
      });
    }
    if obstacles.len() > 131_072 {
      return Err(DiagramError::Geometry);
    }

    let mut bends = Vec::new();
    if arrow.bend.abs() <= options.max_bend {
      bends.push(arrow.bend);
    }
    if !bends.contains(&0.) {
      bends.push(0.);
    }
    let sign = if arrow.bend < 0. { -1. } else { 1. };
    let mut step = options.bend_step;
    // Limit candidate construction independently of caller-controlled bend ranges.
    while step <= options.max_bend && bends.len() < 129 {
      for bend in [sign * step, -sign * step] {
        if !bends.contains(&bend) && bends.len() < 129 {
          bends.push(bend);
        }
      }
      step += options.bend_step;
    }
    if options.max_bend > 0. {
      for bend in [sign * options.max_bend, -sign * options.max_bend] {
        if !bends.contains(&bend) {
          bends.push(bend);
        }
      }
    }
    // Explicit controls are an authored path, never silently replaced by bends.
    if arrow.controls.is_some() {
      bends = vec![arrow.bend];
    }
    let mut candidates: Vec<_> = bends.into_iter().map(|bend| ElectronArrow { bend, ..arrow }).collect();
    let auto_path = matches!(arrow.curve, ElectronCurve::Arc { path, .. } if path.automatic());
    if let ElectronCurve::Arc { sweep, path } = arrow.curve {
      if arrow.controls.is_some()
        || !sweep.is_finite()
        || !(std::f64::consts::PI / 180. ..=std::f64::consts::PI).contains(&sweep.abs())
      {
        return Err(DiagramError::Geometry);
      }
      let mut angles = vec![sweep, -sweep];
      for degrees in [30_f64, 60., 90., 120., 150., 180.] {
        for angle in [degrees.to_radians(), -degrees.to_radians()] {
          if !angles.contains(&angle) {
            angles.push(angle);
          }
        }
      }
      candidates.clear();
      for &choice in path.candidates() {
        for &sweep in &angles {
          if auto_path && choice == ArcPath::Major && sweep.abs() < std::f64::consts::PI / 3. {
            continue;
          }
          candidates.push(ElectronArrow {
            curve: ElectronCurve::Arc { sweep, path: choice },
            ..arrow
          });
        }
      }
    }
    let total = candidates.len();
    let mut result = RoutingReport {
      arrow,
      clear: false,
      collisions: usize::MAX,
      evaluated: 0,
      work_used: 0,
      limited: false,
    };
    let mut best_readable = false;
    for candidate in candidates.into_iter().take(options.max_candidates) {
      let mut primitives = Vec::new();
      match arrows::electron(self, &candidate, &mut primitives) {
        Ok(()) => {}
        Err(DiagramError::Geometry) => continue,
        Err(error) => return Err(error),
      }
      let drawing = Drawing::new(primitives)?;
      let Some(curve) = drawing
        .leaf_primitives()
        .find(|p| matches!(p, Primitive::CubicBezier(_) | Primitive::CircularArc(_)))
      else {
        return Err(DiagramError::Geometry);
      };
      let readable = match curve {
        Primitive::CircularArc(arc) if auto_path => arrows::readable_arc(arc, arrow.style),
        _ => true,
      };
      let points = match curve {
        Primitive::CubicBezier(c) => {
          let mut points = vec![c.start];
          if !flatten([c.start, c.control1, c.control2, c.end], 0, &mut points) {
            continue;
          }
          points
        }
        Primitive::CircularArc(c) => {
          let Some(points) = arc_points(c) else {
            continue;
          };
          points
        }
        _ => return Err(DiagramError::Geometry),
      };
      let clearance = options.margin + arrow.style.width.max(arrow.style.head_width) / 2. + FLATNESS;
      let start_reach = raw_start.distance(points[0]) + clearance + arrow.style.head_length;
      let end_reach =
        raw_end.distance(*points.last().ok_or(DiagramError::Geometry)?) + clearance + arrow.style.head_length;
      let mut collisions = 0;
      for segment in points.windows(2) {
        for obstacle in &obstacles {
          if result.work_used >= options.work_budget {
            result.limited = true;
            return Ok(result);
          }
          result.work_used += 1;
          let contact = (obstacle.start_attachment && segment.iter().all(|p| p.distance(raw_start) <= start_reach))
            || (obstacle.end_attachment && segment.iter().all(|p| p.distance(raw_end) <= end_reach));
          if contact && !obstacle.label {
            continue;
          }
          // Endpoint clearance may consume the extra routing margin, but never glyph ink.
          let clearance = if contact {
            arrow.style.width / 2. + FLATNESS
          } else {
            clearance
          };
          let hit = match obstacle.shape {
            Shape::Rect(low, high) => rect_hit(segment[0], segment[1], low, high, clearance),
            Shape::Segment(a, b, radius) => segment_distance(segment[0], segment[1], a, b) <= clearance + radius,
          };
          collisions += usize::from(hit);
        }
      }
      // Check the actual arrowhead envelope as well as the shaft corridor.
      for polygon in drawing
        .leaf_primitives()
        .filter_map(|p| if let Primitive::Polygon(p) = p { Some(p) } else { None })
      {
        let Some(bounds) = Bounds::from_points(polygon.points.iter().copied()).map_err(|_| DiagramError::Geometry)?
        else {
          continue;
        };
        for obstacle in &obstacles {
          if result.work_used >= options.work_budget {
            result.limited = true;
            return Ok(result);
          }
          result.work_used += 1;
          let contact = obstacle.end_attachment && polygon.points.iter().all(|p| p.distance(raw_end) <= end_reach);
          if contact && !obstacle.label {
            continue;
          }
          let margin = if contact { 0. } else { options.margin };
          let hit = match obstacle.shape {
            Shape::Rect(low, high) => {
              bounds.min().x - margin <= high.x
                && bounds.max().x + margin >= low.x
                && bounds.min().y - margin <= high.y
                && bounds.max().y + margin >= low.y
            }
            Shape::Segment(a, b, radius) => rect_hit(a, b, bounds.min(), bounds.max(), margin + radius),
          };
          collisions += usize::from(hit);
        }
      }
      result.evaluated += 1;
      if collisions < result.collisions || (collisions == result.collisions && readable && !best_readable) {
        result.arrow = candidate;
        result.collisions = collisions;
        best_readable = readable;
      }
      if collisions == 0 && readable {
        result.clear = true;
        return Ok(result);
      }
    }
    result.limited = total > options.max_candidates;
    result.clear = result.evaluated > 0 && result.collisions == 0;
    Ok(result)
  }
}

fn source(p: &Primitive) -> Option<Source> {
  match p {
    Primitive::CircularArc(p) => p.source,
    Primitive::Line(p) => p.source,
    Primitive::Text(p) => p.source,
    Primitive::Polygon(p) => p.source,
    Primitive::Circle(p) => p.source,
    Primitive::Disk(p) => p.source,
    Primitive::CubicBezier(p) => p.source,
    _ => None,
  }
}

fn collect(p: &Primitive, start: bool, end: bool, out: &mut Vec<Obstacle>) -> Result<(), DiagramError> {
  let scene = Drawing::new(vec![p.clone()])?;
  if let Some(b) = scene.bounds()
    && [b.min().x, b.min().y, b.max().x, b.max().y]
      .iter()
      .any(|x| x.abs() > 1e9)
  {
    return Err(DiagramError::Geometry);
  }
  let mut push = |shape| {
    out.push(Obstacle {
      shape,
      start_attachment: start,
      end_attachment: end,
      // A circled charge is label ink, not an incident bond eligible for the
      // near-endpoint attachment exemption.
      label: matches!(p, Primitive::Text(_))
        || matches!(p, Primitive::Circle(c) if matches!(c.source, Some(Source::Atom(_)))),
    })
  };
  match p {
    Primitive::CircularArc(p) => {
      let points = arc_points(p).ok_or(DiagramError::Geometry)?;
      for s in points.windows(2) {
        push(Shape::Segment(s[0], s[1], p.width / 2. + FLATNESS));
      }
    }
    Primitive::Line(p) => push(Shape::Segment(p.start, p.end, p.width / 2.)),
    Primitive::CubicBezier(p) => {
      let mut points = vec![p.start];
      if !flatten([p.start, p.control1, p.control2, p.end], 0, &mut points) {
        return Err(DiagramError::Geometry);
      }
      for s in points.windows(2) {
        push(Shape::Segment(s[0], s[1], p.width / 2. + FLATNESS));
      }
    }
    _ => {
      let drawing = Drawing::new(vec![p.clone()])?;
      if let Some(bounds) = drawing.bounds() {
        push(Shape::Rect(bounds.min(), bounds.max()));
      }
    }
  }
  Ok(())
}

fn midpoint(a: Point, b: Point) -> Point {
  Point::new(a.x / 2. + b.x / 2., a.y / 2. + b.y / 2.)
}

/// Circular sagitta is at most r*delta^2/8; cap both error and segment length.
fn arc_points(arc: &crate::scene::CircularArc) -> Option<Vec<Point>> {
  let delta = (8. * FLATNESS / arc.radius).sqrt().min(std::f64::consts::PI / 8.);
  let steps = (arc.sweep.abs() / delta)
    .ceil()
    .max((arc.radius * arc.sweep.abs() / 8.).ceil());
  if !steps.is_finite() || !(1. ..=4096.).contains(&steps) {
    return None;
  }
  let steps = steps as usize;
  Some((0..=steps).map(|i| arc.point(i as f64 / steps as f64)).collect())
}

fn flatten(c: [Point; 4], depth: usize, points: &mut Vec<Point>) -> bool {
  if c[0].distance(c[3]) <= 8.
    && point_segment(c[1], c[0], c[3]) <= FLATNESS
    && point_segment(c[2], c[0], c[3]) <= FLATNESS
  {
    points.push(c[3]);
    return true;
  }
  if depth == 12 {
    return false;
  }
  let a = midpoint(c[0], c[1]);
  let b = midpoint(c[1], c[2]);
  let d = midpoint(c[2], c[3]);
  let e = midpoint(a, b);
  let f = midpoint(b, d);
  let g = midpoint(e, f);
  flatten([c[0], a, e, g], depth + 1, points) && flatten([g, f, d, c[3]], depth + 1, points)
}

fn point_segment(p: Point, a: Point, b: Point) -> f64 {
  let dx = b.x - a.x;
  let dy = b.y - a.y;
  let denominator = dx * dx + dy * dy;
  if denominator == 0. {
    return p.distance(a);
  }
  let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / denominator).clamp(0., 1.);
  p.distance(Point::new(a.x + t * dx, a.y + t * dy))
}

fn rect_hit(a: Point, b: Point, low: Point, high: Point, margin: f64) -> bool {
  let mut enter: f64 = 0.;
  let mut exit: f64 = 1.;
  for (origin, delta, low, high) in [
    (a.x, b.x - a.x, low.x - margin, high.x + margin),
    (a.y, b.y - a.y, low.y - margin, high.y + margin),
  ] {
    if delta.abs() < 1e-12 {
      if origin < low || origin > high {
        return false;
      }
    } else {
      let t1 = (low - origin) / delta;
      let t2 = (high - origin) / delta;
      enter = enter.max(t1.min(t2));
      exit = exit.min(t1.max(t2));
      if enter > exit {
        return false;
      }
    }
  }
  true
}

fn segment_distance(a: Point, b: Point, c: Point, d: Point) -> f64 {
  let cross = |a: Point, b: Point, c: Point| (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
  if cross(a, b, c) * cross(a, b, d) <= 0.
    && cross(c, d, a) * cross(c, d, b) <= 0.
    && a.x.min(b.x) <= c.x.max(d.x)
    && c.x.min(d.x) <= a.x.max(b.x)
    && a.y.min(b.y) <= c.y.max(d.y)
    && c.y.min(d.y) <= a.y.max(b.y)
  {
    return 0.;
  }
  point_segment(a, c, d)
    .min(point_segment(b, c, d))
    .min(point_segment(c, a, b))
    .min(point_segment(d, a, b))
}
