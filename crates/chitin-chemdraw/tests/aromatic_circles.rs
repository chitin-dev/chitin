//! Inner aromatic circles are optional complete-ring display substitutions.

use chitin_chem::{AtomId, io::smiles::read_smiles};
use chitin_chemdraw::{
  Circle, Color, Drawing,
  depict::{self, AromaticStyle, ColorScheme, DepictOptions},
  geometry::Point,
  layout::{self, Layout2d, LayoutOptions},
  scene::{DrawingError, Primitive, Source},
  svg,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Selects the optional display mode without changing any other default.
fn circles() -> DepictOptions {
  DepictOptions {
    aromatic_style: AromaticStyle::Circle,
    ..Default::default()
  }
}

/// Extracts only generic circles, not an inferred chemical ring catalogue.
fn marks(drawing: &Drawing) -> Vec<&Circle> {
  drawing
    .primitives()
    .iter()
    .filter_map(|primitive| match primitive {
      Primitive::Circle(circle) => Some(circle),
      _ => None,
    })
    .collect()
}

#[test]
fn simple_aromatic_rings_should_replace_inner_doubles_without_changing_facts() -> TestResult {
  for source in ["c1ccccc1", "c1ccncc1", "c1ccoc1", "c1ccsc1", "Cc1ccc(O)cc1"] {
    let graph = read_smiles(source)?.into_graph();
    let before = graph.clone();
    let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
    let original = coordinates.clone();
    let options = DepictOptions {
      bond_color_style: depict::BondColorStyle::Uniform,
      ..circles()
    };
    let drawing = depict::depict(&graph, &coordinates, &options)?;
    assert_eq!(marks(&drawing).len(), 1, "{source}");
    for (id, _) in graph.bonds() {
      let strokes = drawing
        .primitives()
        .iter()
        .filter(|primitive| matches!(primitive, Primitive::Line(line) if line.source == Some(Source::Bond(id))))
        .count();
      assert_eq!(strokes, 1, "{source}: {id}");
    }
    assert_eq!(graph, before);
    assert_eq!(coordinates, original);
    let output = svg::export(&drawing, &svg::SvgOptions::default())?;
    assert_eq!(output.matches("<circle ").count(), 1);
  }
  Ok(())
}

#[test]
fn fused_unmarked_and_explicit_alternating_inputs_should_fall_back_completely() -> TestResult {
  for source in ["c1ccc2ccccc2c1", "C1=CC=CC=C1", "c1=cc=cc=c1", "C1CCCCC1"] {
    let graph = read_smiles(source)?.into_graph();
    let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
    let baseline = depict::depict(&graph, &coordinates, &DepictOptions::default())?;
    let drawing = depict::depict(&graph, &coordinates, &circles())?;
    assert!(marks(&drawing).is_empty());
    assert_eq!(drawing, baseline, "{source}");
  }
  Ok(())
}

#[test]
fn linked_independent_rings_should_each_receive_their_own_circle() -> TestResult {
  let graph = read_smiles("c1ccccc1-c2ccncc2")?.into_graph();
  let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
  let options = DepictOptions {
    bond_color_style: depict::BondColorStyle::Uniform,
    ..circles()
  };
  let drawing = depict::depict(&graph, &coordinates, &options)?;
  assert_eq!(marks(&drawing).len(), 2);
  assert_eq!(
    drawing
      .primitives()
      .iter()
      .filter(|p| matches!(p, Primitive::Line(_)))
      .count(),
    graph.bond_count()
  );
  Ok(())
}

#[test]
fn ring_substitution_should_not_remove_external_carbonyls_or_other_ring_doubles() -> TestResult {
  for (source, expected_circles) in [("CC(=O)Oc1ccccc1", 1), ("c1ccccc1-c2ccc3ccccc3c2", 1)] {
    let graph = read_smiles(source)?.into_graph();
    let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
    let drawing = depict::depict(&graph, &coordinates, &circles())?;
    assert_eq!(marks(&drawing).len(), expected_circles);
    let lines = drawing
      .primitives()
      .iter()
      .filter(|p| matches!(p, Primitive::Line(_)))
      .count();
    assert!(
      lines > graph.bond_count(),
      "missing non-circle double strokes: {source}"
    );
  }
  Ok(())
}

#[test]
fn thick_strokes_should_fall_back_instead_of_touching_or_filling_the_circle() -> TestResult {
  let graph = read_smiles("c1ccccc1")?.into_graph();
  let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
  let options = DepictOptions {
    stroke_width: 20.,
    ..circles()
  };
  let baseline = DepictOptions {
    aromatic_style: AromaticStyle::Kekule,
    ..options.clone()
  };
  assert_eq!(
    depict::depict(&graph, &coordinates, &options)?,
    depict::depict(&graph, &coordinates, &baseline)?
  );
  Ok(())
}

#[test]
fn default_kekule_mode_should_remain_identical() -> TestResult {
  let graph = read_smiles("c1ccccc1")?.into_graph();
  let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
  let options = DepictOptions::default();
  assert_eq!(options.aromatic_style, AromaticStyle::Kekule);
  let drawing = depict::depict(&graph, &coordinates, &options)?;
  assert!(marks(&drawing).is_empty());
  assert_eq!(drawing.primitives().len(), 9);
  Ok(())
}

#[test]
fn distorted_manual_geometry_should_retain_the_complete_kekule_drawing() -> TestResult {
  let graph = read_smiles("c1ccccc1")?.into_graph();
  let original = layout::generate(&graph, &LayoutOptions::default())?;
  let distorted = Layout2d::from_positions(
    original
      .positions()
      .iter()
      .map(|(&id, &p)| (id, Point::new(p.x * 1.4, p.y)))
      .collect(),
  )?;
  assert_eq!(
    depict::depict(&graph, &distorted, &circles())?,
    depict::depict(&graph, &distorted, &DepictOptions::default())?
  );
  Ok(())
}

#[test]
fn regular_circle_geometry_should_follow_rotation_reflection_and_translation() -> TestResult {
  let graph = read_smiles("c1ccccc1")?.into_graph();
  let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
  let base = depict::depict(&graph, &coordinates, &circles())?;
  let radius = marks(&base)[0].radius;
  for reflect in [-1., 1.] {
    let transformed = Layout2d::from_positions(
      coordinates
        .positions()
        .iter()
        .map(|(&id, p)| {
          let angle = 0.7_f64;
          (
            id,
            Point::new(
              reflect * (p.x * angle.cos() - p.y * angle.sin()) + 4.,
              p.x * angle.sin() + p.y * angle.cos() - 3.,
            ),
          )
        })
        .collect(),
    )?;
    let drawing = depict::depict(&graph, &transformed, &circles())?;
    assert_eq!(marks(&drawing).len(), 1);
    assert!((marks(&drawing)[0].radius - radius).abs() < 1e-8);
  }
  Ok(())
}

/// Finds a source ring's mean normalized location without assuming atom handles.
fn center(points: impl Iterator<Item = Point>) -> Point {
  let points: Vec<_> = points.collect();
  Point::new(
    points.iter().map(|p| p.x / points.len() as f64).sum(),
    points.iter().map(|p| p.y / points.len() as f64).sum(),
  )
}

#[test]
fn inward_labels_or_unrelated_bonds_should_cancel_the_whole_circle_substitution() -> TestResult {
  for source in ["c1ccccc1.[Na+]", "c1ccccc1.CC"] {
    let graph = read_smiles(source)?.into_graph();
    let original = layout::generate(&graph, &LayoutOptions::default())?;
    let ring_ids: Vec<AtomId> = graph
      .atoms()
      .filter(|(_, a)| a.is_aromatic())
      .map(|(id, _)| id)
      .collect();
    let middle = center(ring_ids.iter().filter_map(|id| original.position(*id)));
    let mut points = original.positions().clone();
    for (i, (id, _)) in graph.atoms().filter(|(id, _)| !ring_ids.contains(id)).enumerate() {
      points.insert(id, Point::new(middle.x + i as f64 - 0.5, middle.y));
    }
    let coordinates = Layout2d::from_positions(points)?;
    assert_eq!(
      depict::depict(&graph, &coordinates, &circles())?,
      depict::depict(&graph, &coordinates, &DepictOptions::default())?
    );
  }
  Ok(())
}

#[test]
fn circle_strokes_should_use_foreground_not_heteroatom_colors() -> TestResult {
  let graph = read_smiles("c1ccncc1")?.into_graph();
  let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
  let options = DepictOptions {
    color_scheme: ColorScheme::Element,
    foreground: Color {
      red: 12,
      green: 34,
      blue: 56,
      alpha: 128,
    },
    ..circles()
  };
  let drawing = depict::depict(&graph, &coordinates, &options)?;
  assert_eq!(marks(&drawing).len(), 1);
  assert_eq!(marks(&drawing)[0].color, options.foreground);
  assert_eq!(marks(&drawing)[0].width, options.stroke_width);
  assert!(
    drawing
      .primitives()
      .iter()
      .any(|p| matches!(p, Primitive::Text(t) if t.content == "N" && t.color != options.foreground))
  );
  let output = svg::export(&drawing, &svg::SvgOptions::default())?;
  assert!(output.contains("stroke=\"#0c2238\""));
  assert!(output.contains("stroke-opacity=\"0.5019607843137255\""));
  Ok(())
}

#[test]
fn generic_circle_bounds_should_include_stroke_and_reject_invalid_dimensions() -> TestResult {
  let circle = Circle {
    center: Point::new(3., -4.),
    radius: 2.,
    width: 0.5,
    color: Color::BLACK,
    source: None,
  };
  let drawing = Drawing::new(vec![Primitive::Circle(circle.clone())])?;
  let bounds = drawing.bounds().ok_or("missing bounds")?;
  assert_eq!(bounds.min(), Point::new(0.75, -6.25));
  assert_eq!(bounds.max(), Point::new(5.25, -1.75));
  for radius in [0., -1., f64::NAN, f64::INFINITY] {
    assert!(matches!(
      Drawing::new(vec![Primitive::Circle(Circle {
        radius,
        ..circle.clone()
      })]),
      Err(DrawingError::Dimensions(0))
    ));
  }
  assert!(
    Drawing::new(vec![Primitive::Circle(Circle {
      center: Point::new(f64::NAN, 0.),
      ..circle
    })])
    .is_err()
  );
  Ok(())
}
