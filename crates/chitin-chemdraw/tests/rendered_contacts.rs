//! Contact acceptance must see the same logical labels and filled wedges as users.

use chitin_chem::{AtomId, io::smiles::read_smiles};
use chitin_chemdraw::{
  font::FontFace,
  geometry::Point,
  pipeline::{DrawingContact, contacts},
  scene::{Color, Drawing, Line, LineCap, Polygon, Primitive, Source, Text},
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Places reproducibly measured text without depending on installed fonts.
fn label(source: Source, origin: Point) -> Result<Primitive, Box<dyn std::error::Error>> {
  Ok(Primitive::Text(Text {
    font: FontFace::NotoSans,
    origin,
    content: "O".into(),
    font_size: 12.,
    width: FontFace::NotoSans.measure("O", 12.)?.advance,
    color: Color::BLACK,
    source: Some(source),
  }))
}

/// A large wedge encloses the entire label without crossing any of its box edges.
fn enclosing_wedge(source: Source) -> Primitive {
  Primitive::Polygon(Polygon {
    points: vec![Point::new(-20., -20.), Point::new(40., -20.), Point::new(10., 40.)],
    color: Color::BLACK,
    source: Some(source),
  })
}

#[test]
fn a_label_inside_a_filled_wedge_should_report_one_source_contact() -> TestResult {
  let graph = read_smiles("CC.O")?.into_graph();
  let oxygen = graph.atoms().last().ok_or("missing atom")?.0;
  let bond = graph.bonds().next().ok_or("missing bond")?.0;
  let wedge = enclosing_wedge(Source::Bond(bond));
  let drawing = Drawing::new(vec![
    label(Source::Atom(oxygen), Point::new(0., 0.))?,
    wedge.clone(),
    wedge,
  ])?;
  assert_eq!(contacts(&graph, &drawing)?, [DrawingContact::LabelBond(oxygen, bond)]);
  Ok(())
}

#[test]
fn a_wedge_bounding_box_alone_should_not_report_a_contact() -> TestResult {
  let graph = read_smiles("CC.O")?.into_graph();
  let oxygen = graph.atoms().last().ok_or("missing atom")?.0;
  let bond = graph.bonds().next().ok_or("missing bond")?.0;
  let drawing = Drawing::new(vec![
    label(Source::Atom(oxygen), Point::new(14., 18.))?,
    Primitive::Polygon(Polygon {
      points: vec![Point::new(0., 0.), Point::new(20., 0.), Point::new(0., 20.)],
      color: Color::BLACK,
      source: Some(Source::Bond(bond)),
    }),
  ])?;
  assert!(contacts(&graph, &drawing)?.is_empty());
  Ok(())
}

#[test]
fn an_expanded_hydrogen_should_be_distinct_from_its_host_label() -> TestResult {
  let graph = read_smiles("C")?.into_graph();
  let host = graph.atoms().next().ok_or("missing atom")?.0;
  let sources = [Source::Atom(host), Source::AttachedHydrogen(host)];
  let drawing = Drawing::new(
    sources
      .map(|source| label(source, Point::new(0., 0.)))
      .into_iter()
      .collect::<Result<_, _>>()?,
  )?;
  assert_eq!(
    contacts(&graph, &drawing)?,
    [DrawingContact::AuxiliaryLabels(
      sources[0].min(sources[1]),
      sources[0].max(sources[1])
    )]
  );
  Ok(())
}

#[test]
fn an_expanded_hydrogen_should_not_inherit_its_hosts_incident_bond_exemption() -> TestResult {
  let graph = read_smiles("CC")?.into_graph();
  let host = graph.atoms().next().ok_or("missing atom")?.0;
  let bond = graph.bonds().next().ok_or("missing bond")?.0;
  let drawing = Drawing::new(vec![
    label(Source::AttachedHydrogen(host), Point::new(0., 0.))?,
    Primitive::Line(Line {
      start: Point::new(-5., -4.),
      end: Point::new(20., -4.),
      width: 1.,
      color: Color::BLACK,
      caps: [LineCap::Butt; 2],
      source: Some(Source::Bond(bond)),
    }),
  ])?;
  assert_eq!(
    contacts(&graph, &drawing)?,
    [DrawingContact::AuxiliaryLabelStroke(
      Source::AttachedHydrogen(host),
      Source::Bond(bond)
    )]
  );
  Ok(())
}

#[test]
fn an_expanded_hydrogen_should_exclude_only_its_own_drawn_bond() -> TestResult {
  let graph = read_smiles("C.O")?.into_graph();
  let ids: Vec<AtomId> = graph.atoms().map(|(id, _)| id).collect();
  let drawing = Drawing::new(vec![
    label(Source::AttachedHydrogen(ids[0]), Point::new(0., 0.))?,
    label(Source::Atom(ids[1]), Point::new(0., 0.))?,
    enclosing_wedge(Source::AttachedHydrogen(ids[0])),
  ])?;
  let actual = contacts(&graph, &drawing)?;
  assert!(actual.contains(&DrawingContact::AuxiliaryLabelStroke(
    Source::Atom(ids[1]),
    Source::AttachedHydrogen(ids[0])
  )));
  assert!(!actual.contains(&DrawingContact::AuxiliaryLabelStroke(
    Source::AttachedHydrogen(ids[0]),
    Source::AttachedHydrogen(ids[0])
  )));
  Ok(())
}

#[test]
fn foreign_auxiliary_sources_should_be_rejected_before_contact_testing() -> TestResult {
  let graph = read_smiles("C")?.into_graph();
  let other = read_smiles("C")?.into_graph();
  let foreign = other.atoms().next().ok_or("missing atom")?.0;
  let drawing = Drawing::new(vec![enclosing_wedge(Source::AttachedHydrogen(foreign))])?;
  assert!(contacts(&graph, &drawing).is_err());
  Ok(())
}
