//! Unpacked fragments must not influence another component's local geometry.

use chitin_chem::io::smiles::read_smiles;
use chitin_chemdraw::layout::{self, LayoutOptions};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn unrelated_fragments_should_not_bend_a_zigzag_chain() -> TestResult {
  for source in [
    "CCC",
    "CCC.C",
    "CCC.CCC",
    "CCC.C1CCCCC1",
    &format!("CCC.{}", "C.".repeat(151).trim_end_matches('.')),
  ] {
    let graph = read_smiles(source)?.into_graph();
    for passes in [0, 4] {
      let coordinates = layout::generate(
        &graph,
        &LayoutOptions {
          refinement_passes: passes,
          ..Default::default()
        },
      )?;
      let points = graph
        .atoms()
        .take(3)
        .map(|(id, _)| coordinates.position(id).ok_or("missing chain atom"))
        .collect::<Result<Vec<_>, _>>()?;
      let [a, center, b] = [points[0], points[1], points[2]];
      let cosine = ((a.x - center.x) * (b.x - center.x) + (a.y - center.y) * (b.y - center.y))
        / (a.distance(center) * b.distance(center));
      assert!((cosine + 0.5).abs() < 0.001, "{source}, passes={passes}: {cosine}");
    }
  }
  Ok(())
}

#[test]
fn unrelated_fragments_should_not_change_internal_ring_and_branch_distances() -> TestResult {
  let source = "CC1CCCCC1O";
  let graph = read_smiles(source)?.into_graph();
  let reference = layout::generate(&graph, &Default::default())?;
  let reference: Vec<_> = graph
    .atoms()
    .map(|(id, _)| reference.position(id).ok_or("missing reference"))
    .collect::<Result<_, _>>()?;
  // Crossing the wide-search threshold must not change this small component.
  let padded = read_smiles(&format!("{source}.{}", "C.".repeat(151).trim_end_matches('.')))?.into_graph();
  let coordinates = layout::generate(&padded, &Default::default())?;
  let actual: Vec<_> = padded
    .atoms()
    .take(reference.len())
    .map(|(id, _)| coordinates.position(id).ok_or("missing padded atom"))
    .collect::<Result<_, _>>()?;
  for i in 0..reference.len() {
    for j in i + 1..reference.len() {
      let error = (reference[i].distance(reference[j]) - actual[i].distance(actual[j])).abs();
      // Native coordinate assignment quantizes to 0.01 drawing units before
      // normalizing by fifty; unrelated-component transforms retain this bound.
      assert!(error < 0.001, "pair {i}/{j}: {error}");
    }
  }
  Ok(())
}
