//! Depiction H expansion preserves source identity and relative configuration.

use std::collections::{BTreeMap, BTreeSet};

use chitin_chem::{
  AtomId, Element, StereoLigand,
  io::smiles::{read_smiles, write_smiles},
};
use chitin_chemdraw::{
  depict::{self, ChiralHydrogenMode, DepictOptions},
  geometry::Point,
  layout::{self, Layout2d, LayoutOptions},
  scene::{Drawing, Primitive, Source, Text},
  svg,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Selects expansion without changing defaults for existing skeletal clients.
fn explicit() -> DepictOptions {
  DepictOptions {
    chiral_hydrogens: ChiralHydrogenMode::Explicit,
    ..Default::default()
  }
}

/// Extracts only labels originating in declared-H expansion, not graph H vertices.
fn labels(drawing: &Drawing) -> BTreeMap<AtomId, &Text> {
  drawing
    .primitives()
    .iter()
    .filter_map(|p| match p {
      Primitive::Text(text) => match text.source {
        Some(Source::AttachedHydrogen(center)) => Some((center, text)),
        _ => None,
      },
      _ => None,
    })
    .collect()
}

/// Counts solids generated for attached H only; ordinary graph bonds are excluded.
fn solids(drawing: &Drawing) -> usize {
  drawing
    .primitives()
    .iter()
    .filter(|p| matches!(p,Primitive::Polygon(p) if matches!(p.source,Some(Source::AttachedHydrogen(_)))))
    .count()
}

#[test]
fn fused_steroid_should_expand_each_declared_chiral_h_without_mutating_graph_or_layout() -> TestResult {
  let source = "C[C@H](CCCC(C)C)[C@H]1CC[C@@H]2[C@@]1(CC[C@H]3[C@H]2CC=C4[C@@]3(CC[C@@H](C4)O)C)C";
  let graph = read_smiles(source)?.into_graph();
  let before = graph.clone();
  let serialized = write_smiles(&graph)?;
  let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
  let original_coordinates = coordinates.clone();
  let expected: BTreeSet<_> = graph
    .tetrahedral_centers()
    .filter_map(|(center, s)| s.ligands().contains(&StereoLigand::Hydrogen).then_some(center))
    .collect();
  assert_eq!(expected.len(), 6);
  let drawing = depict::depict(&graph, &coordinates, &explicit())?;
  let drawn = labels(&drawing);
  assert_eq!(drawn.keys().copied().collect::<BTreeSet<_>>(), expected);
  for &center in &expected {
    assert_eq!(drawn[&center].content, "H");
    let geometry: Vec<_> = drawing
      .primitives()
      .iter()
      .filter(|p| match p {
        Primitive::Line(p) => p.source == Some(Source::AttachedHydrogen(center)),
        Primitive::Polygon(p) => p.source == Some(Source::AttachedHydrogen(center)),
        _ => false,
      })
      .collect();
    assert!(geometry.len() == 1 || geometry.len() == 7);
    // All existing graph bonds at this expanded center stay ordinary lines.
    for (neighbor, edge) in graph.neighbors(center)? {
      let strokes: Vec<_> = drawing
        .primitives()
        .iter()
        .filter(|p| match p {
          Primitive::Line(p) => p.source == Some(Source::Bond(edge)),
          Primitive::Polygon(p) => p.source == Some(Source::Bond(edge)),
          _ => false,
        })
        .collect();
      let options = explicit();
      let element = graph.atom(neighbor).ok_or("missing neighbor")?.element();
      let split = options.label_color(element) != options.label_color(chitin_chem::Element::Carbon);
      assert_eq!(strokes.len(), if split { 2 } else { 1 });
      assert!(strokes.iter().all(|p| matches!(p, Primitive::Line(_))));
    }
  }
  assert_eq!(graph, before);
  assert_eq!(coordinates, original_coordinates);
  assert_eq!(write_smiles(&graph)?, serialized);
  assert_eq!(depict::depict(&graph, &coordinates, &explicit())?, drawing);
  assert!(svg::export(&drawing, &svg::SvgOptions::default())?.contains("aria-label=\"H\""));
  Ok(())
}

#[test]
fn explicit_h_wedges_should_use_original_winding_under_rotation_and_reflection() -> TestResult {
  for (source, expected_solid) in [("[C@H](F)(Cl)Br", 1), ("[C@@H](F)(Cl)Br", 0)] {
    let graph = read_smiles(source)?.into_graph();
    for angle in [0_f64, 0.7, 2.1] {
      for reflect in [1., -1.] {
        let points: BTreeMap<_, _> = graph
          .atoms()
          .map(|(id, _)| id)
          .zip(
            [(0., 0.), (1., 0.), (-0.5, 0.8660254), (-0.5, -0.8660254)].map(|(x, y)| {
              Point::new(
                reflect * (x * angle.cos() - y * angle.sin()),
                x * angle.sin() + y * angle.cos(),
              )
            }),
          )
          .collect();
        let coordinates = Layout2d::from_positions(points)?;
        let drawing = depict::depict(&graph, &coordinates, &explicit())?;
        assert_eq!(labels(&drawing).len(), 1);
        assert_eq!(
          solids(&drawing),
          if reflect > 0. {
            expected_solid
          } else {
            1 - expected_solid
          },
          "{source}, {angle}, {reflect}"
        );
      }
    }
  }
  Ok(())
}

#[test]
fn defaults_explicit_vertices_and_untagged_hydrogens_should_remain_unchanged() -> TestResult {
  for (source, generated) in [
    ("[C@H](F)(Cl)Br", 1),
    ("[H][C@](F)(Cl)Br", 0),
    ("[2H][C@](F)(Cl)Br", 0),
    ("[C@](F)(Cl)(Br)I", 0),
    ("C[CH](O)N", 0),
    ("CCO", 0),
  ] {
    let graph = read_smiles(source)?.into_graph();
    let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
    let implicit = depict::depict(&graph, &coordinates, &DepictOptions::default())?;
    assert!(labels(&implicit).is_empty());
    let drawing = depict::depict(&graph, &coordinates, &explicit())?;
    assert_eq!(labels(&drawing).len(), generated, "{source}");
    if generated == 0 {
      assert_eq!(drawing, implicit);
    }
    for (id, atom) in graph.atoms().filter(|(_, a)| a.element() == Element::Hydrogen) {
      assert!(drawing.primitives().iter().any(
        |p| matches!(p,Primitive::Text(t) if t.source == Some(Source::Atom(id)) && t.content == atom.element().symbol())
      ));
    }
  }
  Ok(())
}

#[test]
fn host_isotope_label_should_not_repeat_the_expanded_h() -> TestResult {
  let graph = read_smiles("[13C@H](F)(Cl)Br")?.into_graph();
  let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
  let drawing = depict::depict(&graph, &coordinates, &explicit())?;
  let (center, _) = labels(&drawing).into_iter().next().ok_or("missing H")?;
  let host: Vec<_> = drawing
    .primitives()
    .iter()
    .filter_map(|p| match p {
      Primitive::Text(t) if t.source == Some(Source::Atom(center)) => Some(t.content.as_str()),
      _ => None,
    })
    .collect();
  assert!(host.contains(&"13") && host.contains(&"C"));
  assert!(!host.contains(&"H"));
  Ok(())
}

#[test]
fn three_neighbor_stereocenters_should_not_need_an_unannotated_bond_when_h_is_expanded() -> TestResult {
  let graph = read_smiles("[C@H]([C@H](F)Cl)([C@H](F)Cl)[C@H](F)Cl")?.into_graph();
  let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
  assert!(matches!(
    depict::depict(&graph, &coordinates, &DepictOptions::default()),
    Err(depict::DepictError::Stereo(_))
  ));
  let drawing = depict::depict(&graph, &coordinates, &explicit())?;
  assert_eq!(labels(&drawing).len(), 4);
  Ok(())
}

#[test]
fn h_position_should_avoid_existing_measured_labels_without_changing_polarity() -> TestResult {
  let graph = read_smiles("[C@H](F)(Cl)Br.[He]")?.into_graph();
  let points: BTreeMap<_, _> = graph
    .atoms()
    .map(|(id, _)| id)
    .zip([
      Point::new(0., 0.),
      Point::new(1., 0.),
      Point::new(-0.5, 0.8660254),
      Point::new(-0.5, -0.8660254),
      Point::new(-0.70, 0.),
    ])
    .collect();
  let drawing = depict::depict(&graph, &Layout2d::from_positions(points)?, &explicit())?;
  let expanded = labels(&drawing);
  let hydrogen = expanded.values().next().ok_or("missing H")?;
  let [a, b] = hydrogen.envelope()?;
  for p in drawing.primitives() {
    if let Primitive::Text(other) = p
      && matches!(other.source, Some(Source::Atom(_)))
    {
      let [c, d] = other.envelope()?;
      assert!(!(a.x < d.x && c.x < b.x && a.y < d.y && c.y < b.y));
    }
  }
  assert_eq!(solids(&drawing), 1);
  Ok(())
}

#[test]
fn explicit_h_should_reject_degenerate_geometry_instead_of_hiding_a_failed_expansion() -> TestResult {
  let graph = read_smiles("[C@H](F)(Cl)Br")?.into_graph();
  let points = graph
    .atoms()
    .enumerate()
    .map(|(i, (id, _))| (id, Point::new(i as f64, 0.)))
    .collect();
  assert!(matches!(
    depict::depict(&graph, &Layout2d::from_positions(points)?, &explicit()),
    Err(depict::DepictError::Stereo(_))
  ));
  Ok(())
}
