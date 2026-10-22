//! Whole-block embedding regressions retain original source topology and stereo.

use chitin_chem::{MolecularGraph, io::smiles::read_smiles};
use chitin_chemdraw::{
  depict, layout,
  scene::{Primitive, Source},
  svg,
};
use std::collections::BTreeSet;

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// The reported failing examples, independent of the batch configuration order.
fn cases() -> impl Iterator<Item = &'static str> {
  include_str!("fixtures/complex_rings.smi")
    .lines()
    .filter(|line| !line.starts_with('#') && !line.is_empty())
}

/// Includes ring-closing edges, not only traversal-tree connections.
fn units(graph: &MolecularGraph, coordinates: &layout::Layout2d) -> TestResult {
  for (_, bond) in graph.bonds() {
    let [a, b] = bond.atoms();
    let a = coordinates.position(a).ok_or("missing endpoint")?;
    let b = coordinates.position(b).ok_or("missing endpoint")?;
    assert!((a.distance(b) - 1.).abs() <= 1e-9);
  }
  Ok(())
}

#[test]
fn complex_ring_inputs_should_generate_complete_uncrowded_drawings() -> TestResult {
  for (i, source) in cases().enumerate() {
    let graph = read_smiles(source)?.into_graph();
    let before = graph.clone();
    let coordinates = layout::generate(&graph, &Default::default())?;
    assert!(
      coordinates.diagnostics().is_empty(),
      "case {}: {:?}",
      i + 7,
      coordinates.diagnostics()
    );
    assert!(coordinates.bond_crossings().is_empty());
    assert_eq!(coordinates.positions().len(), graph.atom_count());
    units(&graph, &coordinates)?;
    let drawing = depict::depict(&graph, &coordinates, &Default::default())?;
    let mut bonds = BTreeSet::new();
    for primitive in drawing.primitives() {
      let source = match primitive {
        Primitive::Line(line) => line.source,
        Primitive::Polygon(polygon) => polygon.source,
        Primitive::Circle(circle) => circle.source,
        Primitive::Text(text) => text.source,
        _ => return Err("unexpected primitive".into()),
      };
      if let Some(Source::Bond(id)) = source {
        bonds.insert(id);
      }
    }
    assert_eq!(bonds, graph.bonds().map(|(id, _)| id).collect());
    let output = svg::export(&drawing, &Default::default())?;
    assert!(!output.contains("NaN") && !output.contains("inf"));
    assert_eq!(graph, before);
    assert_eq!(coordinates, layout::generate(&graph, &Default::default())?);
  }
  Ok(())
}

#[test]
fn sparse_source_handles_should_not_change_complex_ring_coordinates() -> TestResult {
  for source in cases() {
    let graph = read_smiles(source)?.into_graph();
    let expected = layout::generate(&graph, &Default::default())?;
    let mut sparse = read_smiles(&format!("C.{source}"))?.into_graph();
    let removed = sparse.atoms().next().map(|(id, _)| id).ok_or("empty graph")?;
    sparse.remove_atom(removed)?;
    let observed = layout::generate(&sparse, &Default::default())?;
    assert!(observed.diagnostics().is_empty());
    for ((_, a), (_, b)) in expected.positions().iter().zip(observed.positions()) {
      assert!(a.distance(*b) < 1e-8);
    }
  }
  Ok(())
}

#[test]
fn complex_ring_search_should_respect_work_limits_without_partial_output() -> TestResult {
  for source in cases().take(2) {
    let graph = read_smiles(source)?.into_graph();
    let before = graph.clone();
    assert!(matches!(
      layout::generate(
        &graph,
        &layout::LayoutOptions {
          ring_search_budget: 100,
          fallback_budget: 0,
          ..Default::default()
        }
      ),
      Err(layout::LayoutError::RingSearchBudgetExceeded)
    ));
    let coordinates = layout::generate(
      &graph,
      &layout::LayoutOptions {
        refinement_passes: 0,
        ..Default::default()
      },
    )?;
    assert_eq!(coordinates.positions().len(), graph.atom_count());
    units(&graph, &coordinates)?;
    depict::depict(&graph, &coordinates, &Default::default())?;
    assert_eq!(graph, before);
  }
  Ok(())
}

#[test]
fn cross_linked_aromatic_islands_should_keep_regular_local_angles() -> TestResult {
  let graph = read_smiles(cases().next().ok_or("missing fixture")?)?.into_graph();
  let coordinates = layout::generate(&graph, &Default::default())?;
  for (id, atom) in graph.atoms().filter(|(_, atom)| atom.is_aromatic()) {
    let neighbors: Vec<_> = graph
      .neighbors(id)?
      .filter_map(|(other, edge)| {
        graph
          .bond(edge)
          .filter(|bond| bond.order() == chitin_chem::BondOrder::Aromatic)
          .map(|_| other)
      })
      .collect();
    assert_eq!(neighbors.len(), 2, "{:?}", atom.element());
    let center = coordinates.position(id).ok_or("missing center")?;
    let a = coordinates.position(neighbors[0]).ok_or("missing neighbor")?;
    let b = coordinates.position(neighbors[1]).ok_or("missing neighbor")?;
    let cosine = ((a.x - center.x) * (b.x - center.x) + (a.y - center.y) * (b.y - center.y))
      / (a.distance(center) * b.distance(center));
    assert!((cosine + 0.5).abs() < 1e-7, "{id:?}: {cosine}");
  }
  Ok(())
}

#[test]
fn optional_folding_should_not_consume_the_required_placement_reserve() -> TestResult {
  let graph = read_smiles(cases().next().ok_or("missing fixture")?)?.into_graph();
  for budget in [2_000_000, 2_100_000, 2_200_000, 2_300_000, 4_000_000] {
    let coordinates = layout::generate(
      &graph,
      &layout::LayoutOptions {
        ring_search_budget: budget,
        refinement_passes: 0,
        ..Default::default()
      },
    )
    .map_err(|error| format!("budget {budget}: {error}"))?;
    assert_eq!(coordinates.positions().len(), graph.atom_count());
    units(&graph, &coordinates)?;
  }
  Ok(())
}

#[test]
fn optional_folding_should_wait_until_all_components_have_closed_ring_seeds() -> TestResult {
  let source = cases().next().ok_or("missing fixture")?;
  let graph = read_smiles(&format!("{source}.{source}"))?.into_graph();
  let coordinates = layout::generate(
    &graph,
    &layout::LayoutOptions {
      ring_search_budget: 4_000_000,
      refinement_passes: 0,
      ..Default::default()
    },
  )?;
  assert_eq!(coordinates.positions().len(), graph.atom_count());
  units(&graph, &coordinates)?;
  Ok(())
}
