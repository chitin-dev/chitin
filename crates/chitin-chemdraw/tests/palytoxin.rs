//! Very large flexible inputs must not place two atom centers at one point.

use chitin_chem::io::smiles::read_smiles;
use chitin_chemdraw::layout;
use chitin_chemdraw::{depict, layout::LayoutDiagnostic};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn unannotated_large_polyol_should_reduce_footprint_without_new_crossings() -> TestResult {
  let graph = read_smiles(include_str!("fixtures/palytoxin_unannotated.smi").trim())?.into_graph();
  let before = layout::generate(
    &graph,
    &layout::LayoutOptions {
      backbone_budget: 0,
      ..Default::default()
    },
  )?;
  let after = layout::generate(&graph, &Default::default())?;
  let a = before.bounds().ok_or("missing before bounds")?;
  let b = after.bounds().ok_or("missing after bounds")?;
  assert!(
    b.width() * b.height() < a.width() * a.height(),
    "{}x{} -> {}x{}",
    a.width(),
    a.height(),
    b.width(),
    b.height()
  );
  assert!(
    !after
      .diagnostics()
      .iter()
      .any(|d| matches!(d, LayoutDiagnostic::CrossingBonds { .. }))
  );
  depict::depict(&graph, &after, &Default::default())?;
  Ok(())
}

#[test]
fn long_acyclic_backbones_should_support_bounded_row_routing() -> TestResult {
  let graph = read_smiles(&"C".repeat(160))?.into_graph();
  let before = layout::generate(
    &graph,
    &layout::LayoutOptions {
      backbone_budget: 0,
      ..Default::default()
    },
  )?;
  let after = layout::generate(&graph, &Default::default())?;
  let a = before.bounds().ok_or("missing initial bounds")?;
  let b = after.bounds().ok_or("missing routed bounds")?;
  assert!(
    b.width().max(b.height()) < 0.7 * a.width().max(a.height()),
    "{}x{} -> {}x{}",
    a.width(),
    a.height(),
    b.width(),
    b.height()
  );
  assert!(!after.diagnostics().iter().any(|d| matches!(
    d,
    LayoutDiagnostic::CrowdedAtoms { .. } | LayoutDiagnostic::CrossingBonds { .. }
  )));
  for (_, bond) in graph.bonds() {
    let [a, b] = bond.atoms();
    assert!(
      (after
        .position(a)
        .ok_or("missing atom")?
        .distance(after.position(b).ok_or("missing atom")?)
        - 1.)
        .abs()
        < 1e-8
    );
  }
  Ok(())
}

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

#[test]
fn backbone_routing_should_reduce_footprint_without_deforming_rings_or_stereo() -> TestResult {
  let graph = read_smiles(include_str!("fixtures/palytoxin.smi").trim())?.into_graph();
  let source = graph.clone();
  let before = layout::generate(
    &graph,
    &layout::LayoutOptions {
      backbone_budget: 0,
      ..Default::default()
    },
  )?;
  let after = layout::generate(&graph, &Default::default())?;
  let a = before.bounds().ok_or("missing initial bounds")?;
  let b = after.bounds().ok_or("missing final bounds")?;
  assert!(
    b.width() * b.height() < a.width() * a.height() * 0.85,
    "unrouted area {} -> routed area {}",
    a.width() * a.height(),
    b.width() * b.height()
  );
  let crowds = |layout: &layout::Layout2d| {
    layout
      .diagnostics()
      .iter()
      .filter(|d| matches!(d, LayoutDiagnostic::CrowdedAtoms { .. }))
      .count()
  };
  // Final terminal clearance also repairs the unrouted baseline. Routing must
  // still shrink the drawing, but no longer needs to remove a remaining pair.
  assert!(
    crowds(&after) <= crowds(&before) && crowds(&after) <= 1,
    "unrouted crowds {} -> routed crowds {}",
    crowds(&before),
    crowds(&after)
  );
  assert!(
    !after
      .diagnostics()
      .iter()
      .any(|d| matches!(d, LayoutDiagnostic::CrossingBonds { .. }))
  );
  for (_, bond) in graph.bonds() {
    let [a, b] = bond.atoms();
    let old = before
      .position(a)
      .ok_or("missing old atom")?
      .distance(before.position(b).ok_or("missing old atom")?);
    let new = after
      .position(a)
      .ok_or("missing new atom")?
      .distance(after.position(b).ok_or("missing new atom")?);
    assert!((old - new).abs() < 1e-8);
  }
  for block in chitin_chem::perception::analyze_rings(&graph)?
    .blocks()
    .iter()
    .filter(|block| block.cycle_rank() > 0)
  {
    for (i, &a) in block.atoms().iter().enumerate() {
      for &b in &block.atoms()[i + 1..] {
        let old = before
          .position(a)
          .ok_or("missing old ring atom")?
          .distance(before.position(b).ok_or("missing old ring atom")?);
        let new = after
          .position(a)
          .ok_or("missing new ring atom")?
          .distance(after.position(b).ok_or("missing new ring atom")?);
        assert!((old - new).abs() < 1e-8);
      }
    }
  }
  depict::depict(&graph, &after, &Default::default())?;
  assert_eq!(graph, source);
  assert_eq!(after, layout::generate(&graph, &Default::default())?);
  Ok(())
}

#[test]
fn exhausted_backbone_budget_should_keep_the_ordinary_layout() -> TestResult {
  let graph = read_smiles(include_str!("fixtures/palytoxin.smi").trim())?.into_graph();
  let before = layout::generate(
    &graph,
    &layout::LayoutOptions {
      backbone_budget: 0,
      ..Default::default()
    },
  )?;
  let after = layout::generate(
    &graph,
    &layout::LayoutOptions {
      backbone_budget: 1,
      ..Default::default()
    },
  )?;
  assert_eq!(before, after);
  Ok(())
}
