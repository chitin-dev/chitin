//! Generic drawing primitives and provenance, independent of molecular layout.
//!
//! Scene construction validates numeric/text boundaries once. Exporters receive
//! immutable primitives and cannot silently repair chemistry or move atoms.

use crate::font::{FontError, FontFace};
use crate::geometry::{Bounds, GeometryError, Point};
use chitin_chem::{AtomId, BondId};

mod arc;
pub use arc::CircularArc;

/// Straight-alpha sRGB color, with no frontend-owned color type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
  /// Red channel.
  pub red: u8,
  /// Green channel.
  pub green: u8,
  /// Blue channel.
  pub blue: u8,
  /// Opacity: zero is transparent, 255 is opaque.
  pub alpha: u8,
}

impl Color {
  /// Opaque black, the default monochrome foreground.
  pub const BLACK: Self = Self {
    red: 0,
    green: 0,
    blue: 0,
    alpha: 255,
  };
  /// Opaque white, suitable for a standalone document background.
  pub const WHITE: Self = Self {
    red: 255,
    green: 255,
    blue: 255,
    alpha: 255,
  };
}

/// Optional link to chemical input; handles remain scoped to that input graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Source {
  /// A depiction-only attached H and its bond, identified by its host atom.
  /// No atom/bond handle is allocated and no source graph vertex is implied.
  AttachedHydrogen(AtomId),
  /// An atom label or annotation.
  Atom(AtomId),
  /// A bond stroke; multiple strokes may share this source.
  Bond(BondId),
}

/// Endpoint shape for a scene stroke, independent of chemical bond order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineCap {
  /// Semicircle extending half the stroke width beyond the endpoint.
  Round,
  /// Flat endpoint; neighboring colored halves meet without overlap.
  Butt,
}

/// One line in y-down drawing units with independently specified endpoint caps.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
  /// First endpoint, after label clipping if produced by chemical depiction.
  pub start: Point,
  /// Second endpoint.
  pub end: Point,
  /// Positive full stroke width, not half-width.
  pub width: f64,
  /// Stroke color.
  pub color: Color,
  /// Start/end caps; depiction uses round outer ends and flat color seams.
  pub caps: [LineCap; 2],
  /// Chemical origin, absent for generic decorations.
  pub source: Option<Source>,
}

/// An unfilled stroked circle, independent of any chemical ring interpretation.
#[derive(Debug, Clone, PartialEq)]
pub struct Circle {
  /// Center in y-down drawing units.
  pub center: Point,
  /// Positive radius to the stroke's centerline.
  pub radius: f64,
  /// Positive full stroke width.
  pub width: f64,
  /// Stroke color, including alpha.
  pub color: Color,
  /// Optional input locator. Aromatic annotations use their smallest atom handle,
  /// not a fabricated ring identity or attribution to a single ring bond.
  pub source: Option<Source>,
}

/// A filled closed polygon in y-down drawing units, with no implicit stroke.
#[derive(Debug, Clone, PartialEq)]
pub struct Polygon {
  /// At least three finite vertices in boundary order; closure is implicit.
  pub points: Vec<Point>,
  /// Fill color with straight alpha.
  pub color: Color,
  /// Chemical source, absent for generic decorations.
  pub source: Option<Source>,
}

/// A filled disk, useful for atom highlights and authored electron dots.
#[derive(Debug, Clone, PartialEq)]
pub struct Disk {
  /// Center in drawing coordinates.
  pub center: Point,
  /// Positive radius of the filled region.
  pub radius: f64,
  /// Fill color, including alpha.
  pub color: Color,
  /// Optional chemical origin.
  pub source: Option<Source>,
}

/// One stroked cubic Bézier curve, independent of arrow or chemical semantics.
#[derive(Debug, Clone, PartialEq)]
pub struct CubicBezier {
  /// Curve origin.
  pub start: Point,
  /// Departure control point.
  pub control1: Point,
  /// Arrival control point.
  pub control2: Point,
  /// Curve destination.
  pub end: Point,
  /// Positive stroke width; flat caps do not protrude beyond an attached arrowhead.
  pub width: f64,
  /// Stroke color, including alpha.
  pub color: Color,
  /// Optional chemical origin.
  pub source: Option<Source>,
}

