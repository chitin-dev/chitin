//! Clear-route guarantees, bounded failures and immutable chemical attachments.

use chitin_chem::io::smiles::read_smiles;
use chitin_chemdraw::{
  Primitive,
  depict::DepictOptions,
  diagram::{
    Anchor, Annotation, ArrowStyle, Diagram, ElectronArrow, ElectronHead, Placement, ReactionArrow, ReactionArrowKind,
    RoutingOptions,
  },
  geometry::{Bounds, Point},
  layout::Layout2d,
};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn fixture() -> Result<(Diagram, ElectronArrow)> {
  let graph = read_smiles("C")?.into_graph();
  let atom = graph.atoms().next().ok_or("missing atom")?.0;
  let layout = Layout2d::from_positions([(atom, Point::new(0., 0.))].into())?;
  let mut diagram = Diagram::new();
  let options = DepictOptions::default();
  let a = diagram.add_molecule(&graph, &layout, &options, Placement::default())?;
  let b = diagram.add_molecule(
    &graph,
    &layout,
    &options,
    Placement {
      origin: Point::new(180., 0.),
      scale: 1.,
    },
  )?;
  let arrow = ElectronArrow {
    from: Anchor::atom(a, atom),
    to: Anchor::atom(b, atom),
    bend: 0.,
    head: ElectronHead::Pair,
    style: ArrowStyle {
      width: 0.8,
      head_length: 4.,
      head_width: 3.,
      clearance: 1.5,
      ..Default::default()
    },
  };
  Ok((diagram, arrow))
}

fn obstacle(low: [f64; 2], high: [f64; 2]) -> Result<Bounds> {
  Bounds::from_points([Point::new(low[0], low[1]), Point::new(high[0], high[1])])?.ok_or_else(|| "empty bounds".into())
}

#[test]
fn clear_authored_bends_should_be_retained_without_mutating_the_diagram() -> Result {
  let (diagram, mut arrow) = fixture()?;
  arrow.bend = -35.;
  let before = diagram.render()?;
  let report = diagram.route_electron_arrow(arrow, &Default::default(), &[])?;
  assert!(report.clear, "{report:?}");
  assert_eq!(report.arrow, arrow);
  assert_eq!(report.evaluated, 1);
  assert_eq!(diagram.render()?, before);
  Ok(())
}

#[test]
fn measured_host_boxes_should_force_a_clear_deterministic_detour() -> Result {
  let (mut diagram, arrow) = fixture()?;
  let bounds = obstacle([70., -20.], [110., 20.])?;
  let report = diagram.route_electron_arrow(arrow, &Default::default(), &[bounds])?;
  assert!(report.clear, "{report:?}");
  assert!(report.arrow.bend.abs() > 25.);
  assert_eq!(
    report,
    diagram.route_electron_arrow(arrow, &Default::default(), &[bounds])?
  );
  assert_eq!(report.arrow.from, arrow.from);
  assert_eq!(report.arrow.to, arrow.to);
  diagram.annotate(Annotation::ElectronArrow(report.arrow));
  let drawing = diagram.render()?;
  let curve = drawing
    .leaf_primitives()
    .find_map(|p| {
      if let Primitive::CubicBezier(c) = p {
        Some(c)
      } else {
        None
      }
    })
    .ok_or("missing curve")?;
  for i in 0..=2000 {
    let t = i as f64 / 2000.;
    let u = 1. - t;
    let x = u * u * u * curve.start.x
      + 3. * u * u * t * curve.control1.x
      + 3. * u * t * t * curve.control2.x
      + t * t * t * curve.end.x;
    let y = u * u * u * curve.start.y
      + 3. * u * u * t * curve.control1.y
      + 3. * u * t * t * curve.control2.y
      + t * t * t * curve.end.y;
    assert!(x < 68. || x > 112. || y < -22. || y > 22.);
  }
  Ok(())
}

