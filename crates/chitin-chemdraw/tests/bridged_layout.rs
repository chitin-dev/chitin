//! Bridge-path layout regressions, independent of the fused-face solver.

use chitin_chem::{io::smiles::read_smiles, perception::analyze_rings};
use chitin_chemdraw::{
  depict,
  layout::{self, LayoutOptions},
  svg,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Substituted, unsaturated, heteroatom, spiro-entered and chain-linked bridges.
const CASES: &[&str] = &[
  "C1CC2CCC1C2",
  "CC1CC2COC(C1)O2",
  "CC1CC2(C(OC(C1)(O2)C)C)C",
  "C1=CC2CCC1C2",
  "N[C@H]1C[C@@H]2CC[C@@H]1C2",
  "C1CC2(CCC3CC4CCC3C4)CCC1C2",
  "C1CC2(CC1)CC3CCC2C3",
];

/// Tied shortest paths, including full symmetry and common heteroatom variants.
fn symmetric_cases() -> impl Iterator<Item = &'static str> {
  include_str!("fixtures/symmetric_bridges.smi")
    .lines()
    .filter(|line| !line.is_empty() && !line.starts_with('#'))
}

#[test]
fn bridge_paths_should_close_with_unit_bonds_before_and_after_refinement() -> TestResult {
  for &source in CASES {
    let graph = read_smiles(source)?.into_graph();
    let before = graph.clone();
    for passes in [0, 4] {
      let options = LayoutOptions {
        refinement_passes: passes,
        ..Default::default()
      };
      let coordinates = layout::generate(&graph, &options)?;
      assert_eq!(coordinates.positions().len(), graph.atom_count());
      assert!(
        coordinates.diagnostics().is_empty(),
        "{source}: {:?}",
        coordinates.diagnostics()
      );
      for (_, bond) in graph.bonds() {
        let [a, b] = bond.atoms();
        let p = coordinates.position(a).ok_or("missing endpoint")?;
        let q = coordinates.position(b).ok_or("missing endpoint")?;
        assert!(p.is_finite() && q.is_finite());
        assert!((p.distance(q) - 1.).abs() < 1e-8, "{source}: {a}-{b}");
      }
      assert_eq!(coordinates, layout::generate(&graph, &options)?);
    }
    assert_eq!(graph, before);
  }
  Ok(())
}

#[test]
fn bridge_depiction_should_preserve_graph_and_source_stereo_in_both_h_modes() -> TestResult {
  for source in CASES.iter().copied().chain(symmetric_cases()) {
    let graph = read_smiles(source)?.into_graph();
    let before = graph.clone();
    let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
    for mode in [
      depict::ChiralHydrogenMode::Implicit,
      depict::ChiralHydrogenMode::Explicit,
    ] {
      let drawing = depict::depict(
        &graph,
        &coordinates,
        &depict::DepictOptions {
          chiral_hydrogens: mode,
          ..Default::default()
        },
      )?;
      let document = svg::export(&drawing, &svg::SvgOptions::default())?;
      assert!(document.starts_with("<svg "));
      assert!(!document.contains("NaN"));
    }
    assert_eq!(graph, before);
  }
  Ok(())
}

#[test]
fn bridge_layout_should_charge_the_shared_ring_budget() -> TestResult {
  for source in ["CC1CC2COC(C1)O2", "C1C2CC1C2", "C1CC2CCC1CC2"] {
    let graph = read_smiles(source)?.into_graph();
    let before = graph.clone();
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
    assert_eq!(graph, before);
  }
  Ok(())
}

#[test]
fn projected_bridge_lengths_should_survive_refinement_and_not_enter_the_unit_optimizer() -> TestResult {
  use layout::optimization::{self, OptimizationError};
  use std::collections::BTreeSet;

  for source in ["C1C2CC1C2", "C1CC2CCC1CC2C3CCCCCCCCCCC3"] {
    let graph = read_smiles(source)?.into_graph();
    let initial = layout::generate(
      &graph,
      &LayoutOptions {
        refinement_passes: 0,
        ..Default::default()
      },
    )?;
    let refined = layout::generate(&graph, &LayoutOptions::default())?;
    let mut disabled = LayoutOptions::default();
    disabled.optimization.iterations = 0;
    assert_eq!(refined, layout::generate(&graph, &disabled)?);
    for (_, bond) in graph.bonds() {
      let [a, b] = bond.atoms();
      let length = |coordinates: &layout::Layout2d| -> Result<f64, &'static str> {
        Ok(
          coordinates
            .position(a)
            .ok_or("missing atom")?
            .distance(coordinates.position(b).ok_or("missing atom")?),
        )
      };
      assert!((length(&initial)? - length(&refined)?).abs() < 1e-8, "{source}");
    }
    assert!(matches!(
      optimization::optimize(&graph, &refined, &BTreeSet::new(), &Default::default()),
      Err(OptimizationError::NonUnitBond(_))
    ));
  }
  Ok(())
}

