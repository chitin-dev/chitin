//! File-writing example; orchestration stays outside layout, scene, and SVG modules.

use chitin_chem::io::smiles::read_smiles;
use chitin_chemdraw::{depict, layout, pipeline, svg};

/// Runs the explicit graph -> coordinates -> scene -> export pipeline.
///
/// # Errors
/// Propagates parsing, unsupported topology/depiction, and file I/O failures.
fn main() -> Result<(), Box<dyn std::error::Error>> {
  let mut args = std::env::args().skip(1);
  let mut options = depict::DepictOptions::default();
  let mut font_file = None;
  let mut font_index = 0;
  let source = loop {
    let argument = args.next().ok_or(
      "usage: smiles-to-svg [--font-file path] [--font-index n] [--explicit-chiral-hydrogens] [--color-scheme monochrome|element] [--bond-color-style split|uniform] [--aromatic-style kekule|circle] '<SMILES>' <output.svg>",
    )?;
    match argument.as_str() {
      "--coordination-center" => {
        return Err(
          "the separate coordination backend was removed; automatic layout uses the native fragment engine".into(),
        );
      }
      "--font-file" => {
        font_file = Some(args.next().ok_or("--font-file requires a font path")?);
      }
      "--font-index" => {
        font_index = args
          .next()
          .ok_or("--font-index requires a collection index")?
          .parse::<u32>()?;
      }
      "--explicit-chiral-hydrogens" => options.chiral_hydrogens = depict::ChiralHydrogenMode::Explicit,
      "--color-scheme" => {
        options.color_scheme = match args.next().as_deref() {
          Some("monochrome") => depict::ColorScheme::Monochrome,
          Some("element") => depict::ColorScheme::Element,
          _ => return Err("--color-scheme requires monochrome or element".into()),
        };
      }
      "--bond-color-style" => {
        options.bond_color_style = match args.next().as_deref() {
          Some("split") => depict::BondColorStyle::Split,
          Some("uniform") => depict::BondColorStyle::Uniform,
          _ => return Err("--bond-color-style requires split or uniform".into()),
        };
      }
      "--aromatic-style" => {
        options.aromatic_style = match args.next().as_deref() {
          Some("kekule") => depict::AromaticStyle::Kekule,
          Some("circle") => depict::AromaticStyle::Circle,
          _ => return Err("--aromatic-style requires kekule or circle".into()),
        };
      }
      _ if argument.starts_with("--") => return Err(format!("unknown option: {argument}").into()),
      _ => break argument,
    }
  };
  let path = args.next().ok_or("missing output.svg path")?;
  if args.next().is_some() {
    return Err("too many arguments".into());
  }
  if let Some(path) = font_file {
    options.font = chitin_chemdraw::font::FontFace::from_bytes(std::fs::read(path)?, font_index)?;
  } else if font_index != 0 {
    return Err("--font-index requires --font-file".into());
  }
  let graph = read_smiles(&source)?.into_graph();
  let layout_options = layout::LayoutOptions::default();
  let rendered = pipeline::generate(&graph, &layout_options, &options)?;
  if !rendered.layout.diagnostics().is_empty() {
    eprintln!("layout diagnostics: {:?}", rendered.layout.diagnostics());
  }
  if !rendered.contacts.is_empty() {
    eprintln!("drawing contacts: {:?}", rendered.contacts);
  }
  let output = svg::export(&rendered.drawing, &svg::SvgOptions::default())?;
  std::fs::write(path, output)?;
  Ok(())
}
