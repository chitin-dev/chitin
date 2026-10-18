//! Bridge-path layout regressions, independent of the fused-face solver.

use chitin_chem::{io::smiles::read_smiles, perception::analyze_rings};
use chitin_chemdraw::{
  depict,
  layout::{self, LayoutOptions},
  svg,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Substituted, unsaturated, heteroatom, spiro-entered and chain-linked bridges.
const CASES: &[&str] = &[
  "C1CC2CCC1C2",
  "CC1CC2COC(C1)O2",
  "CC1CC2(C(OC(C1)(O2)C)C)C",
  "C1=CC2CCC1C2",
  "N[C@H]1C[C@@H]2CC[C@@H]1C2",
  "C1CC2(CCC3CC4CCC3C4)CCC1C2",
  "C1CC2(CC1)CC3CCC2C3",
];

#[test]
fn bridge_paths_should_close_with_unit_bonds_before_and_after_refinement() -> TestResult {
  for &source in CASES {
    let graph = read_smiles(source)?.into_graph();
    let before = graph.clone();
    for passes in [0, 4] {
      let options = LayoutOptions {
        refinement_passes: passes,
        ..Default::default()
      };
      let coordinates = layout::generate(&graph, &options)?;
      assert_eq!(coordinates.positions().len(), graph.atom_count());
      assert!(
        coordinates.diagnostics().is_empty(),
        "{source}: {:?}",
        coordinates.diagnostics()
      );
      for (_, bond) in graph.bonds() {
        let [a, b] = bond.atoms();
        let p = coordinates.position(a).ok_or("missing endpoint")?;
        let q = coordinates.position(b).ok_or("missing endpoint")?;
        assert!(p.is_finite() && q.is_finite());
        assert!((p.distance(q) - 1.).abs() < 1e-8, "{source}: {a}-{b}");
      }
      assert_eq!(coordinates, layout::generate(&graph, &options)?);
    }
    assert_eq!(graph, before);
  }
  Ok(())
}

#[test]
fn bridge_depiction_should_preserve_graph_and_source_stereo_in_both_h_modes() -> TestResult {
  for &source in CASES {
    let graph = read_smiles(source)?.into_graph();
    let before = graph.clone();
    let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
    for mode in [
      depict::ChiralHydrogenMode::Implicit,
      depict::ChiralHydrogenMode::Explicit,
    ] {
      let drawing = depict::depict(
        &graph,
        &coordinates,
        &depict::DepictOptions {
          chiral_hydrogens: mode,
          ..Default::default()
        },
      )?;
      let document = svg::export(&drawing, &svg::SvgOptions::default())?;
      assert!(document.starts_with("<svg "));
      assert!(!document.contains("NaN"));
    }
    assert_eq!(graph, before);
  }
  Ok(())
}

#[test]
fn bridge_layout_should_charge_the_shared_ring_budget() -> TestResult {
  let graph = read_smiles("CC1CC2COC(C1)O2")?.into_graph();
  let before = graph.clone();
  assert!(matches!(
    layout::generate(
      &graph,
      &LayoutOptions {
        ring_search_budget: 100,
        ..Default::default()
      }
    ),
    Err(layout::LayoutError::RingSearchBudgetExceeded)
  ));
  assert_eq!(graph, before);
  Ok(())
}

#[test]
fn equal_shortest_bridges_should_not_be_silently_superimposed() -> TestResult {
  let graph = read_smiles("C1C2CC1C2")?.into_graph();
  assert!(matches!(
    layout::generate(&graph, &LayoutOptions::default()),
    Err(layout::LayoutError::RingSystem { cycles: 2, .. })
  ));
  Ok(())
}

#[test]
fn reversed_edges_and_sparse_handles_should_keep_bridge_geometry() -> TestResult {
  let mut graph = read_smiles("C.CC1CC2COC(C1)O2")?.into_graph();
  let first = graph.atoms().next().map(|(id, _)| id).ok_or("no atom")?;
  graph.remove_atom(first)?;
  let expected = layout::generate(&graph, &LayoutOptions::default())?;
  let mut reordered = graph.clone();
  let edges: Vec<_> = graph
    .bonds()
    .map(|(id, edge)| (id, edge.atoms(), edge.order()))
    .collect();
  for &(id, _, _) in &edges {
    reordered.remove_bond(id)?;
  }
  for &(_, [a, b], order) in edges.iter().rev() {
    reordered.add_bond(b, a, order)?;
  }
  let observed = layout::generate(&reordered, &LayoutOptions::default())?;
  for (&id, point) in expected.positions() {
    assert!(point.distance(observed.position(id).ok_or("missing atom")?) < 1e-8);
  }
  Ok(())
}

#[test]
fn supplied_large_polycycle_should_generate_complete_svg() -> TestResult {
  let graph = read_smiles(include_str!("fixtures/large_bridged.smi").trim())?.into_graph();
  let before = graph.clone();
  assert!(graph.atom_count() > 150);
  assert!(
    analyze_rings(&graph)?
      .blocks()
      .iter()
      .any(|block| block.cycle_rank() == 2)
  );
  let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
  assert_eq!(coordinates.positions().len(), graph.atom_count());
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
  let drawing = depict::depict(&graph, &coordinates, &depict::DepictOptions::default())?;
  assert!(svg::export(&drawing, &svg::SvgOptions::default())?.starts_with("<svg "));
  assert_eq!(graph, before);
  Ok(())
}