#[test]
fn equal_shortest_bridges_should_close_without_coincident_atoms_or_crossings() -> TestResult {
  for source in symmetric_cases() {
    let graph = read_smiles(source)?.into_graph();
    let before = graph.clone();
    for passes in [0, 4] {
      let options = LayoutOptions {
        refinement_passes: passes,
        ..Default::default()
      };
      let coordinates = layout::generate(&graph, &options)?;
      assert_eq!(coordinates.positions().len(), graph.atom_count());
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
      let mut shortened = 0;
      for (_, bond) in graph.bonds() {
        let [a, b] = bond.atoms();
        let length = coordinates
          .position(a)
          .ok_or("missing atom")?
          .distance(coordinates.position(b).ok_or("missing atom")?);
        assert!((0.55..=1. + 1e-8).contains(&length), "{source}: {length}");
        shortened += usize::from(length < 1. - 1e-8);
      }
      assert!(shortened >= 2, "{source}");
      assert_eq!(coordinates, layout::generate(&graph, &options)?);
    }
    assert_eq!(graph, before);
  }
  Ok(())
}

#[test]
fn unmatched_polycycles_should_still_report_the_typed_embedding_boundary() -> TestResult {
  for source in ["C1CC2C3CCC1C23", "C1=C2CC3CC1CC(C2)C3"] {
    let graph = read_smiles(source)?.into_graph();
    assert!(matches!(
      layout::generate(&graph, &LayoutOptions::default()),
      Err(layout::LayoutError::RingSystem { .. })
    ));
  }
  Ok(())
}

#[test]
fn reversed_edges_and_sparse_handles_should_keep_bridge_geometry() -> TestResult {
  for source in ["C.CC1CC2COC(C1)O2", "C.C1C2CC1C2", "C.C1CN2CCN1CC2", "C.CC1CC2CCC1CC2"] {
    let mut graph = read_smiles(source)?.into_graph();
    let first = graph.atoms().next().map(|(id, _)| id).ok_or("no atom")?;
    graph.remove_atom(first)?;
    let expected = layout::generate(&graph, &LayoutOptions::default())?;
    let mut reordered = graph.clone();
    let edges: Vec<_> = graph
      .bonds()
      .map(|(id, edge)| (id, edge.atoms(), edge.order()))
      .collect();
    for &(id, _, _) in &edges {
      reordered.remove_bond(id)?;
    }
    for &(_, [a, b], order) in edges.iter().rev() {
      reordered.add_bond(b, a, order)?;
    }
    let observed = layout::generate(&reordered, &LayoutOptions::default())?;
    for (&id, point) in expected.positions() {
      assert!(point.distance(observed.position(id).ok_or("missing atom")?) < 1e-8);
    }
  }
  Ok(())
}

#[test]
fn supplied_large_polycycle_should_generate_complete_svg() -> TestResult {
  let graph = read_smiles(include_str!("fixtures/large_bridged.smi").trim())?.into_graph();
  let before = graph.clone();
  assert!(graph.atom_count() > 150);
  assert!(
    analyze_rings(&graph)?
      .blocks()
      .iter()
      .any(|block| block.cycle_rank() == 2)
  );
  let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
  assert_eq!(coordinates.positions().len(), graph.atom_count());
  for (_, bond) in graph.bonds() {
    let [a, b] = bond.atoms();
    assert!(
      (coordinates
        .position(a)
        .ok_or("missing atom")?
        .distance(coordinates.position(b).ok_or("missing atom")?)
        - 1.)
        .abs()
        < 1e-8
    );
  }
  let drawing = depict::depict(&graph, &coordinates, &depict::DepictOptions::default())?;
  assert!(svg::export(&drawing, &svg::SvgOptions::default())?.starts_with("<svg "));
  assert_eq!(graph, before);
  Ok(())
}