/// An isolated compositing group; opacity is applied once to the painted union.
///
/// Opaque, same-colored highlight pieces therefore do not darken where they meet.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
  /// Validated child scene with its own painter order.
  pub drawing: Drawing,
  /// Group opacity, from transparent zero to opaque 255.
  pub opacity: u8,
}

/// One positioned text run with an explicit horizontal advance.
///
/// Font identity and advances belong to the scene, not an export backend's
/// installed fonts. Optional target-width fitting uses the same measured glyphs.
/// Host-measured runs must be drawn by the host identified by `font`; outline
/// exporters refuse them instead of substituting a different font.
#[derive(Debug, Clone, PartialEq)]
pub struct Text {
  /// Reproducible font identity, shared by metrics and all export consumers.
  pub font: FontFace,
  /// Left baseline point in y-down drawing coordinates.
  pub origin: Point,
  /// Literal Unicode text, not SVG markup.
  pub content: String,
  /// Positive font size.
  pub font_size: f64,
  /// Positive target advance used by drawing bounds and export.
  pub width: f64,
  /// Glyph color.
  pub color: Color,
  /// Chemical origin for all glyphs in this run.
  pub source: Option<Source>,
}

impl Text {
  /// Returns a measured envelope, including target-width scaling and glyph overhangs.
  ///
  /// # Errors
  /// Propagates unsupported glyphs and invalid natural metrics. `Drawing` also
  /// validates the raw target width and derived coordinate finiteness.
  pub fn envelope(&self) -> Result<[Point; 2], FontError> {
    let metrics = self.font.measure(&self.content, self.font_size)?;
    let scale = self.width / metrics.advance;
    Ok([
      self.origin.translated(metrics.min.x * scale, metrics.min.y),
      self.origin.translated(metrics.max.x * scale, metrics.max.y),
    ])
  }
}

/// Frontend-independent scene commands; future path/image variants may extend it.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Primitive {
  /// An exact constant-radius circular stroke, not a cubic approximation.
  CircularArc(CircularArc),
  /// An isolated child scene with shared opacity.
  Group(Group),
  /// A filled circular region.
  Disk(Disk),
  /// A curved stroke with explicit control points.
  CubicBezier(CubicBezier),
  /// An unfilled stroked circular boundary.
  Circle(Circle),
  /// A filled closed boundary, such as a solid stereochemical wedge.
  Polygon(Polygon),
  /// A stroked segment.
  Line(Line),
  /// A baseline-positioned glyph run.
  Text(Text),
}

