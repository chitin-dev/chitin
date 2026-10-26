//! Endpoint-local curve controls, formal-charge envelopes and reactive sites.

use chitin_chem::{MolecularGraph, io::smiles::read_smiles};
use chitin_chemdraw::{
  Color, Primitive,
  depict::{self, ChargeStyle, DepictOptions},
  diagram::{
    Anchor, Annotation, ArrowStyle, Diagram, ElectronArrow, ElectronDot, ElectronHead, Placement, RoutingOptions,
  },
  font::FontFace,
  geometry::Point,
  layout::Layout2d,
  scene::Source,
  svg,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn redraw_should_keep_live_attachments_and_reject_invalid_updates_atomically() -> TestResult {
  let (graph, coordinates) = fixture("CO")?;
  let atom = graph.atoms().next().ok_or("atom")?.0;
  let mut diagram = Diagram::new();
  let id = diagram.add_molecule(&graph, &coordinates, &DepictOptions::default(), Placement::default())?;
  let anchor = Anchor::radical(id, atom, Point::new(0., 12.));
  diagram.annotate(Annotation::ElectronDot(ElectronDot {
    anchor,
    radius: 1.,
    color: Color::BLACK,
  }));
  let moved = Layout2d::from_positions(
    coordinates
      .positions()
      .iter()
      .map(|(&id, p)| (id, Point::new(p.x, p.y + 1.)))
      .collect(),
  )?;
  diagram.redraw_molecule(id, &graph, &moved, &DepictOptions::default())?;
  assert_eq!(diagram.resolve(anchor)?, Point::new(0., -36.));
  let expected = diagram.render()?;
  let invalid = Layout2d::from_positions(Default::default())?;
  assert!(
    diagram
      .redraw_molecule(id, &graph, &invalid, &DepictOptions::default())
      .is_err()
  );
  assert_eq!(diagram.render()?, expected);
  Ok(())
}

/// Uses supplied positions so notation tests do not depend on embedding choices.
fn fixture(smiles: &str) -> Result<(MolecularGraph, Layout2d), Box<dyn std::error::Error>> {
  let graph = read_smiles(smiles)?.into_graph();
  let positions = graph
    .atoms()
    .enumerate()
    .map(|(i, (id, _))| (id, Point::new(i as f64, 0.)))
    .collect();
  Ok((graph, Layout2d::from_positions(positions)?))
}

#[test]
fn controls_and_formation_sites_should_follow_both_occurrences() -> TestResult {
  let (graph, coordinates) = fixture("C")?;
  let atom = graph.atoms().next().ok_or("atom")?.0;
  let mut diagram = Diagram::new();
  let a = diagram.add_molecule(&graph, &coordinates, &DepictOptions::default(), Placement::default())?;
  let b = diagram.add_molecule(
    &graph,
    &coordinates,
    &DepictOptions::default(),
    Placement {
      origin: Point::new(200., 0.),
      scale: 1.,
    },
  )?;
  let site = Anchor::bond_formation(a, atom, b, atom);
  assert_eq!(diagram.resolve(site)?, Point::new(100., 0.));
  let arrow = ElectronArrow {
    curve: Default::default(),
    from: Anchor::atom(a, atom),
    to: Anchor::atom(b, atom),
    controls: Some([Point::new(30., -50.), Point::new(-30., -40.)]),
    bend: 0.,
    head: ElectronHead::Pair,
    style: ArrowStyle::default(),
  };
  diagram.annotate(Annotation::ElectronArrow(arrow));
  diagram.set_placement(
    a,
    Placement {
      origin: Point::new(10., 20.),
      scale: 2.,
    },
  )?;
  diagram.set_placement(
    b,
    Placement {
      origin: Point::new(250., 30.),
      scale: 1.5,
    },
  )?;
  let drawing = diagram.render()?;
  let curve = drawing
    .leaf_primitives()
    .find_map(|p| match p {
      Primitive::CubicBezier(c) => Some(c),
      _ => None,
    })
    .ok_or("curve")?;
  assert_eq!(curve.control1, Point::new(70., -80.));
  assert_eq!(curve.control2, Point::new(205., -30.));
  assert_eq!(diagram.resolve(site)?, Point::new(130., 25.));
  assert!(svg::export(&drawing, &svg::SvgOptions::default())?.contains(" C"));
  Ok(())
}

#[test]
fn invalid_controls_and_foreign_formation_atoms_should_be_rejected() -> TestResult {
  let (graph, coordinates) = fixture("C")?;
  let (other, _) = fixture("C")?;
  let atom = graph.atoms().next().ok_or("atom")?.0;
  let foreign = other.atoms().next().ok_or("foreign atom")?.0;
  let mut diagram = Diagram::new();
  let a = diagram.add_molecule(&graph, &coordinates, &DepictOptions::default(), Placement::default())?;
  let b = diagram.add_molecule(
    &graph,
    &coordinates,
    &DepictOptions::default(),
    Placement {
      origin: Point::new(200., 0.),
      scale: 1.,
    },
  )?;
  assert!(diagram.resolve(Anchor::bond_formation(a, atom, b, foreign)).is_err());
  diagram.annotate(Annotation::ElectronArrow(ElectronArrow {
    curve: Default::default(),
    from: Anchor::atom(a, atom),
    to: Anchor::atom(b, atom),
    controls: Some([Point::new(f64::NAN, 0.), Point::new(-30., 20.)]),
    bend: 0.,
    head: ElectronHead::Pair,
    style: ArrowStyle::default(),
  }));
  assert!(diagram.render().is_err());
  Ok(())
}

#[test]
fn explicit_radical_dots_should_scale_and_follow_their_atom() -> TestResult {
  let (graph, coordinates) = fixture("C")?;
  let atom = graph.atoms().next().ok_or("atom")?.0;
  let mut diagram = Diagram::new();
  let a = diagram.add_molecule(&graph, &coordinates, &DepictOptions::default(), Placement::default())?;
  let anchor = Anchor::radical(a, atom, Point::new(15., -8.));
  diagram.annotate(Annotation::ElectronDot(ElectronDot {
    anchor,
    radius: 1.,
    color: Color::BLACK,
  }));
  diagram.set_placement(
    a,
    Placement {
      origin: Point::new(40., 20.),
      scale: 2.,
    },
  )?;
  let drawing = diagram.render()?;
  let dot = drawing
    .leaf_primitives()
    .find_map(|p| match p {
      Primitive::Disk(d) => Some(d),
      _ => None,
    })
    .ok_or("dot")?;
  assert_eq!(dot.center, Point::new(70., 4.));
  assert_eq!(dot.radius, 2.);
  Ok(())
}

#[test]
fn opposite_charges_should_share_circle_size_and_vertical_alignment() -> TestResult {
  for size in [12., 24.] {
    for pair in [["[N+]", "[N-]"], ["[Fe+2]", "[Fe-2]"]] {
      let mut circles = Vec::new();
      for source in pair {
        let (graph, coordinates) = fixture(source)?;
        let options = DepictOptions {
          font_size: size,
          charge_style: ChargeStyle::Circled,
          ..Default::default()
        };
        let drawing = depict::depict(&graph, &coordinates, &options)?;
        let circle = drawing
          .leaf_primitives()
          .find_map(|p| match p {
            Primitive::Circle(c) => Some(c.clone()),
            _ => None,
          })
          .ok_or("charge circle")?;
        circles.push(circle);
      }
      assert_eq!(circles[0].radius, circles[1].radius);
      assert!(circles[0].center.distance(circles[1].center) < 1e-10);
    }
  }
  Ok(())
}

#[test]
fn circled_charges_should_enclose_measured_text_and_clip_bonds() -> TestResult {
  for source in ["[O-]C", "[NH3+]C", "[Mg+2]C"] {
    let (graph, coordinates) = fixture(source)?;
    let options = DepictOptions {
      charge_style: ChargeStyle::Circled,
      ..Default::default()
    };
    let drawing = depict::depict(&graph, &coordinates, &options)?;
    let circle = drawing
      .leaf_primitives()
      .find_map(|p| match p {
        Primitive::Circle(c) => Some(c),
        _ => None,
      })
      .ok_or("charge circle")?;
    let charge = drawing
      .leaf_primitives()
      .find_map(|p| match p {
        Primitive::Text(t) if t.content.ends_with(['+', '-']) => Some(t),
        _ => None,
      })
      .ok_or("charge text")?;
    let [min, max] = charge.envelope()?;
    for p in [min, max, Point::new(min.x, max.y), Point::new(max.x, min.y)] {
      assert!(p.distance(circle.center) < circle.radius);
    }
    let atom = graph.atoms().next().ok_or("atom")?.0;
    assert_eq!(circle.source, Some(Source::Charge(atom)));
    for p in drawing.leaf_primitives() {
      if let Primitive::Line(line) = p {
        assert!(line.start.distance(circle.center) > circle.radius + circle.width / 2.);
      }
    }
    assert!(svg::export(&drawing, &svg::SvgOptions::default())?.contains("<circle"));
  }
  Ok(())
}

#[test]
fn circled_charge_measurement_requests_should_use_ordinary_host_glyphs() -> TestResult {
  let (graph, coordinates) = fixture("[NH3+]C")?;
  let mut options = DepictOptions {
    charge_style: ChargeStyle::Circled,
    ..Default::default()
  };
  let requests = depict::measurement_requests(&graph, &options)?;
  let entries = requests
    .into_iter()
    .map(|r| Ok((r.clone(), FontFace::NotoSans.measure(r.content(), r.font_size())?)))
    .collect::<Result<Vec<_>, chitin_chemdraw::font::FontError>>()?;
  let expected = depict::depict(&graph, &coordinates, &options)?;
  options.font = FontFace::from_measurements("Measured", entries)?;
  let measured = depict::depict(&graph, &coordinates, &options)?;
  assert_eq!(expected.bounds(), measured.bounds());
  Ok(())
}

#[test]
fn automatic_routing_should_not_replace_explicit_controls() -> TestResult {
  let (graph, coordinates) = fixture("C")?;
  let atom = graph.atoms().next().ok_or("atom")?.0;
  let mut diagram = Diagram::new();
  let a = diagram.add_molecule(&graph, &coordinates, &DepictOptions::default(), Placement::default())?;
  let b = diagram.add_molecule(
    &graph,
    &coordinates,
    &DepictOptions::default(),
    Placement {
      origin: Point::new(200., 0.),
      scale: 1.,
    },
  )?;
  let arrow = ElectronArrow {
    curve: Default::default(),
    from: Anchor::atom(a, atom),
    to: Anchor::atom(b, atom),
    controls: Some([Point::new(40., -50.), Point::new(-40., -50.)]),
    bend: 0.,
    head: ElectronHead::Pair,
    style: ArrowStyle::default(),
  };
  let report = diagram.route_electron_arrow(arrow, &RoutingOptions::default(), &[])?;
  assert!(report.clear);
  assert_eq!(report.arrow, arrow);
  assert_eq!(report.evaluated, 1);
  Ok(())
}
