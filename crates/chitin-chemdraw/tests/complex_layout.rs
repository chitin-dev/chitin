//! Coupled polycycles and explicit coordination use different drawing constraints.

use chitin_chem::{Element, coordination::CoordinationSite, io::smiles::read_smiles};
use chitin_chemdraw::{depict, geometry::Point, layout, pipeline};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn anchored_routing_should_compact_the_iron_peptide_without_sacrificing_clearance() -> TestResult {
  let graph = read_smiles(include_str!("fixtures/iron_chelate.smi").trim())?.into_graph();
  let center = graph
    .atoms()
    .find(|(_, atom)| atom.element() == Element::Iron)
    .map(|(id, _)| id)
    .ok_or("missing center")?;
  let site = CoordinationSite::new(&graph, center, graph.neighbors(center)?.map(|(_, edge)| edge))?;
  let disabled = layout::coordination::CoordinationOptions {
    routing_budget: 0,
    ..Default::default()
  };
  let before = layout::coordination::generate(&graph, &site, &Default::default(), &disabled)?;
  let after = layout::coordination::generate(&graph, &site, &Default::default(), &Default::default())?;
  let before_extent = squared_extent(&before);
  let after_extent = squared_extent(&after);
  assert!(
    after_extent < 0.98 * before_extent,
    "{after_extent} >= {}",
    0.98 * before_extent
  );
  assert_eq!(before.positions()[&center], after.positions()[&center]);
  assert!(after.diagnostics().is_empty(), "{:?}", after.diagnostics());
  assert!(pipeline::contacts(&graph, &depict::depict(&graph, &after, &Default::default())?)?.is_empty());
  Ok(())
}

/// Measures normalized drawing extent independently of translation or rotation.
fn squared_extent(coords: &layout::Layout2d) -> f64 {
  let n = coords.positions().len() as f64;
  let mean = Point::new(
    coords.positions().values().map(|p| p.x).sum::<f64>() / n,
    coords.positions().values().map(|p| p.y).sum::<f64>() / n,
  );
  coords
    .positions()
    .values()
    .map(|p| p.distance(mean).powi(2))
    .sum::<f64>()
    / n
}

#[test]
fn three_bidentate_ligands_should_form_a_clean_compact_core_without_assigning_stereo() -> TestResult {
  let graph = read_smiles("O1CCO[Fe]123(OCCO2)OCCO3")?.into_graph();
  let before = graph.clone();
  let center = graph
    .atoms()
    .find(|(_, atom)| atom.element() == Element::Iron)
    .map(|(id, _)| id)
    .ok_or("missing center")?;
  let site = CoordinationSite::new(&graph, center, graph.neighbors(center)?.map(|(_, edge)| edge))?;
  let options = layout::coordination::CoordinationOptions::default();
  let coordinates = layout::coordination::generate(&graph, &site, &Default::default(), &options)?;
  let scene = depict::depict(&graph, &coordinates, &Default::default())?;
  assert!(coordinates.diagnostics().is_empty(), "{:?}", coordinates.diagnostics());
  assert!(pipeline::contacts(&graph, &scene)?.is_empty());
  for (_, bond) in graph.bonds() {
    let [a, b] = bond.atoms();
    let target = if a == center || b == center {
      options.attachment_length
    } else {
      1.
    };
    assert!((coordinates.positions()[&a].distance(coordinates.positions()[&b]) - target).abs() < 1e-7);
  }
  assert_eq!(graph, before);
  Ok(())
}

#[test]
fn coupled_polycycle_should_clear_skeleton_and_actual_label_contacts_at_default_limits() -> TestResult {
  let graph = read_smiles(include_str!("fixtures/coupled_polycycle.smi").trim())?.into_graph();
  let before = graph.clone();
  let rendered = pipeline::generate(&graph, &Default::default(), &Default::default())?;
  assert!(
    rendered.layout.diagnostics().is_empty(),
    "{:?}",
    rendered.layout.diagnostics()
  );
  assert!(rendered.contacts.is_empty(), "{:?}", rendered.contacts);
  assert_eq!(rendered.layout.positions().len(), graph.atom_count());
  for (_, bond) in graph.bonds() {
    let [a, b] = bond.atoms();
    assert!((rendered.layout.positions()[&a].distance(rendered.layout.positions()[&b]) - 1.).abs() < 1e-8);
  }
  assert_eq!(graph, before);
  Ok(())
}

