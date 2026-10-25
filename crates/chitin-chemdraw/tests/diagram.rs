//! Instance identity, annotation geometry and scene-export integration contracts.

use chitin_chem::{MolecularGraph, io::smiles::read_smiles};
use chitin_chemdraw::{
  Color, CubicBezier, Drawing, Group, Primitive,
  depict::DepictOptions,
  diagram::{
    Anchor, Annotation, ArrowStyle, Diagram, DiagramError, ElectronArrow, ElectronHead, Highlight, LonePair,
    MoleculeId, Placement, ReactionArrow, ReactionArrowKind,
  },
  geometry::Point,
  layout::Layout2d,
  svg,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn fixture(source: &str) -> Result<(MolecularGraph, Layout2d), Box<dyn std::error::Error>> {
  let graph = read_smiles(source)?.into_graph();
  let positions = graph
    .atoms()
    .enumerate()
    .map(|(i, (id, _))| (id, Point::new(i as f64, 0.)))
    .collect();
  Ok((graph, Layout2d::from_positions(positions)?))
}

fn insert(diagram: &mut Diagram, graph: &MolecularGraph, layout: &Layout2d) -> Result<MoleculeId, DiagramError> {
  diagram.add_molecule(graph, layout, &DepictOptions::default(), Placement::default())
}

#[test]
fn handles_should_distinguish_occurrences_and_independently_created_diagrams() -> TestResult {
  let (graph, layout) = fixture("CO")?;
  let atom = graph.atoms().next().ok_or("missing atom")?.0;
  let mut first = Diagram::new();
  let mut second = Diagram::new();
  let a = insert(&mut first, &graph, &layout)?;
  let b = insert(&mut first, &graph, &layout)?;
  let c = insert(&mut second, &graph, &layout)?;
  assert_ne!(a, b);
  assert_ne!(a, c);
  assert!(matches!(
    second.resolve(Anchor::atom(a, atom)),
    Err(DiagramError::MissingMolecule)
  ));
  first.set_placement(
    b,
    Placement {
      origin: Point::new(200., 30.),
      scale: 2.,
    },
  )?;
  assert_eq!(first.resolve(Anchor::atom(a, atom))?, Point::new(0., 0.));
  assert_eq!(first.resolve(Anchor::atom(b, atom))?, Point::new(200., 30.));
  Ok(())
}

#[test]
fn anchors_should_reject_foreign_atom_and_bond_namespaces() -> TestResult {
  let (graph, layout) = fixture("CO")?;
  let (foreign, _) = fixture("CO")?;
  let mut diagram = Diagram::new();
  let id = insert(&mut diagram, &graph, &layout)?;
  let atom = foreign.atoms().next().ok_or("missing foreign atom")?.0;
  let bond = foreign.bonds().next().ok_or("missing foreign bond")?.0;
  assert!(matches!(
    diagram.resolve(Anchor::atom(id, atom)),
    Err(DiagramError::MissingAtom(_))
  ));
  assert!(matches!(
    diagram.resolve(Anchor::bond(id, bond)),
    Err(DiagramError::MissingBond(_))
  ));
  Ok(())
}

#[test]
fn highlight_should_composite_once_below_unchanged_molecule() -> TestResult {
  let (graph, layout) = fixture("CCC")?;
  let before = graph.clone();
  let mut diagram = Diagram::new();
  let id = insert(&mut diagram, &graph, &layout)?;
  let baseline = diagram.render()?;
  diagram.annotate(Annotation::Highlight(Highlight {
    molecule: id,
    atoms: graph.atoms().map(|(id, _)| id).collect(),
    bonds: graph.bonds().map(|(id, _)| id).collect(),
    radius: 6.,
    color: Color {
      red: 80,
      green: 180,
      blue: 140,
      alpha: 90,
    },
  }));
  let drawing = diagram.render()?;
  let Primitive::Group(highlight) = &drawing.primitives()[0] else {
    return Err("missing highlight group".into());
  };
  assert_eq!(highlight.opacity, 90);
  assert!(highlight.drawing.primitives().iter().all(|p| match p {
    Primitive::Disk(d) => d.color.alpha == 255,
    Primitive::Line(l) => l.color.alpha == 255,
    _ => false,
  }));
  assert_eq!(&drawing.primitives()[1..], baseline.primitives());
  assert_eq!(graph, before);
  let svg = svg::export(&drawing, &svg::SvgOptions::default())?;
  assert!(svg.contains("<g opacity=\"0.35294117647058826\">"));
  Ok(())
}

#[test]
fn invalid_placements_should_leave_the_previous_snapshot_unchanged() -> TestResult {
  let (graph, layout) = fixture("CO")?;
  let mut diagram = Diagram::new();
  let id = insert(&mut diagram, &graph, &layout)?;
  let before = diagram.render()?;
  for placement in [
    Placement {
      scale: -1.,
      ..Placement::default()
    },
    Placement {
      origin: Point::new(f64::NAN, 0.),
      ..Placement::default()
    },
    Placement {
      scale: f64::MAX,
      ..Placement::default()
    },
  ] {
    assert!(diagram.set_placement(id, placement).is_err());
    assert_eq!(diagram.render()?, before);
  }
  Ok(())
}

#[test]
fn electron_arrows_should_clear_labels_and_follow_changed_placements() -> TestResult {
  let (graph, layout) = fixture("[OH-]")?;
  let atom = graph.atoms().next().ok_or("missing atom")?.0;
  let mut diagram = Diagram::new();
  let a = insert(&mut diagram, &graph, &layout)?;
  let b = insert(&mut diagram, &graph, &layout)?;
  diagram.set_placement(
    b,
    Placement {
      origin: Point::new(180., 0.),
      scale: 1.,
    },
  )?;
  diagram.annotate(Annotation::ElectronArrow(ElectronArrow {
    from: Anchor::atom(a, atom),
    to: Anchor::atom(b, atom),
    bend: 55.,
    head: ElectronHead::Pair,
    style: ArrowStyle::default(),
  }));
  let old = diagram.render()?;
  let curve = last_curve(&old).ok_or("missing curve")?;
  assert!(curve.start.x > 3. && curve.end.x < 177.);
  let Primitive::Group(molecule) = &old.primitives()[1] else {
    return Err("missing molecule".into());
  };
  for text in molecule.drawing.primitives() {
    if let Primitive::Text(text) = text {
      let [min, max] = text.envelope()?;
      assert!(!(curve.end.x >= min.x && curve.end.x <= max.x && curve.end.y >= min.y && curve.end.y <= max.y));
    }
  }
  diagram.set_placement(
    b,
    Placement {
      origin: Point::new(240., 20.),
      scale: 1.5,
    },
  )?;
  let new = diagram.render()?;
  assert_ne!(last_curve(&old), last_curve(&new));
  let output = svg::export(&new, &svg::SvgOptions::default())?;
  assert!(output.contains(" C"));
  assert!(!output.contains("NaN"));
  Ok(())
}

fn last_curve(drawing: &Drawing) -> Option<&CubicBezier> {
  drawing.primitives().iter().rev().find_map(|p| match p {
    Primitive::Group(g) => g.drawing.primitives().iter().find_map(|p| {
      if let Primitive::CubicBezier(c) = p {
        Some(c)
      } else {
        None
      }
    }),
    _ => None,
  })
}

#[test]
fn fishhooks_and_reaction_arrows_should_preserve_their_distinct_heads() -> TestResult {
  let mut diagram = Diagram::new();
  for (i, kind) in [ReactionArrowKind::Forward, ReactionArrowKind::Equilibrium]
    .into_iter()
    .enumerate()
  {
    diagram.annotate(Annotation::ReactionArrow(ReactionArrow {
      start: Point::new(0., i as f64 * 60.),
      end: Point::new(100., i as f64 * 60.),
      kind,
      style: ArrowStyle::default(),
    }));
  }
  let drawing = diagram.render()?;
  let Primitive::Group(forward) = &drawing.primitives()[0] else {
    return Err("missing forward arrow".into());
  };
  let Primitive::Group(equilibrium) = &drawing.primitives()[1] else {
    return Err("missing equilibrium arrow".into());
  };
  assert_eq!(forward.drawing.primitives().len(), 2);
  assert_eq!(equilibrium.drawing.primitives().len(), 4);
  Ok(())
}

#[test]
fn lone_pair_should_follow_its_explicit_local_offset_and_scale() -> TestResult {
  let (graph, layout) = fixture("O")?;
  let atom = graph.atoms().next().ok_or("missing atom")?.0;
  let mut diagram = Diagram::new();
  let id = insert(&mut diagram, &graph, &layout)?;
  diagram.set_placement(
    id,
    Placement {
      origin: Point::new(100., 50.),
      scale: 2.,
    },
  )?;
  let anchor = Anchor::atom(id, atom).with_offset(Point::new(0., -15.));
  diagram.annotate(Annotation::LonePair(LonePair {
    anchor,
    angle: 0.,
    separation: 4.,
    radius: 1.,
    color: Color::BLACK,
  }));
  let drawing = diagram.render()?;
  let Primitive::Disk(dot) = &drawing.primitives()[1] else {
    return Err("missing electron dot".into());
  };
  assert_eq!(dot.center, Point::new(96., 20.));
  assert_eq!(dot.radius, 2.);
  Ok(())
}

#[test]
fn curves_should_validate_controls_and_bound_their_entire_hull() -> TestResult {
  let curve = CubicBezier {
    start: Point::new(0., 0.),
    control1: Point::new(10., -90.),
    control2: Point::new(90., -90.),
    end: Point::new(100., 0.),
    width: 2.,
    color: Color::BLACK,
    source: None,
  };
  let drawing = Drawing::new(vec![Primitive::CubicBezier(curve.clone())])?;
  let bounds = drawing.bounds().ok_or("missing bounds")?;
  assert_eq!(bounds.min(), Point::new(-1., -91.));
  assert_eq!(bounds.max(), Point::new(101., 1.));
  let invalid = CubicBezier {
    control1: Point::new(f64::INFINITY, 0.),
    ..curve
  };
  assert!(Drawing::new(vec![Primitive::CubicBezier(invalid)]).is_err());
  let group = Drawing::new(vec![Primitive::Group(Group {
    drawing: drawing.clone(),
    opacity: 100,
  })])?;
  assert_eq!(group.bounds(), drawing.bounds());
  Ok(())
}

#[test]
fn degenerate_arrows_and_invalid_highlights_should_be_rejected() -> TestResult {
  let mut diagram = Diagram::new();
  diagram.annotate(Annotation::ReactionArrow(ReactionArrow {
    start: Point::new(0., 0.),
    end: Point::new(0., 0.),
    kind: ReactionArrowKind::Forward,
    style: ArrowStyle::default(),
  }));
  assert!(matches!(diagram.render(), Err(DiagramError::Geometry)));
  let (graph, layout) = fixture("CC")?;
  let mut diagram = Diagram::new();
  let id = insert(&mut diagram, &graph, &layout)?;
  diagram.annotate(Annotation::Highlight(Highlight {
    molecule: id,
    atoms: vec![],
    bonds: vec![],
    radius: f64::NAN,
    color: Color::BLACK,
  }));
  assert!(matches!(diagram.render(), Err(DiagramError::Geometry)));
  Ok(())
}

#[test]
fn bond_anchors_should_use_the_logical_axis_midpoint() -> TestResult {
  let (graph, layout) = fixture("C=O")?;
  let bond = graph.bonds().next().ok_or("missing bond")?.0;
  let mut diagram = Diagram::new();
  let id = insert(&mut diagram, &graph, &layout)?;
  assert_eq!(diagram.resolve(Anchor::bond(id, bond))?, Point::new(24., 0.));
  Ok(())
}

#[test]
fn foreign_highlight_handles_should_fail_instead_of_painting_another_graph() -> TestResult {
  let (graph, layout) = fixture("CC")?;
  let (foreign, _) = fixture("CC")?;
  let bond = foreign.bonds().next().ok_or("missing foreign bond")?.0;
  let mut diagram = Diagram::new();
  let id = insert(&mut diagram, &graph, &layout)?;
  diagram.annotate(Annotation::Highlight(Highlight {
    molecule: id,
    atoms: vec![],
    bonds: vec![bond],
    radius: 5.,
    color: Color::BLACK,
  }));
  assert!(matches!(diagram.render(), Err(DiagramError::MissingBond(_))));
  Ok(())
}

#[test]
fn highlighting_bonds_should_cover_their_measured_endpoint_labels() -> TestResult {
  let (graph, layout) = fixture("CN")?;
  let nitrogen = graph.atoms().nth(1).ok_or("missing nitrogen")?.0;
  let mut diagram = Diagram::new();
  let id = insert(&mut diagram, &graph, &layout)?;
  diagram.annotate(Annotation::Highlight(Highlight {
    molecule: id,
    atoms: vec![],
    bonds: graph.bonds().map(|(id, _)| id).collect(),
    radius: 5.,
    color: Color::BLACK,
  }));
  let drawing = diagram.render()?;
  let Primitive::Group(highlight) = &drawing.primitives()[0] else {
    return Err("missing highlight".into());
  };
  assert!(
    highlight
      .drawing
      .leaf_primitives()
      .any(|p| matches!(p, Primitive::Line(l) if l.source == Some(chitin_chemdraw::Source::Atom(nitrogen))))
  );
  Ok(())
}

#[test]
fn electron_fishhooks_should_select_opposite_sides_of_the_final_tangent() -> TestResult {
  let (graph, layout) = fixture("C")?;
  let atom = graph.atoms().next().ok_or("missing carbon")?.0;
  let mut results = Vec::new();
  for head in [ElectronHead::FishhookLeft, ElectronHead::FishhookRight] {
    let mut diagram = Diagram::new();
    let a = insert(&mut diagram, &graph, &layout)?;
    let b = insert(&mut diagram, &graph, &layout)?;
    diagram.set_placement(
      b,
      Placement {
        origin: Point::new(200., 0.),
        scale: 1.,
      },
    )?;
    diagram.annotate(Annotation::ElectronArrow(ElectronArrow {
      from: Anchor::atom(a, atom),
      to: Anchor::atom(b, atom),
      bend: 60.,
      head,
      style: ArrowStyle::default(),
    }));
    let drawing = diagram.render()?;
    let curve = last_curve(&drawing).ok_or("missing electron curve")?;
    let polygon = drawing
      .leaf_primitives()
      .filter_map(|p| if let Primitive::Polygon(p) = p { Some(p) } else { None })
      .last()
      .ok_or("missing arrowhead")?;
    let t = Point::new(curve.end.x - curve.control2.x, curve.end.y - curve.control2.y);
    let signed_area: f64 = polygon
      .points
      .iter()
      .map(|p| t.x * (p.y - curve.end.y) - t.y * (p.x - curve.end.x))
      .sum();
    results.push(signed_area);
  }
  assert!(results[0] < 0. && results[1] > 0.);
  Ok(())
}

#[test]
fn electron_arrows_should_reject_nonfinite_and_collapsed_geometry() -> TestResult {
  let (graph, layout) = fixture("C")?;
  let atom = graph.atoms().next().ok_or("missing atom")?.0;
  for bend in [0., f64::NAN, f64::INFINITY] {
    let mut diagram = Diagram::new();
    let id = insert(&mut diagram, &graph, &layout)?;
    diagram.annotate(Annotation::ElectronArrow(ElectronArrow {
      from: Anchor::atom(id, atom),
      to: Anchor::atom(id, atom),
      bend,
      head: ElectronHead::Pair,
      style: ArrowStyle::default(),
    }));
    assert!(matches!(diagram.render(), Err(DiagramError::Geometry)));
  }
  Ok(())
}

#[test]
fn grouped_scene_inspection_should_preserve_source_validation() -> TestResult {
  let (graph, layout) = fixture("CO")?;
  let (foreign, foreign_layout) = fixture("CO")?;
  let mut diagram = Diagram::new();
  insert(&mut diagram, &graph, &layout)?;
  insert(&mut diagram, &foreign, &foreign_layout)?;
  let drawing = diagram.render()?;
  assert!(chitin_chemdraw::pipeline::contacts(&graph, &drawing).is_err());
  assert!(drawing.leaf_primitives().any(|p| matches!(p, Primitive::Text(_))));
  Ok(())
}

#[test]
fn an_empty_diagram_should_remain_an_exportable_empty_scene() -> TestResult {
  let drawing = Diagram::new().render()?;
  assert_eq!(drawing.bounds(), None);
  assert!(svg::export(&drawing, &svg::SvgOptions::default())?.contains("<svg"));
  Ok(())
}
