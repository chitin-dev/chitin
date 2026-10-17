//! Shared-edge ring-system regression cases and geometry invariants.

use chitin_chem::{BondOrder, io::smiles::read_smiles};
use chitin_chemdraw::{
  depict,
  layout::{self, LayoutOptions},
  svg,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[path = "support/layout_cases.rs"]
mod layout_cases;

/// Constructs a native carbon skeleton from ordered faces, retaining shared edges once.
///
/// # Parameters
/// - `faces`: zero-based vertices with implicit closure; all indices must be used.
///
/// # Returns
/// An editable single-bond graph with the same bare-carbon facts as parsed input.
/// This helper supplies topology only, never reference layout coordinates.
///
/// # Errors
/// Propagates fixture parsing and structural mutation failures.
fn skeleton(faces: &[Vec<usize>]) -> Result<chitin_chem::MolecularGraph, Box<dyn std::error::Error>> {
  let source = read_smiles("C")?.into_graph();
  let carbon = source
    .atoms()
    .next()
    .map(|(_, atom)| atom.clone())
    .ok_or("missing carbon")?;
  let count = faces.iter().flatten().max().ok_or("empty faces")? + 1;
  let mut graph = chitin_chem::MolecularGraph::new();
  let ids = (0..count)
    .map(|_| graph.add_atom(carbon.clone()))
    .collect::<Result<Vec<_>, _>>()?;
  for face in faces {
    for (&a, &b) in face.iter().zip(face.iter().cycle().skip(1)) {
      if graph.bond_between(ids[a], ids[b]).is_none() {
        graph.add_bond(ids[a], ids[b], BondOrder::Single)?;
      }
    }
  }
  Ok(graph)
}

#[test]
fn regression_corpus_should_preserve_unit_bonds_and_refined_quality() -> TestResult {
  for layout_cases::Case { name, source, rank } in layout_cases::load()? {
    let graph = read_smiles(source)?.into_graph();
    assert_eq!(graph.bond_count() + 1 - graph.atom_count(), rank, "{name}");
    let before = graph.clone();
    for passes in [0, 4] {
      let options = LayoutOptions {
        refinement_passes: passes,
        ..LayoutOptions::default()
      };
      let positions = layout::generate(&graph, &options)?;
      assert_eq!(positions.positions().len(), graph.atom_count(), "{name}");
      // Rigid fused blocks must close cleanly before refinement. Flexible
      // appendages in the functional-group cases may need the later search.
      if rank > 1 || passes > 0 {
        assert!(
          positions.diagnostics().is_empty(),
          "{name}: {:?}",
          positions.diagnostics()
        );
      }
      for (_, bond) in graph.bonds() {
        let [a, b] = bond.atoms();
        assert!(
          (positions
            .position(a)
            .ok_or("missing atom")?
            .distance(positions.position(b).ok_or("missing atom")?)
            - 1.)
            .abs()
            < 1e-8,
          "{name}: {a}-{b}"
        );
      }
      let drawing = depict::depict(&graph, &positions, &depict::DepictOptions::default())?;
      let output = svg::export(&drawing, &svg::SvgOptions::default())?;
      assert!(!output.contains("NaN") && !output.contains("inf"));
      assert_eq!(positions, layout::generate(&graph, &options)?);
    }
    assert_eq!(graph, before);
  }
  Ok(())
}

#[test]
fn six_membered_fused_centers_should_preserve_120_degree_sectors() -> TestResult {
  for layout_cases::Case { name, source, .. } in layout_cases::load()?.into_iter().take(2) {
    let graph = read_smiles(source)?.into_graph();
    let positions = layout::generate(&graph, &LayoutOptions::default())?;
    for (id, _) in graph.atoms() {
      let neighbors: Vec<_> = graph.neighbors(id)?.collect();
      if neighbors.len() != 3 {
        continue;
      }
      let center = positions.position(id).ok_or("missing center")?;
      for i in 0..3 {
        for j in i + 1..3 {
          let a = positions.position(neighbors[i].0).ok_or("missing neighbor")?;
          let b = positions.position(neighbors[j].0).ok_or("missing neighbor")?;
          let dot = (a.x - center.x) * (b.x - center.x) + (a.y - center.y) * (b.y - center.y);
          assert!((dot + 0.5).abs() < 1e-8, "{name}: center {id}");
        }
      }
    }
  }
  Ok(())
}

#[test]
fn three_faces_meeting_at_one_atom_should_check_all_known_closure_anchors() -> TestResult {
  let faces = [
    vec![0, 1, 2, 3, 4, 5],
    vec![1, 0, 6, 7, 8, 9],
    vec![5, 0, 6, 10, 11, 12],
  ];
  let graph = skeleton(&faces)?;
  let positions = layout::generate(&graph, &LayoutOptions::default())?;
  assert_eq!(positions.positions().len(), 13);
  assert!(positions.diagnostics().is_empty(), "{:?}", positions.diagnostics());
  for (_, bond) in graph.bonds() {
    let [a, b] = bond.atoms();
    assert!(
      (positions
        .position(a)
        .ok_or("missing atom")?
        .distance(positions.position(b).ok_or("missing atom")?)
        - 1.)
        .abs()
        < 1e-8
    );
  }
  Ok(())
}

#[test]
fn incompatible_regular_polygon_closures_should_not_be_stretched() -> TestResult {
  let graph = skeleton(&[vec![0, 1, 2, 3, 4], vec![1, 0, 5, 6, 7], vec![4, 0, 5, 8, 9]])?;
  let before = graph.clone();
  assert!(matches!(
    layout::generate(&graph, &LayoutOptions::default()),
    Err(layout::LayoutError::RingSystem { .. })
  ));
  assert_eq!(graph, before);
  Ok(())
}

#[test]
fn bond_insertion_order_and_endpoint_direction_should_not_change_fused_geometry() -> TestResult {
  for layout_cases::Case { source, .. } in layout_cases::load()?.into_iter().filter(|case| case.rank > 1) {
    let graph = read_smiles(source)?.into_graph();
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
    for (&id, &point) in expected.positions() {
      assert!(point.distance(observed.position(id).ok_or("missing atom")?) < 1e-8);
    }
  }
  Ok(())
}

#[test]
fn substituted_fused_systems_should_remain_rigid_before_external_branch_expansion() -> TestResult {
  for source in [
    "CC(=O)OC1=CC=C2C=C3C=CC=CC3=CC2=C1",
    "O=C(O)C1=CC=C2C=C3C=CC=CC3=CC2=C1",
    "C1CCC2(CC1)CCC1CCCCC12",
    "C1=CC=C2C=C3C=CC=CC3=CC2=C1C4CCCCC4",
    "C1CCCCC1C2(C)CC3CC4CCCCC4CC3C2",
  ] {
    let graph = read_smiles(source)?.into_graph();
    let positions = layout::generate(&graph, &LayoutOptions::default())?;
    assert_eq!(positions.positions().len(), graph.atom_count());
    assert!(
      positions.diagnostics().is_empty(),
      "{source}: {:?}",
      positions.diagnostics()
    );
    let scene = depict::depict(&graph, &positions, &depict::DepictOptions::default())?;
    assert!(!svg::export(&scene, &svg::SvgOptions::default())?.contains("NaN"));
  }
  Ok(())
}

#[test]
fn sparse_atom_handles_should_not_be_used_as_dense_cycle_indices() -> TestResult {
  let mut graph = read_smiles(&format!("C.{}", layout_cases::load()?[0].source))?.into_graph();
  let first = graph.atoms().next().map(|(id, _)| id).ok_or("missing atom")?;
  graph.remove_atom(first)?;
  let positions = layout::generate(&graph, &LayoutOptions::default())?;
  assert_eq!(positions.positions().len(), graph.atom_count());
  assert!(positions.diagnostics().is_empty());
  Ok(())
}

#[test]
fn ring_work_budget_should_fail_explicitly_and_remain_shared_across_fragments() -> TestResult {
  let options = LayoutOptions {
    ring_search_budget: 1000,
    ..LayoutOptions::default()
  };
  let single = read_smiles("C1CC1")?.into_graph();
  assert!(layout::generate(&single, &options).is_ok());
  let many = read_smiles(&["C1CC1"; 20].join("."))?.into_graph();
  let before = many.clone();
  assert_eq!(
    layout::generate(&many, &options),
    Err(layout::LayoutError::RingSearchBudgetExceeded)
  );
  assert_eq!(many, before);
  let invalid = LayoutOptions {
    ring_search_budget: 0,
    ..LayoutOptions::default()
  };
  assert_eq!(layout::generate(&single, &invalid), Err(layout::LayoutError::Options));
  Ok(())
}

#[test]
fn long_fused_strips_should_not_have_a_fixed_ring_count_limit() -> TestResult {
  let mut faces = vec![vec![0, 1, 2, 3, 4, 5]];
  for i in 1..128 {
    let previous = &faces[i - 1];
    let start = 6 + (i - 1) * 4;
    faces.push(vec![previous[4], previous[3], start, start + 1, start + 2, start + 3]);
  }
  let graph = skeleton(&faces)?;
  let positions = layout::generate(&graph, &LayoutOptions::default())?;
  assert_eq!(positions.positions().len(), graph.atom_count());
  assert!(positions.diagnostics().is_empty(), "{:?}", positions.diagnostics());
  Ok(())
}
