//! SVG serialization of validated scenes; no molecular graph is consulted.

use crate::{
  font::{Command, FontError},
  geometry::{GeometryError, Point},
  scene::{Color, Drawing, Line, LineCap, Primitive, Text},
};

/// Standalone document framing, separate from chemical depiction options.
#[derive(Debug, Clone, PartialEq)]
pub struct SvgOptions {
  /// Nonnegative padding outside the scene envelope, in drawing units.
  pub padding: f64,
  /// Optional full-document background; `None` preserves transparency.
  pub background: Option<Color>,
}

impl Default for SvgOptions {
  /// Uses 12 drawing units of padding and an opaque white document background.
  fn default() -> Self {
    Self {
      padding: 12.,
      background: Some(Color::WHITE),
    }
  }
}

/// Invalid document framing; export never returns a partially formed document.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SvgError {
  /// A scene font/outline could not be represented without substitution.
  #[error(transparent)]
  Font(#[from] FontError),
  /// Padding must be finite and nonnegative.
  #[error("SVG padding must be finite and nonnegative")]
  Padding,
  /// Framing would overflow or have nonpositive dimensions.
  #[error(transparent)]
  Geometry(#[from] GeometryError),
}

/// Escapes literal content for XML text and quoted attributes, never raw markup.
fn escape(text: &str) -> String {
  let mut output = String::new();
  for character in text.chars() {
    match character {
      '&' => output.push_str("&amp;"),
      '<' => output.push_str("&lt;"),
      '>' => output.push_str("&gt;"),
      '"' => output.push_str("&quot;"),
      '\'' => output.push_str("&apos;"),
      _ => output.push(character),
    }
  }
  output
}

/// Formats an sRGB channel triplet; opacity remains a separate SVG attribute.
fn color(value: Color) -> String {
  format!("#{:02x}{:02x}{:02x}", value.red, value.green, value.blue)
}

/// Serializes general font outline commands; no chemical label rules live here.
fn outline(commands: &[Command]) -> String {
  let mut path = String::new();
  for command in commands {
    let segment = match command {
      Command::Move(x, y) => format!("M{x} {y}"),
      Command::Line(x, y) => format!("L{x} {y}"),
      Command::Quad(x1, y1, x, y) => format!("Q{x1} {y1} {x} {y}"),
      Command::Curve(x1, y1, x2, y2, x, y) => format!("C{x1} {y1} {x2} {y2} {x} {y}"),
      Command::Close => "Z".into(),
    };
    path.push_str(&segment);
  }
  path
}

/// Emits reproducible vector glyphs while retaining literal text for accessibility.
///
/// # Returns
/// Outlined glyphs transformed from font y-up units to scene y-down units, with
/// target-width fitting applied once to the complete run. No system fonts or
/// SVG textLength implementation are involved.
///
/// # Errors
/// Propagates face/measurement failures rather than drawing replacement glyphs.
fn text_run(text: &Text) -> Result<String, FontError> {
  let metrics = text.font.measure(&text.content, text.font_size)?;
  let (glyphs, units) = text.font.outlines(&text.content)?;
  let sy = text.font_size / units;
  let sx = sy * (text.width / metrics.advance);
  if !sx.is_finite() || sx <= 0. || !sy.is_finite() || sy <= 0. {
    return Err(FontError::Measurement);
  }
  let mut output = format!(
    "<g aria-label=\"{}\" role=\"img\" fill=\"{}\" fill-opacity=\"{}\" transform=\"translate({} {}) scale({} {})\">\n",
    escape(&text.content),
    color(text.color),
    f64::from(text.color.alpha) / 255.,
    text.origin.x,
    text.origin.y,
    sx,
    -sy
  );
  for glyph in glyphs {
    if !glyph.commands.is_empty() {
      output.push_str(&format!(
        "<path transform=\"translate({} 0)\" d=\"{}\"/>\n",
        glyph.offset,
        outline(&glyph.commands)
      ));
    }
  }
  output.push_str("</g>\n");
  Ok(output)
}

/// Exports one immutable drawing, with no font/chemical state owned by the backend.
///
/// # Parameters
/// - `drawing`: validated y-down primitives and their shared envelopes.
/// - `options`: document framing only, not chemical or coordinate-layout policy.
///
/// # Returns
/// A complete standalone SVG document. Text uses measured outlined glyphs and
/// escaped accessibility labels. Empty scenes get a 1x1 content canvas
/// before padding. Source associations remain in `Drawing`, not encoded as IDs.
///
/// # Errors
/// Rejects invalid padding or overflowing/nonpositive framed extents. SVG export
/// does not repair scene geometry, perform layout, or re-evaluate chemical labels.
pub fn export(drawing: &Drawing, options: &SvgOptions) -> Result<String, SvgError> {
  if !options.padding.is_finite() || options.padding < 0. {
    return Err(SvgError::Padding);
  }
  let (min, width, height) = match drawing.bounds() {
    Some(bounds) => (bounds.min(), bounds.width().max(1.), bounds.height().max(1.)),
    None => (Point::new(0., 0.), 1., 1.),
  };
  let x = min.x - options.padding;
  let y = min.y - options.padding;
  let width = width + 2. * options.padding;
  let height = height + 2. * options.padding;
  if ![x, y, width, height, x + width, y + height]
    .into_iter()
    .all(f64::is_finite)
    || width <= 0.
    || height <= 0.
  {
    return Err(GeometryError::NonFinite.into());
  }
  let mut output = format!(
    "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"{x} {y} {width} {height}\">\n"
  );
  if let Some(background) = options.background {
    output.push_str(&format!(
      "<rect x=\"{x}\" y=\"{y}\" width=\"{width}\" height=\"{height}\" fill=\"{}\" fill-opacity=\"{}\"/>\n",
      color(background),
      f64::from(background.alpha) / 255.
    ));
  }
  scene_run(drawing, &mut output)?;
  output.push_str("</svg>\n");
  Ok(output)
}

/// Recurses only through already validated child scenes, preserving group opacity.
fn scene_run(drawing: &Drawing, output: &mut String) -> Result<(), SvgError> {
  for primitive in drawing.primitives() {
    match primitive {
      Primitive::Group(group) => {
        output.push_str(&format!("<g opacity=\"{}\">\n", f64::from(group.opacity) / 255.));
        scene_run(&group.drawing, output)?;
        output.push_str("</g>\n");
      }
      Primitive::Disk(disk) => output.push_str(&format!(
        "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{}\" fill-opacity=\"{}\"/>\n",
        disk.center.x, disk.center.y, disk.radius, color(disk.color), f64::from(disk.color.alpha) / 255.
      )),
      Primitive::CubicBezier(curve) => output.push_str(&format!(
        "<path d=\"M{} {} C{} {} {} {} {} {}\" fill=\"none\" stroke=\"{}\" stroke-opacity=\"{}\" stroke-width=\"{}\" stroke-linecap=\"butt\"/>\n",
        curve.start.x, curve.start.y, curve.control1.x, curve.control1.y,
        curve.control2.x, curve.control2.y, curve.end.x, curve.end.y,
        color(curve.color), f64::from(curve.color.alpha) / 255., curve.width
      )),
      Primitive::Circle(circle) => output.push_str(&format!(
        "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"none\" stroke=\"{}\" stroke-opacity=\"{}\" stroke-width=\"{}\"/>\n",
        circle.center.x, circle.center.y, circle.radius, color(circle.color), f64::from(circle.color.alpha) / 255., circle.width
      )),
      Primitive::Polygon(polygon) => output.push_str(&format!(
        "<polygon points=\"{}\" fill=\"{}\" fill-opacity=\"{}\"/>\n",
        polygon.points.iter().map(|point| format!("{},{}", point.x, point.y)).collect::<Vec<_>>().join(" "),
        color(polygon.color), f64::from(polygon.color.alpha) / 255.
      )),
      Primitive::Line(line) => output.push_str(&line_run(line)),
      Primitive::Text(text) => output.push_str(&text_run(text)?),
    }
  }
  Ok(())
}

/// Serializes asymmetric caps as one filled outline, avoiding alpha overdraw.
///
/// Ordinary strokes keep the compact SVG line representation. A split stroke
/// needs a round exterior and flat interior; independent circles would composite
/// translucent paint twice where they overlap the stroke rectangle.
fn line_run(line: &Line) -> String {
  if line.caps[0] == line.caps[1] || line.start == line.end {
    let cap = if line.caps[0] == LineCap::Round {
      "round"
    } else {
      "butt"
    };
    return format!(
      "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" fill=\"none\" stroke=\"{}\" stroke-opacity=\"{}\" stroke-width=\"{}\" stroke-linecap=\"{cap}\"/>\n",
      line.start.x,
      line.start.y,
      line.end.x,
      line.end.y,
      color(line.color),
      f64::from(line.color.alpha) / 255.,
      line.width
    );
  }
  let radius = line.width / 2.;
  let length = line.start.distance(line.end);
  let normal = Point::new(
    -(line.end.y - line.start.y) / length * radius,
    (line.end.x - line.start.x) / length * radius,
  );
  let a = line.start.translated(normal.x, normal.y);
  let b = line.end.translated(normal.x, normal.y);
  let c = line.end.translated(-normal.x, -normal.y);
  let d = line.start.translated(-normal.x, -normal.y);
  let mut path = format!("M{} {}L{} {}", a.x, a.y, b.x, b.y);
  for (index, (cap, point)) in [(line.caps[1], c), (line.caps[0], a)].into_iter().enumerate() {
    if index == 1 {
      path.push_str(&format!("L{} {}", d.x, d.y));
    }
    match cap {
      LineCap::Round => path.push_str(&format!("A{radius} {radius} 0 0 0 {} {}", point.x, point.y)),
      LineCap::Butt => path.push_str(&format!("L{} {}", point.x, point.y)),
    }
  }
  format!(
    "<path d=\"{path}Z\" fill=\"{}\" fill-opacity=\"{}\"/>\n",
    color(line.color),
    f64::from(line.color.alpha) / 255.
  )
}
