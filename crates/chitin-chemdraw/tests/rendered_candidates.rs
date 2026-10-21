//! Complete drawing regression for label-aware, bounded fragment combinations.

use chitin_chem::io::smiles::read_smiles;
use chitin_chemdraw::{layout::LayoutDiagnostic, pipeline};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn label_aware_search_should_clear_the_compact_ester_without_changing_chemistry() -> TestResult {
  let graph = read_smiles("COC(=O)[C@H](OC(C)=O)C1=CC=C(N)C=C1C(=O)O")?.into_graph();
  let original = graph.clone();
  let result = pipeline::generate(&graph, &Default::default(), &Default::default())?;
  assert!(result.contacts.is_empty(), "{:?}", result.contacts);
  assert!(!result.layout.diagnostics().iter().any(|d| matches!(
    d,
    LayoutDiagnostic::CrowdedAtoms { .. } | LayoutDiagnostic::CrossingBonds { .. }
  )));
  for (_, bond) in graph.bonds() {
    let [a, b] = bond.atoms();
    let a = result.layout.position(a).ok_or("missing endpoint")?;
    let b = result.layout.position(b).ok_or("missing endpoint")?;
    assert!((a.distance(b) - 1.).abs() < 1e-8);
  }
  assert_eq!(graph, original);
  assert_eq!(
    result,
    pipeline::generate(&graph, &Default::default(), &Default::default())?
  );
  Ok(())
}
