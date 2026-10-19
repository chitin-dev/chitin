//! Perspective order is explicit layout presentation, never guessed from crossings.

use chitin_chem::{BondId, BondOrder, Element, io::smiles::read_smiles};
use chitin_chemdraw::{
  Color,
  depict::{self, DepictOptions},
  geometry::Point,
  layout::{self, Layout2d, LayoutOptions},
  scene::{Line, Primitive, Source},
  svg,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Selects the source bond's actual scene strokes, not SVG-specific path syntax.
fn lines(primitives: &[Primitive], edge: BondId) -> Vec<&Line> {
  primitives
    .iter()
    .filter_map(|p| match p {
      Primitive::Line(line) if line.source == Some(Source::Bond(edge)) => Some(line),
      _ => None,
    })
    .collect()
}

/// Solves a proper crossing in the same coordinate frame as the generated scene.
fn intersection(a: Point, b: Point, c: Point, d: Point) -> Point {
  let v = Point::new(b.x - a.x, b.y - a.y);
  let w = Point::new(d.x - c.x, d.y - c.y);
  let t = ((c.x - a.x) * w.y - (c.y - a.y) * w.x) / (v.x * w.y - v.y * w.x);
  Point::new(a.x + v.x * t, a.y + v.y * t)
}

#[test]
fn parent_and_single_site_carbon_derivatives_should_keep_an_explicit_ordered_crossing() -> TestResult {
  for source in [
    "C1C2CC3CC1CC(C2)C3",
    "NC12CC3CC(CC(C3)C1)C2",
    "C[C@H](N)C12CC3CC(CC(C3)C1)C2",
  ] {
    let graph = read_smiles(source)?.into_graph();
    let before = graph.clone();
    for passes in [0, 4] {
      let coordinates = layout::generate(
        &graph,
        &LayoutOptions {
          refinement_passes: passes,
          ..Default::default()
        },
      )?;
      assert_eq!(coordinates.bond_crossings().len(), 1);
      assert!(coordinates.diagnostics().is_empty());
      let crossing = coordinates.bond_crossings()[0];
      assert_ne!(crossing.front, crossing.back);
      assert!(graph.bond(crossing.front).is_some() && graph.bond(crossing.back).is_some());
      let [a, b] = graph.bond(crossing.front).ok_or("missing front")?.atoms();
      assert!(
        (coordinates.position(a).ok_or("missing atom")?.x - coordinates.position(b).ok_or("missing atom")?.x).abs()
          < 1e-8
      );
    }
    assert_eq!(graph, before);
  }
  Ok(())
}

#[test]
fn uncertain_core_variants_should_keep_the_flat_fallback_not_an_assumed_configuration() -> TestResult {
  for source in [
    "C1N2CC3CC1CC(C2)C3",
    "CC1N2CN3CN1CN(C2)C3",
    "C1[15N]2CN3CN1CN(C2)C3",
    "C1[NH+]2CN3CN1CN(C2)C3",
    "[13CH2]1C2CC3CC1CC(C2)C3",
    "C1C2(N)CC3CC1(O)CC(C2)C3",
    "[C@H]12CC3CC(CC(C3)C1)C2",
    "C/C=C/C12CC3CC(CC(C3)C1)C2",
    "C12C3C4C1C5C2C3N45",
    "[13CH]12C3C4C1C5C2C3C45",
    "NC12C3(O)C4C1C5C2C3C45",
    "C/C=C/C12C3C4C1C5C2C3C45",
  ] {
    let graph = read_smiles(source)?.into_graph();
    let coordinates = layout::generate(&graph, &Default::default())?;
    assert!(coordinates.bond_crossings().is_empty(), "{source}");
    assert!(
      depict::depict(&graph, &coordinates, &Default::default()).is_ok(),
      "{source}"
    );
  }
  Ok(())
}

#[test]
fn cube_projection_should_keep_both_rear_gaps_after_refinement() -> TestResult {
  for source in [
    "C12C3C4C1C5C2C3C45",
    "O=C(O)C12C3C4C1C5C2C3C45",
    "O=C(Nc1ccccc1)C12C3C4C1C5C2C3C45",
    "C12C3C4C1C5C2C3C45.C/C=C/C",
  ] {
    let graph = read_smiles(source)?.into_graph();
    let before = graph.clone();
    for passes in [0, 4] {
      let coordinates = layout::generate(
        &graph,
        &LayoutOptions {
          refinement_passes: passes,
          ..Default::default()
        },
      )?;
      assert_eq!(coordinates.bond_crossings().len(), 2, "{source}");
      assert!(coordinates.diagnostics().is_empty(), "{source}");
      for width in [1., 1.6, 4.] {
        let options = DepictOptions {
          stroke_width: width,
          ..Default::default()
        };
        let drawing = depict::depict(&graph, &coordinates, &options)?;
        for crossing in coordinates.bond_crossings() {
          assert_eq!(lines(drawing.primitives(), crossing.front).len(), 1);
          let rear = lines(drawing.primitives(), crossing.back);
          assert_eq!(rear.len(), 2);
          // These cube crossings are perpendicular; the cap-inclusive visible
          // gap is three stroke widths, independent of drawing/export scale.
          let gap = [rear[0].start, rear[0].end]
            .into_iter()
            .flat_map(|a| [rear[1].start, rear[1].end].map(|b| a.distance(b)))
            .fold(f64::INFINITY, f64::min)
            - width;
          assert!(gap >= 2. * width - 1e-8);
        }
        let output = svg::export(
          &drawing,
          &svg::SvgOptions {
            background: None,
            ..Default::default()
          },
        )?;
        assert!(!output.contains("<rect"));
      }
    }
    assert_eq!(graph, before);
  }
  Ok(())
}

#[test]
fn symmetric_heterocages_should_keep_labels_color_and_a_real_rear_gap() -> TestResult {
  let graph = read_smiles("C1N2CN3CN1CN(C2)C3")?.into_graph();
  let before = graph.clone();
  for passes in [0, 4] {
    let coordinates = layout::generate(
      &graph,
      &LayoutOptions {
        refinement_passes: passes,
        ..Default::default()
      },
    )?;
    assert_eq!(coordinates.bond_crossings().len(), 1);
    assert!(coordinates.diagnostics().is_empty());
    let crossing = coordinates.bond_crossings()[0];
    let options = DepictOptions::default();
    let drawing = depict::depict(&graph, &coordinates, &options)?;
    // The gap can straddle the C–N color seam: count alone cannot prove a cut.
    let rear = lines(drawing.primitives(), crossing.back);
    assert!(rear.len() >= 2);
    assert!(
      rear
        .iter()
        .any(|line| line.color == options.label_color(Element::Nitrogen))
    );
    assert!(
      rear
        .iter()
        .any(|line| line.color == options.label_color(Element::Carbon))
    );
    let axis = |id| -> Result<_, &'static str> {
      let bond = graph.bond(id).ok_or("missing bond")?;
      let [a, b] = bond.atoms().map(|atom| coordinates.position(atom));
      Ok(
        [a.ok_or("missing atom")?, b.ok_or("missing atom")?]
          .map(|p| Point::new(p.x * options.bond_length, -p.y * options.bond_length)),
      )
    };
    let [a, b] = axis(crossing.front)?;
    let [c, d] = axis(crossing.back)?;
    let center = intersection(a, b, c, d);
    assert!(
      rear
        .iter()
        .all(|line| { center.distance(line.start).min(center.distance(line.end)) >= 2. * options.stroke_width - 1e-8 })
    );
    let labels: Vec<_> = drawing
      .primitives()
      .iter()
      .filter_map(|primitive| match primitive {
        Primitive::Text(text) => Some(text),
        _ => None,
      })
      .collect();
    assert_eq!(labels.len(), 4);
    assert!(labels.iter().all(|text| {
      text.content == "N" && text.color == options.label_color(Element::Nitrogen)
        && matches!(text.source, Some(Source::Atom(id)) if graph.atom(id).is_some_and(|atom| atom.element() == Element::Nitrogen))
    }));
    svg::export(&drawing, &Default::default())?;
  }
  assert_eq!(graph, before);
  Ok(())
}

