//! Multi-stage embedding contracts, without insertion-order-dependent snapshots.

use chitin_chem::io::smiles::read_smiles;
use chitin_chemdraw::{
  depict,
  layout::{self, LayoutDiagnostic, LayoutOptions},
  svg,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn acyclic_carbonyl_centers_should_keep_three_120_degree_sectors() -> TestResult {
  for source in ["CC(=O)OC1=CC=CC=C1C(=O)O", "CC(=O)C", "O=C(O)C1=CC=CC=C1", "CC(=O)OC"] {
    let graph = read_smiles(source)?.into_graph();
    for passes in [0, 4] {
      let options = LayoutOptions {
        refinement_passes: passes,
        ..LayoutOptions::default()
      };
      let coordinates = layout::generate(&graph, &options)?;
      for (id, atom) in graph.atoms() {
        let neighbors: Vec<_> = graph.neighbors(id)?.collect();
        if atom.element() != chitin_chem::Element::Carbon
          || neighbors.len() != 3
          || !neighbors.iter().any(|&(_, edge)| {
            graph
              .bond(edge)
              .is_some_and(|bond| bond.order() == chitin_chem::BondOrder::Double)
          })
        {
          continue;
        }
        let center = coordinates.position(id).ok_or("missing center")?;
        for i in 0..3 {
          for j in i + 1..3 {
            let a = coordinates.position(neighbors[i].0).ok_or("missing neighbor")?;
            let b = coordinates.position(neighbors[j].0).ok_or("missing neighbor")?;
            let cosine = ((a.x - center.x) * (b.x - center.x) + (a.y - center.y) * (b.y - center.y))
              / (a.distance(center) * b.distance(center));
            assert!(
              (cosine + 0.5).abs() < 1e-8,
              "{source}, passes={passes}, center={id}, cosine={cosine}"
            );
          }
        }
      }
    }
  }
  Ok(())
}

#[test]
fn refinement_should_preserve_every_initial_local_bond_angle() -> TestResult {
  for source in [
    "CC(=O)OC1=CC=CC=C1C(=O)O",
    "CC(C)(CC1CCCCC1)CC2CCCCC2",
    "C(CC)(CC)(CC)(CC)(CC)CC",
  ] {
    let graph = read_smiles(source)?.into_graph();
    let initial = layout::generate(
      &graph,
      &LayoutOptions {
        refinement_passes: 0,
        ..LayoutOptions::default()
      },
    )?;
    let refined = layout::generate(&graph, &LayoutOptions::default())?;
    for (id, _) in graph.atoms() {
      let neighbors: Vec<_> = graph.neighbors(id)?.collect();
      for i in 0..neighbors.len() {
        for j in i + 1..neighbors.len() {
          let mut cosines = Vec::new();
          for coordinates in [&initial, &refined] {
            let center = coordinates.position(id).ok_or("missing center")?;
            let a = coordinates.position(neighbors[i].0).ok_or("missing neighbor")?;
            let b = coordinates.position(neighbors[j].0).ok_or("missing neighbor")?;
            cosines.push(
              ((a.x - center.x) * (b.x - center.x) + (a.y - center.y) * (b.y - center.y))
                / (a.distance(center) * b.distance(center)),
            );
          }
          assert!((cosines[0] - cosines[1]).abs() < 1e-8, "{source}: center {id}");
        }
      }
    }
  }
  Ok(())
}

#[test]
fn linked_spiro_and_fused_rings_should_preserve_every_unit_bond() -> TestResult {
  for source in [
    "C1CCCCC1C2CCCCC2",
    "C1CCCCC1CCC2CCCC2",
    "C1=CC=CC=C1C2=CC=CC=C2",
    "C1CCC2(CC1)CCCC2",
    "C1CC2(CC1)CC3(CC2)CCCC3",
    "C1CCC2CCCCC2C1",
    "C1CC2CCC2C1",
    "C1=CC2=CC=CC=C2C=C1",
    "OC1CCC2CCCCC2C1C#N",
    "C1CCC2CCCCC2C1.C1CC1",
  ] {
    let graph = read_smiles(source)?.into_graph();
    let before = graph.clone();
    let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
    assert_eq!(coordinates.positions().len(), graph.atom_count());
    for (_, bond) in graph.bonds() {
      let [a, b] = bond.atoms();
      let distance = coordinates
        .position(a)
        .ok_or("missing atom")?
        .distance(coordinates.position(b).ok_or("missing atom")?);
      assert!(
        (distance - 1.).abs() < 1e-8,
        "{source}: bond {a}-{b} has length {distance}"
      );
    }
    let scene = depict::depict(&graph, &coordinates, &depict::DepictOptions::default())?;
    assert!(!svg::export(&scene, &svg::SvgOptions::default())?.contains("NaN"));
    assert_eq!(coordinates, layout::generate(&graph, &LayoutOptions::default())?);
    assert_eq!(graph, before);
  }
  Ok(())
}

#[test]
fn ordinary_linked_and_fused_rings_should_not_overlap() -> TestResult {
  for source in [
    "C1CCCCC1C2CCCCC2",
    "C1CCC2(CC1)CCCC2",
    "C1CCC2CCCCC2C1",
    "C1=CC2=CC=CC=C2C=C1",
  ] {
    let graph = read_smiles(source)?.into_graph();
    let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
    assert!(
      coordinates.diagnostics().is_empty(),
      "{source}: {:?}",
      coordinates.diagnostics()
    );
  }
  Ok(())
}

#[test]
fn disabled_refinement_should_still_produce_complete_deterministic_geometry() -> TestResult {
  let graph = read_smiles("CC(C)(CC1CCCCC1)CC2CCCCC2")?.into_graph();
  let options = LayoutOptions {
    refinement_passes: 0,
    refinement_budget: 0,
    ..LayoutOptions::default()
  };
  let coordinates = layout::generate(&graph, &options)?;
  assert_eq!(coordinates.positions().len(), graph.atom_count());
  assert!(
    !coordinates
      .diagnostics()
      .contains(&LayoutDiagnostic::RefinementBudgetExhausted)
  );
  assert_eq!(coordinates, layout::generate(&graph, &options)?);
  Ok(())
}

#[test]
fn exhausting_search_budget_should_report_quality_without_losing_atoms() -> TestResult {
  let graph = read_smiles(&format!("C{}", "(CCC)".repeat(12)))?.into_graph();
  let options = LayoutOptions {
    refinement_budget: 1,
    ..LayoutOptions::default()
  };
  let coordinates = layout::generate(&graph, &options)?;
  assert_eq!(coordinates.positions().len(), graph.atom_count());
  assert!(
    coordinates
      .diagnostics()
      .contains(&LayoutDiagnostic::RefinementBudgetExhausted)
  );
  Ok(())
}

#[test]
fn unrelated_fragments_should_not_change_internal_ring_geometry() -> TestResult {
  let source = "C1CCCCC1CCC2CCCC2";
  for passes in [0, 4] {
    let graph = read_smiles(source)?.into_graph();
    let joined = read_smiles(&format!("CC.{source}"))?.into_graph();
    let options = LayoutOptions {
      refinement_passes: passes,
      ..LayoutOptions::default()
    };
    let alone = layout::generate(&graph, &options)?;
    let together = layout::generate(&joined, &options)?;
    let a: Vec<_> = alone.positions().values().copied().collect();
    let b: Vec<_> = together.positions().values().copied().skip(2).collect();
    for (p, q) in a.iter().zip(&b) {
      assert!(((p.x - a[0].x) - (q.x - b[0].x)).abs() < 1e-8);
      assert!(((p.y - a[0].y) - (q.y - b[0].y)).abs() < 1e-8);
    }
  }
  Ok(())
}

#[test]
fn linked_rings_should_keep_linear_triple_centers_after_refinement() -> TestResult {
  let graph = read_smiles("C1CCCCC1CC#CC2CCCCC2")?.into_graph();
  let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
  for (id, _) in graph.atoms() {
    let neighbors: Vec<_> = graph.neighbors(id)?.collect();
    if neighbors.len() != 2
      || !neighbors.iter().any(|&(_, edge)| {
        graph
          .bond(edge)
          .is_some_and(|bond| bond.order() == chitin_chem::BondOrder::Triple)
      })
    {
      continue;
    }
    let a = coordinates.position(neighbors[0].0).ok_or("missing atom")?;
    let b = coordinates.position(neighbors[1].0).ok_or("missing atom")?;
    assert!((a.distance(b) - 2.).abs() < 1e-8);
  }
  Ok(())
}

#[test]
fn slender_ring_pairs_should_be_oriented_horizontally() -> TestResult {
  let graph = read_smiles("C1CCCCC1C2CCCCC2")?.into_graph();
  let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
  let bounds = coordinates.bounds().ok_or("missing bounds")?;
  assert!(bounds.width() > bounds.height());
  Ok(())
}
