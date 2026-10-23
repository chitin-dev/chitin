//! Complete-assembly selection and constraint-directed cyclic search regressions.

use chitin_chem::{DoubleBondRelation, MolecularGraph, io::smiles::read_smiles};
use chitin_chemdraw::{
  layout::{LayoutDiagnostic, LayoutOptions},
  pipeline,
  scene::{Primitive, Source},
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Loads fixed inputs without coupling production dispatch to dataset identities.
fn sources() -> impl Iterator<Item = (&'static str, &'static str)> {
  include_str!("fixtures/assembly.tsv")
    .lines()
    .filter_map(|line| line.split_once('\t'))
}

/// Independently checks supplied alkene reference sides in a complete drawing.
fn check_sides(graph: &MolecularGraph, result: &pipeline::RenderedMolecule) -> TestResult {
  for (_, stereo) in graph.double_bond_stereos() {
    let [a, b] = stereo.ends().map(|id| result.layout.position(id).ok_or("missing axis"));
    let (a, b) = (a?, b?);
    let [ra, rb] = stereo
      .references()
      .map(|id| result.layout.position(id).ok_or("missing reference"));
    let (ra, rb) = (ra?, rb?);
    let side = |p: chitin_chemdraw::geometry::Point| (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
    let product = side(ra) * side(rb);
    assert!(product.abs() > 1e-8);
    assert_eq!(product > 0., stereo.relation() == DoubleBondRelation::SameSide);
  }
  Ok(())
}

#[test]
fn alternate_assembly_should_keep_source_chemistry_coverage_and_determinism() -> TestResult {
  for (name, source) in sources() {
    let graph = read_smiles(source)?.into_graph();
    let before = graph.clone();
    let result = pipeline::generate(&graph, &Default::default(), &Default::default())?;
    assert_eq!(graph, before, "{name}");
    assert_eq!(result.layout.positions().len(), graph.atom_count(), "{name}");
    assert!(result.layout.positions().values().all(|point| point.is_finite()));
    for (bond, _) in graph.bonds() {
      assert!(
        result.drawing.primitives().iter().any(|primitive| match primitive {
          Primitive::Line(line) => line.source == Some(Source::Bond(bond)),
          Primitive::Polygon(polygon) => polygon.source == Some(Source::Bond(bond)),
          _ => false,
        }),
        "missing bond in {name}"
      );
    }
    check_sides(&graph, &result)?;
    assert_eq!(
      result,
      pipeline::generate(&graph, &Default::default(), &Default::default())?,
      "{name}"
    );
  }
  Ok(())
}

#[test]
fn constrained_macrocycle_and_linked_ring_blocks_should_clear_contacts() -> TestResult {
  for index in [2, 5] {
    let (name, source) = sources().nth(index).ok_or("missing source")?;
    let graph = read_smiles(source)?.into_graph();
    let result = pipeline::generate(&graph, &Default::default(), &Default::default())?;
    assert!(result.contacts.is_empty(), "{name}: {:?}", result.contacts);
    assert!(
      !result.layout.diagnostics().iter().any(|diagnostic| matches!(
        diagnostic,
        LayoutDiagnostic::CrowdedAtoms { .. } | LayoutDiagnostic::CrossingBonds { .. }
      )),
      "{name}: {:?}",
      result.layout.diagnostics()
    );
    assert!(
      !result
        .layout
        .diagnostics()
        .contains(&LayoutDiagnostic::ConstraintClosure),
      "{name}"
    );
    for (_, bond) in graph.bonds() {
      let [a, b] = bond.atoms().map(|id| result.layout.positions()[&id]);
      assert!((a.distance(b) - 1.).abs() < 1e-7, "{name}");
    }
  }
  Ok(())
}

#[test]
fn zero_optional_budget_should_retain_the_baseline_drawing() -> TestResult {
  let source = sources().nth(5).ok_or("missing source")?.1;
  let graph = read_smiles(source)?.into_graph();
  let options = LayoutOptions {
    fallback_budget: 0,
    ..Default::default()
  };
  let geometry = chitin_chemdraw::layout::generate(&graph, &options)?;
  let before = pipeline::finish(&graph, geometry, &options, &Default::default())?;
  assert_eq!(before, pipeline::generate(&graph, &options, &Default::default())?);
  Ok(())
}

#[test]
fn optional_search_reserves_should_not_change_a_clear_preferred_polycycle() -> TestResult {
  let graph = read_smiles("COC(=O)C1=C2NC3=CC=CC=C3[C@@]23CCN2[C@@H](O)[C@]4(C[C@@H]5C[C@]67CCO[C@H]6CCN6CC[C@@]8(C9=CC=CC(OC)=C9N(C4)[C@@]58O)[C@@H]67)[C@H]4OCC[C@]4(C1)[C@H]23")?.into_graph();
  let result = pipeline::generate(&graph, &Default::default(), &Default::default())?;
  assert!(result.contacts.is_empty(), "{:?}", result.contacts);
  assert!(
    !result.layout.diagnostics().iter().any(|diagnostic| matches!(
      diagnostic,
      LayoutDiagnostic::CrowdedAtoms { .. } | LayoutDiagnostic::CrossingBonds { .. }
    )),
    "{:?}",
    result.layout.diagnostics()
  );
  Ok(())
}

#[test]
fn odd_constrained_cycles_should_unfold_without_losing_unit_edges_or_stereo() -> TestResult {
  for line in include_str!("fixtures/odd_constrained_cycles.tsv").lines() {
    let (_, source) = line.split_once('\t').ok_or("missing source")?;
    let graph = read_smiles(source)?.into_graph();
    let before = graph.clone();
    let result = pipeline::generate(&graph, &Default::default(), &Default::default())?;
    assert_eq!(graph, before);
    assert!(result.contacts.len() <= 2, "{:?}", result.contacts);
    assert!(
      !result.layout.diagnostics().iter().any(|diagnostic| matches!(
        diagnostic,
        LayoutDiagnostic::CrowdedAtoms { .. }
          | LayoutDiagnostic::CrossingBonds { .. }
          | LayoutDiagnostic::ConstraintClosure
      )),
      "{:?}",
      result.layout.diagnostics()
    );
    for (_, bond) in graph.bonds() {
      let [a, b] = bond.atoms().map(|id| result.layout.positions()[&id]);
      assert!((a.distance(b) - 1.).abs() < 1e-7);
    }
    check_sides(&graph, &result)?;
    assert_eq!(
      result,
      pipeline::generate(&graph, &Default::default(), &Default::default())?
    );
  }
  Ok(())
}

#[test]
fn shared_paths_and_dense_macrocycles_should_keep_complete_clear_drawings() -> TestResult {
  let fixtures: Vec<_> = include_str!("fixtures/macrocycles.tsv")
    .lines()
    .filter_map(|line| line.split_once('\t'))
    .collect();
  for index in [0, 3, 5] {
    let (name, source) = fixtures[index];
    let graph = read_smiles(source)?.into_graph();
    let original = graph.clone();
    let result = pipeline::generate(&graph, &Default::default(), &Default::default())?;
    assert_eq!(graph, original, "{name}");
    assert!(result.contacts.is_empty(), "{name}: {:?}", result.contacts);
    assert!(
      result.layout.diagnostics().iter().all(|diagnostic| !matches!(
        diagnostic,
        LayoutDiagnostic::CrowdedAtoms { .. }
          | LayoutDiagnostic::CrossingBonds { .. }
          | LayoutDiagnostic::ConstraintClosure
      )),
      "{name}: {:?}",
      result.layout.diagnostics()
    );
    for (_, bond) in graph.bonds() {
      let [a, b] = bond.atoms().map(|id| result.layout.positions()[&id]);
      assert!((a.distance(b) - 1.).abs() < 1e-7, "{name}");
    }
    check_sides(&graph, &result)?;
    assert_eq!(
      result,
      pipeline::generate(&graph, &Default::default(), &Default::default())?,
      "{name}"
    );
  }
  Ok(())
}
