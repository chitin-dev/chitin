//! File-writing example; orchestration stays outside layout, scene, and SVG modules.

use chitin_chem::io::smiles::read_smiles;
use chitin_chemdraw::{depict, layout, svg};

/// Runs the explicit graph -> coordinates -> scene -> export pipeline.
///
/// # Errors
/// Propagates parsing, unsupported topology/depiction, and file I/O failures.
fn main() -> Result<(), Box<dyn std::error::Error>> {
  let mut args = std::env::args().skip(1);
  let first = args
    .next()
    .ok_or("usage: smiles_to_svg [--explicit-chiral-hydrogens] '<SMILES>' <output.svg>")?;
  let mut options = depict::DepictOptions::default();
  let source = if first == "--explicit-chiral-hydrogens" {
    options.chiral_hydrogens = depict::ChiralHydrogenMode::Explicit;
    args.next().ok_or("missing SMILES after --explicit-chiral-hydrogens")?
  } else if first.starts_with("--") {
    return Err(format!("unknown option: {first}").into());
  } else {
    first
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
