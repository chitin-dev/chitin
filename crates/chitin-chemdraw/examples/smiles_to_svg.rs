//! File-writing example; orchestration stays outside layout, scene, and SVG modules.

use chitin_chem::io::smiles::read_smiles;
use chitin_chemdraw::{depict, layout, svg};

/// Runs the explicit graph -> coordinates -> scene -> export pipeline.
///
/// # Errors
/// Propagates parsing, unsupported topology/depiction, and file I/O failures.
fn main() -> Result<(), Box<dyn std::error::Error>> {
  let mut args = std::env::args().skip(1);
  let mut options = depict::DepictOptions::default();
  let source = loop {
    let argument = args.next().ok_or(
      "usage: smiles_to_svg [--explicit-chiral-hydrogens] [--color-scheme monochrome|element] [--aromatic-style kekule|circle] '<SMILES>' <output.svg>",
    )?;
    match argument.as_str() {
      "--explicit-chiral-hydrogens" => options.chiral_hydrogens = depict::ChiralHydrogenMode::Explicit,
      "--color-scheme" => {
        options.color_scheme = match args.next().as_deref() {
          Some("monochrome") => depict::ColorScheme::Monochrome,
          Some("element") => depict::ColorScheme::Element,
          _ => return Err("--color-scheme requires monochrome or element".into()),
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
  let graph = read_smiles(&source)?.into_graph();
  let coordinates = layout::generate(&graph, &layout::LayoutOptions::default())?;
  if !coordinates.diagnostics().is_empty() {
    eprintln!("layout diagnostics: {:?}", coordinates.diagnostics());
  }
  let drawing = depict::depict(&graph, &coordinates, &options)?;
  let output = svg::export(&drawing, &svg::SvgOptions::default())?;
  std::fs::write(path, output)?;
  Ok(())
}
