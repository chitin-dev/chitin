//! Charge labels and aromatic strokes must not share an atom-label identity.

use chitin_chem::{MolecularGraph, io::smiles::read_smiles};
use chitin_chemdraw::{
  Color, Drawing, Primitive,
  depict::{self, AromaticStyle, ChargeStyle, DepictOptions},
  diagram::{
    Anchor, Annotation, ArcPath, ArrowStyle, Diagram, ElectronArrow, ElectronCurve, ElectronHead, Highlight, Placement,
    RoutingOptions,
  },
  font::FontFace,
  geometry::Point,
  layout::{self, Layout2d},
  pipeline::{DrawingContact, contacts},
  scene::{Circle, Source, Text},
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Enables both circle-producing styles with ordinary deterministic coordinates.
fn fixture(smiles: &str) -> Result<(MolecularGraph, Layout2d, DepictOptions), Box<dyn std::error::Error>> {
  let graph = read_smiles(smiles)?.into_graph();
  let coordinates = layout::generate(&graph, &Default::default())?;
  let options = DepictOptions {
    aromatic_style: AromaticStyle::Circle,
    charge_style: ChargeStyle::Circled,
    ..Default::default()
  };
  Ok((graph, coordinates, options))
}

#[test]
fn both_circle_styles_should_have_distinct_namespace_valid_sources() -> TestResult {
  let (graph, coordinates, options) = fixture("[O-]c1ccccc1")?;
  let oxygen = graph.atoms().next().ok_or("atom")?.0;
  let drawing = depict::depict(&graph, &coordinates, &options)?;
  let mut charges = 0;
  let mut rings = 0;
  for primitive in drawing.leaf_primitives() {
    match primitive {
      Primitive::Circle(c) => match c.source {
        Some(Source::Charge(atom)) => {
          assert_eq!(atom, oxygen);
          charges += 1;
        }
        Some(Source::AromaticRing(edge)) => {
          let bond = graph.bond(edge).ok_or("ring locator")?;
          assert!(
            bond
              .atoms()
              .iter()
              .all(|id| graph.atom(*id).is_some_and(|a| a.is_aromatic()))
          );
          rings += 1;
        }
        _ => return Err("ambiguous circle provenance".into()),
      },
      Primitive::Text(t) if t.content == "-" => assert_eq!(t.source, Some(Source::Charge(oxygen))),
      _ => {}
    }
  }
  assert_eq!((charges, rings), (1, 1));
  Ok(())
}

#[test]
fn highlighting_one_aromatic_atom_should_not_include_the_inner_circle() -> TestResult {
  let (graph, coordinates, options) = fixture("[O-]c1ccccc1")?;
  let atoms: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
  let mut diagram = Diagram::new();
  let id = diagram.add_molecule(&graph, &coordinates, &options, Placement::default())?;
  diagram.annotate(Annotation::Highlight(Highlight {
    molecule: id,
    atoms: vec![atoms[1]],
    bonds: vec![],
    radius: 3.,
    color: Color::BLACK,
  }));
  let drawing = diagram.render()?;
  let Some(Primitive::Group(highlight)) = drawing.primitives().first() else {
    return Err("highlight".into());
  };
  assert_eq!(highlight.drawing.primitives().len(), 1);
  assert!(matches!(highlight.drawing.primitives()[0], Primitive::Disk(_)));
  assert_eq!(highlight.drawing.bounds().ok_or("bounds")?.width(), 6.);
  Ok(())
}

#[test]
fn highlighting_a_charged_atom_should_still_cover_its_charge_circle() -> TestResult {
  let (graph, coordinates, options) = fixture("[O-]c1ccccc1")?;
  let oxygen = graph.atoms().next().ok_or("atom")?.0;
  let mut diagram = Diagram::new();
  let id = diagram.add_molecule(&graph, &coordinates, &options, Placement::default())?;
  diagram.annotate(Annotation::Highlight(Highlight {
    molecule: id,
    atoms: vec![oxygen],
    bonds: vec![],
    radius: 3.,
    color: Color::BLACK,
  }));
  let drawing = diagram.render()?;
  let Some(Primitive::Group(highlight)) = drawing.primitives().first() else {
    return Err("highlight".into());
  };
  let bounds = highlight.drawing.bounds().ok_or("bounds")?;
  let charge = drawing
    .leaf_primitives()
    .find_map(|p| match p {
      Primitive::Circle(c) if c.source == Some(Source::Charge(oxygen)) => Some(c),
      _ => None,
    })
    .ok_or("charge")?;
  let radius = charge.radius + charge.width / 2.;
  assert!(bounds.min().x <= charge.center.x - radius && bounds.max().x >= charge.center.x + radius);
  assert!(bounds.min().y <= charge.center.y - radius && bounds.max().y >= charge.center.y + radius);
  Ok(())
}

#[test]
fn bezier_and_arc_clearance_should_not_use_the_aromatic_circle_as_a_label() -> TestResult {
  let (graph, coordinates, options) = fixture("n1ccccc1")?;
  let atoms: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
  for curve in [
    ElectronCurve::Bezier,
    ElectronCurve::Arc {
      sweep: std::f64::consts::FRAC_PI_2,
      path: ArcPath::Minor,
    },
  ] {
    let mut diagram = Diagram::new();
    let id = diagram.add_molecule(&graph, &coordinates, &options, Placement::default())?;
    let from = Anchor::atom(id, atoms[0]);
    let origin = diagram.resolve(from)?;
    // The manual curve may cross ring ink; only atom-label clearance is under
    // test here. Collision-aware routing is tested independently below.
    diagram.annotate(Annotation::ElectronArrow(ElectronArrow {
      curve,
      controls: None,
      from,
      to: Anchor::atom(id, atoms[3]),
      bend: 0.,
      head: ElectronHead::Pair,
      style: ArrowStyle {
        width: 0.5,
        head_length: 2.,
        head_width: 2.,
        clearance: 1.,
        ..Default::default()
      },
    }));
    let drawing = diagram.render()?;
    let start = drawing
      .leaf_primitives()
      .find_map(|p| match p {
        Primitive::CubicBezier(c) => Some(c.start),
        Primitive::CircularArc(c) => Some(c.point(0.)),
        _ => None,
      })
      .ok_or("electron curve")?;
    assert!(start.distance(origin) < options.font_size * 1.5);
  }
  Ok(())
}

#[test]
fn aromatic_circle_routing_should_reserve_only_the_circumference() -> TestResult {
  let (graph, coordinates, options) = fixture("n1ccccc1")?;
  let atoms: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
  let drawing = depict::depict(&graph, &coordinates, &options)?;
  let ring = drawing
    .leaf_primitives()
    .find_map(|p| match p {
      Primitive::Circle(c) if matches!(c.source, Some(Source::AromaticRing(_))) => Some(c),
      _ => None,
    })
    .ok_or("aromatic circle")?;
  let mut diagram = Diagram::new();
  let id = diagram.add_molecule(&graph, &coordinates, &options, Placement::default())?;
  let a = diagram.resolve(Anchor::atom(id, atoms[0]))?;
  let b = diagram.resolve(Anchor::atom(id, atoms[3]))?;
  let arrow = ElectronArrow {
    curve: ElectronCurve::Bezier,
    controls: None,
    from: Anchor::atom(id, atoms[0]).with_offset(Point::new(ring.center.x - 5. - a.x, ring.center.y - a.y)),
    to: Anchor::atom(id, atoms[3]).with_offset(Point::new(ring.center.x + 5. - b.x, ring.center.y - b.y)),
    bend: 1.,
    head: ElectronHead::Pair,
    style: ArrowStyle {
      width: 0.3,
      head_length: 1.,
      head_width: 0.6,
      clearance: 0.25,
      ..Default::default()
    },
  };
  assert!(
    diagram
      .route_electron_arrow(arrow, &RoutingOptions::default(), &[])?
      .clear
  );
  // An arrow from the interior to the outside still has to cross the real ink.
  let crossing = ElectronArrow {
    to: Anchor::atom(id, atoms[3]),
    ..arrow
  };
  assert!(
    !diagram
      .route_electron_arrow(crossing, &RoutingOptions::default(), &[])?
      .clear
  );
  Ok(())
}

#[test]
fn identical_circles_with_different_roles_should_not_be_the_same_contact_region() -> TestResult {
  let (graph, coordinates, options) = fixture("[O-]c1ccccc1")?;
  let atoms: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
  let drawing = depict::depict(&graph, &coordinates, &options)?;
  let ring = drawing
    .leaf_primitives()
    .find_map(|p| match p {
      Primitive::Circle(c) if matches!(c.source, Some(Source::AromaticRing(_))) => Some(c.clone()),
      _ => None,
    })
    .ok_or("ring")?;
  let metrics = FontFace::NotoSans.measure("O", 10.)?;
  let label = Primitive::Text(Text {
    font: FontFace::NotoSans,
    origin: ring.center,
    content: "O".into(),
    font_size: 10.,
    width: metrics.advance,
    color: Color::BLACK,
    source: Some(Source::Atom(atoms[0])),
  });
  assert!(
    contacts(
      &graph,
      &Drawing::new(vec![Primitive::Circle(ring.clone()), label.clone()])?
    )?
    .is_empty()
  );
  let charge = Circle {
    source: Some(Source::Charge(atoms[1])),
    ..ring
  };
  assert_eq!(
    contacts(&graph, &Drawing::new(vec![Primitive::Circle(charge), label])?)?,
    [DrawingContact::Labels(atoms[0], atoms[1])]
  );
  Ok(())
}

#[test]
fn charge_and_ring_provenance_should_reject_foreign_graph_handles() -> TestResult {
  let (graph, _, _) = fixture("[O-]c1ccccc1")?;
  let other = read_smiles("CC")?.into_graph();
  let foreign_atom = other.atoms().next().ok_or("atom")?.0;
  let foreign_bond = other.bonds().next().ok_or("bond")?.0;
  for source in [Source::Charge(foreign_atom), Source::AromaticRing(foreign_bond)] {
    let drawing = Drawing::new(vec![Primitive::Circle(Circle {
      center: Point::new(0., 0.),
      radius: 5.,
      width: 1.,
      color: Color::BLACK,
      source: Some(source),
    })])?;
    assert!(contacts(&graph, &drawing).is_err());
  }
  Ok(())
}
