//! Renders fixed layout cases for visual inspection without a GUI dependency.

#[path = "../tests/support/layout_cases.rs"]
mod layout_cases;

use chitin_chem::io::smiles::read_smiles;
use chitin_chemdraw::{depict, layout, svg};

/// Exports the shared regression corpus to an explicitly supplied directory.
///
/// # Errors
/// Reports malformed fixtures, mismatched topology, unresolved layout quality,
/// unsupported depiction, and filesystem errors instead of omitting a case.
fn main() -> Result<(), Box<dyn std::error::Error>> {
  let mut args = std::env::args().skip(1);
  let directory = std::path::PathBuf::from(args.next().ok_or("usage: layout_regressions <output-directory>")?);
  if args.next().is_some() {
    return Err("too many arguments".into());
  }
  std::fs::create_dir_all(&directory)?;
  for case in layout_cases::load()? {
    let graph = read_smiles(case.source)?.into_graph();
    if graph.bond_count() + graph.connected_components().len() - graph.atom_count() != case.rank {
      return Err(format!("incorrect cycle rank for {}", case.name).into());
    }
    let positions = layout::generate(&graph, &layout::LayoutOptions::default())?;
    if !positions.diagnostics().is_empty() {
      return Err(format!("{}: {:?}", case.name, positions.diagnostics()).into());
    }
    let drawing = depict::depict(&graph, &positions, &depict::DepictOptions::default())?;
    let output = svg::export(&drawing, &svg::SvgOptions::default())?;
    let path = directory.join(format!("{}.svg", case.name));
    std::fs::write(&path, output)?;
    println!("{}", path.display());
  }
  Ok(())
}
