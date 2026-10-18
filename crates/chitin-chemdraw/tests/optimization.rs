//! Public projected-refinement contracts over complete coordinate snapshots.

use chitin_chem::{AtomId, MolecularGraph, io::smiles::read_smiles};
use chitin_chemdraw::{
  geometry::Point,
  layout::{
    self, Layout2d, LayoutOptions,
    optimization::{self, BOND_TOLERANCE, OptimizationError, OptimizationOptions},
  },
};
use std::collections::BTreeSet;

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Two folded unit O–C branches; the two carbon positions can be fixed exactly.
fn folded() -> Result<(MolecularGraph, Layout2d, Vec<AtomId>), Box<dyn std::error::Error>> {
  let graph = read_smiles("OCCO")?.into_graph();
  let ids: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
  let oxygen = Point::new(0., 3_f64.sqrt() / 2.);
  let points = [oxygen, Point::new(-0.5, 0.), Point::new(0.5, 0.), oxygen];
  let initial = Layout2d::from_positions(ids.iter().copied().zip(points).collect())?;
  Ok((graph, initial, ids))
}

/// Checks the public unit-distance tolerance on every source edge and closure.
fn units(graph: &MolecularGraph, coordinates: &Layout2d) -> TestResult {
  assert_eq!(coordinates.positions().len(), graph.atom_count());
  for (_, edge) in graph.bonds() {
    let [a, b] = edge.atoms();
    let distance = coordinates
      .position(a)
      .ok_or("missing atom")?
      .distance(coordinates.position(b).ok_or("missing atom")?);
    assert!((distance - 1.).abs() <= BOND_TOLERANCE, "length {distance}");
  }
  Ok(())
}

#[test]
fn crowded_branches_should_improve_with_pinned_hosts_and_unit_bonds() -> TestResult {
  let (graph, initial, ids) = folded()?;
  let before = graph.clone();
  let anchors = BTreeSet::from([ids[1], ids[2]]);
  let result = optimization::optimize(&graph, &initial, &anchors, &OptimizationOptions::default())?;
  assert!(
    result.report.final_geometry_cost + 1e-8 < result.report.initial_geometry_cost,
    "{:?}",
    result.report
  );
  assert_eq!(result.report.improved_components, 1);
  assert!(
    result
      .layout
      .position(ids[0])
      .ok_or("no O")?
      .distance(result.layout.position(ids[3]).ok_or("no O")?)
      > 0.7
  );
  for id in anchors {
    assert_eq!(result.layout.position(id), initial.position(id));
  }
  units(&graph, &result.layout)?;
  assert_eq!(graph, before);
  assert_eq!(initial, folded()?.1);
  assert_eq!(
    result,
    optimization::optimize(
      &graph,
      &initial,
      &BTreeSet::from([ids[1], ids[2]]),
      &OptimizationOptions::default()
    )?
  );
  Ok(())
}

#[test]
fn disabled_or_fully_pinned_search_should_retain_exact_positions() -> TestResult {
  let (graph, initial, ids) = folded()?;
  for options in [
    OptimizationOptions {
      iterations: 0,
      ..Default::default()
    },
    OptimizationOptions {
      work_budget: 0,
      ..Default::default()
    },
    OptimizationOptions::default(),
  ] {
    let anchors = if options.iterations > 0 && options.work_budget > 0 {
      ids.iter().copied().collect()
    } else {
      BTreeSet::new()
    };
    let result = optimization::optimize(&graph, &initial, &anchors, &options)?;
    assert_eq!(result.layout.positions(), initial.positions());
    assert_eq!(result.report.initial_geometry_cost, result.report.final_geometry_cost);
    assert_eq!(result.report.improved_components, 0);
  }
  Ok(())
}

#[test]
fn depleted_search_should_return_only_complete_validated_geometry() -> TestResult {
  let (graph, initial, ids) = folded()?;
  let anchors = BTreeSet::from([ids[1], ids[2]]);
  for allowance in [1, 8, 32, 128, 512, 2048, 8192, 32768] {
    let options = OptimizationOptions {
      work_budget: allowance,
      ..Default::default()
    };
    let result = optimization::optimize(&graph, &initial, &anchors, &options)?;
    assert!(result.report.work_used <= allowance);
    assert!(result.report.final_geometry_cost <= result.report.initial_geometry_cost + 1e-12);
    for &id in &anchors {
      assert_eq!(result.layout.position(id), initial.position(id));
    }
    units(&graph, &result.layout)?;
    if allowance < 32 {
      assert!(result.report.budget_exhausted);
      assert_eq!(result.layout.positions(), initial.positions());
    }
  }
  Ok(())
}

