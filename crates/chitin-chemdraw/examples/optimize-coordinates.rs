//! Exports authored coordinate-refinement cases before and after projected search.

use chitin_chem::{MolecularGraph, io::smiles::read_smiles};
use chitin_chemdraw::{
  depict,
  geometry::Point,
  layout::{
    Layout2d,
    optimization::{self, OptimizationOptions},
  },
  svg,
};
use std::{collections::BTreeSet, path::Path};

/// Writes one source-consistent scene to the explicitly supplied artifact directory.
fn export(
  graph: &MolecularGraph,
  coordinates: &Layout2d,
  directory: &Path,
  name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
  let drawing = depict::depict(graph, coordinates, &Default::default())?;
  let document = svg::export(&drawing, &Default::default())?;
  std::fs::write(directory.join(format!("{name}.svg")), document)?;
  Ok(())
}

/// Generates folded branches, an interior crossing, and a crowded closed macrocycle.
///
/// # Errors
/// Reports invalid source geometry, unsuccessful expected improvement, scene
/// conversion, and filesystem failures instead of silently skipping a case.
fn main() -> Result<(), Box<dyn std::error::Error>> {
  let mut args = std::env::args().skip(1);
  let directory = std::path::PathBuf::from(args.next().ok_or("usage: optimize-coordinates <output-directory>")?);
  if args.next().is_some() {
    return Err("too many arguments".into());
  }
  std::fs::create_dir_all(&directory)?;
  for name in ["folded_branches", "crossing", "crowded_macrocycle"] {
    let source = match name {
      "folded_branches" => "OCCO",
      "crossing" => "CCCC",
      _ => "OC1CCCCCCCCCCC1O",
    };
    let graph = read_smiles(source)?.into_graph();
    let ids: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
    let y = 3_f64.sqrt() / 2.;
    let mut anchors = BTreeSet::new();
    let initial = if name == "crowded_macrocycle" {
      let radius = 1. / (2. * (std::f64::consts::PI / 12.).sin());
      let seed = Layout2d::from_positions(
        ids
          .iter()
          .copied()
          .enumerate()
          .map(|(i, id)| {
            let t = i.saturating_sub(1).min(11) as f64 * std::f64::consts::PI / 6.;
            (id, Point::new(radius * t.cos(), radius * t.sin()))
          })
          .collect(),
      )?;
      let a = seed.position(ids[1]).ok_or("missing host")?;
      let b = seed.position(ids[12]).ok_or("missing host")?;
      let oxygen = Point::new((a.x + b.x) / 2. - (b.y - a.y) * y, (a.y + b.y) / 2. + (b.x - a.x) * y);
      let mut points = seed.positions().clone();
      points.insert(ids[0], oxygen);
      points.insert(ids[13], oxygen);
      anchors.extend([ids[0], ids[1]]);
      Layout2d::from_positions(points)?
    } else {
      let points = if name == "crossing" {
        [
          Point::new(-0.5, 0.),
          Point::new(0.5, 0.),
          Point::new(0., y),
          Point::new(0., y - 1.),
        ]
      } else {
        anchors.extend([ids[1], ids[2]]);
        [
          Point::new(0., y),
          Point::new(-0.5, 0.),
          Point::new(0.5, 0.),
          Point::new(0., y),
        ]
      };
      Layout2d::from_positions(ids.iter().copied().zip(points).collect())?
    };
    let result = optimization::optimize(&graph, &initial, &anchors, &OptimizationOptions::default())?;
    if result.report.final_geometry_cost + 1e-8 >= result.report.initial_geometry_cost {
      return Err(format!("{name}: no expected improvement").into());
    }
    export(&graph, &initial, &directory, &format!("{name}_before"))?;
    export(&graph, &result.layout, &directory, &format!("{name}_after"))?;
    println!("{name}: {:?}", result.report);
  }
  Ok(())
}
