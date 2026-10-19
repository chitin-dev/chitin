//! Wedges encode source configuration, not token spelling or endpoint order.

use std::collections::BTreeMap;

use chitin_chem::io::smiles::{read_smiles, write_smiles};
use chitin_chemdraw::{
  depict::{self, DepictOptions},
  geometry::Point,
  layout::{self, Layout2d, LayoutOptions},
  scene::{Color, Drawing, DrawingError, Polygon, Primitive, Source},
  svg,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Keeps each geometric wedge whole; split-color coverage lives in bond_colors.
fn geometry_options() -> DepictOptions {
  DepictOptions {
    bond_color_style: depict::BondColorStyle::Uniform,
    ..Default::default()
  }
}

/// Builds four ligand directions in a known right-handed frame.
fn cross(
  graph: &chitin_chem::MolecularGraph,
  reflected: bool,
  angle: f64,
) -> Result<Layout2d, Box<dyn std::error::Error>> {
  let mut points = BTreeMap::new();
  for ((id, _), (x, y)) in graph.atoms().zip([(0., 0.), (1., 0.), (0., 1.), (-1., 0.), (0., -1.)]) {
    let x = if reflected { -x } else { x };
    points.insert(
      id,
      Point::new(x * angle.cos() - y * angle.sin(), x * angle.sin() + y * angle.cos()),
    );
  }
  Ok(Layout2d::from_positions(points)?)
}

/// Counts solid wedges; a hashed wedge consists solely of short line strokes.
fn solids(drawing: &Drawing) -> usize {
  drawing
    .primitives()
    .iter()
    .filter(|p| matches!(p, Primitive::Polygon(_)))
    .count()
}

#[test]
fn known_four_ligand_projection_should_flip_wedges_for_enantiomers_and_reflections() -> TestResult {
  for (source, solid) in [("[C@](F)(Cl)(Br)I", 1), ("[C@@](F)(Cl)(Br)I", 0)] {
    let graph = read_smiles(source)?.into_graph();
    for angle in [0., 0.7, 2.1] {
      let direct = depict::depict(&graph, &cross(&graph, false, angle)?, &geometry_options())?;
      let reflected = depict::depict(&graph, &cross(&graph, true, angle)?, &geometry_options())?;
      assert_eq!(solids(&direct), solid, "{source}, {angle}");
      assert_eq!(solids(&reflected), 1 - solid, "{source}, {angle}");
      let document = svg::export(&direct, &svg::SvgOptions::default())?;
      assert_eq!(document.contains("<polygon"), solid == 1);
    }
  }
  Ok(())
}

#[test]
fn virtual_hydrogen_projection_should_use_its_declared_reference_slot() -> TestResult {
  for (source, solid) in [("[C@H](F)(Cl)Br", 0), ("[C@@H](F)(Cl)Br", 1)] {
    let graph = read_smiles(source)?.into_graph();
    let points: BTreeMap<_, _> = graph
      .atoms()
      .map(|(id, _)| id)
      .zip([
        Point::new(0., 0.),
        Point::new(1., 0.),
        Point::new(-0.5, 0.8660254),
        Point::new(-0.5, -0.8660254),
      ])
      .collect();
    let drawing = depict::depict(&graph, &Layout2d::from_positions(points)?, &geometry_options())?;
    assert_eq!(solids(&drawing), solid, "{source}");
    // Ordinary chiral carbon and its declared H remain skeletal rather than
    // introducing crowded CH labels at every wedge origin.
    assert!(
      !drawing
        .primitives()
        .iter()
        .any(|p| matches!(p, Primitive::Text(t) if t.content == "C" || t.content == "H"))
    );
  }
  Ok(())
}

#[test]
fn penicillin_input_should_export_every_stereocenter_without_modifying_facts() -> TestResult {
  let source = "CC1([C@@H](N2[C@H](S1)[C@@H](C2=O)NC(=O)Cc3ccccc3)C(=O)O)C";
  let graph = read_smiles(source)?.into_graph();
  let before = graph.clone();
  assert_eq!(graph.tetrahedral_centers().len(), 3);
  let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
  assert!(coordinates.diagnostics().is_empty());
  let drawing = depict::depict(&graph, &coordinates, &geometry_options())?;
  let mut per_bond = BTreeMap::new();
  for primitive in drawing.primitives() {
    let source = match primitive {
      Primitive::Line(line) => line.source,
      Primitive::Polygon(polygon) => polygon.source,
      _ => None,
    };
    if let Some(Source::Bond(id)) = source {
      *per_bond.entry(id).or_insert(0) += 1;
    }
  }
  assert_eq!(per_bond.len(), graph.bond_count());
  assert_eq!(
    per_bond.values().filter(|&&count| count == 7).count() + solids(&drawing),
    3
  );
  assert_eq!(graph, before);
  assert!(!svg::export(&drawing, &svg::SvgOptions::default())?.contains("NaN"));
  assert_eq!(write_smiles(&graph)?, write_smiles(&before)?);
  Ok(())
}

#[test]
fn degenerate_coordinates_should_not_silently_drop_supplied_stereo() -> TestResult {
  let graph = read_smiles("[C@](F)(Cl)(Br)I")?.into_graph();
  let points = graph
    .atoms()
    .enumerate()
    .map(|(i, (id, _))| (id, Point::new(i as f64, 0.)))
    .collect();
  assert!(matches!(
    depict::depict(&graph, &Layout2d::from_positions(points)?, &geometry_options()),
    Err(depict::DepictError::Stereo(_))
  ));
  Ok(())
}

#[test]
fn polygon_scene_should_validate_geometry_and_preserve_alpha_and_bounds() -> TestResult {
  let polygon = Polygon {
    points: vec![Point::new(0., 0.), Point::new(2., 0.), Point::new(1., 3.)],
    color: Color {
      alpha: 128,
      ..Color::BLACK
    },
    source: None,
  };
  let drawing = Drawing::new(vec![Primitive::Polygon(polygon.clone())])?;
  assert_eq!(drawing.bounds().map(|b| (b.width(), b.height())), Some((2., 3.)));
  assert!(svg::export(&drawing, &svg::SvgOptions::default())?.contains("fill-opacity=\"0.5019607843137255\""));
  assert!(matches!(
    Drawing::new(vec![Primitive::Polygon(Polygon {
      points: vec![],
      ..polygon.clone()
    })]),
    Err(DrawingError::Dimensions(0))
  ));
  assert!(
    Drawing::new(vec![Primitive::Polygon(Polygon {
      points: vec![Point::new(f64::NAN, 0.); 3],
      ..polygon
    })])
    .is_err()
  );
  Ok(())
}

#[test]
fn wedge_width_should_be_configurable_and_invalid_widths_rejected() -> TestResult {
  let graph = read_smiles("[C@](F)(Cl)(Br)I")?.into_graph();
  let coordinates = cross(&graph, false, 0.)?;
  for width in [4., 9.] {
    let drawing = depict::depict(
      &graph,
      &coordinates,
      &DepictOptions {
        wedge_width: width,
        ..geometry_options()
      },
    )?;
    let polygon = drawing
      .primitives()
      .iter()
      .find_map(|p| if let Primitive::Polygon(p) = p { Some(p) } else { None })
      .ok_or("missing wedge")?;
    assert!((polygon.points[1].distance(polygon.points[2]) - width).abs() < 1e-8);
  }
  for width in [0., -1., f64::NAN] {
    assert_eq!(
      depict::depict(
        &graph,
        &coordinates,
        &DepictOptions {
          wedge_width: width,
          ..geometry_options()
        }
      ),
      Err(depict::DepictError::Options)
    );
  }
  Ok(())
}

#[test]
fn reversing_bond_storage_should_not_reverse_the_wedge_origin() -> TestResult {
  let mut graph = read_smiles("[C@](F)(Cl)(Br)I")?.into_graph();
  let coordinates = cross(&graph, false, 0.)?;
  let before = depict::depict(&graph, &coordinates, &geometry_options())?;
  let descriptors: Vec<_> = graph.tetrahedral_centers().map(|(id, s)| (id, *s)).collect();
  let bonds: Vec<_> = graph.bonds().map(|(id, b)| (id, b.atoms(), b.order())).collect();
  for &(id, _, _) in &bonds {
    graph.remove_bond(id)?;
  }
  for &(_, [a, b], order) in bonds.iter().rev() {
    graph.add_bond(b, a, order)?;
  }
  for (id, stereo) in descriptors {
    graph.set_tetrahedral(id, Some(stereo))?;
  }
  let after = depict::depict(&graph, &coordinates, &geometry_options())?;
  let polygons = |drawing: &Drawing| {
    drawing
      .primitives()
      .iter()
      .filter_map(|p| {
        if let Primitive::Polygon(p) = p {
          Some(p.points.clone())
        } else {
          None
        }
      })
      .collect::<Vec<_>>()
  };
  assert_eq!(polygons(&before), polygons(&after));
  Ok(())
}
