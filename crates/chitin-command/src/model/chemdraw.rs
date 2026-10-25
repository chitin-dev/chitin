//! Portable chemical drawing requests; finite size values retain exact equality.

use crate::CommandId;
use std::{path::PathBuf, str::FromStr};

/// Positive finite drawing size, with no NaN or signed-zero equality ambiguity.
///
/// Construction through [`FromStr`] validates the value before typed commands
/// enter history or cross frontend boundaries. Exact bits preserve meaningful
/// command equality without allowing an approximate-comparison policy here.
///
/// # Examples
/// ```
/// use chitin_command::DrawingLength;
/// let size: DrawingLength = "1.25".parse()?;
/// assert_eq!(size.get(), 1.25);
/// assert!("NaN".parse::<DrawingLength>().is_err());
/// # Ok::<(), String>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrawingLength(u64);

impl DrawingLength {
  /// Returns the positive size in drawing units.
  pub fn get(self) -> f64 {
    f64::from_bits(self.0)
  }
}

impl FromStr for DrawingLength {
  /// User-facing size validation error shared by typed and textual callers.
  type Err = String;
  /// Parses a positive finite size without silently clamping an invalid value.
  ///
  /// # Errors
  /// Rejects invalid numbers, infinities, NaN, zero and negative sizes.
  fn from_str(value: &str) -> Result<Self, Self::Err> {
    let value = value
      .parse::<f64>()
      .map_err(|_| "expected a positive finite size".to_owned())?;
    if !value.is_finite() || value <= 0. {
      return Err("expected a positive finite size".into());
    }
    Ok(Self(value.to_bits()))
  }
}

/// Depiction options shared by SMILES and diagram-file commands.
///
/// Absent sizes and false flags preserve the drawing engine's defaults. These
/// options affect molecular appearance, not authored diagram placement units.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChemdrawStyle {
  /// Optional positive drawing units per normalized bond; absent uses 48.
  pub bond_length: Option<DrawingLength>,
  /// Optional full bond-stroke width in drawing units.
  pub stroke_width: Option<DrawingLength>,
  /// Caller-selected font file; exported labels use portable glyph outlines.
  pub font_file: Option<PathBuf>,
  /// Use uniform foreground rather than element colors.
  pub monochrome: bool,
  /// Use inner circles for eligible aromatic rings.
  pub aromatic_circles: bool,
  /// Draw declared chiral hydrogens explicitly.
  pub explicit_chiral_hydrogens: bool,
}

/// Input source for frontend-independent chemical drawing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChemdrawInput {
  /// Literal SMILES, or `-` for frontend-provided UTF-8 standard input.
  Smiles(String),
  /// Versioned JSON/YAML diagram file, or `-` for standard input.
  Diagram(PathBuf),
}

/// A file-producing chemical drawing command; no desktop state is required.
///
/// The command retains caller paths rather than resolving them at parse time.
/// This lets process CLI and shell sessions use their own working directories
/// while sharing execution, overwrite protection and canonical output reports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChemdrawCommand {
  /// Input representation and source.
  pub input: ChemdrawInput,
  /// Destination SVG path, relative to the frontend working directory.
  pub output: PathBuf,
  /// Explicit permission to replace an existing output file.
  pub force: bool,
  /// Shared molecular depiction options.
  pub style: ChemdrawStyle,
}

impl ChemdrawCommand {
  /// Returns the stable workflow identity.
  pub fn id(&self) -> CommandId {
    match self.input {
      ChemdrawInput::Smiles(_) => CommandId::ChemdrawSmiles,
      ChemdrawInput::Diagram(_) => CommandId::ChemdrawDiagram,
    }
  }

  /// Returns whether the frontend must supply standard-input bytes.
  pub fn requires_standard_input(&self) -> bool {
    match &self.input {
      ChemdrawInput::Smiles(value) => value == "-",
      ChemdrawInput::Diagram(path) => path.as_os_str() == "-",
    }
  }
}
