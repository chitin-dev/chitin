//! Very large flexible inputs must not place two atom centers at one point.

use chitin_chem::io::smiles::read_smiles;
use chitin_chemdraw::layout;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn large_polyol_should_not_place_two_atoms_at_the_same_point() -> TestResult {
  let graph = read_smiles(include_str!("fixtures/palytoxin.smi").trim())?.into_graph();
  let coordinates = layout::generate(&graph, &layout::LayoutOptions::default())?;
  let ids: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
  for (offset, &a) in ids.iter().enumerate() {
    let Some(first) = coordinates.position(a) else {
      continue;
    };
    for &b in ids.iter().skip(offset + 1) {
      if let Some(second) = coordinates.position(b) {
        assert!(first.distance(second) > 0.1, "coincident atom centers: {a:?}/{b:?}");
      }
    }
  }
  Ok(())
}