/// Rejected scene input; no partial drawing is returned.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DrawingError {
  /// The selected font cannot faithfully represent a requested run.
  #[error(transparent)]
  Font(#[from] FontError),
  /// A geometry coordinate or aggregate bound is not finite.
  #[error(transparent)]
  Geometry(#[from] GeometryError),
  /// Stroke widths, radii and text sizes/advances must be finite and positive.
  #[error("invalid drawing dimensions at primitive {0}")]
  Dimensions(usize),
  /// Empty text or XML-forbidden characters cannot be exported consistently.
  #[error("invalid text at primitive {0}")]
  Text(usize),
}

/// Validated immutable primitives and their ink/layout envelopes.
#[derive(Debug, Clone, PartialEq)]
pub struct Drawing {
  /// Validated commands in painter's order; immutable access protects bounds.
  primitives: Vec<Primitive>,
  /// Union of stroke and text envelopes; empty scenes deliberately have no box.
  bounds: Option<Bounds>,
}

impl Drawing {
  /// Validates scene commands and derives bounds including stroke widths.
  ///
  /// # Parameters
  /// - `primitives`: literal drawing commands in the caller's painter order.
  ///
  /// # Returns
  /// An immutable, exportable scene without chemical interpretation or coordinate
  /// changes. Text bounds use the scene's actual face, including glyph overhangs.
  ///
  /// # Errors
  /// Rejects nonfinite/overflowing geometry, invalid dimensions, or text that
  /// cannot be represented in XML 1.0. An empty scene has no bounds, not a fake box.
  pub fn new(primitives: Vec<Primitive>) -> Result<Self, DrawingError> {
    let mut points = Vec::new();
    for (index, primitive) in primitives.iter().enumerate() {
      match primitive {
        Primitive::CircularArc(arc) => {
          if !positive(arc.radius)
            || !positive(arc.width)
            || !arc.start_angle.is_finite()
            || !arc.sweep.is_finite()
            || arc.sweep == 0.
            || arc.sweep.abs() >= std::f64::consts::TAU
          {
            return Err(DrawingError::Dimensions(index));
          }
          let mut extrema = vec![arc.point(0.), arc.point(1.)];
          for angle in [
            0.,
            std::f64::consts::FRAC_PI_2,
            std::f64::consts::PI,
            3. * std::f64::consts::FRAC_PI_2,
          ] {
            if arc.contains_angle(angle) {
              extrema.push(
                arc
                  .center
                  .translated(arc.radius * angle.cos(), arc.radius * angle.sin()),
              );
            }
          }
          for point in extrema {
            points.push(point.translated(-arc.width / 2., -arc.width / 2.));
            points.push(point.translated(arc.width / 2., arc.width / 2.));
          }
        }
        Primitive::Group(group) => {
          if let Some(bounds) = group.drawing.bounds() {
            points.extend([bounds.min(), bounds.max()]);
          }
        }
        Primitive::Disk(disk) => {
          if !positive(disk.radius) {
            return Err(DrawingError::Dimensions(index));
          }
          points.push(disk.center.translated(-disk.radius, -disk.radius));
          points.push(disk.center.translated(disk.radius, disk.radius));
        }
        Primitive::CubicBezier(curve) => {
          if !positive(curve.width) {
            return Err(DrawingError::Dimensions(index));
          }
          // A cubic lies inside its control hull; this conservative envelope also
          // validates every control point without numerically fragile extrema.
          let radius = curve.width / 2.;
          for point in [curve.start, curve.control1, curve.control2, curve.end] {
            points.push(point.translated(-radius, -radius));
            points.push(point.translated(radius, radius));
          }
        }
        Primitive::Circle(circle) => {
          if !positive(circle.radius) || !positive(circle.width) {
            return Err(DrawingError::Dimensions(index));
          }
          let outer = circle.radius + circle.width / 2.;
          points.push(circle.center.translated(-outer, -outer));
          points.push(circle.center.translated(outer, outer));
        }
        Primitive::Polygon(polygon) => {
          if polygon.points.len() < 3 {
            return Err(DrawingError::Dimensions(index));
          }
          points.extend(polygon.points.iter().copied());
        }
        Primitive::Line(line) => {
          if !positive(line.width) {
            return Err(DrawingError::Dimensions(index));
          }
          let radius = line.width / 2.;
          for point in [line.start, line.end] {
            points.push(point.translated(-radius, -radius));
            points.push(point.translated(radius, radius));
          }
        }
        Primitive::Text(text) => {
          if !positive(text.font_size) || !positive(text.width) {
            return Err(DrawingError::Dimensions(index));
          }
          if text.content.is_empty() || !text.content.chars().all(xml_character) {
            return Err(DrawingError::Text(index));
          }
          points.extend(text.envelope()?);
        }
      }
    }
    let bounds = Bounds::from_points(points)?;
    Ok(Self { primitives, bounds })
  }

  /// Returns immutable commands in painter's order.
  pub fn primitives(&self) -> &[Primitive] {
    &self.primitives
  }

  /// Visits non-group commands in painter order, retaining their source handles.
  ///
  /// This traversal is for inspection, not painting: consumers needing group
  /// opacity must preserve the hierarchy returned by [`Self::primitives`].
  pub fn leaf_primitives(&self) -> impl Iterator<Item = &Primitive> {
    let mut stack = vec![self.primitives.iter()];
    std::iter::from_fn(move || {
      loop {
        let frame = stack.last_mut()?;
        match frame.next() {
          Some(Primitive::Group(group)) => stack.push(group.drawing.primitives.iter()),
          Some(primitive) => return Some(primitive),
          None => {
            stack.pop();
          }
        }
      }
    })
  }
  /// Returns bounds, or `None` for an empty scene.
  pub const fn bounds(&self) -> Option<Bounds> {
    self.bounds
  }
}

/// Tests an extent before it participates in geometry calculations.
pub(crate) fn positive(value: f64) -> bool {
  value.is_finite() && value > 0.
}

/// XML 1.0 permits these scalar ranges; escaping cannot repair forbidden controls.
fn xml_character(value: char) -> bool {
  matches!(value, '\t' | '\n' | '\r' | '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}')
}
