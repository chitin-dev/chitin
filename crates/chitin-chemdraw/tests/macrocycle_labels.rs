//! Printed labels on compact macrocycles stay separated at drawing scale.

use chitin_chem::{io::smiles::read_smiles, perception::analyze_rings};
use chitin_chemdraw::{depict, layout, scene::Primitive};
use std::{collections::BTreeSet, error::Error};

type TestResult = Result<(), Box<dyn Error>>;

/// The reported cross-linked macrocycle, independent of batch configuration.
fn vancomycin() -> &'static str {
  include_str!("fixtures/complex_rings.smi")
    .lines()
    .find(|line| !line.starts_with('#') && !line.is_empty())
    .unwrap_or_default()
}

#[test]
fn compact_macrocycle_labels_should_not_overlap() -> TestResult {
  let graph = read_smiles(vancomycin())?.into_graph();
  let coordinates = layout::generate(&graph, &Default::default())?;
  assert!(coordinates.diagnostics().is_empty());
  let drawing = depict::depict(&graph, &coordinates, &Default::default())?;
  let mut texts: Vec<(String, [chitin_chemdraw::geometry::Point; 2])> = Vec::new();
  for primitive in drawing.primitives() {
    if let Primitive::Text(text) = primitive {
      texts.push((text.content.clone(), text.envelope()?));
    }
  }
  let mut overlapping = BTreeSet::new();
  for (i, (first, a)) in texts.iter().enumerate() {
    for (second, b) in texts.iter().skip(i + 1) {
      let width = (a[1].x.min(b[1].x) - a[0].x.max(b[0].x)).max(0.);
      let height = (a[1].y.min(b[1].y) - a[0].y.max(b[0].y)).max(0.);
      if width > 0.5 && height > 0.5 {
        overlapping.insert((first.clone(), second.clone()));
      }
    }
  }
  assert!(overlapping.is_empty(), "overlapping labels: {overlapping:?}");
  Ok(())
}

#[test]
fn compact_macrocycle_should_keep_zigzag_chain_angles() -> TestResult {
  let graph = read_smiles(vancomycin())?.into_graph();
  let coordinates = layout::generate(&graph, &Default::default())?;
  let report = analyze_rings(&graph)?;
  let block: BTreeSet<_> = report
    .blocks()
    .iter()
    .filter(|block| block.cycle_rank() >= 3)
    .flat_map(|block| block.atoms().iter().copied())
    .collect();
  let mut straight = 0;
  let mut total = 0;
  for (id, _) in graph.atoms().filter(|(id, _)| block.contains(id)) {
    let neighbors: Vec<_> = graph.neighbors(id)?.map(|(other, _)| other).collect();
    if neighbors.len() != 2 {
      continue;
    }
    let (Some(center), Some(a), Some(b)) = (
      coordinates.position(id),
      coordinates.position(neighbors[0]),
      coordinates.position(neighbors[1]),
    ) else {
      continue;
    };
    let cosine = ((a.x - center.x) * (b.x - center.x) + (a.y - center.y) * (b.y - center.y))
      / (a.distance(center) * b.distance(center));
    total += 1;
    if cosine < -0.866 {
      straight += 1;
    }
  }
  assert!(total >= 15, "block chain count {total}");
  assert!(
    straight * 10 <= total * 2,
    "near-straight macrocycle angles: {straight}/{total}"
  );
  Ok(())
}
