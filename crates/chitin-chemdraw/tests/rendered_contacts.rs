//! Contact acceptance must see the same logical labels and filled wedges as users.

use chitin_chem::{AtomId, io::smiles::read_smiles};
use chitin_chemdraw::{
  depict::{self, ChargeStyle, DepictOptions},
  font::FontFace,
  geometry::Point,
  layout::{Layout2d, LayoutOptions},
  pipeline::{DrawingContact, contacts, finish},
  scene::{Circle, Color, Drawing, Group, Line, LineCap, Polygon, Primitive, Source, Text},
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// The sign's measured box is much smaller than the six-unit outer circle.
fn charge_marker(source: Source, center: Point) -> Result<Vec<Primitive>, Box<dyn std::error::Error>> {
  let metrics = FontFace::NotoSans.measure("-", 8.)?;
  Ok(vec![
    Primitive::Text(Text {
      font: FontFace::NotoSans,
      origin: Point::new(
        center.x - (metrics.min.x + metrics.max.x) / 2.,
        center.y - (metrics.min.y + metrics.max.y) / 2.,
      ),
      content: "-".into(),
      font_size: 8.,
      width: metrics.advance,
      color: Color::BLACK,
      source: Some(source),
    }),
    Primitive::Circle(Circle {
      center,
      radius: 5.,
      width: 2.,
      color: Color::BLACK,
      source: Some(source),
    }),
  ])
}

#[test]
fn a_label_touching_only_a_charge_circle_should_report_one_logical_contact() -> TestResult {
  let graph = read_smiles("[O-].O")?.into_graph();
  let atoms: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
  let mut pieces = charge_marker(Source::Atom(atoms[0]), Point::new(0., 0.))?;
  pieces.push(label(Source::Atom(atoms[1]), Point::new(5., 0.))?);
  let drawing = Drawing::new(vec![
    Primitive::Group(Group {
      drawing: Drawing::new(pieces.clone())?,
      opacity: 255,
    }),
    Primitive::Group(Group {
      drawing: Drawing::new(pieces)?,
      opacity: 255,
    }),
  ])?;
  assert_eq!(
    contacts(&graph, &drawing)?,
    [DrawingContact::Labels(atoms[0], atoms[1])]
  );
  Ok(())
}

#[test]
fn a_label_in_a_charge_circles_empty_box_corner_should_not_count_as_overlap() -> TestResult {
  let graph = read_smiles("[O-].O")?.into_graph();
  let atoms: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
  let mut pieces = charge_marker(Source::Atom(atoms[0]), Point::new(0., 0.))?;
  let mut other = label(Source::Atom(atoms[1]), Point::new(0., 0.))?;
  let Primitive::Text(text) = &mut other else {
    return Err("text".into());
  };
  let [min, _] = text.envelope()?;
  text.origin = Point::new(5.5 - min.x, 5.5 - min.y);
  pieces.push(other);
  assert!(contacts(&graph, &Drawing::new(pieces)?)?.is_empty());
  Ok(())
}

#[test]
fn charge_circle_pairs_should_use_radii_not_overlapping_box_corners() -> TestResult {
  let graph = read_smiles("[O-].[O-]")?.into_graph();
  let atoms: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
  for (center, overlaps) in [(Point::new(11., 0.), true), (Point::new(11., 11.), false)] {
    let mut pieces = charge_marker(Source::Atom(atoms[0]), Point::new(0., 0.))?;
    pieces.extend(charge_marker(Source::Atom(atoms[1]), center)?);
    let actual = contacts(&graph, &Drawing::new(pieces)?)?;
    assert_eq!(!actual.is_empty(), overlaps);
    if overlaps {
      assert_eq!(actual, [DrawingContact::Labels(atoms[0], atoms[1])]);
    }
  }
  Ok(())
}

#[test]
fn circle_stroke_contacts_should_include_both_outline_and_bond_widths() -> TestResult {
  let graph = read_smiles("[O-].CC")?.into_graph();
  let atom = graph.atoms().next().ok_or("atom")?.0;
  let bond = graph.bonds().next().ok_or("bond")?.0;
  let mut pieces = charge_marker(Source::Atom(atom), Point::new(0., 0.))?;
  let stroke = Primitive::Line(Line {
    start: Point::new(6.4, -2.),
    end: Point::new(6.4, 2.),
    width: 1.,
    caps: [LineCap::Butt; 2],
    color: Color::BLACK,
    source: Some(Source::Bond(bond)),
  });
  pieces.extend([stroke.clone(), stroke]);
  assert_eq!(
    contacts(&graph, &Drawing::new(pieces)?)?,
    [DrawingContact::LabelBond(atom, bond)]
  );
  Ok(())
}

#[test]
fn circle_stroke_contacts_should_respect_flat_and_round_caps() -> TestResult {
  let graph = read_smiles("[O-].CC")?.into_graph();
  let atom = graph.atoms().next().ok_or("atom")?.0;
  let bond = graph.bonds().next().ok_or("bond")?.0;
  for (cap, touches) in [(LineCap::Butt, false), (LineCap::Round, true)] {
    let mut pieces = charge_marker(Source::Atom(atom), Point::new(0., 0.))?;
    pieces.push(Primitive::Line(Line {
      start: Point::new(6.5, 0.),
      end: Point::new(10., 0.),
      width: 2.,
      caps: [cap, LineCap::Butt],
      color: Color::BLACK,
      source: Some(Source::Bond(bond)),
    }));
    assert_eq!(!contacts(&graph, &Drawing::new(pieces)?)?.is_empty(), touches);
  }
  Ok(())
}

#[test]
fn circle_polygon_contacts_should_test_filled_area_not_its_box() -> TestResult {
  let graph = read_smiles("[O-].CC")?.into_graph();
  let atom = graph.atoms().next().ok_or("atom")?.0;
  let bond = graph.bonds().next().ok_or("bond")?.0;
  for (points, touches) in [
    (
      vec![Point::new(5.5, -1.), Point::new(8., -1.), Point::new(8., 1.)],
      true,
    ),
    (
      vec![Point::new(5.5, 5.5), Point::new(8., 5.5), Point::new(8., 8.)],
      false,
    ),
    (
      vec![Point::new(-20., -20.), Point::new(40., -20.), Point::new(10., 40.)],
      true,
    ),
  ] {
    let mut pieces = charge_marker(Source::Atom(atom), Point::new(0., 0.))?;
    pieces.push(Primitive::Polygon(Polygon {
      points,
      color: Color::BLACK,
      source: Some(Source::Bond(bond)),
    }));
    assert_eq!(!contacts(&graph, &Drawing::new(pieces)?)?.is_empty(), touches);
  }
  Ok(())
}

#[test]
fn charge_circles_should_keep_incident_bond_exemptions_and_validate_sources() -> TestResult {
  let graph = read_smiles("[O-]C")?.into_graph();
  let atom = graph.atoms().next().ok_or("atom")?.0;
  let bond = graph.bonds().next().ok_or("bond")?.0;
  for (source, touches) in [(Source::Atom(atom), false), (Source::AttachedHydrogen(atom), true)] {
    let mut pieces = charge_marker(source, Point::new(0., 0.))?;
    pieces.push(enclosing_wedge(Source::Bond(bond)));
    assert_eq!(!contacts(&graph, &Drawing::new(pieces)?)?.is_empty(), touches);
  }
  let foreign = read_smiles("O")?.into_graph().atoms().next().ok_or("atom")?.0;
  let drawing = Drawing::new(vec![Primitive::Circle(Circle {
    center: Point::new(0., 0.),
    radius: 5.,
    width: 2.,
    color: Color::BLACK,
    source: Some(Source::Atom(foreign)),
  })])?;
  assert!(contacts(&graph, &drawing).is_err());
  Ok(())
}

#[test]
fn actual_circled_charge_contacts_should_be_reported_by_pipeline() -> TestResult {
  let graph = read_smiles("C([O-])N")?.into_graph();
  let atoms: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
  let options = DepictOptions {
    charge_style: ChargeStyle::Circled,
    ..Default::default()
  };
  let initial = Layout2d::from_positions(
    [
      (atoms[0], Point::new(0., -1.)),
      (atoms[1], Point::new(0., 0.)),
      (atoms[2], Point::new(5., 0.)),
    ]
    .into(),
  )?;
  let drawing = depict::depict(&graph, &initial, &options)?;
  let circle = drawing
    .leaf_primitives()
    .find_map(|p| match p {
      Primitive::Circle(c) => Some(c),
      _ => None,
    })
    .ok_or("charge circle")?;
  let n = drawing
    .leaf_primitives()
    .find_map(|p| match p {
      Primitive::Text(t) if t.source == Some(Source::Atom(atoms[2])) && t.content == "N" => Some(t),
      _ => None,
    })
    .ok_or("nitrogen label")?;
  let [min, max] = n.envelope()?;
  let x = circle.center.x + circle.radius + circle.width / 2. - 0.1 + (max.x - min.x) / 2.;
  // This placement leaves a circle-only contact after depiction's bounded
  // label shifts; a tiny overlap at the unshifted glyph is repaired earlier.
  let crowded = Layout2d::from_positions(
    [
      (atoms[0], Point::new(0., -1.)),
      (atoms[1], Point::new(0., 0.)),
      (
        atoms[2],
        Point::new(
          (x - 20.) / options.bond_length,
          (-circle.center.y - 9.) / options.bond_length,
        ),
      ),
    ]
    .into(),
  )?;
  let drawing = depict::depict(&graph, &crowded, &options)?;
  let without_circle = Drawing::new(
    drawing
      .primitives()
      .iter()
      .filter(|p| !matches!(p, Primitive::Circle(_)))
      .cloned()
      .collect(),
  )?;
  assert!(contacts(&graph, &without_circle)?.is_empty());
  let expected = vec![DrawingContact::Labels(atoms[1], atoms[2])];
  assert_eq!(contacts(&graph, &drawing)?, expected);
  let baseline = finish(
    &graph,
    crowded.clone(),
    &LayoutOptions {
      refinement_passes: 0,
      ..Default::default()
    },
    &options,
  )?;
  assert_eq!(baseline.contacts, expected);
  let result = finish(
    &graph,
    crowded,
    &LayoutOptions {
      refinement_passes: 2,
      refinement_budget: 1024,
      ..Default::default()
    },
    &options,
  )?;
  // Repair is bounded and may retain a collision, but must never declare a
  // drawing clean just because its remaining contact is a charge outline.
  assert_eq!(result.contacts, contacts(&graph, &result.drawing)?);
  assert!(result.contacts.len() <= baseline.contacts.len());
  Ok(())
}

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
