//! YAML transport configuration; chemical and rendering defaults stay in their crates.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use serde::Deserialize;

use chitin_chemdraw::{
  depict::{AromaticStyle, ChiralHydrogenMode, DepictOptions},
  svg::SvgOptions,
};

mod colors;
use colors::{Palette, Scheme};

/// Ordered inputs and shared drawing policy for a single batch.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Config {
  /// Destination relative to the configuration file, or an absolute path.
  #[serde(default = "default_output")]
  pub output_dir: PathBuf,
  /// Each source keeps its one-based filename even if an earlier source fails.
  pub smiles: Vec<String>,
  /// Optional visual overrides, never chemical graph edits.
  #[serde(default)]
  pub depict: DepictConfig,
  /// Document framing independent of molecular geometry.
  #[serde(default)]
  pub svg: SvgConfig,
}

/// Default destination intentionally matches the tool's ignored directory.
fn default_output() -> PathBuf {
  PathBuf::from("generated")
}

impl Config {
  /// Rejects empty jobs and resolves the destination without depending on CWD.
  ///
  /// # Parameters
  /// - `path`: path used to load this configuration.
  ///
  /// # Errors
  /// Returns an error for an empty SMILES array or empty output path. Individual
  /// empty/invalid SMILES remain entry failures so later entries can still run.
  pub fn destination(&self, path: &Path) -> Result<PathBuf> {
    ensure!(!self.smiles.is_empty(), "smiles must contain at least one entry");
    ensure!(!self.output_dir.as_os_str().is_empty(), "output_dir must not be empty");
    Ok(path.parent().unwrap_or_else(|| Path::new(".")).join(&self.output_dir))
  }
}

/// Small CLI-specific override surface; absent fields inherit library defaults.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct DepictConfig {
  /// Expand declared H only on annotated tetrahedral carbon centers.
  explicit_chiral_hydrogens: bool,
  /// Label-only color policy; does not color bond strokes.
  color_scheme: Scheme,
  /// Validated exact element symbols mapped to RGB/RGBA values.
  element_colors: Palette,
  /// Inner circles are display annotations, not aromaticity perception.
  aromatic_style: AromaticConfig,
  /// Drawing units per normalized bond unit.
  bond_length: Option<f64>,
  /// Element-label size in drawing units.
  font_size: Option<f64>,
  /// Optional immutable font file, loaded relative to this YAML document.
  font: Option<FontConfig>,
  /// Width of ordinary and hashed strokes.
  stroke_width: Option<f64>,
  /// Full broad-end width of solid stereo wedges.
  wedge_width: Option<f64>,
  /// Separation of multiple-bond strokes.
  bond_spacing: Option<f64>,
  /// Additional clearance around element labels.
  label_padding: Option<f64>,
}

impl DepictConfig {
  /// Resolves visual overrides and loads a font once for the complete batch.
  ///
  /// # Parameters
  /// - `config_path`: YAML location used to resolve relative font paths.
  ///
  /// # Errors
  /// Missing, unreadable or invalid fonts fail before rendering any entry.
  /// Final size validation still belongs to depiction, not YAML transport.
  pub fn options(&self, config_path: &Path) -> Result<DepictOptions> {
    let defaults = DepictOptions::default();
    Ok(DepictOptions {
      font: match &self.font {
        Some(font) => font.load(config_path)?,
        None => defaults.font.clone(),
      },
      color_scheme: self.color_scheme.into(),
      element_colors: self.element_colors.0.clone(),
      aromatic_style: match self.aromatic_style {
        AromaticConfig::Kekule => AromaticStyle::Kekule,
        AromaticConfig::Circle => AromaticStyle::Circle,
      },
      chiral_hydrogens: if self.explicit_chiral_hydrogens {
        ChiralHydrogenMode::Explicit
      } else {
        ChiralHydrogenMode::Implicit
      },
      bond_length: self.bond_length.unwrap_or(defaults.bond_length),
      font_size: self.font_size.unwrap_or(defaults.font_size),
      stroke_width: self.stroke_width.unwrap_or(defaults.stroke_width),
      wedge_width: self.wedge_width.unwrap_or(defaults.wedge_width),
      bond_spacing: self.bond_spacing.unwrap_or(defaults.bond_spacing),
      label_padding: self.label_padding.unwrap_or(defaults.label_padding),
      ..defaults
    })
  }
}