#[test]
fn independent_fragments_should_not_be_repacked_or_dragged_by_search() -> TestResult {
  let graph = read_smiles("OCCO.CC")?.into_graph();
  let ids: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
  let (_, folded, _) = folded()?;
  let mut points = folded.positions().clone();
  points.insert(ids[4], Point::new(10.125, 7.25));
  points.insert(ids[5], Point::new(11.125, 7.25));
  let initial = Layout2d::from_positions(points)?;
  let result = optimization::optimize(&graph, &initial, &BTreeSet::new(), &OptimizationOptions::default())?;
  assert_eq!(result.layout.position(ids[4]), initial.position(ids[4]));
  assert_eq!(result.layout.position(ids[5]), initial.position(ids[5]));
  units(&graph, &result.layout)
}

#[test]
fn invalid_ids_options_bonds_and_anchors_should_fail_before_search() -> TestResult {
  let (graph, initial, ids) = folded()?;
  let mut missing = initial.positions().clone();
  missing.remove(&ids[0]);
  assert!(matches!(
    optimization::optimize(
      &graph,
      &Layout2d::from_positions(missing)?,
      &BTreeSet::new(),
      &Default::default()
    ),
    Err(OptimizationError::Coverage)
  ));
  let other = read_smiles("CCCCC")?.into_graph();
  let alien = other.atoms().last().map(|(id, _)| id).ok_or("no alien")?;
  assert!(
    matches!(optimization::optimize(&graph,&initial,&BTreeSet::from([alien]),&Default::default()),Err(OptimizationError::Anchor(id)) if id == alien)
  );
  let mut stretched = initial.positions().clone();
  stretched.insert(ids[0], Point::new(20., 20.));
  assert!(matches!(
    optimization::optimize(
      &graph,
      &Layout2d::from_positions(stretched)?,
      &BTreeSet::new(),
      &Default::default()
    ),
    Err(OptimizationError::NonUnitBond(_))
  ));
  for options in [
    OptimizationOptions {
      projection_passes: 0,
      ..Default::default()
    },
    OptimizationOptions {
      max_atoms: 0,
      ..Default::default()
    },
  ] {
    assert!(matches!(
      optimization::optimize(&graph, &initial, &BTreeSet::new(), &options),
      Err(OptimizationError::Options)
    ));
  }
  assert!(matches!(
    optimization::optimize(
      &graph,
      &initial,
      &BTreeSet::new(),
      &OptimizationOptions {
        max_atoms: 3,
        ..Default::default()
      }
    ),
    Err(OptimizationError::AtomBudget { .. })
  ));
  Ok(())
}

#[test]
fn inverted_initial_double_configuration_should_not_be_silently_repaired() -> TestResult {
  let graph = read_smiles("F/C=C/F")?.into_graph();
  let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
  let (bond, stereo) = graph.double_bond_stereos().next().ok_or("no stereo")?;
  let [a, b] = stereo.ends();
  let [_, reference] = stereo.references();
  let a = coordinates.position(a).ok_or("no axis")?;
  let b = coordinates.position(b).ok_or("no axis")?;
  let p = coordinates.position(reference).ok_or("no reference")?;
  let (x, y) = (p.x - a.x, p.y - a.y);
  let (ux, uy) = (b.x - a.x, b.y - a.y);
  let dot = x * ux + y * uy;
  let mut points = coordinates.positions().clone();
  points.insert(reference, Point::new(a.x + 2. * dot * ux - x, a.y + 2. * dot * uy - y));
  assert!(
    matches!(optimization::optimize(&graph,&Layout2d::from_positions(points)?,&BTreeSet::new(),&Default::default()),Err(OptimizationError::DoubleBondStereo(id)) if id == bond)
  );
  Ok(())
}

#[test]
fn normal_ring_and_stereo_inputs_should_remain_depictable() -> TestResult {
  for source in [
    "c1ccccc1",
    "F/C=C/[C@H](Cl)Br",
    "C/1=C\\CCCCCCCCCCC1",
    "C1CCCCCCCCCCC2CCCCC12",
  ] {
    let graph = read_smiles(source)?.into_graph();
    let initial = layout::generate(&graph, &LayoutOptions::default())?;
    let result = optimization::optimize(&graph, &initial, &BTreeSet::new(), &Default::default())?;
    units(&graph, &result.layout)?;
    chitin_chemdraw::depict::depict(&graph, &result.layout, &Default::default())?;
    assert!(result.report.final_geometry_cost <= result.report.initial_geometry_cost + 1e-12);
  }
  Ok(())
}

