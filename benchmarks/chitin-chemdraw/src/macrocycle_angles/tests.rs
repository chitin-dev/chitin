//! Analytic geometry, source-domain and missingness checks for planar turns.

use super::*;
use chitin_chem::io::smiles::read_smiles;
use chitin_chemdraw::layout::{self, LayoutOptions};

/// Regular source-order polygon whose coordinates need no layout heuristic.
fn polygon(n: usize) -> Result<(MolecularGraph, Layout2d)> {
  let graph = read_smiles(&format!("C1{}1", "C".repeat(n - 1)))?.into_graph();
  let points = graph
    .atoms()
    .enumerate()
    .map(|(i, (id, _))| {
      let a = std::f64::consts::TAU * i as f64 / n as f64;
      (id, Point::new(a.cos(), a.sin()))
    })
    .collect();
  Ok((graph, Layout2d::from_positions(points)?))
}

#[test]
fn a_round_large_ring_should_expose_near_linear_angles_and_runs() -> Result<()> {
  let (graph, positions) = polygon(30)?;
  let metrics = crate::metrics::measure(&graph, &positions)?;
  assert_eq!(metrics.measurements[KEYS[0]].value, Some(1.));
  assert_eq!(metrics.measurements[KEYS[3]].violations, Some(30));
  assert_eq!(metrics.measurements[KEYS[4]].value, Some(30.));
  assert_eq!(metrics.measurements[KEYS[5]].value, Some(30.));
  assert!((metrics.measurements[KEYS[1]].value.unwrap_or(0.) - 33.).abs() < 1e-8);
  assert_eq!(metrics.macrocycle_geometry.as_ref().map(|g| g.samples.len()), Some(30));
  Ok(())
}

#[test]
fn ordinary_small_rings_and_chains_should_have_no_large_cycle_measurement() -> Result<()> {
  for source in ["CCCC", "C1CCCCC1", "c1ccc2ccccc2c1"] {
    let graph = read_smiles(source)?.into_graph();
    let positions = layout::generate(&graph, &LayoutOptions::default())?;
    let metrics = crate::metrics::measure(&graph, &positions)?;
    assert_eq!(metrics.measurements[KEYS[0]].state, State::NotApplicable, "{source}");
    assert!(
      metrics
        .macrocycle_geometry
        .as_ref()
        .is_some_and(|g| g.samples.is_empty())
    );
  }
  Ok(())
}

#[test]
fn reflection_should_invert_turns_without_changing_scalar_quality() -> Result<()> {
  let (graph, positions) = polygon(30)?;
  let reflected = Layout2d::from_positions(
    positions
      .positions()
      .iter()
      .map(|(&id, p)| (id, Point::new(-3. * p.x + 4., 3. * p.y - 7.)))
      .collect(),
  )?;
  let a = crate::metrics::measure(&graph, &positions)?;
  let b = crate::metrics::measure(&graph, &reflected)?;
  for key in KEYS {
    assert!(
      (a.measurements[key].value.unwrap_or(-1.) - b.measurements[key].value.unwrap_or(-1.)).abs() < 1e-8,
      "{key}"
    );
  }
  let a = a.macrocycle_geometry.ok_or_else(|| anyhow::anyhow!("missing turns"))?;
  let b = b.macrocycle_geometry.ok_or_else(|| anyhow::anyhow!("missing turns"))?;
  for (a, b) in a.samples.iter().zip(b.samples) {
    assert_eq!(a.atoms, b.atoms);
    assert!((a.turns[0] + b.turns[0]).abs() < 1e-8);
  }
  Ok(())
}

#[test]
fn crossed_cycles_should_still_have_local_turn_observations() -> Result<()> {
  let (graph, positions) = polygon(12)?;
  let ids: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
  let mut points = positions.positions().clone();
  let (a, b) = (points[&ids[1]], points[&ids[7]]);
  points.insert(ids[1], b);
  points.insert(ids[7], a);
  let metrics = crate::metrics::measure(&graph, &Layout2d::from_positions(points)?)?;
  assert_eq!(metrics.measurements[KEYS[0]].value, Some(1.));
  assert_eq!(metrics.macrocycle_geometry.as_ref().map(|g| g.samples.len()), Some(12));
  assert_eq!(
    metrics.measurements["maximum_ring_cavity_radius"].state,
    State::NotImplemented
  );
  Ok(())
}

#[test]
fn collapsed_bonds_should_reduce_coverage_instead_of_becoming_zero_turns() -> Result<()> {
  let (graph, positions) = polygon(12)?;
  let ids: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
  let mut points = positions.positions().clone();
  points.insert(ids[1], points[&ids[0]]);
  let metrics = crate::metrics::measure(&graph, &Layout2d::from_positions(points)?)?;
  let geometry = metrics
    .macrocycle_geometry
    .ok_or_else(|| anyhow::anyhow!("missing geometry"))?;
  assert_eq!(geometry.valid_angles, 10);
  assert_eq!(geometry.unique_angles, 12);
  Ok(())
}

#[test]
fn cyclic_runs_should_join_the_ends_without_counting_a_lap_twice() {
  assert_eq!(longest_run(&[1, 1, 0, 1]), 3);
  assert_eq!(longest_run(&[1, 1, 1]), 3);
  assert_eq!(longest_run(&[1, -1, 1, -1]), 1);
  assert_eq!(longest_run(&[0, 0]), 0);
}

#[test]
fn discovery_budget_should_not_return_a_partial_basis() {
  assert!(ring_cycles::basis(&[0, 1, 2], &[[0, 1], [1, 2], [2, 0]], 1, &mut Budget(1)).is_none());
}

#[test]
fn shared_paths_should_not_duplicate_angles_or_adjacent_turn_pairs() -> Result<()> {
  let graph = read_smiles("C1CCCCCC2CCCCCC1CCCCCC2")?.into_graph();
  let n = graph.atom_count();
  let points = graph
    .atoms()
    .enumerate()
    .map(|(i, (id, _))| {
      let a = std::f64::consts::TAU * i as f64 / n as f64;
      (id, Point::new(a.cos(), a.sin()))
    })
    .collect();
  let metrics = crate::metrics::measure(&graph, &Layout2d::from_positions(points)?)?;
  let g = metrics
    .macrocycle_geometry
    .ok_or_else(|| anyhow::anyhow!("missing geometry"))?;
  assert_eq!(g.selected_large_cycles, 2);
  let unique: BTreeSet<_> = g.samples.iter().map(|s| s.atoms).collect();
  assert_eq!(unique.len(), g.samples.len());
  assert!(g.samples.len() < g.selected_large_cycles * 13);
  assert!(g.samples.iter().any(|s| s.classes.contains(&CenterClass::Junction)));
  Ok(())
}

#[test]
fn required_linear_centers_should_not_receive_near_straight_flags() -> Result<()> {
  let graph = read_smiles("C1CCCC#CCCCCCCC1")?.into_graph();
  let n = graph.atom_count();
  let points = graph
    .atoms()
    .enumerate()
    .map(|(i, (id, _))| {
      let a = std::f64::consts::TAU * i as f64 / n as f64;
      (id, Point::new(a.cos(), a.sin()))
    })
    .collect();
  let metrics = crate::metrics::measure(&graph, &Layout2d::from_positions(points)?)?;
  assert_eq!(metrics.measurements[KEYS[3]].eligible, n - 2);
  assert!(
    metrics
      .macrocycle_geometry
      .as_ref()
      .is_some_and(|g| g.samples.iter().any(|s| s.classes.contains(&CenterClass::Linear)))
  );
  Ok(())
}
