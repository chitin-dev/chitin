//! Metamorphic and adversarial tests for the evaluator, not the layout optimizer.

use super::*;
use chitin_chem::io::smiles::read_smiles;
use chitin_chemdraw::layout::{self, LayoutOptions};

/// Attaches deliberately authored coordinates without invoking placement repair.
fn authored(source: &str, points: &[(f64, f64)]) -> Result<(MolecularGraph, Layout2d)> {
  let graph = read_smiles(source)?.into_graph();
  let coordinates = Layout2d::from_positions(
    graph
      .atoms()
      .zip(points)
      .map(|((id, _), &(x, y))| (id, Point::new(x, y)))
      .collect(),
  )?;
  Ok((graph, coordinates))
}

#[test]
fn nonbonded_contacts_ignore_connected_short_edges() -> Result<()> {
  let (graph, layout) = authored("CCC", &[(0., 0.), (0.1, 0.), (1., 0.)])?;
  let result = measure(&graph, &layout)?;
  assert_eq!(result.measurements["nonbonded_atom_proximity"].eligible, 1);
  assert_eq!(result.measurements["nonbonded_atom_proximity"].violations, Some(0));
  Ok(())
}

#[test]
fn relative_measurements_survive_rotation_translation_and_scale() -> Result<()> {
  let base = [(0., 0.), (1., 0.), (0.1, 0.1)];
  let transformed: Vec<_> = base.iter().map(|&(x, y)| (-y * 30. + 200., x * 30. - 100.)).collect();
  let (a, la) = authored("CCC", &base)?;
  let (b, lb) = authored("CCC", &transformed)?;
  let a = measure(&a, &la)?;
  let b = measure(&b, &lb)?;
  for key in [
    "nonbonded_atom_proximity",
    "atom_edge_proximity",
    "relative_bond_length_rmse",
    "component_radius_gyration",
  ] {
    let (Some(a), Some(b)) = (a.measurements[key].value, b.measurements[key].value) else {
      anyhow::bail!("missing {key}");
    };
    assert!((a - b).abs() < 1e-8, "{key}: {a} vs {b}");
  }
  Ok(())
}

#[test]
fn regular_triangle_has_no_universal_120_degree_penalty() -> Result<()> {
  let (graph, layout) = authored("C1CC1", &[(0., 0.), (1., 0.), (0.5, 3_f64.sqrt() / 2.)])?;
  let result = measure(&graph, &layout)?;
  assert_eq!(result.measurements["simple_ring_angle_rmse_deg"].violations, Some(0));
  assert_eq!(
    result.measurements["acyclic_carbon_angle_rmse_deg"].state,
    State::NotApplicable
  );
  Ok(())
}

#[test]
fn macrocycles_are_not_required_to_be_regular_polygons() -> Result<()> {
  let graph = read_smiles("C1CCCCCCCCCCCC1")?.into_graph();
  let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
  assert_eq!(
    measure(&graph, &coordinates)?.measurements["simple_ring_angle_rmse_deg"].state,
    State::NotApplicable
  );
  Ok(())
}

#[test]
fn native_cages_should_not_be_implicitly_classified_as_perspective() -> Result<()> {
  let graph = read_smiles("C12C3C4C1C5C2C3C45")?.into_graph();
  let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
  let result = measure(&graph, &coordinates)?;
  assert_eq!(result.profile, "planar");
  assert_eq!(result.measurements["relative_bond_length_rmse"].state, State::Measured);
  Ok(())
}

#[test]
fn reversed_double_bond_relation_is_detected() -> Result<()> {
  let (graph, good) = authored("F/C=C/F", &[(-1., 1.), (0., 0.), (1., 0.), (2., -1.)])?;
  assert_eq!(
    measure(&graph, &good)?.measurements["double_bond_relation"].violations,
    Some(0)
  );
  let mut points = good.positions().clone();
  if let Some(last) = points.last_entry() {
    *last.into_mut() = Point::new(2., 1.);
  }
  assert_eq!(
    measure(&graph, &Layout2d::from_positions(points)?)?.measurements["double_bond_relation"].violations,
    Some(1)
  );
  Ok(())
}

#[test]
fn rejects_incomplete_coordinate_coverage() -> Result<()> {
  let graph = read_smiles("CC")?.into_graph();
  assert!(measure(&graph, &Layout2d::from_positions(Default::default())?).is_err());
  Ok(())
}

#[test]
fn collinear_overlaps_and_endpoint_contacts_are_detected() {
  assert!(segments_contact(
    Point::new(0., 0.),
    Point::new(3., 0.),
    Point::new(1., 0.),
    Point::new(4., 0.),
    1e-9
  ));
  assert!(!segments_contact(
    Point::new(0., 0.),
    Point::new(1., 0.),
    Point::new(2., 0.),
    Point::new(3., 0.),
    1e-9
  ));
}

#[test]
fn packing_distance_does_not_change_component_spread() -> Result<()> {
  let (a, la) = authored("CC.CC", &[(0., 0.), (1., 0.), (10., 0.), (11., 0.)])?;
  let (b, lb) = authored("CC.CC", &[(0., 0.), (1., 0.), (100., 0.), (101., 0.)])?;
  assert_eq!(
    measure(&a, &la)?.measurements["component_radius_gyration"].value,
    measure(&b, &lb)?.measurements["component_radius_gyration"].value
  );
  Ok(())
}
