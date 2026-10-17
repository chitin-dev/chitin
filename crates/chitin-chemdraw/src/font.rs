//! Bundled font identity and measured glyph advances shared by all scene consumers.
//!
//! Chemical labels need only direct glyph mapping and advances, not a complex-
//! script shaping engine. SVG uses these same outlines, avoiding installed-font
//! substitutions and renderer-dependent `textLength` behavior.

use crate::geometry::Point;
use std::sync::OnceLock;

/// Fixed reproducible face; the original font remains under SIL OFL-1.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FontFace {
  /// Bundled unmodified Noto Sans Regular, a sans-serif chemical-label face.
  NotoSans,
}

/// Text that cannot be measured or represented by the selected face.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FontError {
  /// The bundled face could not be parsed; indicates an asset/build problem.
  #[error("invalid bundled font")]
  InvalidFont,
  /// A requested Unicode scalar has no glyph or horizontal advance.
  #[error("font has no supported glyph for {0:?}")]
  MissingGlyph(char),
  /// Size/advance calculations are nonfinite or nonpositive.
  #[error("invalid text measurement")]
  Measurement,
}

/// Measured baseline-relative envelope and natural horizontal advance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextMetrics {
  /// Natural advance in requested font-size units, without kerning/shaping.
  pub advance: f64,
  /// Minimum x/y, including actual glyph overhangs above the baseline.
  pub min: Point,
  /// Maximum x/y, including advance and glyph descent below the baseline.
  pub max: Point,
}

/// One outline command in unscaled font units, independent of SVG syntax.
pub(crate) enum Command {
  Move(f32, f32),
  Line(f32, f32),
  Quad(f32, f32, f32, f32),
  Curve(f32, f32, f32, f32, f32, f32),
  Close,
}

/// Collector used by the general TrueType parser, with no chemical interpretation.
#[derive(Default)]
struct Outline(Vec<Command>);

impl ttf_parser::OutlineBuilder for Outline {
  fn move_to(&mut self, x: f32, y: f32) {
    self.0.push(Command::Move(x, y));
  }
  fn line_to(&mut self, x: f32, y: f32) {
    self.0.push(Command::Line(x, y));
  }
  fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
    self.0.push(Command::Quad(x1, y1, x, y));
  }
  fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
    self.0.push(Command::Curve(x1, y1, x2, y2, x, y));
  }
  fn close(&mut self) {
    self.0.push(Command::Close);
  }
}

/// One glyph's natural text offset and unscaled outline.
pub(crate) struct Glyph {
  /// Horizontal baseline offset in font units, before target-width scaling.
  pub offset: f64,
  /// Original font-unit curves; backends serialize or rasterize them directly.
  pub commands: Vec<Command>,
}

impl FontFace {
  /// Returns immutable font bytes so non-SVG backends can use the same face.
  pub fn data(self) -> &'static [u8] {
    match self {
      Self::NotoSans => include_bytes!("../assets/NotoSans-Regular.ttf"),
    }
  }

  /// Parses the static asset once; cached data is immutable and frontend-independent.
  fn face(self) -> Result<&'static ttf_parser::Face<'static>, FontError> {
    static FACE: OnceLock<Result<ttf_parser::Face<'static>, FontError>> = OnceLock::new();
    let cache = match self {
      Self::NotoSans => &FACE,
    };
    cache
      .get_or_init(|| ttf_parser::Face::parse(self.data(), 0).map_err(|_| FontError::InvalidFont))
      .as_ref()
      .map_err(|error| *error)
  }

  /// Measures literal glyphs without a platform font lookup or chemical heuristics.
  ///
  /// # Parameters
  /// - `text`: nonempty simple-script label; unsupported scalars are errors.
  /// - `size`: positive finite font size in drawing units.
  ///
  /// # Returns
  /// Natural advance and a baseline-relative y-down envelope. No script shaping
  /// or kerning is claimed; exact glyph advances/overhangs come from this face.
  ///
  /// # Errors
  /// Rejects missing glyphs, an invalid font asset, and nonfinite/zero advances.
  pub fn measure(self, text: &str, size: f64) -> Result<TextMetrics, FontError> {
    if !size.is_finite() || size <= 0. {
      return Err(FontError::Measurement);
    }
    let face = self.face()?;
    let scale = size / f64::from(face.units_per_em());
    let mut advance = 0.;
    let mut min_x = 0_f64;
    let mut max_x = 0_f64;
    let mut min_y = 0_f64;
    let mut max_y = 0_f64;
    for character in text.chars() {
      let glyph = face.glyph_index(character).ok_or(FontError::MissingGlyph(character))?;
      if let Some(bounds) = face.glyph_bounding_box(glyph) {
        min_x = min_x.min(advance + f64::from(bounds.x_min) * scale);
        max_x = max_x.max(advance + f64::from(bounds.x_max) * scale);
        min_y = min_y.min(-f64::from(bounds.y_max) * scale);
        max_y = max_y.max(-f64::from(bounds.y_min) * scale);
      }
      advance += f64::from(
        face
          .glyph_hor_advance(glyph)
          .ok_or(FontError::MissingGlyph(character))?,
      ) * scale;
    }
    max_x = max_x.max(advance);
    if ![advance, min_x, max_x, min_y, max_y].into_iter().all(f64::is_finite) || advance <= 0. {
      return Err(FontError::Measurement);
    }
    Ok(TextMetrics {
      advance,
      min: Point::new(min_x, min_y),
      max: Point::new(max_x, max_y),
    })
  }

  /// Returns matching glyph outlines, offsets, and units-per-em for export.
  ///
  /// # Parameters
  /// - `text`: literal run already checked by scene construction.
  ///
  /// # Returns
  /// Natural font-unit outlines and baseline offsets, without SVG syntax or
  /// frontend-specific coordinates. Backends apply the scene run's scale once.
  ///
  /// # Errors
  /// Missing glyphs or malformed face data fail instead of substituting a glyph.
  pub(crate) fn outlines(self, text: &str) -> Result<(Vec<Glyph>, f64), FontError> {
    let face = self.face()?;
    let mut glyphs = Vec::new();
    let mut offset = 0.;
    for character in text.chars() {
      let id = face.glyph_index(character).ok_or(FontError::MissingGlyph(character))?;
      let mut outline = Outline::default();
      face.outline_glyph(id, &mut outline);
      glyphs.push(Glyph {
        offset,
        commands: outline.0,
      });
      offset += f64::from(face.glyph_hor_advance(id).ok_or(FontError::MissingGlyph(character))?);
    }
    Ok((glyphs, f64::from(face.units_per_em())))
  }
}
