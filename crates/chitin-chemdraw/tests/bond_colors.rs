//! Endpoint color seams are scene geometry, not an SVG-only palette effect.

use chitin_chem::{Element, io::smiles::read_smiles};
use chitin_chemdraw::{
  Color,
  depict::{self, BondColorStyle, ColorScheme, DepictOptions},
  geometry::Point,
  layout::Layout2d,
  scene::{LineCap, Primitive, Source},
  svg,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn defaults_should_color_labels_and_bond_halves_by_element() {
  let options = DepictOptions::default();
  assert_eq!(options.color_scheme, ColorScheme::Element);
  assert_eq!(options.bond_color_style, BondColorStyle::Split);
}

#[test]
fn clipped_single_double_and_triple_strokes_should_share_the_original_axis_bisector() -> TestResult {
  for (smiles, strokes) in [("CO", 1), ("C=O", 2), ("C#N", 3), ("NO", 1)] {
    let graph = read_smiles(smiles)?.into_graph();
    let before = graph.clone();
    let atoms: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
    for reverse in [false, true] {
      let coordinates = Layout2d::from_positions(
        atoms
          .iter()
          .enumerate()
          .map(|(i, &id)| (id, Point::new(if reverse { 1. - i as f64 } else { i as f64 }, 0.)))
          .collect(),
      )?;
      let original = coordinates.clone();
      let options = DepictOptions::default();
      let drawing = depict::depict(&graph, &coordinates, &options)?;
      let lines: Vec<_> = drawing
        .primitives()
        .iter()
        .filter_map(|p| match p {
          Primitive::Line(line) => Some(line),
          _ => None,
        })
        .collect();
      assert_eq!(lines.len(), strokes * 2, "{smiles}");
      for line in lines {
        let center = options.bond_length / 2.;
        let side = (line.start.x + line.end.x) / 2.;
        let index = usize::from((side > center) != reverse);
        let element = graph.atom(atoms[index]).ok_or("missing atom")?.element();
        assert_eq!(line.color, options.label_color(element));
        assert!(line.start.x == center || line.end.x == center, "{smiles}: {line:?}");
        let seam = if line.start.x == center { 0 } else { 1 };
        assert_eq!(line.caps[seam], LineCap::Butt);
        assert_eq!(line.caps[1 - seam], LineCap::Round);
        assert!(matches!(line.source, Some(Source::Bond(_))));
      }
      assert_eq!(coordinates, original);
    }
    assert_eq!(graph, before);
  }
  Ok(())
}

#[test]
fn equal_endpoint_colors_should_not_add_seams_and_uniform_should_keep_labels_colored() -> TestResult {
  for (source, options) in [
    ("NN", DepictOptions::default()),
    (
      "CO",
      DepictOptions {
        color_scheme: ColorScheme::Monochrome,
        ..Default::default()
      },
    ),
    (
      "CO",
      DepictOptions {
        bond_color_style: BondColorStyle::Uniform,
        ..Default::default()
      },
    ),
  ] {
    let graph = read_smiles(source)?.into_graph();
    let coordinates = chitin_chemdraw::layout::generate(&graph, &Default::default())?;
    let drawing = depict::depict(&graph, &coordinates, &options)?;
    let lines: Vec<_> = drawing
      .primitives()
      .iter()
      .filter_map(|p| match p {
        Primitive::Line(line) => Some(line),
        _ => None,
      })
      .collect();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].caps, [LineCap::Round; 2]);
    if options.bond_color_style == BondColorStyle::Uniform {
      assert_eq!(lines[0].color, options.foreground);
      assert!(
        drawing
          .primitives()
          .iter()
          .any(|p| matches!(p, Primitive::Text(text) if text.color == options.label_color(Element::Oxygen)))
      );
    }
  }
  Ok(())
}

#[test]
fn transparent_split_strokes_should_export_flat_seams_without_circle_overdraw() -> TestResult {
  let graph = read_smiles("CO")?.into_graph();
  let coordinates = chitin_chemdraw::layout::generate(&graph, &Default::default())?;
  let mut options = DepictOptions::default();
  options.element_colors.insert(
    Element::Oxygen,
    Color {
      red: 255,
      green: 0,
      blue: 0,
      alpha: 128,
    },
  );
  let drawing = depict::depict(&graph, &coordinates, &options)?;
  let output = svg::export(
    &drawing,
    &svg::SvgOptions {
      background: None,
      ..Default::default()
    },
  )?;
  assert!(output.contains("fill=\"#ff0000\" fill-opacity=\"0.5019607843137255\""));
  assert!(output.contains("A0.8 0.8 0 0 0"));
  assert!(!output.contains("<circle"));
  Ok(())
}

#[test]
fn stereo_marks_should_retain_bond_sources_and_use_endpoint_colors() -> TestResult {
  for source in ["[C@](F)(Cl)(Br)I", "[C@@](F)(Cl)(Br)I"] {
    let graph = read_smiles(source)?.into_graph();
    let before = graph.clone();
    let coordinates = chitin_chemdraw::layout::generate(&graph, &Default::default())?;
    let options = DepictOptions::default();
    let drawing = depict::depict(&graph, &coordinates, &options)?;
    let mut colors = Vec::new();
    for primitive in drawing.primitives() {
      let (color, source) = match primitive {
        Primitive::Line(line) => (line.color, line.source),
        Primitive::Polygon(polygon) => (polygon.color, polygon.source),
        _ => continue,
      };
      let Some(Source::Bond(id)) = source else {
        return Err("lost stereo bond source".into());
      };
      let bond = graph.bond(id).ok_or("missing bond")?;
      let [a, b] = bond.atoms();
      let endpoints = [
        graph.atom(a).ok_or("missing atom")?.element(),
        graph.atom(b).ok_or("missing atom")?.element(),
      ];
      assert!(
        endpoints
          .into_iter()
          .any(|element| options.label_color(element) == color)
      );
      colors.push(color);
    }
    assert!(colors.contains(&options.foreground));
    assert!(colors.contains(&options.label_color(Element::Fluorine)));
    assert_eq!(graph, before);
  }
  Ok(())
}

#[test]
fn generated_chiral_hydrogen_should_use_h_palette_without_inventing_graph_handles() -> TestResult {
  let graph = read_smiles("N[C@@H](C)C(=O)O")?.into_graph();
  let before = graph.clone();
  let coordinates = chitin_chemdraw::layout::generate(&graph, &Default::default())?;
  let mut options = DepictOptions {
    chiral_hydrogens: depict::ChiralHydrogenMode::Explicit,
    ..Default::default()
  };
  let h_color = Color {
    red: 144,
    green: 144,
    blue: 144,
    alpha: 255,
  };
  options.element_colors.insert(Element::Hydrogen, h_color);
  let drawing = depict::depict(&graph, &coordinates, &options)?;
  let mut seen = false;
  for primitive in drawing.primitives() {
    if let Primitive::Text(text) = primitive
      && let Some(Source::AttachedHydrogen(host)) = text.source
    {
      assert!(graph.atom(host).is_some());
      assert_eq!(text.color, h_color);
      seen = true;
    }
  }
  assert!(seen);
  assert!(drawing.primitives().iter().any(|p| match p {
    Primitive::Line(line) => matches!(line.source, Some(Source::AttachedHydrogen(_))) && line.color == h_color,
    Primitive::Polygon(polygon) =>
      matches!(polygon.source, Some(Source::AttachedHydrogen(_))) && polygon.color == h_color,
    _ => false,
  }));
  assert_eq!(graph, before);
  Ok(())
}
