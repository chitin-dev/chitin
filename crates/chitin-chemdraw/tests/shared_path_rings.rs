//! Whole-block fallback preserves every source edge instead of opening a ring.

use chitin_chem::io::smiles::read_smiles;
use chitin_chemdraw::{
  layout::{self, LayoutError, LayoutOptions},
  pipeline,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn shared_path_blocks_should_close_all_edges_without_changing_source_facts() -> TestResult {
  for line in include_str!("fixtures/shared_path_rings.tsv")
    .lines()
    .filter(|line| !line.starts_with('#'))
  {
    let (name, source) = line.split_once('\t').ok_or("invalid fixture")?;
    let graph = read_smiles(source)?.into_graph();
    let original = graph.clone();
    let result = pipeline::generate(&graph, &LayoutOptions::default(), &Default::default())?;
    assert_eq!(result.layout.positions().len(), graph.atom_count(), "{name}");
    for (_, bond) in graph.bonds() {
      let [a, b] = bond.atoms();
      let a = result.layout.position(a).ok_or("missing atom")?;
      let b = result.layout.position(b).ok_or("missing atom")?;
      assert!((a.distance(b) - 1.).abs() < 1e-8, "{name}");
    }
    assert!(result.contacts.is_empty(), "{name}: {:?}", result.contacts);
    assert!(result.layout.bond_crossings().is_empty(), "{name}");
    assert!(result.layout.drawing_depths().is_empty(), "{name}");
    assert_eq!(graph, original, "{name}");
    assert_eq!(
      result,
      pipeline::generate(&graph, &LayoutOptions::default(), &Default::default())?,
      "{name}"
    );
  }
  Ok(())
}

#[test]
fn ring_work_exhaustion_should_not_publish_partial_fallback_geometry() -> TestResult {
  let source = include_str!("fixtures/shared_path_rings.tsv")
    .lines()
    .find(|line| !line.starts_with('#'))
    .and_then(|line| line.split_once('\t'))
    .ok_or("missing fixture")?
    .1;
  let graph = read_smiles(source)?.into_graph();
  let options = LayoutOptions {
    ring_search_budget: 1,
    fallback_budget: 0,
    ..Default::default()
  };
  assert!(matches!(
    layout::generate(&graph, &options),
    Err(LayoutError::RingSearchBudgetExceeded)
  ));
  Ok(())
}

#[test]
fn cyclic_alkene_constraints_should_survive_complete_fallback_closure() -> TestResult {
  let graph = read_smiles("C/C1=C/CC[C@@H](C)[C@@H]2CC[C@@]3(C)[C@@H]2[C@@H](C)C(=C[C@@H]3O)CC1")?.into_graph();
  assert!(graph.double_bond_stereos().next().is_some());
  let result = pipeline::generate(&graph, &LayoutOptions::default(), &Default::default())?;
  assert_eq!(result.layout.positions().len(), graph.atom_count());
  for (_, bond) in graph.bonds() {
    let [a, b] = bond.atoms();
    assert!(
      (result
        .layout
        .position(a)
        .ok_or("no a")?
        .distance(result.layout.position(b).ok_or("no b")?)
        - 1.)
        .abs()
        < 1e-7
    );
  }
  Ok(())
}
