//! Immutable font resources and measurements shared by all scene consumers.
//!
//! Native faces use direct glyph mapping without kerning or complex-script
//! shaping. Hosts may supply their own exact run measurements instead. No font
//! lookup, file I/O, hidden fallback or process-global font registry runs here.

use crate::geometry::Point;
use std::sync::OnceLock;

mod resources;
pub use resources::{LoadedFont, MeasuredFont, TextRequest};

/// Immutable font resource retained by every text run that uses its measurements.
#[derive(Debug, Clone, Default, PartialEq)]
#[non_exhaustive]
pub enum FontFace {
  /// Bundled unmodified Noto Sans Regular, a sans-serif chemical-label face.
  #[default]
  NotoSans,
  /// Caller-owned TrueType/OpenType bytes; clones share immutable storage.
  Loaded(LoadedFont),
  /// Exact host-measured runs, drawn by that host rather than an outline exporter.
  Measured(MeasuredFont),
}

/// Text that cannot be measured or represented by the selected face.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FontError {
  /// The bytes or requested collection face cannot be parsed.
  #[error("invalid font data or face index")]
  InvalidFont,
  /// A measured-font identity is empty or contains control characters.
  #[error("invalid host font identity")]
  Identity,
  /// This exact text/size pair was not supplied by the measuring host.
  #[error("missing host font measurement for {0}")]
  MissingMeasurement(TextRequest),
  /// A request occurs more than once in an immutable measurement table.
  #[error("duplicate host font measurement for {0}")]
  DuplicateMeasurement(TextRequest),
  /// The external measuring host did not provide portable glyph outlines.
  #[error("host-measured text requires its host renderer; no glyph outlines are available")]
  OutlinesUnavailable,
  /// A requested Unicode scalar has no glyph or horizontal advance.
  #[error("font has no supported glyph for {0:?}")]
  MissingGlyph(char),
  /// A visible glyph exists but has no supported vector outline.
  #[error("font has no vector outline for {0:?}")]
  MissingOutline(char),
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

impl TextMetrics {
  /// Checks a finite positive advance and an ordered baseline-relative envelope.
  ///
  /// # Errors
  /// Rejects nonfinite values, inverted bounds, and nonpositive advances before
  /// externally measured data can affect clipping or drawing bounds.
  pub fn validate(self) -> Result<Self, FontError> {
    if !self.advance.is_finite()
      || self.advance <= 0.
      || !self.min.is_finite()
      || !self.max.is_finite()
      || self.min.x > self.max.x
      || self.min.y > self.max.y
      || !(self.max.x - self.min.x).is_finite()
      || !(self.max.y - self.min.y).is_finite()
    {
      return Err(FontError::Measurement);
    }
    Ok(self)
  }
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
  /// Loads immutable face bytes without looking up fonts or reading files.
  ///
  /// # Parameters
  /// - `data`: font bytes whose lifetime is shared by options and output scenes.
  /// - `face_index`: collection face index, or zero for an ordinary font file.
  ///
  /// # Returns
  /// A reproducible face using the same bytes for measurement and outlines.
  /// Variable fonts use their default instance; arbitrary shaping is not claimed.
  ///
  /// # Errors
  /// Rejects invalid data and nonexistent collection faces before publication.
  ///
  /// # Examples
  /// ```
  /// use chitin_chemdraw::font::FontFace;
  /// let bytes = FontFace::NotoSans.data().ok_or("missing bundled font")?;
  /// let selected = FontFace::from_bytes(bytes, 0)?;
  /// assert_eq!(selected.measure("Cl", 18.)?, FontFace::NotoSans.measure("Cl", 18.)?);
  /// # Ok::<(), Box<dyn std::error::Error>>(())
  /// ```
  pub fn from_bytes(data: impl Into<std::sync::Arc<[u8]>>, face_index: u32) -> Result<Self, FontError> {
    Ok(Self::Loaded(LoadedFont::new(data.into(), face_index)?))
  }

