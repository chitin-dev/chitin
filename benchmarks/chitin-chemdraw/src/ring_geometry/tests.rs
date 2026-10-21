//! Authored geometry separates evaluator correctness from layout quality.

use super::*;
use chitin_chem::io::smiles::read_smiles;
use chitin_chemdraw::layout::{self, Layout2d, LayoutOptions};

fn authored(smiles: &str, xy: &[(f64, f64)]) -> Result<(MolecularGraph, Layout2d)> {
  let graph = read_smiles(smiles)?.into_graph();
  let positions = graph
    .atoms()
    .zip(xy)
    .map(|((id, _), &(x, y))| (id, Point::new(x, y)))
    .collect();
  Ok((graph, Layout2d::from_positions(positions)?))
}

#[test]
fn square_has_analytic_cavity_roundness_and_footprint() -> Result<()> {
  let (graph, layout) = authored("C1CCC1", &[(0., 0.), (1., 0.), (1., 1.), (0., 1.)])?;
  let result = crate::metrics::measure(&graph, &layout)?;
  assert!((result.measurements["maximum_ring_cavity_radius"].value.unwrap_or(-1.) - 0.5).abs() <= CAVITY_TOLERANCE);
  assert!(
    (result.measurements["ring_isoperimetric_ratio"].value.unwrap_or(-1.) - std::f64::consts::PI / 4.).abs() < 1e-10
  );
  assert_eq!(result.measurements["normalized_footprint"].value, Some(0.25));
  assert_eq!(result.ring_geometry.as_ref().map(|r| r.faces.len()), Some(1));
  Ok(())
}

#[test]
fn whitespace_is_scale_rotation_translation_and_reflection_invariant() -> Result<()> {
  let source = [(0., 0.), (2., 0.), (2., 1.), (0., 1.)];
  let (graph, base) = authored("C1CCC1", &source)?;
  let transformed: BTreeMap<_, _> = base
    .positions()
    .iter()
    .map(|(&id, p)| {
      let angle = 0.71_f64;
      (
        id,
        Point::new(
          100. + 3. * (p.x * angle.cos() - p.y * angle.sin()),
          -200. - 3. * (p.x * angle.sin() + p.y * angle.cos()),
        ),
      )
    })
    .collect();
  let first = crate::metrics::measure(&graph, &base)?;
  let second = crate::metrics::measure(&graph, &Layout2d::from_positions(transformed)?)?;
  for key in [
    "maximum_ring_cavity_radius",
    "ring_isoperimetric_ratio",
    "normalized_footprint",
  ] {
    assert!(
      (first.measurements[key].value.unwrap_or(-1.) - second.measurements[key].value.unwrap_or(-2.)).abs()
        <= 2. * CAVITY_TOLERANCE,
      "{key}"
    );
  }
  Ok(())
}

#[test]
fn fused_faces_are_not_replaced_by_the_outer_composite_cycle() -> Result<()> {
  let (graph, layout) = authored("C1CC2CC12", &[(0., 0.), (1., 0.), (1., 1.), (0.5, 2.), (0., 1.)])?;
  let result = crate::metrics::measure(&graph, &layout)?;
  let rings = result
    .ring_geometry
    .ok_or_else(|| anyhow::anyhow!("missing ring details"))?;
  let mut sizes: Vec<_> = rings.faces.iter().map(|face| face.atoms.len()).collect();
  sizes.sort_unstable();
  assert_eq!(sizes, [3, 4]);
  Ok(())
}

#[test]
fn a_bond_inside_the_ring_obstructs_its_empty_cavity() -> Result<()> {
  let square = [
    Point::new(0., 0.),
    Point::new(4., 0.),
    Point::new(4., 4.),
    Point::new(0., 4.),
  ];
  let mut edges: Vec<_> = square
    .iter()
    .zip(square.iter().cycle().skip(1))
    .map(|(&a, &b)| (a, b))
    .collect();
  let empty = cavity(&square, &edges, CAVITY_CELL_BUDGET).0;
  edges.push((Point::new(2., 0.), Point::new(2., 4.)));
  let split = cavity(&square, &edges, CAVITY_CELL_BUDGET).0;
  assert!((empty - 2.).abs() < CAVITY_TOLERANCE && (split - 1.).abs() < CAVITY_TOLERANCE);
  Ok(())
}

