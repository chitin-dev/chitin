//! One Clap grammar reused by the process CLI and the built-in shell.

use crate::{ChemdrawCommand, ChemdrawInput, ChemdrawStyle, DrawingLength, PortableCommand};
use clap::{Args, Subcommand, ValueEnum};
use std::path::PathBuf;

/// Frontend-neutral chemical drawing command group.
#[derive(Debug, Args)]
#[command(arg_required_else_help = true)]
pub struct ChemdrawArgs {
  /// Selected source workflow; execution resources are supplied by the frontend.
  #[command(subcommand)]
  command: Workflow,
}

/// Source-specific positional arguments with one shared set of export options.
#[derive(Debug, Subcommand)]
enum Workflow {
  /// Draw a SMILES molecule as SVG. Quote the notation; use - to read stdin.
  Smiles {
    /// Literal notation or stdin sentinel; shell quoting preserves stereo slashes.
    #[arg(value_name = "SMILES")]
    smiles: String,
    /// Output and molecular appearance settings shared with diagram input.
    #[command(flatten)]
    options: Options,
  },
  /// Draw an authored reaction diagram from versioned JSON or YAML.
  Diagram {
    /// Versioned diagram source path or frontend-provided stdin sentinel.
    #[arg(value_name = "FILE")]
    input: PathBuf,
    /// Output and molecular appearance settings shared with SMILES input.
    #[command(flatten)]
    options: Options,
  },
}

/// File publication and appearance switches; no application state is captured.
#[derive(Debug, Args)]
struct Options {
  /// Destination SVG file; the parent directory must already exist.
  #[arg(short, long, value_name = "SVG")]
  output: PathBuf,
  /// Explicitly allow replacing an existing output file.
  #[arg(long)]
  force: bool,
  /// Positive drawing units per bond; scales all default depiction dimensions.
  #[arg(long)]
  bond_length: Option<DrawingLength>,
  /// Positive full bond-stroke width, such as 1.6.
  #[arg(long)]
  stroke_width: Option<DrawingLength>,
  /// TrueType/OpenType font file; glyph outlines are embedded in the SVG.
  #[arg(long)]
  font_file: Option<PathBuf>,
  /// Draw labels and bonds in the uniform foreground color.
  #[arg(long)]
  monochrome: bool,
  /// Enclose formal-charge superscripts in vector circles.
  #[arg(long)]
  circled_charges: bool,
  /// Aromatic ring display; unsupported circle cases retain Kekule strokes.
  #[arg(long, value_enum, default_value = "kekule")]
  aromatic_style: Aromatic,
  /// Show declared chiral hydrogens as separate labels and stereobonds.
  #[arg(long)]
  explicit_chiral_hydrogens: bool,
}

/// Textual aromatic display policy, independent of source aromaticity facts.
#[derive(Debug, Clone, Copy, ValueEnum)]
enum Aromatic {
  /// Retain assigned alternating bond strokes.
  Kekule,
  /// Request an inner circle only where the depiction engine permits one.
  Circle,
}

impl ChemdrawArgs {
  /// Converts validated syntax into a replayable portable request.
  ///
  /// # Returns
  /// Caller paths and explicit appearance flags without file I/O or font lookup.
  /// Numeric sizes were validated while parsing; source chemistry and output
  /// permissions remain execution-time checks against the supplied context.
  pub(super) fn into_command(self) -> PortableCommand {
    let (input, options) = match self.command {
      Workflow::Smiles { smiles, options } => (ChemdrawInput::Smiles(smiles), options),
      Workflow::Diagram { input, options } => (ChemdrawInput::Diagram(input), options),
    };
    ChemdrawCommand {
      input,
      output: options.output,
      force: options.force,
      style: ChemdrawStyle {
        circled_charges: options.circled_charges,
        bond_length: options.bond_length,
        stroke_width: options.stroke_width,
        font_file: options.font_file,
        monochrome: options.monochrome,
        aromatic_circles: matches!(options.aromatic_style, Aromatic::Circle),
        explicit_chiral_hydrogens: options.explicit_chiral_hydrogens,
      },
    }
    .into()
  }
}