  /// Installs exact host measurements for a specific immutable font/style identity.
  ///
  /// # Parameters
  /// - `identity`: host-resolved family/style/features identity, not a font lookup.
  /// - `entries`: exact text/size pairs, with y-down baseline-relative envelopes.
  ///
  /// # Returns
  /// A scene-retained resource suitable for host drawing. Obtain the required
  /// pairs from `depict::measurement_requests`; missing entries never fall back.
  ///
  /// # Errors
  /// Rejects invalid identities, measurements and duplicate keys. This resource
  /// has no outlines: standalone SVG export fails explicitly rather than using
  /// a different font. The host must render the returned text with this identity.
  pub fn from_measurements(
    identity: impl Into<String>,
    entries: impl IntoIterator<Item = (TextRequest, TextMetrics)>,
  ) -> Result<Self, FontError> {
    Ok(Self::Measured(MeasuredFont::new(identity.into(), entries)?))
  }

  /// Returns face bytes, or `None` when text is measured and drawn by a host.
  pub fn data(&self) -> Option<&[u8]> {
    match self {
      Self::NotoSans => Some(include_bytes!("../assets/NotoSans-Regular.ttf")),
      Self::Loaded(font) => Some(&font.data),
      Self::Measured(_) => None,
    }
  }

  /// Returns the selected collection index, absent for an external measuring host.
  pub fn face_index(&self) -> Option<u32> {
    match self {
      Self::NotoSans => Some(0),
      Self::Loaded(font) => Some(font.index),
      Self::Measured(_) => None,
    }
  }

  /// Returns the exact host identity needed to draw externally measured runs.
  pub fn host_identity(&self) -> Option<&str> {
    match self {
      Self::Measured(font) => Some(&font.identity),
      _ => None,
    }
  }

  /// Borrows the static cache or a short-lived view of immutable caller bytes.
  fn with_face<T>(
    &self,
    operation: impl FnOnce(&ttf_parser::Face<'_>) -> Result<T, FontError>,
  ) -> Result<T, FontError> {
    static FACE: OnceLock<Result<ttf_parser::Face<'static>, FontError>> = OnceLock::new();
    match self {
      Self::NotoSans => operation(
        FACE
          .get_or_init(|| {
            ttf_parser::Face::parse(include_bytes!("../assets/NotoSans-Regular.ttf"), 0)
              .map_err(|_| FontError::InvalidFont)
          })
          .as_ref()
          .map_err(Clone::clone)?,
      ),
      Self::Loaded(font) => {
        // Parse a borrowed view on demand instead of leaking bytes or creating
        // a self-referential owner. The parser performs no heap allocation.
        let face = ttf_parser::Face::parse(&font.data, font.index).map_err(|_| FontError::InvalidFont)?;
        operation(&face)
      }
      Self::Measured(_) => Err(FontError::OutlinesUnavailable),
    }
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
  pub fn measure(&self, text: &str, size: f64) -> Result<TextMetrics, FontError> {
    if !size.is_finite() || size <= 0. {
      return Err(FontError::Measurement);
    }
    if let Self::Measured(font) = self {
      return font.measure(text, size);
    }
    self.with_face(|face| {
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
      TextMetrics {
        advance,
        min: Point::new(min_x, min_y),
        max: Point::new(max_x, max_y),
      }
      .validate()
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
  pub(crate) fn outlines(&self, text: &str) -> Result<(Vec<Glyph>, f64), FontError> {
    self.with_face(|face| {
      let mut glyphs = Vec::new();
      let mut offset = 0.;
      for character in text.chars() {
        let id = face.glyph_index(character).ok_or(FontError::MissingGlyph(character))?;
        let mut outline = Outline::default();
        if face.outline_glyph(id, &mut outline).is_none() && !character.is_whitespace() {
          return Err(FontError::MissingOutline(character));
        }
        glyphs.push(Glyph {
          offset,
          commands: outline.0,
        });
        offset += f64::from(face.glyph_hor_advance(id).ok_or(FontError::MissingGlyph(character))?);
      }
      Ok((glyphs, f64::from(face.units_per_em())))
    })
  }
}
