//! YAML transport configuration; chemical and rendering defaults stay in their crates.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use serde::Deserialize;

use chitin_chemdraw::{
  depict::{AromaticStyle, ChiralHydrogenMode, DepictOptions},
  svg::SvgOptions,
};

mod colors;
use colors::{BondStyle, Palette, Scheme};

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
  /// Explicit entry-local layout declarations; none are inferred from elements.
  #[serde(default)]
  pub layout: LayoutConfig,
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
    let mut entries = std::collections::BTreeSet::new();
    for site in &self.layout.coordination {
      ensure!(
        site.entry > 0 && site.entry <= self.smiles.len(),
        "coordination entry must name an existing one-based SMILES entry"
      );
      ensure!(
        entries.insert(site.entry),
        "only one coordination site per entry is supported"
      );
    }
    Ok(path.parent().unwrap_or_else(|| Path::new(".")).join(&self.output_dir))
  }
}

/// Entry-specific layout input, independent of colors, fonts and SVG framing.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct LayoutConfig {
  /// Entries whose existing center attachments are explicitly declared coordination.
  coordination: Vec<CoordinationConfig>,
}

/// Explicit interpretation of one source center; not a chemical perception rule.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CoordinationConfig {
  /// One-based input entry, matching the generated SVG number.
  entry: usize,
  /// Zero-based atom ordinal in parsed source order, not an opaque handle value.
  center_atom: usize,
  /// Optional drawing-only attachment length, in ordinary bond units.
  attachment_length: Option<f64>,
  /// Optional anchored-backbone search allowance; zero retains ordinary closure.
  routing_budget: Option<usize>,
}

impl LayoutConfig {
  /// Resolves explicitly declared source ordinals only after parsing one entry.
  ///
  /// # Errors
  /// Rejects absent centers, invalid attachment orders and inconsistent topology.
  /// No declaration means ordinary layout, including metal-containing input.
  pub fn site(
    &self,
    entry: usize,
    graph: &chitin_chem::MolecularGraph,
  ) -> Result<
    Option<(
      chitin_chem::coordination::CoordinationSite,
      chitin_chemdraw::layout::coordination::CoordinationOptions,
    )>,
  > {
    let Some(config) = self.coordination.iter().find(|site| site.entry == entry) else {
      return Ok(None);
    };
    let center = graph
      .atoms()
      .nth(config.center_atom)
      .map(|(id, _)| id)
      .context("coordination center_atom is outside the parsed source")?;
    let site =
      chitin_chem::coordination::CoordinationSite::new(graph, center, graph.neighbors(center)?.map(|(_, edge)| edge))?;
    let defaults = chitin_chemdraw::layout::coordination::CoordinationOptions::default();
    Ok(Some((
      site,
      chitin_chemdraw::layout::coordination::CoordinationOptions {
        attachment_length: config.attachment_length.unwrap_or(defaults.attachment_length),
        routing_budget: config.routing_budget.unwrap_or(defaults.routing_budget),
        ..defaults
      },
    )))
  }
}

/// Small CLI-specific override surface; absent fields inherit library defaults.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct DepictConfig {
  /// Explicit true/false retain all/none; omission inherits selective expansion.
  explicit_chiral_hydrogens: Option<bool>,
  /// Element palette shared by labels and endpoint-colored bonds.
  color_scheme: Scheme,
  /// Independent choice between uniform foreground and endpoint-colored halves.
  bond_color_style: BondStyle,
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
      bond_color_style: self.bond_color_style.into(),
      element_colors: self.element_colors.0.clone(),
      aromatic_style: match self.aromatic_style {
        AromaticConfig::Kekule => AromaticStyle::Kekule,
        AromaticConfig::Circle => AromaticStyle::Circle,
      },
      chiral_hydrogens: match self.explicit_chiral_hydrogens {
        Some(true) => ChiralHydrogenMode::Explicit,
        Some(false) => ChiralHydrogenMode::Implicit,
        None => defaults.chiral_hydrogens,
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
  fn coordination_routing_should_be_independently_disabled_by_yaml() -> Result<()> {
    let config: Config = serde_yaml::from_str(
      "smiles: ['O[Fe]O']\nlayout:\n  coordination: [{entry: 1, center_atom: 1, routing_budget: 0}]",
    )?;
    let graph = chitin_chem::io::smiles::read_smiles(&config.smiles[0])?.into_graph();
    let (_, options) = config.layout.site(1, &graph)?.context("missing declaration")?;
    assert_eq!(options.routing_budget, 0);
    assert!(options.work_budget > 0);
    Ok(())
  }

  #[test]
  fn coordination_declarations_should_reject_duplicate_or_nonexistent_entries() -> Result<()> {
    for declarations in [
      "[{entry: 0, center_atom: 1}]",
      "[{entry: 2, center_atom: 1}]",
      "[{entry: 1, center_atom: 1}, {entry: 1, center_atom: 1}]",
    ] {
      let config: Config =
        serde_yaml::from_str(&format!("smiles: ['O[Fe]O']\nlayout:\n  coordination: {declarations}"))?;
      assert!(config.destination(Path::new("test.yaml")).is_err());
    }
    Ok(())
  }

  #[test]
  fn declared_h_overrides_should_preserve_explicit_false_and_true() -> Result<()> {
    for (flag, mode) in [
      (false, ChiralHydrogenMode::Implicit),
      (true, ChiralHydrogenMode::Explicit),
    ] {
      let config: Config =
        serde_yaml::from_str(&format!("smiles: ['C']\ndepict: {{explicit_chiral_hydrogens: {flag}}}"))?;
      assert_eq!(config.depict.options(Path::new("job.yaml"))?.chiral_hydrogens, mode);
    }
    assert_eq!(DepictOptions::default().chiral_hydrogens, ChiralHydrogenMode::Auto);
    Ok(())
  }

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
      "smiles: ['C']\ndepict: {bond_color_style: gradient}",
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
  fn uniform_bonds_should_be_an_independent_yaml_override() -> Result<()> {
    let config: Config = serde_yaml::from_str("smiles: ['CO']\ndepict: {bond_color_style: uniform}")?;
    let options = config.depict.options(Path::new("job.yaml"))?;
    assert_eq!(options.color_scheme, chitin_chemdraw::depict::ColorScheme::Element);
    assert_eq!(
      options.bond_color_style,
      chitin_chemdraw::depict::BondColorStyle::Uniform
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