/// Filesystem transport for one selected TrueType/OpenType collection face.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FontConfig {
  /// File path; font-family lookup is deliberately not performed by the core.
  path: PathBuf,
  /// Collection index, or zero for a standalone font file.
  #[serde(default)]
  face_index: u32,
}

impl FontConfig {
  /// Reads bytes at the orchestration boundary, retaining no filesystem handle.
  fn load(&self, config_path: &Path) -> Result<chitin_chemdraw::font::FontFace> {
    ensure!(!self.path.as_os_str().is_empty(), "depict.font.path must not be empty");
    let path = config_path.parent().unwrap_or_else(|| Path::new(".")).join(&self.path);
    let bytes = std::fs::read(&path).with_context(|| format!("read font {}", path.display()))?;
    chitin_chemdraw::font::FontFace::from_bytes(bytes, self.face_index)
      .with_context(|| format!("load font {} face {}", path.display(), self.face_index))
  }
}

/// Strict YAML spellings without adding serialization dependencies to drawing.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum AromaticConfig {
  /// Preserve existing alternating single/double strokes.
  #[default]
  Kekule,
  /// Request circles where topology and geometry permit them.
  Circle,
}

/// SVG page options without coupling serialization to the domain crates.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct SvgConfig {
  /// Additional outer margin; absent means the exporter default.
  padding: Option<f64>,
  /// Omit the opaque white background rectangle.
  transparent: bool,
}

impl SvgConfig {
  /// Builds framing options while retaining the exporter's default background.
  pub fn options(&self) -> SvgOptions {
    let defaults = SvgOptions::default();
    SvgOptions {
      padding: self.padding.unwrap_or(defaults.padding),
      background: if self.transparent { None } else { defaults.background },
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn omitted_options_should_inherit_library_defaults() -> Result<()> {
    let config: Config = serde_yaml::from_str("smiles: ['CCO']")?;
    assert_eq!(config.depict.options(Path::new("job.yaml"))?, DepictOptions::default());
    assert_eq!(config.svg.options(), SvgOptions::default());
    Ok(())
  }

  #[test]
  fn destination_should_resolve_relative_to_configuration() -> Result<()> {
    let config: Config = serde_yaml::from_str("smiles: ['CCO']\noutput_dir: pictures")?;
    assert_eq!(
      config.destination(Path::new("configs/job.yaml"))?,
      Path::new("configs/pictures")
    );
    Ok(())
  }

  #[test]
  fn empty_batch_should_be_rejected() -> Result<()> {
    let config: Config = serde_yaml::from_str("smiles: []")?;
    assert!(config.destination(Path::new("job.yaml")).is_err());
    Ok(())
  }

  #[test]
  fn misspelled_options_should_be_rejected() {
    for yaml in [
      "smiles: ['C']\noutput: pictures",
      "smiles: ['C']\ndepict: {stroke_with: 2}",
      "smiles: ['C']\nsvg: {pad: 2}",
      "smiles: ['C']\ndepict: {color_scheme: rainbow}",
      "smiles: ['C']\ndepict: {aromatic_style: dashed}",
      "smiles: ['C']\ndepict: {element_colors: {oxygen: '#ff0000'}}",
      "smiles: ['C']\ndepict: {element_colors: {O: '#ff00zz'}}",
    ] {
      assert!(serde_yaml::from_str::<Config>(yaml).is_err());
    }
  }

  #[test]
  fn palette_should_resolve_element_keys_and_rgba_during_yaml_parsing() -> Result<()> {
    let config: Config =
      serde_yaml::from_str("smiles: ['CO']\ndepict:\n  color_scheme: element\n  element_colors:\n    O: '#12ab3480'")?;
    let options = config.depict.options(Path::new("job.yaml"))?;
    assert_eq!(options.color_scheme, chitin_chemdraw::depict::ColorScheme::Element);
    assert_eq!(
      options.label_color(chitin_chem::Element::Oxygen),
      chitin_chemdraw::Color {
        red: 0x12,
        green: 0xab,
        blue: 0x34,
        alpha: 128
      }
    );
    Ok(())
  }

  #[test]
  fn aromatic_circle_style_should_be_read_from_yaml() -> Result<()> {
    let config: Config = serde_yaml::from_str("smiles: ['c1ccccc1']\ndepict: {aromatic_style: circle}")?;
    assert_eq!(
      config.depict.options(Path::new("job.yaml"))?.aromatic_style,
      AromaticStyle::Circle
    );
    Ok(())
  }
}