#[test]
fn a_crossing_should_be_removed_without_breaking_unit_edges() -> TestResult {
  let graph = read_smiles("CCCC")?.into_graph();
  let ids: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
  let y = 3_f64.sqrt() / 2.;
  let points = [
    Point::new(-0.5, 0.),
    Point::new(0.5, 0.),
    Point::new(0., y),
    Point::new(0., y - 1.),
  ];
  let initial = Layout2d::from_positions(ids.into_iter().zip(points).collect())?;
  let result = optimization::optimize(&graph, &initial, &BTreeSet::new(), &Default::default())?;
  assert!(
    result.report.final_geometry_cost + 1e-8 < result.report.initial_geometry_cost,
    "{:?}",
    result.report
  );
  assert!(
    !result
      .layout
      .diagnostics()
      .iter()
      .any(|d| matches!(d, layout::LayoutDiagnostic::CrossingBonds { .. })),
    "{:?}",
    result.report
  );
  units(&graph, &result.layout)
}

#[test]
fn a_crowded_macrocycle_should_allow_joint_coordinate_adjustment() -> TestResult {
  let graph = read_smiles("OC1CCCCCCCCCCC1O")?.into_graph();
  let ids: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
  let mut options = LayoutOptions::default();
  options.optimization.iterations = 0;
  let seed = layout::generate(&graph, &options)?;
  let a = seed.position(ids[1]).ok_or("no host")?;
  let b = seed.position(ids[12]).ok_or("no host")?;
  let shared = Point::new(
    (a.x + b.x) / 2. - (b.y - a.y) * 3_f64.sqrt() / 2.,
    (a.y + b.y) / 2. + (b.x - a.x) * 3_f64.sqrt() / 2.,
  );
  let mut points = seed.positions().clone();
  points.insert(ids[0], shared);
  points.insert(ids[13], shared);
  let initial = Layout2d::from_positions(points)?;
  let anchors = BTreeSet::from([ids[0], ids[1]]);
  let result = optimization::optimize(&graph, &initial, &anchors, &Default::default())?;
  assert!(
    result.report.final_geometry_cost + 1e-8 < result.report.initial_geometry_cost,
    "{:?}",
    result.report
  );
  assert!(ids[2..13].iter().any(|&id| {
    result
      .layout
      .position(id)
      .zip(initial.position(id))
      .is_some_and(|(a, b)| a.distance(b) > 1e-5)
  }));
  for id in anchors {
    assert_eq!(result.layout.position(id), initial.position(id));
  }
  units(&graph, &result.layout)
}

#[test]
fn automatic_projected_refinement_should_improve_a_crowded_macrocycle() -> TestResult {
  for branch in ["(C)", "(CC)"] {
    let source = format!("C1{branch}{}C1{branch}", format!("C{branch}").repeat(10));
    let graph = read_smiles(&source)?.into_graph();
    let mut discrete_options = LayoutOptions::default();
    discrete_options.optimization.iterations = 0;
    let before = layout::generate(&graph, &discrete_options)?;
    let after = layout::generate(&graph, &LayoutOptions::default())?;
    let inspection = OptimizationOptions {
      iterations: 0,
      ..Default::default()
    };
    let before_cost = optimization::optimize(&graph, &before, &BTreeSet::new(), &inspection)?
      .report
      .initial_geometry_cost;
    let after_cost = optimization::optimize(&graph, &after, &BTreeSet::new(), &inspection)?
      .report
      .initial_geometry_cost;
    assert!(
      after_cost + 1e-8 < before_cost,
      "{branch}: {before_cost} -> {after_cost}"
    );
    units(&graph, &after)?;
    assert_eq!(after, layout::generate(&graph, &LayoutOptions::default())?);
  }
  Ok(())
}

#[test]
fn sparse_handles_and_fragment_changes_should_not_change_constraint_identity() -> TestResult {
  let (dense, initial, ids) = folded()?;
  let dense_result = optimization::optimize(&dense, &initial, &BTreeSet::from([ids[1], ids[2]]), &Default::default())?;
  let mut graph = read_smiles("C.OCCO")?.into_graph();
  let first = graph.atoms().next().map(|(id, _)| id).ok_or("no atom")?;
  graph.remove_atom(first)?;
  let sparse: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
  let remapped = Layout2d::from_positions(
    sparse
      .iter()
      .copied()
      .zip(initial.positions().values().copied())
      .collect(),
  )?;
  let result = optimization::optimize(
    &graph,
    &remapped,
    &BTreeSet::from([sparse[1], sparse[2]]),
    &Default::default(),
  )?;
  for (dense, sparse) in dense_result
    .layout
    .positions()
    .values()
    .zip(result.layout.positions().values())
  {
    assert!(dense.distance(*sparse) < 1e-10);
  }
  assert_eq!(dense_result.report, result.report);
  units(&graph, &result.layout)
}
