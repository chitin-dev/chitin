//! Default-budget regressions for independent exits and repeated rigid trials.

use chitin_chem::io::smiles::read_smiles;
use chitin_chemdraw::{
  depict,
  layout::{self, LayoutDiagnostic},
  svg,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn default_search_should_clear_late_contacts_without_changing_graph_or_bond_lengths() -> TestResult {
  for source in include_str!("fixtures/late_branch_clearance.smi")
    .lines()
    .filter(|line| !line.is_empty() && !line.starts_with('#'))
  {
    let graph = read_smiles(source)?.into_graph();
    let original = graph.clone();
    let coordinates = layout::generate(&graph, &Default::default())?;
    assert!(
      !coordinates.diagnostics().iter().any(|diagnostic| matches!(
        diagnostic,
        LayoutDiagnostic::CrowdedAtoms { .. } | LayoutDiagnostic::CrossingBonds { .. }
      )),
      "{:?}",
      coordinates.diagnostics()
    );
    for (_, bond) in graph.bonds() {
      let [a, b] = bond.atoms();
      let p = coordinates.position(a).ok_or("missing atom")?;
      let q = coordinates.position(b).ok_or("missing atom")?;
      assert!((p.distance(q) - 1.).abs() < 1e-8);
    }
    let drawing = depict::depict(&graph, &coordinates, &Default::default())?;
    assert!(svg::export(&drawing, &Default::default())?.starts_with("<svg "));
    assert_eq!(graph, original);
    assert_eq!(coordinates, layout::generate(&graph, &Default::default())?);
  }
  Ok(())
}

#[test]
fn terminal_trial_reuse_should_not_add_chain_angle_violations() -> TestResult {
  let source = include_str!("fixtures/late_branch_angles.smi")
    .lines()
    .find(|line| !line.is_empty() && !line.starts_with('#'))
    .ok_or("missing fixture")?;
  let graph = read_smiles(source)?.into_graph();
  let rings = chitin_chem::perception::analyze_rings(&graph)?;
  let coordinates = layout::generate(&graph, &Default::default())?;
  let mut violations = 0;
  for (id, atom) in graph.atoms() {
    if atom.element() != chitin_chem::Element::Carbon {
      continue;
    }
    let neighbors: Vec<_> = graph.neighbors(id)?.collect();
    let [(a, first), (b, second)] = neighbors.as_slice() else {
      continue;
    };
    if ![first, second].iter().all(|edge| {
      rings.bridges().contains(edge)
        && graph
          .bond(**edge)
          .is_some_and(|bond| bond.order() == chitin_chem::BondOrder::Single)
    }) {
      continue;
    }
    let center = coordinates.position(id).ok_or("missing center")?;
    let a = coordinates.position(*a).ok_or("missing neighbor")?;
    let b = coordinates.position(*b).ok_or("missing neighbor")?;
    let cosine = ((a.x - center.x) * (b.x - center.x) + (a.y - center.y) * (b.y - center.y))
      / (a.distance(center) * b.distance(center));
    let angle = cosine.clamp(-1., 1.).acos().to_degrees();
    violations += usize::from((angle - 120.).abs() > 15.);
  }
  assert!(violations <= 2, "existing two deviations grew to {violations}");
  Ok(())
}
