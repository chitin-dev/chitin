//! Complete cage projections preserve source topology and never infer stereo.

use std::collections::BTreeSet;

use chitin_chem::{MolecularGraph, io::smiles::read_smiles};
use chitin_chemdraw::{
  depict::{self, ChiralHydrogenMode, DepictOptions},
  layout::{self, LayoutOptions, optimization},
  scene::{Primitive, Source},
  svg,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Locally authored inputs, not a promise of general cage-graph embedding.
fn cases() -> impl Iterator<Item = &'static str> {
  include_str!("fixtures/cages.smi")
    .lines()
    .filter(|line| !line.is_empty() && !line.starts_with('#'))
}

#[test]
fn supported_cores_and_derivatives_should_have_complete_distinct_noncrossing_projections() -> TestResult {
  for source in cases() {
    let graph = read_smiles(source)?.into_graph();
    let before = graph.clone();
    for passes in [0, 4] {
      let options = LayoutOptions {
        refinement_passes: passes,
        ..Default::default()
      };
      let coordinates = layout::generate(&graph, &options)?;
      assert_eq!(coordinates.positions().len(), graph.atom_count(), "{source}");
      assert!(
        coordinates.diagnostics().is_empty(),
        "{source}: {:?}",
        coordinates.diagnostics()
      );
      let points: Vec<_> = coordinates.positions().values().collect();
      for (i, point) in points.iter().enumerate() {
        assert!(point.is_finite());
        assert!(
          points[i + 1..]
            .iter()
            .all(|other| point.distance(**other) >= 0.55 - 1e-8),
          "{source}"
        );
      }
      for (_, bond) in graph.bonds() {
        let [a, b] = bond.atoms();
        let length = coordinates
          .position(a)
          .ok_or("missing atom")?
          .distance(coordinates.position(b).ok_or("missing atom")?);
        assert!((0.55..=1.8).contains(&length), "{source}: {length}");
      }
      assert_eq!(coordinates, layout::generate(&graph, &options)?);
    }
    assert_eq!(graph, before);
  }
  Ok(())
}

#[test]
fn cage_depiction_should_keep_all_edge_sources_and_only_source_supplied_stereo() -> TestResult {
  for source in cases() {
    let graph = read_smiles(source)?.into_graph();
    let before = graph.clone();
    let coordinates = layout::generate(&graph, &Default::default())?;
    for mode in [ChiralHydrogenMode::Implicit, ChiralHydrogenMode::Explicit] {
      let drawing = depict::depict(
        &graph,
        &coordinates,
        &DepictOptions {
          chiral_hydrogens: mode,
          ..Default::default()
        },
      )?;
      let mut edges = BTreeSet::new();
      for primitive in drawing.primitives() {
        let source = match primitive {
          Primitive::Line(line) => line.source,
          Primitive::Polygon(polygon) => {
            assert!(graph.tetrahedral_centers().next().is_some());
            polygon.source
          }
          Primitive::Text(text) => text.source,
          Primitive::Circle(circle) => circle.source,
          _ => return Err("unexpected scene primitive".into()),
        };
        if let Some(Source::Bond(id)) = source {
          assert!(graph.bond(id).is_some());
          edges.insert(id);
        }
      }
      assert_eq!(edges.len(), graph.bond_count(), "{source}");
      let document = svg::export(&drawing, &Default::default())?;
      assert!(document.starts_with("<svg ") && !document.contains("NaN"));
    }
    assert_eq!(graph, before);
  }
  Ok(())
}

#[test]
fn matching_should_ignore_edge_order_endpoint_direction_and_sparse_handles() -> TestResult {
  for source in cases() {
    let mut graph = read_smiles(&format!("C.{source}"))?.into_graph();
    let first = graph.atoms().next().map(|(id, _)| id).ok_or("no atom")?;
    graph.remove_atom(first)?;
    let expected = layout::generate(&graph, &Default::default())?;
    let stereo: Vec<_> = graph.tetrahedral_centers().map(|(id, s)| (id, *s)).collect();
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
    for (id, s) in stereo {
      graph.set_tetrahedral(id, Some(s))?;
    }
    let observed = layout::generate(&graph, &Default::default())?;
    for (&id, point) in expected.positions() {
      assert!(
        point.distance(observed.position(id).ok_or("missing atom")?) < 1e-8,
        "{source}"
      );
    }
  }
  Ok(())
}

#[test]
fn changing_atom_allocation_order_should_not_disable_core_matching() -> TestResult {
  for source in ["C1C2CC3CC1CC(C2)C3", "C12C3C4C1C5C2C3C45"] {
    let graph = read_smiles(source)?.into_graph();
    let atoms: Vec<_> = graph.atoms().collect();
    let mut reordered = MolecularGraph::new();
    let mut ids = std::collections::BTreeMap::new();
    for &(id, atom) in atoms.iter().rev() {
      ids.insert(id, reordered.add_atom(atom.clone())?);
    }
    for (_, edge) in graph.bonds() {
      let [a, b] = edge.atoms();
      reordered.add_bond(ids[&a], ids[&b], edge.order())?;
    }
    let coordinates = layout::generate(&reordered, &Default::default())?;
    assert_eq!(coordinates.positions().len(), graph.atom_count());
    assert!(coordinates.diagnostics().is_empty());
  }
  Ok(())
}

#[test]
fn cage_search_should_respect_budgets_and_keep_unit_optimizer_contract() -> TestResult {
  for source in ["C1C2CC3CC1CC(C2)C3", "C12C3C4C1C5C2C3C45"] {
    let graph = read_smiles(source)?.into_graph();
    assert!(matches!(
      layout::generate(
        &graph,
        &LayoutOptions {
          ring_search_budget: 100,
          ..Default::default()
        }
      ),
      Err(layout::LayoutError::RingSearchBudgetExceeded)
    ));
    let initial = layout::generate(
      &graph,
      &LayoutOptions {
        refinement_passes: 0,
        ..Default::default()
      },
    )?;
    let refined = layout::generate(&graph, &Default::default())?;
    for (_, edge) in graph.bonds() {
      let [a, b] = edge.atoms();
      let length = |coordinates: &layout::Layout2d| -> Result<f64, &'static str> {
        Ok(
          coordinates
            .position(a)
            .ok_or("missing atom")?
            .distance(coordinates.position(b).ok_or("missing atom")?),
        )
      };
      assert!((length(&initial)? - length(&refined)?).abs() < 1e-8);
    }
    assert!(matches!(
      optimization::optimize(&graph, &refined, &BTreeSet::new(), &Default::default()),
      Err(optimization::OptimizationError::NonUnitBond(_))
    ));
  }
  Ok(())
}