#[test]
fn preexisting_reaction_arrows_should_be_routing_obstacles() -> Result {
  let (mut diagram, arrow) = fixture()?;
  diagram.annotate(Annotation::ReactionArrow(ReactionArrow {
    start: Point::new(70., 0.),
    end: Point::new(110., 0.),
    kind: ReactionArrowKind::Forward,
    style: arrow.style,
  }));
  let report = diagram.route_electron_arrow(arrow, &Default::default(), &[])?;
  assert!(report.clear, "{report:?}");
  assert_ne!(report.arrow.bend, 0.);
  Ok(())
}

#[test]
fn impossible_routes_should_report_failure_without_adding_an_arrow() -> Result {
  let (diagram, arrow) = fixture()?;
  let report = diagram.route_electron_arrow(
    arrow,
    &Default::default(),
    &[obstacle([-1000., -1000.], [1000., 1000.])?],
  )?;
  assert!(!report.clear);
  assert!(report.collisions > 0);
  assert!(
    !diagram
      .render()?
      .leaf_primitives()
      .any(|p| matches!(p, Primitive::CubicBezier(_)))
  );
  Ok(())
}

#[test]
fn exhausted_work_should_never_claim_an_unchecked_route() -> Result {
  let (diagram, arrow) = fixture()?;
  let options = RoutingOptions {
    work_budget: 1,
    ..Default::default()
  };
  let report = diagram.route_electron_arrow(arrow, &options, &[obstacle([70., -20.], [110., 20.])?])?;
  assert!(!report.clear);
  assert!(report.limited);
  assert_eq!(report.work_used, 1);
  assert_eq!(report.evaluated, 0);
  Ok(())
}

#[test]
fn invalid_routing_dimensions_and_limits_should_be_rejected() -> Result {
  let (diagram, arrow) = fixture()?;
  for options in [
    RoutingOptions {
      margin: f64::NAN,
      ..Default::default()
    },
    RoutingOptions {
      max_candidates: 0,
      ..Default::default()
    },
    RoutingOptions {
      max_candidates: 130,
      ..Default::default()
    },
    RoutingOptions {
      bend_step: 0.,
      ..Default::default()
    },
  ] {
    assert!(diagram.route_electron_arrow(arrow, &options, &[]).is_err());
  }
  Ok(())
}

#[test]
fn pair_suggestions_should_clear_ink_without_changing_electron_count_or_graph() -> Result {
  use chitin_chemdraw::{
    Color,
    diagram::{LonePair, LonePairOptions},
  };
  let (diagram, arrow) = fixture()?;
  let before = diagram.render()?;
  let pair = LonePair {
    anchor: arrow.from,
    angle: 0.,
    separation: 3.,
    radius: 0.7,
    color: Color::BLACK,
  };
  let suggestion = diagram.suggest_lone_pair(pair, &LonePairOptions::default(), &[])?;
  assert!(suggestion.clear);
  assert_ne!(suggestion.pair.anchor.offset, pair.anchor.offset);
  assert_eq!(suggestion.pair.anchor.molecule, pair.anchor.molecule);
  assert_eq!(suggestion.pair.anchor.target, pair.anchor.target);
  assert_eq!(suggestion.pair.separation, pair.separation);
  assert_eq!(suggestion.pair.radius, pair.radius);
  assert_eq!(diagram.render()?, before);
  Ok(())
}

#[test]
fn blocked_pair_suggestions_should_fail_without_inserting_dots() -> Result {
  use chitin_chemdraw::{Color, diagram::LonePair};
  let (diagram, arrow) = fixture()?;
  let pair = LonePair {
    anchor: arrow.from,
    angle: 0.,
    separation: 3.,
    radius: 0.7,
    color: Color::BLACK,
  };
  let suggestion = diagram.suggest_lone_pair(
    pair,
    &Default::default(),
    &[obstacle([-1000., -1000.], [1000., 1000.])?],
  )?;
  assert!(!suggestion.clear);
  assert!(suggestion.evaluated > 0);
  Ok(())
}