#[test]
fn symmetric_heterocage_perspective_should_survive_reversed_atom_allocation() -> TestResult {
  let graph = read_smiles("C1N2CN3CN1CN(C2)C3")?.into_graph();
  let mut reordered = chitin_chem::MolecularGraph::new();
  let mut ids = std::collections::BTreeMap::new();
  let atoms: Vec<_> = graph.atoms().collect();
  for &(id, atom) in atoms.iter().rev() {
    ids.insert(id, reordered.add_atom(atom.clone())?);
  }
  for (_, bond) in graph.bonds() {
    let [a, b] = bond.atoms();
    reordered.add_bond(ids[&b], ids[&a], bond.order())?;
  }
  let coordinates = layout::generate(&reordered, &Default::default())?;
  assert_eq!(coordinates.bond_crossings().len(), 1);
  assert!(coordinates.diagnostics().is_empty());
  depict::depict(&reordered, &coordinates, &Default::default())?;
  Ok(())
}

#[test]
fn unrelated_alkene_fragments_should_not_change_the_cage_view() -> TestResult {
  let graph = read_smiles("C1C2CC3CC1CC(C2)C3.C/C=C/C")?.into_graph();
  let coordinates = layout::generate(&graph, &Default::default())?;
  assert_eq!(coordinates.bond_crossings().len(), 1);
  assert!(coordinates.diagnostics().is_empty());
  Ok(())
}

