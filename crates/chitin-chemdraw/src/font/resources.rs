//! Owned, immutable font bytes and exact host measurement tables.

use super::{FontError, TextMetrics};
use std::{collections::BTreeMap, fmt, sync::Arc};

/// Font bytes and one selected collection face, independent of filesystem paths.
#[derive(Clone, PartialEq, Eq)]
pub struct LoadedFont {
  /// Immutable backing bytes, cheaply shared by every emitted run.
  pub(super) data: Arc<[u8]>,
  /// Face within a collection, or zero for a standalone font.
  pub(super) index: u32,
}

impl fmt::Debug for LoadedFont {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("LoadedFont")
      .field("byte_length", &self.data.len())
      .field("face_index", &self.index)
      .finish()
  }
}

impl LoadedFont {
  /// Validates the selected borrowed view before sharing ownership with scenes.
  pub(super) fn new(data: Arc<[u8]>, index: u32) -> Result<Self, FontError> {
    ttf_parser::Face::parse(&data, index).map_err(|_| FontError::InvalidFont)?;
    Ok(Self { data, index })
  }
}

/// One exact literal run at a requested size, before any placement or clipping.
///
/// Sizes are stored as positive finite bits so equality and deduplication never
/// round two distinct requests into one. The measuring host must preserve this
/// size and use the same resolved font/style when it finally draws the run.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TextRequest {
  /// Literal run content, not markup or a chemical formula language.
  content: String,
  /// Exact positive finite size in drawing units.
  size_bits: u64,
}

impl TextRequest {
  /// Creates an exact measurement key without guessing a font or glyph width.
  ///
  /// # Errors
  /// Rejects empty content, nonfinite size and nonpositive size.
  pub fn new(content: impl Into<String>, font_size: f64) -> Result<Self, FontError> {
    let content = content.into();
    if content.is_empty() || !font_size.is_finite() || font_size <= 0. {
      return Err(FontError::Measurement);
    }
    Ok(Self {
      content,
      size_bits: font_size.to_bits(),
    })
  }
  /// Returns the exact literal text that the host must measure.
  pub fn content(&self) -> &str {
    &self.content
  }
  /// Returns font size in the same units as the host's returned measurements.
  pub fn font_size(&self) -> f64 {
    f64::from_bits(self.size_bits)
  }
}

impl fmt::Display for TextRequest {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{:?} at size {}", self.content, self.font_size())
  }
}

/// Immutable host-resolved identity and exact, validated run measurements.
#[derive(Debug, Clone, PartialEq)]
pub struct MeasuredFont {
  /// Caller-resolved font/style/features identity, not an OS family lookup.
  pub(super) identity: Arc<str>,
  /// Text first permits borrowed lookup without allocating on successful queries.
  measurements: Arc<BTreeMap<String, BTreeMap<u64, TextMetrics>>>,
}

impl MeasuredFont {
  /// Validates a complete immutable table; failed construction publishes nothing.
  pub(super) fn new(
    identity: String,
    entries: impl IntoIterator<Item = (TextRequest, TextMetrics)>,
  ) -> Result<Self, FontError> {
    if identity.trim().is_empty() || identity.chars().any(char::is_control) {
      return Err(FontError::Identity);
    }
    let mut measurements = BTreeMap::<String, BTreeMap<u64, TextMetrics>>::new();
    for (request, metrics) in entries {
      let metrics = metrics.validate()?;
      let sizes = measurements.entry(request.content.clone()).or_default();
      if sizes.insert(request.size_bits, metrics).is_some() {
        return Err(FontError::DuplicateMeasurement(request));
      }
    }
    Ok(Self {
      identity: identity.into(),
      measurements: Arc::new(measurements),
    })
  }

  /// Resolves an exact text/size pair without interpolating or silently falling back.
  pub(super) fn measure(&self, text: &str, size: f64) -> Result<TextMetrics, FontError> {
    self
      .measurements
      .get(text)
      .and_then(|sizes| sizes.get(&size.to_bits()))
      .copied()
      .ok_or_else(|| TextRequest::new(text, size).map_or_else(|error| error, FontError::MissingMeasurement))
  }
}
