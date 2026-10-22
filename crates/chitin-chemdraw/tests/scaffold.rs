//! Source-complete initial embedding for interlinked flexible cyclic paths.

use chitin_chem::io::smiles::read_smiles;
use chitin_chemdraw::{
  layout::{self, LayoutDiagnostic, LayoutOptions},
  pipeline,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn crosslinked_macrocycle_should_unfold_without_losing_source_bonds_or_annotations() -> TestResult {
  let graph = read_smiles(include_str!("fixtures/crosslinked_macrocycle.smi").trim())?.into_graph();
  let original = graph.clone();
  let result = pipeline::generate(&graph, &LayoutOptions::default(), &Default::default())?;
  assert_eq!(graph, original);
  assert_eq!(result.layout.positions().len(), 268);
  assert_eq!(graph.bond_count(), 275);
  assert_eq!(graph.tetrahedral_centers().count(), 26);
  assert!(result.layout.bond_crossings().is_empty());
  assert!(result.layout.drawing_depths().is_empty());
  let mut close = 0;
  let atoms: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
  for (i, &a) in atoms.iter().enumerate() {
    for &b in &atoms[i + 1..] {
      if graph.bond_between(a, b).is_none()
        && result
          .layout
          .position(a)
          .ok_or("missing a")?
          .distance(result.layout.position(b).ok_or("missing b")?)
          < 0.55
      {
        close += 1;
      }
    }
  }
  // The former scatter seed produced 571 near nonbonded pairs on this source.
  // This regression gate requires real unfolding, not merely finite coordinates.
  assert!(close < 60, "remaining near pairs: {close}");
  for (_, bond) in graph.bonds() {
    let [a, b] = bond.atoms();
    assert!(
      (result
        .layout
        .position(a)
        .ok_or("missing a")?
        .distance(result.layout.position(b).ok_or("missing b")?)
        - 1.)
        .abs()
        < 1e-7
    );
  }
  assert!(
    result
      .layout
      .diagnostics()
      .contains(&LayoutDiagnostic::ConstraintClosure)
  );
  assert_eq!(
    result,
    pipeline::generate(&graph, &LayoutOptions::default(), &Default::default())?
  );
  Ok(())
}

#[test]
fn preferred_success_should_not_enter_the_new_scaffold_strategy() -> TestResult {
  for source in ["c1ccccc1", "C1CCCCC1", "C1C2CC3CC1CC(C2)C3", "CC(=O)NC(C)C(=O)NC"] {
    let graph = read_smiles(source)?.into_graph();
    let standard = layout::generate(&graph, &LayoutOptions::default())?;
    let disabled = layout::generate(
      &graph,
      &LayoutOptions {
        fallback_budget: 0,
        ..Default::default()
      },
    )?;
    assert_eq!(standard, disabled);
  }
  Ok(())
}
