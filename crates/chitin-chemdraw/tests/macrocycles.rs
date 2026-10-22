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
        // Mixed auxiliary faces also produce composite reentrant angles;
        // every corner must remain visibly bent, not a parity shortcut.
        assert!(
          (-3_f64.sqrt() / 2. - 1e-8..=0.5 + 1e-8).contains(&dot),
          "size {size}: bond angle cosine {dot}"
        );
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
    "C/1=C\\CCCCCCCCCCC1",
    "C/1=C/CCCCCCCCCCC1",
    "F/C1=C(/Cl)CCCCCCCCCCC1",
    "F/C1=C(\\Cl)CCCCCCCCCCC1",
    "C/1=C\\CC/C=C/CCCCCCC1",
    "C/1=C\\CC/C=C/CCCCCCCCC1",
    "C/1=C\\CC/C=C/CC/C=C\\CCCCC1",
    "C1CCCCCCCCCC/C2CCCC\\C1=2",
    "C1CCCCCCCCCC/C2CCCC/C1=2",
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
    "OC1CCCCCCCCCCCC1N",
    "C1CCCCC1CCC2CCCCCCCCCCCC2",
    "C1CCC2(CC1)CCCCCCCCCCCC2",
    "C1CCCCCCCCCCCC1.CC",
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
fn small_rings_should_keep_the_existing_regular_polygon_model() -> TestResult {
  for size in 3..=8 {
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
fn nine_through_eleven_membered_rings_should_use_closed_reentrant_outlines() -> TestResult {
  for size in 9..=11 {
    let graph = read_smiles(&format!("C1{}1", "C".repeat(size - 1)))?.into_graph();
    let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
    unit_edges(&graph, &coordinates)?;
    assert!(coordinates.diagnostics().is_empty());
    let points: Vec<_> = graph
      .atoms()
      .map(|(id, _)| coordinates.position(id).ok_or("missing center"))
      .collect::<Result<_, _>>()?;
    let turns: Vec<_> = (0..size)
      .map(|i| {
        let a = points[(i + size - 1) % size];
        let c = points[i];
        let b = points[(i + 1) % size];
        (c.x - a.x) * (b.y - c.y) - (c.y - a.y) * (b.x - c.x)
      })
      .collect();
    assert!(turns.iter().any(|&turn| turn > 1e-8));
    assert!(turns.iter().any(|&turn| turn < -1e-8));
  }
  Ok(())
}

#[test]
fn odd_macrocycles_should_have_unit_closure_and_visible_nonregular_bends() -> TestResult {
  for size in (13..=41).step_by(2) {
    let graph = read_smiles(&format!("C1{}1", "C".repeat(size - 1)))?.into_graph();
    let before = graph.clone();
    for passes in [0, 4] {
      let options = LayoutOptions {
        refinement_passes: passes,
        ..Default::default()
      };
      let coordinates = layout::generate(&graph, &options).map_err(|e| format!("size {size}: {e}"))?;
      unit_edges(&graph, &coordinates)?;
      assert!(
        coordinates.diagnostics().is_empty(),
        "size {size}: {:?}",
        coordinates.diagnostics()
      );
      let mut cosines = Vec::new();
      for (id, _) in graph.atoms() {
        let p = coordinates.position(id).ok_or("missing center")?;
        let neighbors: Vec<_> = graph.neighbors(id)?.map(|(id, _)| coordinates.position(id)).collect();
        let a = neighbors[0].ok_or("missing neighbor")?;
        let b = neighbors[1].ok_or("missing neighbor")?;
        let cosine = (a.x - p.x) * (b.x - p.x) + (a.y - p.y) * (b.y - p.y);
        assert!((-0.9..=0.5).contains(&cosine), "size {size}: {cosine}");
        cosines.push(cosine);
      }
      assert!(cosines.iter().any(|v| (v - cosines[0]).abs() > 1e-6));
      assert_eq!(coordinates, layout::generate(&graph, &options)?);
    }
    assert_eq!(graph, before);
  }
  Ok(())
}

#[test]
fn macrocycle_candidate_search_should_obey_the_shared_ring_budget() -> TestResult {
  for source in ["C1CCCCCCCCCCC1", "C1CCCCCCCCCCCC1", "C1CCCCCCCCCCC2CCCCC12"] {
    let graph = read_smiles(source)?.into_graph();
    let before = graph.clone();
    for budget in [100, 1000] {
      assert!(matches!(
        layout::generate(
          &graph,
          &LayoutOptions {
            ring_search_budget: budget,
            fallback_budget: 0,
            ..Default::default()
          }
        ),
        Err(layout::LayoutError::RingSearchBudgetExceeded)
      ));
    }
    assert_eq!(graph, before);
  }
  Ok(())
}

#[test]
fn macrocycle_geometry_should_not_depend_on_edge_insertion_order_or_dense_handles() -> TestResult {
  for source in ["C.OC1CCCCCCCCCCC1N", "C.OC1CCCCCCCCCCCC1N", "C.C1CCCCCCCCCCC2CCCCC12"] {
    let mut graph = read_smiles(source)?.into_graph();
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
    unit_edges(&graph, &observed)?;
  }
  Ok(())
}

#[test]
fn one_small_fused_face_should_preserve_macrocycle_bends_and_every_shared_edge() -> TestResult {
  for size in 12..=21 {
    let source = format!("C1{}2CCCCC12", "C".repeat(size - 2));
    let graph = read_smiles(&source)?.into_graph();
    let before = graph.clone();
    for passes in [0, 4] {
      let options = LayoutOptions {
        refinement_passes: passes,
        ..Default::default()
      };
      let coordinates = layout::generate(&graph, &options).map_err(|e| format!("{source}: {e}"))?;
      unit_edges(&graph, &coordinates)?;
      assert!(
        coordinates.diagnostics().is_empty(),
        "{source}: {:?}",
        coordinates.diagnostics()
      );
      assert_eq!(coordinates, layout::generate(&graph, &options)?);
      let drawing = depict::depict(&graph, &coordinates, &Default::default())?;
      assert!(!svg::export(&drawing, &Default::default())?.contains("NaN"));
    }
    assert_eq!(graph, before);
  }
  Ok(())
}

#[test]
fn fused_macrocycles_should_cover_supported_small_face_sizes() -> TestResult {
  for size in [12, 13, 15] {
    for cap in 3..=8 {
      let source = format!("C1{}2{}12", "C".repeat(size - 2), "C".repeat(cap - 1));
      let graph = read_smiles(&source)?.into_graph();
      let coordinates = layout::generate(&graph, &LayoutOptions::default()).map_err(|e| format!("{source}: {e}"))?;
      unit_edges(&graph, &coordinates)?;
      assert!(
        coordinates.diagnostics().is_empty(),
        "{source}: {:?}",
        coordinates.diagnostics()
      );
    }
  }
  Ok(())
}

#[test]
fn mixed_turn_candidates_should_satisfy_the_previously_unavailable_shared_double_relation() -> TestResult {
  let graph = read_smiles("C1CCCCCCCCCC/C2CCCC/C1=2")?.into_graph();
  let before = graph.clone();
  let positions = layout::generate(&graph, &LayoutOptions::default())?;
  unit_edges(&graph, &positions)?;
  depict::depict(&graph, &positions, &Default::default())?;
  assert_eq!(graph, before);
  Ok(())
}
