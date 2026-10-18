//! Macrocycle closure, constraint, and resource regressions through public APIs.

use chitin_chem::{DoubleBondRelation, MolecularGraph, io::smiles::read_smiles};
use chitin_chemdraw::{
  depict,
  layout::{self, Layout2d, LayoutOptions},
  svg,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Verifies every edge, including ring closure and external attachments.
fn unit_edges(graph: &MolecularGraph, coordinates: &Layout2d) -> TestResult {
  assert_eq!(coordinates.positions().len(), graph.atom_count());
  for (_, bond) in graph.bonds() {
    let [a, b] = bond.atoms();
    let a = coordinates.position(a).ok_or("missing atom")?;
    let b = coordinates.position(b).ok_or("missing atom")?;
    assert!((a.distance(b) - 1.).abs() < 1e-8, "edge length {}", a.distance(b));
  }
  Ok(())
}

#[test]
fn even_macrocycles_should_have_visible_bends_and_exact_closed_unit_edges() -> TestResult {
  for size in (12..=40).step_by(2) {
    let graph = read_smiles(&format!("C1{}1", "C".repeat(size - 1)))?.into_graph();
    let before = graph.clone();
    for passes in [0, 4] {
      let options = LayoutOptions {
        refinement_passes: passes,
        ..Default::default()
      };
      let coordinates = layout::generate(&graph, &options).map_err(|error| format!("size {size}: {error}"))?;
      unit_edges(&graph, &coordinates)?;
      assert!(
        coordinates.diagnostics().is_empty(),
        "size {size}: {:?}",
        coordinates.diagnostics()
      );
      for (id, _) in graph.atoms() {
        let p = coordinates.position(id).ok_or("missing center")?;
        let neighbors: Vec<_> = graph.neighbors(id)?.map(|(id, _)| coordinates.position(id)).collect();
        let a = neighbors[0].ok_or("missing neighbor")?;
        let b = neighbors[1].ok_or("missing neighbor")?;
        let dot = (a.x - p.x) * (b.x - p.x) + (a.y - p.y) * (b.y - p.y);
        assert!((dot + 0.5).abs() < 1e-8, "size {size}: bond angle cosine {dot}");
      }
      assert_eq!(coordinates, layout::generate(&graph, &options)?);
    }
    assert_eq!(graph, before);
  }
  Ok(())
}

#[test]
fn cyclic_double_bond_candidates_should_preserve_source_and_exocyclic_references() -> TestResult {
  for source in [
    "C/1=C\\CCCCCCCCCC1",
    "C/1=C/CCCCCCCCCC1",
    "F/C1=C(/Cl)CCCCCCCCCC1",
    "F/C1=C(\\Cl)CCCCCCCCCC1",
    "C/1=C\\CC/C=C/CCCCCC1",
  ] {
    let graph = read_smiles(source)?.into_graph();
    let before = graph.clone();
    assert!(graph.double_bond_stereos().next().is_some());
    for passes in [0, 4] {
      let coordinates = layout::generate(
        &graph,
        &LayoutOptions {
          refinement_passes: passes,
          ..Default::default()
        },
      )
      .map_err(|error| format!("{source}: {error}"))?;
      unit_edges(&graph, &coordinates)?;
      for (_, stereo) in graph.double_bond_stereos() {
        let [a, b] = stereo.ends();
        let a = coordinates.position(a).ok_or("missing axis")?;
        let b = coordinates.position(b).ok_or("missing axis")?;
        let [ra, rb] = stereo.references();
        let side = |id| -> Result<f64, Box<dyn std::error::Error>> {
          let r = coordinates.position(id).ok_or("missing reference")?;
          Ok((b.x - a.x) * (r.y - a.y) - (b.y - a.y) * (r.x - a.x))
        };
        let product = side(ra)? * side(rb)?;
        assert!(product.abs() > 1e-8);
        assert_eq!(
          product > 0.,
          stereo.relation() == DoubleBondRelation::SameSide,
          "{source}"
        );
      }
      let drawing = depict::depict(&graph, &coordinates, &Default::default())?;
      assert!(!svg::export(&drawing, &Default::default())?.contains("NaN"));
    }
    assert_eq!(graph, before);
  }
  Ok(())
}

#[test]
fn macrocycles_should_support_linked_and_spiro_entry_without_moving_fixed_anchors() -> TestResult {
  for source in [
    "OC1CCCCCCCCCCC1N",
    "C1CCCCC1CCC2CCCCCCCCCCC2",
    "C1CCC2(CC1)CCCCCCCCCCC2",
    "C1CCCCCCCCCCC1.CC",
  ] {
    let graph = read_smiles(source)?.into_graph();
    let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
    unit_edges(&graph, &coordinates)?;
    assert_eq!(coordinates, layout::generate(&graph, &LayoutOptions::default())?);
    let drawing = depict::depict(&graph, &coordinates, &Default::default())?;
    svg::export(&drawing, &Default::default())?;
  }
  Ok(())
}

#[test]
fn odd_and_small_rings_should_keep_the_existing_regular_polygon_model() -> TestResult {
  for size in [6, 10, 13, 17] {
    let graph = read_smiles(&format!("C1{}1", "C".repeat(size - 1)))?.into_graph();
    let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
    unit_edges(&graph, &coordinates)?;
    let expected = -(std::f64::consts::TAU / size as f64).cos();
    for (id, _) in graph.atoms() {
      let p = coordinates.position(id).ok_or("missing center")?;
      let neighbors: Vec<_> = graph.neighbors(id)?.map(|(id, _)| coordinates.position(id)).collect();
      let a = neighbors[0].ok_or("missing neighbor")?;
      let b = neighbors[1].ok_or("missing neighbor")?;
      assert!(((a.x - p.x) * (b.x - p.x) + (a.y - p.y) * (b.y - p.y) - expected).abs() < 1e-8);
    }
  }
  Ok(())
}

#[test]
fn macrocycle_candidate_search_should_obey_the_shared_ring_budget() -> TestResult {
  let graph = read_smiles("C1CCCCCCCCCCC1")?.into_graph();
  let before = graph.clone();
  for budget in [100, 1000] {
    assert!(matches!(
      layout::generate(
        &graph,
        &LayoutOptions {
          ring_search_budget: budget,
          ..Default::default()
        }
      ),
      Err(layout::LayoutError::RingSearchBudgetExceeded)
    ));
  }
  assert_eq!(graph, before);
  Ok(())
}

#[test]
fn macrocycle_geometry_should_not_depend_on_edge_insertion_order_or_dense_handles() -> TestResult {
  let mut graph = read_smiles("C.OC1CCCCCCCCCCC1N")?.into_graph();
  let first = graph.atoms().next().map(|(id, _)| id).ok_or("no atom")?;
  graph.remove_atom(first)?;
  let expected = layout::generate(&graph, &LayoutOptions::default())?;
  let edges: Vec<_> = graph
    .bonds()
    .map(|(id, edge)| (id, edge.atoms(), edge.order()))
    .collect();
  for &(id, _, _) in &edges {
    graph.remove_bond(id)?;
  }
  for &(_, [a, b], order) in edges.iter().rev() {
    graph.add_bond(b, a, order)?;
  }
  let observed = layout::generate(&graph, &LayoutOptions::default())?;
  for (&id, p) in expected.positions() {
    assert!(p.distance(observed.position(id).ok_or("missing atom")?) < 1e-8);
  }
  unit_edges(&graph, &observed)
}
