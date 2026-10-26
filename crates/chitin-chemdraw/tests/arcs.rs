//! True-circle geometry, clipping, SVG path commands and bounded arc routing.

use chitin_chem::io::smiles::read_smiles;
use chitin_chemdraw::{
  CircularArc, Color, Drawing, Primitive,
  depict::{ChargeStyle, DepictOptions},
  diagram::{
    Anchor, Annotation, ArcPath, ArrowStyle, Diagram, ElectronArrow, ElectronCurve, ElectronHead, Placement,
    RoutingOptions,
  },
  geometry::{Bounds, Point},
  layout::Layout2d,
  svg,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn automatic_routing_should_fall_back_to_major_paths_when_minor_paths_are_blocked() -> TestResult {
  let (diagram, mut arrow) = fixture(1.2)?;
  let obstacle = Bounds::from_points([Point::new(50., -110.), Point::new(150., 110.)])?.ok_or("bounds")?;
  arrow.curve = ElectronCurve::Arc {
    sweep: 1.2,
    path: ArcPath::Minor,
  };
  assert!(
    !diagram
      .route_electron_arrow(arrow, &RoutingOptions::default(), &[obstacle])?
      .clear
  );
  arrow.curve = ElectronCurve::Arc {
    sweep: 1.2,
    path: ArcPath::PreferMinor,
  };
  let result = diagram.route_electron_arrow(arrow, &RoutingOptions::default(), &[obstacle])?;
  assert!(result.clear);
  assert!(matches!(
    result.arrow.curve,
    ElectronCurve::Arc {
      path: ArcPath::Major,
      ..
    }
  ));
  Ok(())
}

#[test]
fn complementary_paths_should_share_the_same_circle_and_reverse_sweep() -> TestResult {
  for sweep in [1.2, -1.2] {
    let mut circles = Vec::new();
    for path in [ArcPath::Minor, ArcPath::Major] {
      let (mut diagram, mut arrow) = fixture(sweep)?;
      arrow.curve = ElectronCurve::Arc { sweep, path };
      diagram.annotate(Annotation::ElectronArrow(arrow));
      let drawing = diagram.render()?;
      circles.push(arc(&drawing)?.clone());
    }
    assert_eq!(circles[0].center, circles[1].center);
    assert_eq!(circles[0].radius, circles[1].radius);
    assert_eq!(circles[0].sweep.signum(), -circles[1].sweep.signum());
    assert!(circles[1].sweep.abs() > std::f64::consts::PI);
  }
  Ok(())
}

#[test]
fn automatic_paths_should_favor_readability_for_short_carbonyl_arrows() -> TestResult {
  let graph = read_smiles("C=O")?.into_graph();
  let atoms: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
  let bond = graph.bonds().next().ok_or("bond")?.0;
  let coordinates = Layout2d::from_positions([(atoms[0], Point::new(0., 0.)), (atoms[1], Point::new(0., 1.))].into())?;
  let options = DepictOptions {
    bond_length: 24.,
    font_size: 14.,
    ..Default::default()
  };
  let mut arcs = Vec::new();
  for path in [ArcPath::Minor, ArcPath::Major, ArcPath::PreferMinor] {
    let mut diagram = Diagram::new();
    let id = diagram.add_molecule(&graph, &coordinates, &options, Placement::default())?;
    diagram.annotate(Annotation::ElectronArrow(ElectronArrow {
      curve: ElectronCurve::Arc {
        sweep: std::f64::consts::FRAC_PI_2,
        path,
      },
      controls: None,
      from: Anchor::bond(id, bond),
      to: Anchor::atom(id, atoms[1]),
      bend: 0.,
      head: ElectronHead::Pair,
      style: ArrowStyle {
        clearance: 0.5,
        head_length: 2.5,
        head_width: 2.,
        width: 0.7,
        ..Default::default()
      },
    }));
    arcs.push(arc(&diagram.render()?)?.clone());
  }
  assert!(arcs[0].sweep.abs() < std::f64::consts::PI);
  assert!(arcs[1].sweep.abs() > std::f64::consts::PI);
  assert_eq!(arcs[1], arcs[2]);
  Ok(())
}

#[test]
fn readable_automatic_paths_should_obey_preference_without_creating_near_loops() -> TestResult {
  for (path, major) in [(ArcPath::PreferMinor, false), (ArcPath::PreferMajor, true)] {
    let (mut diagram, mut arrow) = fixture(1.2)?;
    arrow.curve = ElectronCurve::Arc { sweep: 1.2, path };
    diagram.annotate(Annotation::ElectronArrow(arrow));
    let drawing = diagram.render()?;
    assert_eq!(arc(&drawing)?.sweep.abs() > std::f64::consts::PI, major);
  }
  let (mut diagram, mut arrow) = fixture(0.1)?;
  arrow.curve = ElectronCurve::Arc {
    sweep: 0.1,
    path: ArcPath::PreferMajor,
  };
  diagram.annotate(Annotation::ElectronArrow(arrow));
  assert!(arc(&diagram.render()?)?.sweep.abs() < std::f64::consts::PI);
  Ok(())
}

/// Isolates arrow tests from layout candidates and keeps opposite charges visible.
fn fixture(sweep: f64) -> Result<(Diagram, ElectronArrow), Box<dyn std::error::Error>> {
  let graph = read_smiles("[Cl-]")?.into_graph();
  let atom = graph.atoms().next().ok_or("atom")?.0;
  let coordinates = Layout2d::from_positions([(atom, Point::new(0., 0.))].into())?;
  let options = DepictOptions {
    charge_style: ChargeStyle::Circled,
    ..Default::default()
  };
  let mut diagram = Diagram::new();
  let a = diagram.add_molecule(&graph, &coordinates, &options, Placement::default())?;
  let b = diagram.add_molecule(
    &graph,
    &coordinates,
    &options,
    Placement {
      origin: Point::new(200., 0.),
      scale: 1.,
    },
  )?;
  Ok((
    diagram,
    ElectronArrow {
      curve: ElectronCurve::Arc {
        sweep,
        path: chitin_chemdraw::diagram::ArcPath::Minor,
      },
      controls: None,
      from: Anchor::atom(a, atom),
      to: Anchor::atom(b, atom),
      bend: 0.,
      head: ElectronHead::Pair,
      style: ArrowStyle::default(),
    },
  ))
}

/// Extracts the authored arc, not a charge-enclosing circle.
fn arc(drawing: &Drawing) -> Result<&CircularArc, Box<dyn std::error::Error>> {
  drawing
    .leaf_primitives()
    .find_map(|p| match p {
      Primitive::CircularArc(a) => Some(a),
      _ => None,
    })
    .ok_or_else(|| "missing arc".into())
}

#[test]
fn arc_arrows_should_preserve_radius_during_label_clipping_and_export_native_paths() -> TestResult {
  for sweep in [
    std::f64::consts::FRAC_PI_2,
    -std::f64::consts::FRAC_PI_2,
    std::f64::consts::PI,
  ] {
    let (mut diagram, arrow) = fixture(sweep)?;
    diagram.annotate(Annotation::ElectronArrow(arrow));
    let drawing = diagram.render()?;
    let a = arc(&drawing)?;
    let expected_radius = 200. / (2. * (sweep / 2.).sin().abs());
    assert!((a.radius - expected_radius).abs() < 1e-10);
    assert!(a.sweep.abs() < sweep.abs());
    for i in 0..=100 {
      assert!((a.point(f64::from(i) / 100.).distance(a.center) - a.radius).abs() < 1e-10);
    }
    assert!(
      !drawing
        .leaf_primitives()
        .any(|p| matches!(p, Primitive::CubicBezier(_)))
    );
    assert!(svg::export(&drawing, &svg::SvgOptions::default())?.contains(" A"));
  }
  Ok(())
}

#[test]
fn arc_geometry_should_follow_changed_molecular_placements() -> TestResult {
  let (mut diagram, arrow) = fixture(1.2)?;
  diagram.annotate(Annotation::ElectronArrow(arrow));
  let before = diagram.render()?;
  let original = arc(&before)?;
  diagram.set_placement(
    arrow.from.molecule,
    Placement {
      origin: Point::new(10., 20.),
      scale: 2.,
    },
  )?;
  diagram.set_placement(
    arrow.to.molecule,
    Placement {
      origin: Point::new(410., 20.),
      scale: 2.,
    },
  )?;
  let after = diagram.render()?;
  let moved = arc(&after)?;
  assert!((moved.radius - original.radius * 2.).abs() < 1e-10);
  assert!(
    moved
      .center
      .distance(Point::new(original.center.x * 2. + 10., original.center.y * 2. + 20.))
      < 1e-10
  );
  Ok(())
}

#[test]
fn arc_arrowheads_should_follow_the_exact_terminal_tangent_for_both_sweeps() -> TestResult {
  for sweep in [1.2, -1.2] {
    for head in [
      ElectronHead::Pair,
      ElectronHead::FishhookLeft,
      ElectronHead::FishhookRight,
    ] {
      let (mut diagram, mut arrow) = fixture(sweep)?;
      arrow.head = head;
      diagram.annotate(Annotation::ElectronArrow(arrow));
      let drawing = diagram.render()?;
      let a = arc(&drawing)?;
      let points = drawing
        .leaf_primitives()
        .find_map(|p| match p {
          Primitive::Polygon(p) => Some(&p.points),
          _ => None,
        })
        .ok_or("head")?;
      assert!(points[0].distance(a.point(1.)) < 1e-10);
      let tangent = a.tangent(1.);
      for base in &points[1..] {
        let forward = (points[0].x - base.x) * tangent.x + (points[0].y - base.y) * tangent.y;
        assert!((forward - arrow.style.head_length).abs() < 1e-10);
      }
    }
  }
  Ok(())
}

#[test]
fn arc_routing_should_change_sweep_without_changing_curve_kind() -> TestResult {
  let (diagram, arrow) = fixture(std::f64::consts::FRAC_PI_2)?;
  let obstacle = Bounds::from_points([Point::new(70., -65.), Point::new(130., -10.)])?.ok_or("bounds")?;
  let result = diagram.route_electron_arrow(arrow, &RoutingOptions::default(), &[obstacle])?;
  assert!(result.clear);
  assert!(matches!(result.arrow.curve, ElectronCurve::Arc { sweep, .. } if sweep < 0.));
  assert_eq!(result.arrow.from, arrow.from);
  assert_eq!(result.arrow.to, arrow.to);
  Ok(())
}

#[test]
fn arc_mode_should_reject_degenerate_angles_and_bezier_controls() -> TestResult {
  for sweep in [0., f64::NAN, f64::INFINITY, 0.001, 3.2] {
    let (mut diagram, arrow) = fixture(sweep)?;
    assert!(
      diagram
        .route_electron_arrow(arrow, &RoutingOptions::default(), &[])
        .is_err()
    );
    diagram.annotate(Annotation::ElectronArrow(arrow));
    assert!(diagram.render().is_err());
  }
  let (mut diagram, mut arrow) = fixture(1.2)?;
  arrow.controls = Some([Point::new(20., 30.), Point::new(-20., 30.)]);
  diagram.annotate(Annotation::ElectronArrow(arrow));
  assert!(diagram.render().is_err());
  Ok(())
}

#[test]
fn arc_scene_bounds_should_cover_extrema_in_both_directions() -> TestResult {
  for sweep in [2.7, -2.7, 4.8, -4.8] {
    let a = CircularArc {
      center: Point::new(12., -4.),
      radius: 40.,
      start_angle: 0.2,
      sweep,
      width: 2.,
      color: Color::BLACK,
      source: None,
    };
    let drawing = Drawing::new(vec![Primitive::CircularArc(a.clone())])?;
    let bounds = drawing.bounds().ok_or("bounds")?;
    for i in 0..=1000 {
      let p = a.point(f64::from(i) / 1000.);
      assert!(p.x >= bounds.min().x && p.x <= bounds.max().x && p.y >= bounds.min().y && p.y <= bounds.max().y);
    }
  }
  Ok(())
}
