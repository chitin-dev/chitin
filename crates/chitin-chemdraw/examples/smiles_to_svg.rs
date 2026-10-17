//! File-writing example; orchestration stays outside layout, scene, and SVG modules.

use chitin_chem::io::smiles::read_smiles;
use chitin_chemdraw::{depict, layout, svg};

/// Runs the explicit graph -> coordinates -> scene -> export pipeline.
///
/// # Errors
/// Propagates parsing, unsupported topology/depiction, and file I/O failures.
fn main() -> Result<(), Box<dyn std::error::Error>> {
  let mut args = std::env::args().skip(1);
  let source = args.next().ok_or("usage: smiles_to_svg '<SMILES>' <output.svg>")?;
  let path = args.next().ok_or("missing output.svg path")?;
  if args.next().is_some() {
    return Err("too many arguments".into());
  }
  let graph = read_smiles(&source)?.into_graph();
  let coordinates = layout::generate(&graph, &layout::LayoutOptions::default())?;
  if !coordinates.diagnostics().is_empty() {
    eprintln!("layout diagnostics: {:?}", coordinates.diagnostics());
  }
  let drawing = depict::depict(&graph, &coordinates, &depict::DepictOptions::default())?;
  let output = svg::export(&drawing, &svg::SvgOptions::default())?;
  std::fs::write(path, output)?;
  Ok(())
}
