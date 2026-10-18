//! Terminal hydroxyls must not collapse into a false shared HOH label.

use std::collections::BTreeMap;

use chitin_chem::{AtomId, Element, MolecularGraph, io::smiles::read_smiles, perception::analyze_rings};
use chitin_chemdraw::{
  depict::{self, DepictOptions},
  geometry::Point,
  layout::{self, LayoutDiagnostic, LayoutOptions},
  scene::{Primitive, Source},
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Derives terminal oxygen identities without assuming input atom numbering.
fn hydroxyls(graph: &MolecularGraph) -> Vec<AtomId> {
  graph
    .atoms()
    .filter_map(|(id, atom)| {
      (atom.element() == Element::Oxygen && graph.neighbors(id).is_ok_and(|row| row.len() == 1)).then_some(id)
    })
    .collect()
}

#[test]
fn large_polycycle_should_separate_hydroxyl_centers_and_their_complete_labels() -> TestResult {
  let graph = read_smiles(include_str!("fixtures/large_bridged.smi").trim())?.into_graph();
  let before = graph.clone();
  let initial = layout::generate(
    &graph,
    &LayoutOptions {
      refinement_passes: 0,
      ..Default::default()
    },
  )?;
  let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
  for (_, bond) in graph.bonds() {
    let [a, b] = bond.atoms();
    assert!(
      (coordinates
        .position(a)
        .ok_or("missing atom")?
        .distance(coordinates.position(b).ok_or("missing atom")?)
        - 1.)
        .abs()
        < 1e-8
    );
  }
  for block in analyze_rings(&graph)?
    .blocks()
    .iter()
    .filter(|block| block.cycle_rank() > 0)
  {
    for &a in block.atoms() {
      for &b in block.atoms() {
        let old = initial
          .position(a)
          .ok_or("missing atom")?
          .distance(initial.position(b).ok_or("missing atom")?);
        let new = coordinates
          .position(a)
          .ok_or("missing atom")?
          .distance(coordinates.position(b).ok_or("missing atom")?);
        assert!((old - new).abs() < 1e-8, "ring deformed: {a}/{b}");
      }
    }
  }
  let oxygens = hydroxyls(&graph);
  for (i, &a) in oxygens.iter().enumerate() {
    for &b in &oxygens[i + 1..] {
      let distance = coordinates
        .position(a)
        .ok_or("missing O")?
        .distance(coordinates.position(b).ok_or("missing O")?);
      assert!(distance >= 0.55, "overlapping terminal oxygen {a}/{b}: {distance}");
    }
  }
  let drawing = depict::depict(&graph, &coordinates, &DepictOptions::default())?;
  let mut envelopes = BTreeMap::<AtomId, (Point, Point)>::new();
  for primitive in drawing.primitives() {
    if let Primitive::Text(text) = primitive
      && let Some(Source::Atom(id)) = text.source
      && oxygens.contains(&id)
    {
      let [min, max] = text.envelope()?;
      envelopes
        .entry(id)
        .and_modify(|(lo, hi)| {
          lo.x = lo.x.min(min.x);
          lo.y = lo.y.min(min.y);
          hi.x = hi.x.max(max.x);
          hi.y = hi.y.max(max.y);
        })
        .or_insert((min, max));
    }
  }
  let labels: Vec<_> = envelopes.iter().collect();
  for (i, &(a, &(lo, hi))) in labels.iter().enumerate() {
    for &(b, &(other_lo, other_hi)) in &labels[i + 1..] {
      assert!(
        !(lo.x < other_hi.x && other_lo.x < hi.x && lo.y < other_hi.y && other_lo.y < hi.y),
        "OH labels overlap: {a}/{b}"
      );
    }
  }
  assert!(
    !coordinates
      .diagnostics()
      .iter()
      .any(|diagnostic| matches!(diagnostic, LayoutDiagnostic::CrossingBonds { .. }))
  );
  assert_eq!(graph, before);
  Ok(())
}