#[test]
fn declared_iron_chelate_should_keep_covalent_lengths_and_separate_all_six_attachments() -> TestResult {
  let graph = read_smiles(include_str!("fixtures/iron_chelate.smi").trim())?.into_graph();
  let before = graph.clone();
  let center = graph
    .atoms()
    .find(|(_, atom)| atom.element() == Element::Iron)
    .map(|(id, _)| id)
    .ok_or("no Fe")?;
  let site = CoordinationSite::new(&graph, center, graph.neighbors(center)?.map(|(_, id)| id))?;
  assert_eq!(site.attachments().len(), 6);
  let options = layout::coordination::CoordinationOptions::default();
  let coords = layout::coordination::generate(&graph, &site, &Default::default(), &options)?;
  assert_eq!(
    coords,
    layout::coordination::generate(&graph, &site, &Default::default(), &options)?
  );
  let rendered = pipeline::finish(&graph, coords, &Default::default(), &Default::default())?;
  assert!(
    rendered.layout.diagnostics().is_empty(),
    "{:?}",
    rendered.layout.diagnostics()
  );
  assert!(rendered.contacts.is_empty(), "{:?}", rendered.contacts);
  for (id, bond) in graph.bonds() {
    let target = if site.attachments().iter().any(|&(edge, _)| edge == id) {
      options.attachment_length
    } else {
      1.
    };
    let [a, b] = bond.atoms();
    assert!(
      (rendered.layout.positions()[&a].distance(rendered.layout.positions()[&b]) - target).abs() < 1e-7,
      "changed source bond {id:?}"
    );
  }
  assert!(rendered.layout.drawing_depths().is_empty());
  assert!(rendered.layout.bond_crossings().is_empty());
  assert_eq!(graph, before);
  Ok(())
}

#[test]
fn declared_site_should_fail_without_complete_closure_instead_of_publishing_stretched_bonds() -> TestResult {
  let graph = read_smiles("O[Fe]O")?.into_graph();
  let center = graph.atoms().nth(1).map(|(id, _)| id).ok_or("no center")?;
  let site = CoordinationSite::new(&graph, center, graph.neighbors(center)?.map(|(_, id)| id))?;
  let options = layout::coordination::CoordinationOptions {
    work_budget: 1,
    ..Default::default()
  };
  assert!(matches!(
    layout::coordination::generate(&graph, &site, &Default::default(), &options),
    Err(layout::coordination::CoordinationLayoutError::Closure)
  ));
  Ok(())
}

#[test]
fn scene_inspection_should_detect_a_double_auxiliary_crossing_a_nonincident_label() -> TestResult {
  let graph = read_smiles("CO.C=C")?.into_graph();
  let ids: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
  let points = [
    Point::new(-1., 0.),
    Point::new(0., 0.),
    Point::new(-1., 0.2),
    Point::new(1., 0.2),
  ];
  let positions = layout::Layout2d::from_positions(ids.iter().copied().zip(points).collect())?;
  let original = positions.clone();
  let scene = depict::depict(&graph, &positions, &Default::default())?;
  let overlaps = pipeline::contacts(&graph, &scene)?;
  assert!(
    overlaps
      .iter()
      .any(|d| matches!(d,pipeline::DrawingContact::LabelBond(atom,_) if *atom==ids[1]))
  );
  let small = depict::DepictOptions {
    font_size: 4.,
    ..Default::default()
  };
  assert!(pipeline::contacts(&graph, &depict::depict(&graph, &positions, &small)?)?.is_empty());
  assert_eq!(positions, original);
  let disabled = layout::LayoutOptions {
    refinement_passes: 0,
    ..Default::default()
  };
  let rendered = pipeline::finish(&graph, positions, &disabled, &Default::default())?;
  assert_eq!(rendered.layout, original);
  assert_eq!(rendered.contacts, overlaps);
  let foreign = read_smiles("CO.C=C")?.into_graph();
  assert!(pipeline::contacts(&foreign, &scene).is_err());
  Ok(())
}