#[test]
fn pair_suggestion_work_limits_should_prevent_unchecked_success() -> Result {
  use chitin_chemdraw::{
    Color,
    diagram::{LonePair, LonePairOptions},
  };
  let (diagram, arrow) = fixture()?;
  let pair = LonePair {
    anchor: arrow.from,
    angle: 0.,
    separation: 3.,
    radius: 0.7,
    color: Color::BLACK,
  };
  let options = LonePairOptions {
    work_budget: 1,
    ..Default::default()
  };
  let suggestion = diagram.suggest_lone_pair(pair, &options, &[])?;
  assert!(!suggestion.clear);
  assert!(suggestion.limited);
  assert_eq!(suggestion.work_used, 1);
  Ok(())
}

#[test]
fn another_occurrences_atom_labels_should_not_be_exempt_as_attachments() -> Result {
  let (mut diagram, arrow) = fixture()?;
  let graph = read_smiles("[NH4+]")?.into_graph();
  let atom = graph.atoms().next().ok_or("missing blocker atom")?.0;
  let layout = Layout2d::from_positions([(atom, Point::new(0., 0.))].into())?;
  diagram.add_molecule(
    &graph,
    &layout,
    &DepictOptions::default(),
    Placement {
      origin: Point::new(90., 0.),
      scale: 1.5,
    },
  )?;
  let report = diagram.route_electron_arrow(arrow, &Default::default(), &[])?;
  assert!(report.clear, "{report:?}");
  assert_ne!(report.arrow.bend, 0.);
  Ok(())
}

#[test]
fn skeletal_atom_attachments_should_allow_local_contact_with_incident_bonds() -> Result {
  let (mut diagram, mut arrow) = fixture()?;
  let graph = read_smiles("CC(C)C")?.into_graph();
  let atoms: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
  let positions = [(-1., 0.), (0., 0.), (0.8, 0.8), (0.8, -0.8)];
  let layout = Layout2d::from_positions(
    atoms
      .iter()
      .copied()
      .zip(positions.map(|(x, y)| Point::new(x, y)))
      .collect(),
  )?;
  let molecule = diagram.add_molecule(
    &graph,
    &layout,
    &DepictOptions::default(),
    Placement {
      origin: Point::new(180., 80.),
      scale: 1.,
    },
  )?;
  arrow.to = Anchor::atom(molecule, atoms[1]);
  arrow.bend = 50.;
  let report = diagram.route_electron_arrow(arrow, &Default::default(), &[])?;
  assert!(report.clear, "{report:?}");
  Ok(())
}

#[test]
fn attachment_exemptions_should_never_allow_curve_ink_through_atom_labels() -> Result {
  let (mut diagram, mut arrow) = fixture()?;
  arrow.bend = 90.;
  let report = diagram.route_electron_arrow(arrow, &Default::default(), &[])?;
  assert!(report.clear);
  diagram.annotate(Annotation::ElectronArrow(report.arrow));
  let drawing = diagram.render()?;
  let curve = drawing
    .leaf_primitives()
    .find_map(|p| {
      if let Primitive::CubicBezier(c) = p {
        Some(c)
      } else {
        None
      }
    })
    .ok_or("missing curve")?;
  let labels = drawing
    .leaf_primitives()
    .filter_map(|p| if let Primitive::Text(t) = p { Some(t) } else { None })
    .map(|t| t.envelope())
    .collect::<std::result::Result<Vec<_>, _>>()?;
  for i in 0..=2000 {
    let t = i as f64 / 2000.;
    let u = 1. - t;
    let p = Point::new(
      u * u * u * curve.start.x
        + 3. * u * u * t * curve.control1.x
        + 3. * u * t * t * curve.control2.x
        + t * t * t * curve.end.x,
      u * u * u * curve.start.y
        + 3. * u * u * t * curve.control1.y
        + 3. * u * t * t * curve.control2.y
        + t * t * t * curve.end.y,
    );
    for [low, high] in &labels {
      let nearest = Point::new(p.x.clamp(low.x, high.x), p.y.clamp(low.y, high.y));
      assert!(p.distance(nearest) > arrow.style.width / 2.);
    }
  }
  Ok(())
}