#[test]
fn rear_gap_should_exceed_two_stroke_widths_and_preserve_transparent_output() -> TestResult {
  let graph = read_smiles("C1C2CC3CC1CC(C2)C3")?.into_graph();
  let coordinates = layout::generate(&graph, &Default::default())?;
  let crossing = coordinates.bond_crossings()[0];
  for width in [1., 1.6, 4.] {
    let mut options = DepictOptions {
      stroke_width: width,
      ..Default::default()
    };
    options.element_colors.insert(
      Element::Carbon,
      Color {
        red: 40,
        green: 80,
        blue: 120,
        alpha: 128,
      },
    );
    let drawing = depict::depict(&graph, &coordinates, &options)?;
    let foreground = lines(drawing.primitives(), crossing.front);
    let background = lines(drawing.primitives(), crossing.back);
    assert_eq!(foreground.len(), 1);
    assert_eq!(background.len(), 2);
    let axis = |edge| -> Result<_, &'static str> {
      let bond = graph.bond(edge).ok_or("missing bond")?;
      let [a, b] = bond.atoms().map(|id| coordinates.position(id));
      Ok(
        [a.ok_or("missing atom")?, b.ok_or("missing atom")?]
          .map(|p| Point::new(p.x * options.bond_length, -p.y * options.bond_length)),
      )
    };
    let [a, b] = axis(crossing.front)?;
    let [c, d] = axis(crossing.back)?;
    let center = intersection(a, b, c, d);
    let ends: Vec<_> = background
      .iter()
      .map(|line| center.distance(line.start).min(center.distance(line.end)))
      .collect();
    assert!(ends.iter().sum::<f64>() - width >= 2. * width - 1e-8);
    assert!(
      background
        .iter()
        .all(|line| line.color == options.label_color(Element::Carbon))
    );
    let svg = svg::export(
      &drawing,
      &svg::SvgOptions {
        background: None,
        ..Default::default()
      },
    )?;
    assert!(!svg.contains("<rect"));
    assert!(svg.contains("stroke-opacity=\"0.5019607843137255\""));
  }
  Ok(())
}

#[test]
fn copying_only_positions_should_not_infer_crossing_order_from_the_graph() -> TestResult {
  let graph = read_smiles("C1C2CC3CC1CC(C2)C3")?.into_graph();
  let auto = layout::generate(&graph, &Default::default())?;
  let manual = Layout2d::from_positions(auto.positions().clone())?;
  assert!(manual.bond_crossings().is_empty());
  let drawing = depict::depict(&graph, &manual, &Default::default())?;
  assert_eq!(lines(drawing.primitives(), auto.bond_crossings()[0].back).len(), 1);
  Ok(())
}

#[test]
fn stale_crossing_order_should_fail_after_source_bond_order_changes() -> TestResult {
  let mut graph = read_smiles("C1C2CC3CC1CC(C2)C3")?.into_graph();
  let coordinates = layout::generate(&graph, &Default::default())?;
  let back = coordinates.bond_crossings()[0].back;
  graph.set_bond_order(back, BondOrder::Double)?;
  assert!(
    matches!(depict::depict(&graph, &coordinates, &Default::default()), Err(depict::DepictError::Perspective(id)) if id == back)
  );
  Ok(())
}
