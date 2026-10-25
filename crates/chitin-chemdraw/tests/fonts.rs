//! Immutable native fonts and the two-stage external measurement boundary.

use chitin_chem::io::smiles::read_smiles;
use chitin_chemdraw::{
  depict::{self, ChiralHydrogenMode, DepictError, DepictOptions},
  font::{FontError, FontFace, TextMetrics, TextRequest},
  geometry::Point,
  layout::{self, LayoutOptions},
  scene::{Drawing, Primitive, Text},
  svg::{self, SvgError, SvgOptions},
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[path = "support/layout_cases.rs"]
mod layout_cases;

#[test]
fn complete_visual_corpus_should_use_the_same_host_label_contract() -> TestResult {
  for case in layout_cases::load()? {
    let graph = read_smiles(case.source)?.into_graph();
    assert_eq!(
      graph.bond_count() + graph.connected_components().len() - graph.atom_count(),
      case.rank
    );
    let positions = layout::generate(&graph, &LayoutOptions::default())?;
    for chiral_hydrogens in [
      ChiralHydrogenMode::Auto,
      ChiralHydrogenMode::Implicit,
      ChiralHydrogenMode::Explicit,
    ] {
      let mut options = DepictOptions {
        chiral_hydrogens,
        ..Default::default()
      };
      let native = depict::depict(&graph, &positions, &options)?;
      options.font = measured_font(&depict::measurement_requests(&graph, &options)?, 1.)?;
      let host = depict::depict(&graph, &positions, &options)?;
      assert_eq!(native.bounds(), host.bounds(), "{}", case.name);
      assert_eq!(text_geometry(&native), text_geometry(&host), "{}", case.name);
    }
  }
  Ok(())
}

/// Uses an existing repository font so tests need no installed-font discovery.
fn mono() -> Result<FontFace, FontError> {
  FontFace::from_bytes(
    include_bytes!("../../../assets/fonts/cascadia-mono/CascadiaMono-Regular.ttf").as_slice(),
    0,
  )
}

/// Measures the exact request set with a known provider, as a host would.
fn measured_font(requests: &[TextRequest], horizontal_scale: f64) -> Result<FontFace, FontError> {
  let entries = requests
    .iter()
    .map(|request| {
      let mut metrics = FontFace::NotoSans.measure(request.content(), request.font_size())?;
      metrics.advance *= horizontal_scale;
      metrics.min.x *= horizontal_scale;
      metrics.max.x *= horizontal_scale;
      Ok((request.clone(), metrics))
    })
    .collect::<Result<Vec<_>, FontError>>()?;
  FontFace::from_measurements("document-font/regular/features-default", entries)
}

/// Compares placed text independently of whether native or host metrics backed it.
fn text_geometry(drawing: &Drawing) -> Vec<(String, Point, f64, f64)> {
  drawing
    .primitives()
    .iter()
    .filter_map(|primitive| match primitive {
      Primitive::Text(run) => Some((run.content.clone(), run.origin, run.font_size, run.width)),
      _ => None,
    })
    .collect()
}

#[test]
fn caller_font_should_control_actual_metrics_without_system_lookup() -> TestResult {
  let font = mono()?;
  assert!((font.measure("III", 18.)?.advance - font.measure("WWW", 18.)?.advance).abs() < 1e-8);
  assert!(FontFace::NotoSans.measure("III", 18.)?.advance < FontFace::NotoSans.measure("WWW", 18.)?.advance);
  assert_ne!(font.measure("O", 18.)?, FontFace::NotoSans.measure("O", 18.)?);
  let copy = font.clone();
  assert!(std::ptr::eq(
    font.data().ok_or("missing bytes")?,
    copy.data().ok_or("missing cloned bytes")?
  ));
  Ok(())
}

#[test]
fn invalid_bytes_and_nonexistent_collection_faces_should_fail_before_use() -> TestResult {
  assert_eq!(FontFace::from_bytes(vec![0_u8; 32], 0), Err(FontError::InvalidFont));
  let bytes = FontFace::NotoSans.data().ok_or("missing asset")?;
  assert_eq!(FontFace::from_bytes(bytes, 1), Err(FontError::InvalidFont));
  Ok(())
}

#[test]
fn custom_face_should_survive_options_and_ownership_changes_in_svg_output() -> TestResult {
  let graph = read_smiles("[13NH2+]CO")?.into_graph();
  let positions = layout::generate(&graph, &LayoutOptions::default())?;
  let drawing = {
    let options = DepictOptions {
      font: mono()?,
      ..Default::default()
    };
    depict::depict(&graph, &positions, &options)?
  };
  for primitive in drawing.primitives() {
    if let Primitive::Text(run) = primitive {
      assert_eq!(run.font, mono()?);
      assert_eq!(run.width, run.font.measure(&run.content, run.font_size)?.advance);
    }
  }
  let output = svg::export(&drawing, &SvgOptions::default())?;
  let default = depict::depict(&graph, &positions, &DepictOptions::default())?;
  assert_ne!(output, svg::export(&default, &SvgOptions::default())?);
  assert!(!output.contains("font-family") && output.contains("<path "));
  assert_eq!(output, svg::export(&drawing, &SvgOptions::default())?);
  Ok(())
}

#[test]
fn selective_h_expansion_should_request_exact_host_glyphs() -> TestResult {
  let graph = read_smiles("[C@H]([C@H](F)Cl)([C@H](F)Cl)[C@H](F)Cl")?.into_graph();
  let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
  let mut options = DepictOptions::default();
  let before = graph.clone();
  let requests = depict::measurement_requests(&graph, &options)?;
  assert!(requests.contains(&TextRequest::new("H", options.font_size)?));
  let native = depict::depict(&graph, &coordinates, &options)?;
  options.font = measured_font(&requests, 1.)?;
  let external = depict::depict(&graph, &coordinates, &options)?;
  assert_eq!(text_geometry(&native), text_geometry(&external));
  assert_eq!(native.bounds(), external.bounds());
  assert_eq!(graph, before);
  Ok(())
}

#[test]
fn external_request_sets_should_cover_every_final_text_run_in_all_h_modes() -> TestResult {
  for source in [
    "[13NH2+]CO",
    "[Ca+2].[O-]C(=O)C",
    "[2H]O",
    "c1cc[nH]c1",
    "F[C@H](Cl)Br",
    "C",
    "C/C=C\\O",
  ] {
    let graph = read_smiles(source)?.into_graph();
    let before = graph.clone();
    let positions = layout::generate(&graph, &LayoutOptions::default())?;
    for mode in [
      ChiralHydrogenMode::Auto,
      ChiralHydrogenMode::Implicit,
      ChiralHydrogenMode::Explicit,
    ] {
      let mut options = DepictOptions {
        chiral_hydrogens: mode,
        ..Default::default()
      };
      let requests = depict::measurement_requests(&graph, &options)?;
      assert!(requests.windows(2).all(|pair| pair[0] < pair[1]));
      let native = depict::depict(&graph, &positions, &options)?;
      options.font = measured_font(&requests, 1.)?;
      let external = depict::depict(&graph, &positions, &options)?;
      assert_eq!(text_geometry(&native), text_geometry(&external), "{source}");
      assert_eq!(native.bounds(), external.bounds());
      for primitive in external.primitives() {
        if let Primitive::Text(run) = primitive {
          assert!(
            requests.contains(&TextRequest::new(&run.content, run.font_size)?),
            "{source}: {}",
            run.content
          );
          assert_eq!(run.font.host_identity(), Some("document-font/regular/features-default"));
        }
      }
    }
    assert_eq!(graph, before);
  }
  Ok(())
}

#[test]
fn requesting_metrics_should_not_require_a_placeholder_font_or_geometry() -> TestResult {
  let graph = read_smiles("[13NH2+]CO")?.into_graph();
  let mut options = DepictOptions::default();
  let requests = depict::measurement_requests(&graph, &options)?;
  options.font = FontFace::from_measurements("not-measured-yet", [])?;
  assert_eq!(requests, depict::measurement_requests(&graph, &options)?);
  assert!(requests.contains(&TextRequest::new("13", options.font_size * 0.7)?));
  assert!(requests.contains(&TextRequest::new("2", options.font_size * 0.7)?));
  assert!(requests.contains(&TextRequest::new("+", options.font_size * 0.7)?));
  Ok(())
}

#[test]
fn wider_host_labels_should_change_clipping_and_scene_bounds() -> TestResult {
  let graph = read_smiles("CO")?.into_graph();
  let positions = layout::generate(&graph, &LayoutOptions::default())?;
  let mut options = DepictOptions::default();
  let native = depict::depict(&graph, &positions, &options)?;
  options.font = measured_font(&depict::measurement_requests(&graph, &options)?, 1.5)?;
  let wide = depict::depict(&graph, &positions, &options)?;
  let lines = |drawing: &Drawing| {
    drawing
      .primitives()
      .iter()
      .filter_map(|primitive| match primitive {
        Primitive::Line(line) => Some((line.start, line.end)),
        _ => None,
      })
      .collect::<Vec<_>>()
  };
  assert_ne!(lines(&native), lines(&wide));
  assert_ne!(native.bounds(), wide.bounds());
  assert_eq!(wide, depict::depict(&graph, &positions, &options)?);
  Ok(())
}

#[test]
fn element_anchor_should_be_centered_using_selected_font_envelope() -> TestResult {
  let graph = read_smiles("O")?.into_graph();
  let id = graph.atoms().next().map(|(id, _)| id).ok_or("missing atom")?;
  let positions = layout::generate(&graph, &LayoutOptions::default())?;
  for font in [FontFace::NotoSans, mono()?] {
    let options = DepictOptions {
      font,
      ..Default::default()
    };
    let drawing = depict::depict(&graph, &positions, &options)?;
    let oxygen = drawing
      .primitives()
      .iter()
      .find_map(|primitive| match primitive {
        Primitive::Text(run) if run.content == "O" => Some(run),
        _ => None,
      })
      .ok_or("missing O")?;
    let [min, max] = oxygen.envelope()?;
    let point = positions.position(id).ok_or("missing position")?;
    assert!(((min.x + max.x) / 2. - point.x * options.bond_length).abs() < 1e-8);
    assert!(((min.y + max.y) / 2. + point.y * options.bond_length).abs() < 1e-8);
  }
  Ok(())
}

#[test]
fn missing_or_stale_host_measurements_should_never_use_default_metrics() -> TestResult {
  let graph = read_smiles("O")?.into_graph();
  let positions = layout::generate(&graph, &LayoutOptions::default())?;
  let mut options = DepictOptions::default();
  let requests = depict::measurement_requests(&graph, &options)?;
  options.font = measured_font(&requests, 1.)?;
  options.font_size += 1.;
  assert!(matches!(
    depict::depict(&graph, &positions, &options),
    Err(DepictError::Font(FontError::MissingMeasurement(_)))
  ));
  options.font_size -= 1.;
  options.font = FontFace::from_measurements("incomplete", [])?;
  assert!(matches!(
    depict::depict(&graph, &positions, &options),
    Err(DepictError::Font(FontError::MissingMeasurement(_)))
  ));
  Ok(())
}

#[test]
fn host_only_scene_should_refuse_outline_export_without_substituting_fonts() -> TestResult {
  let graph = read_smiles("O")?.into_graph();
  let positions = layout::generate(&graph, &LayoutOptions::default())?;
  let mut options = DepictOptions::default();
  options.font = measured_font(&depict::measurement_requests(&graph, &options)?, 1.)?;
  assert!(options.font.data().is_none() && options.font.face_index().is_none());
  let drawing = depict::depict(&graph, &positions, &options)?;
  assert_eq!(
    svg::export(&drawing, &SvgOptions::default()),
    Err(SvgError::Font(FontError::OutlinesUnavailable))
  );
  Ok(())
}

#[test]
fn invalid_metrics_duplicate_requests_and_invalid_identity_should_fail_atomically() -> TestResult {
  let request = TextRequest::new("O", 12.)?;
  let good = FontFace::NotoSans.measure("O", 12.)?;
  for bad in [
    TextMetrics { advance: 0., ..good },
    TextMetrics {
      advance: f64::NAN,
      ..good
    },
    TextMetrics {
      min: Point::new(10., 10.),
      max: Point::new(0., 0.),
      ..good
    },
    TextMetrics {
      min: Point::new(f64::NEG_INFINITY, 0.),
      ..good
    },
    TextMetrics {
      min: Point::new(-f64::MAX, 0.),
      max: Point::new(f64::MAX, 1.),
      ..good
    },
  ] {
    assert_eq!(
      FontFace::from_measurements("bad-metrics", [(request.clone(), bad)]),
      Err(FontError::Measurement)
    );
  }
  assert!(matches!(
    FontFace::from_measurements("duplicate", [(request.clone(), good), (request.clone(), good)]),
    Err(FontError::DuplicateMeasurement(_))
  ));
  for identity in ["", "  ", "font\nname"] {
    assert_eq!(
      FontFace::from_measurements(identity, [(request.clone(), good)]),
      Err(FontError::Identity)
    );
  }
  for size in [0., -1., f64::NAN, f64::INFINITY] {
    assert!(TextRequest::new("O", size).is_err());
  }
  assert!(TextRequest::new("", 12.).is_err());
  Ok(())
}

#[test]
fn host_envelopes_should_retain_overhangs_and_descenders_in_scene_bounds() -> TestResult {
  let font = FontFace::from_measurements(
    "host-serif/italic",
    [(
      TextRequest::new("Cl", 12.)?,
      TextMetrics {
        advance: 10.,
        min: Point::new(-3., -9.),
        max: Point::new(11., 3.),
      },
    )],
  )?;
  let drawing = Drawing::new(vec![Primitive::Text(Text {
    font,
    content: "Cl".into(),
    font_size: 12.,
    width: 10.,
    origin: Point::new(20., 20.),
    color: chitin_chemdraw::Color::BLACK,
    source: None,
  })])?;
  let bounds = drawing.bounds().ok_or("missing bounds")?;
  assert_eq!(bounds.min(), Point::new(17., 11.));
  assert_eq!(bounds.max(), Point::new(31., 23.));
  Ok(())
}
