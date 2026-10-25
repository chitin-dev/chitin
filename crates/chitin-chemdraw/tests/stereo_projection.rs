//! Automatic-coordinate failures must be repaired without changing source stereo.

use chitin_chem::io::smiles::read_smiles;
use chitin_chemdraw::{
  depict::{self, ChiralHydrogenMode, DepictOptions},
  font::FontFace,
  geometry::Point,
  layout::Layout2d,
  scene::{Primitive, Source},
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn repaired_authored_projection_should_preserve_external_font_metrics_and_source() -> TestResult {
  let (graph, coordinates) = antipodal(false)?;
  let before = graph.clone();
  let original = coordinates.clone();
  let mut options = DepictOptions::default();
  let drawing = depict::depict(&graph, &coordinates, &options)?;
  assert!(
    drawing
      .primitives()
      .iter()
      .any(|p| matches!(p,Primitive::Text(t) if matches!(t.source,Some(Source::AttachedHydrogen(_)))))
  );
  let entries = depict::measurement_requests(&graph, &options)?
    .into_iter()
    .map(|request| {
      let metrics = FontFace::NotoSans.measure(request.content(), request.font_size())?;
      Ok((request, metrics))
    })
    .collect::<Result<Vec<_>, chitin_chemdraw::font::FontError>>()?;
  options.font = FontFace::from_measurements("host/stereo-regression", entries)?;
  let external = depict::depict(&graph, &coordinates, &options)?;
  assert_eq!(drawing.bounds(), external.bounds());
  assert_eq!(graph, before);
  assert_eq!(coordinates, original);
  Ok(())
}

/// Builds a three-ligand projection with two annotated neighbors on opposite rays.
fn antipodal(collapsed: bool) -> Result<(chitin_chem::MolecularGraph, Layout2d), Box<dyn std::error::Error>> {
  let graph = read_smiles("[C@H]([C@H](F)Cl)([C@H](F)Cl)O")?.into_graph();
  let points = [
    (0., 0.),
    (1., 0.),
    (1.5, 0.866),
    (1.5, -0.866),
    (-1., 0.),
    (-1.5, 0.866),
    (-1.5, -0.866),
    if collapsed { (2., 0.) } else { (0., 1.) },
  ];
  let coordinates = Layout2d::from_positions(
    graph
      .atoms()
      .map(|(id, _)| id)
      .zip(points.map(|(x, y)| Point::new(x, y)))
      .collect(),
  )?;
  Ok((graph, coordinates))
}

#[test]
fn singular_source_wedge_should_expand_h_only_in_automatic_mode() -> TestResult {
  let (graph, coordinates) = antipodal(false)?;
  let strict = DepictOptions {
    chiral_hydrogens: ChiralHydrogenMode::Implicit,
    ..Default::default()
  };
  assert!(matches!(
    depict::depict(&graph, &coordinates, &strict),
    Err(depict::DepictError::StereoAssignment {
      reason: depict::StereoFailureReason::DegenerateProjection,
      ..
    })
  ));
  let root = graph.atoms().next().ok_or("missing center")?.0;
  let drawing = depict::depict(&graph, &coordinates, &DepictOptions::default())?;
  let hydrogen_hosts: Vec<_> = drawing
    .primitives()
    .iter()
    .filter_map(|p| match p {
      Primitive::Text(text) if text.content == "H" => match text.source {
        Some(Source::AttachedHydrogen(host)) => Some(host),
        _ => None,
      },
      _ => None,
    })
    .collect();
  assert_eq!(hydrogen_hosts, [root]);
  Ok(())
}

#[test]
fn a_truly_collapsed_ligand_projection_should_still_fail_explicitly() -> TestResult {
  let (graph, coordinates) = antipodal(true)?;
  assert!(matches!(
    depict::depict(&graph, &coordinates, &DepictOptions::default()),
    Err(depict::DepictError::StereoAssignment {
      reason: depict::StereoFailureReason::DegenerateProjection,
      ..
    })
  ));
  Ok(())
}
