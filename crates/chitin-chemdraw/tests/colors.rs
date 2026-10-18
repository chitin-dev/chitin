//! Coloring is label-only and must not affect graph facts or drawing geometry.

use chitin_chem::{Element, io::smiles::read_smiles};
use chitin_chemdraw::{
  Color,
  depict::{self, ChiralHydrogenMode, ColorScheme, DepictOptions},
  layout::{self, LayoutOptions},
  scene::{Primitive, Source},
  svg,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// An arbitrary translucent override distinguishes palette styling from defaults.
const CUSTOM: Color = Color {
  red: 0x79,
  green: 0x31,
  blue: 0xa7,
  alpha: 128,
};

#[test]
fn builtin_palette_should_keep_documented_element_colors_stable() {
  let options = DepictOptions {
    color_scheme: ColorScheme::Element,
    ..Default::default()
  };
  for (element, [red, green, blue]) in [
    (Element::Oxygen, [0xe5, 0x39, 0x35]),
    (Element::Nitrogen, [0x30, 0x58, 0xd8]),
    (Element::Sulfur, [0xb5, 0x9b, 0x00]),
    (Element::Phosphorus, [0xd9, 0x77, 0x06]),
    (Element::Boron, [0xb4, 0x5d, 0x8a]),
    (Element::Silicon, [0x9c, 0x6b, 0x46]),
  ] {
    assert_eq!(
      options.label_color(element),
      Color {
        red,
        green,
        blue,
        alpha: 255
      }
    );
  }
  for element in [Element::Fluorine, Element::Chlorine, Element::Bromine, Element::Iodine] {
    assert_eq!(
      options.label_color(element),
      Color {
        red: 0x21,
        green: 0x8c,
        blue: 0x3c,
        alpha: 255
      }
    );
  }
}

#[test]
fn monochrome_should_remain_the_default_and_ignore_element_overrides() -> TestResult {
  let graph = read_smiles("NCC(O)S")?.into_graph();
  let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
  let defaults = DepictOptions::default();
  assert_eq!(defaults.color_scheme, ColorScheme::Monochrome);
  let mut options = defaults.clone();
  options.element_colors.insert(Element::Oxygen, CUSTOM);
  assert_eq!(
    depict::depict(&graph, &coordinates, &defaults)?,
    depict::depict(&graph, &coordinates, &options)?
  );
  Ok(())
}

#[test]
fn element_labels_and_their_attached_hydrogens_should_follow_the_same_palette() -> TestResult {
  let graph = read_smiles("NCC(O)S.CCl.CBr.CF.CI.CP")?.into_graph();
  let before = graph.clone();
  let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
  let original_coordinates = coordinates.clone();
  let options = DepictOptions {
    color_scheme: ColorScheme::Element,
    ..Default::default()
  };
  let drawing = depict::depict(&graph, &coordinates, &options)?;
  let mut colored = 0;
  for primitive in drawing.primitives() {
    match primitive {
      Primitive::Text(text) => {
        let Some(Source::Atom(id)) = text.source else {
          return Err("unexpected label source".into());
        };
        let element = graph.atom(id).ok_or("missing atom")?.element();
        assert_eq!(text.color, options.label_color(element));
        assert_ne!(text.color, Color::BLACK);
        assert_ne!(element, Element::Carbon);
        colored += 1;
      }
      Primitive::Line(line) => assert_eq!(line.color, options.foreground),
      Primitive::Polygon(polygon) => assert_eq!(polygon.color, options.foreground),
      _ => return Err("unexpected drawing primitive".into()),
    }
  }
  assert!(colored > 10);
  assert_eq!(graph, before);
  assert_eq!(coordinates, original_coordinates);
  Ok(())
}

#[test]
fn isotopes_charge_and_hydrogen_counts_should_inherit_the_host_override() -> TestResult {
  let graph = read_smiles("[15NH3+]CC([18O-])=O")?.into_graph();
  let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
  let mut options = DepictOptions {
    color_scheme: ColorScheme::Element,
    ..Default::default()
  };
  options.element_colors.insert(Element::Nitrogen, CUSTOM);
  options.element_colors.insert(Element::Oxygen, CUSTOM);
  let drawing = depict::depict(&graph, &coordinates, &options)?;
  let mut contents = Vec::new();
  for primitive in drawing.primitives() {
    if let Primitive::Text(text) = primitive {
      assert_eq!(text.color, CUSTOM);
      contents.push(text.content.as_str());
    }
  }
  for run in ["15", "18", "H", "3", "+", "-"] {
    assert!(contents.contains(&run), "missing {run}");
  }
  let output = svg::export(&drawing, &svg::SvgOptions::default())?;
  assert!(output.contains("fill=\"#7931a7\""));
  assert!(output.contains("fill-opacity=\"0.5019607843137255\""));
  assert!(output.contains("stroke=\"#000000\""));
  Ok(())
}

#[test]
fn coloring_should_change_only_text_colors_not_geometry_or_stereo_marks() -> TestResult {
  let graph = read_smiles("N[C@@H](C)C(=O)O")?.into_graph();
  let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
  for mode in [ChiralHydrogenMode::Implicit, ChiralHydrogenMode::Explicit] {
    let mono = DepictOptions {
      chiral_hydrogens: mode,
      ..Default::default()
    };
    let mut options = mono.clone();
    options.color_scheme = ColorScheme::Element;
    options.element_colors.insert(Element::Carbon, CUSTOM);
    let baseline = depict::depict(&graph, &coordinates, &mono)?;
    let colored = depict::depict(&graph, &coordinates, &options)?;
    let mut normalized = colored.primitives().to_vec();
    for primitive in &mut normalized {
      if let Primitive::Text(text) = primitive {
        if matches!(text.source, Some(Source::AttachedHydrogen(_))) {
          assert_eq!(text.color, CUSTOM);
        }
        text.color = mono.foreground;
      }
    }
    assert_eq!(normalized, baseline.primitives());
    assert_eq!(colored.bounds(), baseline.bounds());
  }
  Ok(())
}

#[test]
fn unlisted_elements_and_explicit_h_vertices_should_use_the_foreground_fallback() -> TestResult {
  let graph = read_smiles("[Na+].[H]O[H]")?.into_graph();
  let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
  let options = DepictOptions {
    color_scheme: ColorScheme::Element,
    foreground: CUSTOM,
    ..Default::default()
  };
  for element in [Element::Sodium, Element::Hydrogen, Element::Carbon] {
    assert_eq!(options.label_color(element), CUSTOM);
  }
  let drawing = depict::depict(&graph, &coordinates, &options)?;
  for primitive in drawing.primitives() {
    if let Primitive::Text(text) = primitive
      && let Some(Source::Atom(id)) = text.source
    {
      let atom = graph.atom(id).ok_or("no atom")?;
      assert_eq!(text.color, options.label_color(atom.element()));
    }
  }
  Ok(())
}