#[test]
fn self_crossing_rings_are_unavailable_not_zero_sized() -> Result<()> {
  let (graph, layout) = authored("C1CCC1", &[(0., 0.), (1., 1.), (1., 0.), (0., 1.)])?;
  let result = crate::metrics::measure(&graph, &layout)?;
  assert_eq!(
    result.measurements["maximum_ring_cavity_radius"].state,
    State::NotImplemented
  );
  assert_eq!(result.measurements["ring_isoperimetric_ratio"].value, None);
  Ok(())
}

#[test]
fn acyclic_graphs_have_no_ring_measurements_but_keep_true_zero_footprint() -> Result<()> {
  let (graph, layout) = authored("CCC", &[(0., 0.), (1., 0.), (2., 0.)])?;
  let result = crate::metrics::measure(&graph, &layout)?;
  assert_eq!(
    result.measurements["maximum_ring_cavity_radius"].state,
    State::NotApplicable
  );
  assert_eq!(result.measurements["normalized_footprint"].value, Some(0.));
  Ok(())
}

#[test]
fn component_packing_does_not_inflate_footprint_or_fill_an_unrelated_ring() -> Result<()> {
  let (graph, layout) = authored(
    "C1CCC1.CC",
    &[(0., 0.), (4., 0.), (4., 4.), (0., 4.), (2., 0.), (2., 4.)],
  )?;
  let mut moved = layout.positions().clone();
  for (id, _) in graph.atoms().skip(4) {
    if let Some(p) = moved.get_mut(&id) {
      p.x += 1000.;
    }
  }
  let first = crate::metrics::measure(&graph, &layout)?;
  let second = crate::metrics::measure(&graph, &Layout2d::from_positions(moved)?)?;
  for key in ["maximum_ring_cavity_radius", "normalized_footprint"] {
    assert_eq!(first.measurements[key].value, second.measurements[key].value);
  }
  Ok(())
}

#[test]
fn cavity_search_reports_an_unfinished_upper_bound_on_budget_exhaustion() {
  let polygon = [
    Point::new(0., 0.),
    Point::new(1., 0.),
    Point::new(1., 1.),
    Point::new(0., 1.),
  ];
  let edges = polygon
    .iter()
    .zip(polygon.iter().cycle().skip(1))
    .map(|(&a, &b)| (a, b))
    .collect::<Vec<_>>();
  let (low, high) = cavity(&polygon, &edges, 1);
  assert!(high - low > CAVITY_TOLERANCE && low <= 0.5 && high >= 0.5);
}

#[test]
fn regular_macrocycle_cavity_matches_its_analytic_inradius() {
  let n = 24;
  let radius = 1. / (2. * (std::f64::consts::PI / n as f64).sin());
  let polygon: Vec<_> = (0..n)
    .map(|i| {
      let angle = 2. * std::f64::consts::PI * i as f64 / n as f64;
      Point::new(radius * angle.cos(), radius * angle.sin())
    })
    .collect();
  let edges = polygon
    .iter()
    .zip(polygon.iter().cycle().skip(1))
    .map(|(&a, &b)| (a, b))
    .collect::<Vec<_>>();
  let expected = radius * (std::f64::consts::PI / n as f64).cos();
  let (low, high) = cavity(&polygon, &edges, CAVITY_CELL_BUDGET);
  assert!(low <= expected + 1e-10 && high >= expected - 1e-10 && high - low <= CAVITY_TOLERANCE);
}

#[test]
fn concave_ring_search_does_not_assume_its_bounding_box_center_is_inside() {
  let polygon = [(0., 0.), (4., 0.), (4., 1.), (1., 1.), (1., 4.), (0., 4.)].map(|(x, y)| Point::new(x, y));
  let edges = polygon
    .iter()
    .zip(polygon.iter().cycle().skip(1))
    .map(|(&a, &b)| (a, b))
    .collect::<Vec<_>>();
  let (low, high) = cavity(&polygon, &edges, CAVITY_CELL_BUDGET);
  assert!(low > 0.5 && high - low <= CAVITY_TOLERANCE);
}

#[test]
fn perspective_layouts_are_excluded_from_planar_whitespace_metrics() -> Result<()> {
  let graph = read_smiles("C12C3C4C1C5C2C3C45")?.into_graph();
  let drawing = layout::generate(&graph, &LayoutOptions::default())?;
  let result = crate::metrics::measure(&graph, &drawing)?;
  for key in [
    "maximum_ring_cavity_radius",
    "ring_isoperimetric_ratio",
    "normalized_footprint",
  ] {
    assert_eq!(result.measurements[key].state, State::NotApplicable);
  }
  Ok(())
}
