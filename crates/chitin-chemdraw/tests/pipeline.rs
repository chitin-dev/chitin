//! First-profile contracts, independent of exact insertion-order-specific pictures.

use chitin_chem::io::smiles::read_smiles;
use chitin_chem::{Atom, BondOrder, Element, ImplicitHydrogenPolicy, MolecularGraph};
use chitin_chemdraw::{
  depict::{self, DepictError, DepictOptions, HydrogenMode},
  font::FontFace,
  geometry::{Bounds, Point},
  layout::{self, Layout2d, LayoutError, LayoutOptions},
  scene::{Color, Drawing, DrawingError, Line, Primitive, Source, Text},
  svg::{self, SvgOptions},
};
use std::collections::BTreeMap;

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Runs automatic embedding and depiction, asserting that no source facts change.
fn drawing(source: &str) -> Result<Drawing, Box<dyn std::error::Error>> {
  let graph = read_smiles(source)?.into_graph();
  let before = graph.clone();
  let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
  let result = depict::depict(&graph, &coordinates, &DepictOptions::default())?;
  assert_eq!(graph, before);
  Ok(result)
}

#[test]
fn supported_molecules_should_produce_finite_complete_svg() -> TestResult {
  for source in [
    "",
    "C",
    "O",
    "CC",
    "CCO",
    "CC(O)C",
    "CC(=O)O",
    "C#N",
    "C=C=C",
    "C1CCCCC1",
    "C1=CC=CC=C1",
    "OC1CCCCC1",
    "C1CC1",
    "C1CCCCCCC1",
    "CC.O",
    "[NH4+].[Cl-]",
    "[13CH3][O-]",
    "[2H]O[3H]",
    "[Fe+2]",
    "[C]",
    "[CH3]",
    "C1CC1.C1CCC1",
  ] {
    let scene = drawing(source)?;
    let output = svg::export(&scene, &SvgOptions::default())?;
    assert!(output.starts_with("<svg "), "{source}");
    assert!(output.ends_with("</svg>\n"));
    assert!(!output.contains("NaN") && !output.contains("inf"), "{source}");
    assert_eq!(output, svg::export(&scene, &SvgOptions::default())?);
  }
  Ok(())
}

#[test]
fn layout_should_preserve_unit_bonds_and_cover_every_live_id() -> TestResult {
  for source in ["CCCCCC", "CC(C)(C)C", "C1CCCCC1", "CC1CCCC1O", "CC.O.[Cl-]"] {
    let graph = read_smiles(source)?.into_graph();
    let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
    assert_eq!(coordinates.positions().len(), graph.atom_count());
    for (_, bond) in graph.bonds() {
      let [a, b] = bond.atoms();
      let a = coordinates.position(a).ok_or("missing endpoint")?;
      let b = coordinates.position(b).ok_or("missing endpoint")?;
      assert!((a.distance(b) - 1.).abs() < 1e-9, "{source}: {a:?} {b:?}");
    }
    assert_eq!(coordinates, layout::generate(&graph, &LayoutOptions::default())?);
  }
  Ok(())
}

#[test]
fn triple_and_cumulated_double_centers_should_be_linear() -> TestResult {
  for source in ["CC#N", "C=C=C"] {
    let graph = read_smiles(source)?.into_graph();
    let ids: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
    let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
    let a = coordinates.position(ids[0]).ok_or("no position")?;
    let c = coordinates.position(ids[2]).ok_or("no position")?;
    assert!((a.distance(c) - 2.).abs() < 1e-9);
  }
  Ok(())
}

#[test]
fn regular_cycle_should_not_order_vertices_by_handle_instead_of_connectivity() -> TestResult {
  let mut graph = read_smiles("CCCCC")?.into_graph();
  let ids: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
  let edges: Vec<_> = graph.bonds().map(|(id, _)| id).collect();
  for edge in edges {
    graph.remove_bond(edge)?;
  }
  for (a, b) in [(0, 3), (3, 1), (1, 4), (4, 2), (2, 0)] {
    graph.add_bond(ids[a], ids[b], BondOrder::Single)?;
  }
  let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
  for (_, edge) in graph.bonds() {
    let [a, b] = edge.atoms();
    assert!(
      (coordinates
        .position(a)
        .ok_or("no position")?
        .distance(coordinates.position(b).ok_or("no position")?)
        - 1.)
        .abs()
        < 1e-9
    );
  }
  Ok(())
}

#[test]
fn fragments_should_be_packed_without_changing_internal_geometry() -> TestResult {
  let graph = read_smiles("CC.O.C1CC1")?.into_graph();
  let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
  let mut previous_max = None;
  for component in graph.connected_components() {
    let points = component
      .iter()
      .map(|&id| coordinates.position(id).ok_or("missing coordinate"))
      .collect::<Result<Vec<_>, _>>()?;
    let bounds = Bounds::from_points(points)?.ok_or("no bounds")?;
    if let Some(right) = previous_max {
      assert!(bounds.min().x - right >= 3. - 1e-9);
    }
    previous_max = Some(bounds.max().x);
  }
  Ok(())
}

#[test]
fn unsupported_ring_blocks_and_unknown_orders_should_fail_explicitly_in_auto_layout() -> TestResult {
  for source in ["C1CC2CCC1C2", "C1CC2C3CCC1C23"] {
    let graph = read_smiles(source)?.into_graph();
    assert!(matches!(
      layout::generate(&graph, &LayoutOptions::default()),
      Err(LayoutError::RingSystem { .. })
    ));
  }
  let mut graph = read_smiles("CC")?.into_graph();
  let edge = graph.bonds().next().map(|(id, _)| id).ok_or("no bond")?;
  for order in [BondOrder::Unknown, BondOrder::Aromatic] {
    graph.set_bond_order(edge, order)?;
    assert_eq!(
      layout::generate(&graph, &LayoutOptions::default()),
      Err(if order == BondOrder::Aromatic {
        LayoutError::Kekule(chitin_chem::perception::KekuleError::Bond(edge))
      } else {
        LayoutError::Bond(edge)
      })
    );
  }
  Ok(())
}

#[test]
fn layout_should_check_budgets_and_numeric_options_before_work() -> TestResult {
  let graph = read_smiles("CCO")?.into_graph();
  assert!(matches!(
    layout::generate(
      &graph,
      &LayoutOptions {
        max_atoms: 2,
        ..Default::default()
      }
    ),
    Err(LayoutError::AtomBudget { .. })
  ));
  for options in [
    LayoutOptions {
      component_gap: f64::NAN,
      ..Default::default()
    },
    LayoutOptions {
      minimum_spacing: 1.,
      ..Default::default()
    },
    LayoutOptions {
      max_atoms: 0,
      ..Default::default()
    },
  ] {
    assert_eq!(layout::generate(&graph, &options), Err(LayoutError::Options));
  }
  Ok(())
}

#[test]
fn sparse_handles_should_survive_coordinates_and_primitive_provenance() -> TestResult {
  let mut graph = read_smiles("CCO")?.into_graph();
  let first = graph.atoms().next().map(|(id, _)| id).ok_or("no atom")?;
  graph.remove_atom(first)?;
  let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
  assert!(coordinates.position(first).is_none());
  let scene = depict::depict(&graph, &coordinates, &DepictOptions::default())?;
  for primitive in scene.primitives() {
    let source = match primitive {
      Primitive::Line(line) => line.source,
      Primitive::Text(text) => text.source,
      _ => return Err("unexpected primitive".into()),
    };
    match source {
      Some(Source::Atom(id)) => assert!(graph.atom(id).is_some()),
      Some(Source::AttachedHydrogen(id)) => assert!(graph.atom(id).is_some()),
      Some(Source::Bond(id)) => assert!(graph.bond(id).is_some()),
      None => return Err("missing source".into()),
    }
  }
  Ok(())
}

#[test]
fn bond_stroke_count_and_label_clipping_should_match_representation() -> TestResult {
  for (source, expected) in [("CC", 1), ("C=O", 2), ("C#N", 3)] {
    let scene = drawing(source)?;
    let lines: Vec<_> = scene
      .primitives()
      .iter()
      .filter_map(|p| {
        if let Primitive::Line(line) = p {
          Some(line)
        } else {
          None
        }
      })
      .collect();
    assert_eq!(lines.len(), expected);
    if source != "CC" {
      assert!(lines.iter().all(|line| line.start.distance(line.end) < 48.));
    }
  }
  Ok(())
}

#[test]
fn isotope_charge_and_attached_h_should_have_distinct_text_runs() -> TestResult {
  let scene = drawing("[13CH3][O-]")?;
  let runs: Vec<_> = scene
    .primitives()
    .iter()
    .filter_map(|p| if let Primitive::Text(t) = p { Some(t) } else { None })
    .collect();
  for expected in ["13", "C", "H", "3", "O", "-"] {
    assert!(runs.iter().any(|run| run.content == expected), "missing {expected}");
  }
  let base = runs.iter().find(|run| run.content == "C").ok_or("no carbon")?;
  let isotope = runs.iter().find(|run| run.content == "13").ok_or("no isotope")?;
  let count = runs.iter().find(|run| run.content == "3").ok_or("no subscript")?;
  assert!(isotope.origin.y < base.origin.y && isotope.font_size < base.font_size);
  assert!(count.origin.y > base.origin.y && count.font_size < base.font_size);
  Ok(())
}

#[test]
fn explicit_h_vertices_should_not_be_collapsed_or_double_counted() -> TestResult {
  let scene = drawing("[2H]O[3H]")?;
  let contents: Vec<_> = scene
    .primitives()
    .iter()
    .filter_map(|p| {
      if let Primitive::Text(t) = p {
        Some(t.content.as_str())
      } else {
        None
      }
    })
    .collect();
  assert_eq!(contents.iter().filter(|&&t| t == "H").count(), 2);
  assert_eq!(contents.iter().filter(|&&t| t == "O").count(), 1);
  assert!(contents.contains(&"2") && contents.contains(&"3"));
  Ok(())
}

#[test]
fn existing_coordinates_should_bypass_auto_layout_but_require_exact_id_coverage() -> TestResult {
  let graph = read_smiles("CO")?.into_graph();
  let ids: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
  let coordinates = Layout2d::from_positions(BTreeMap::from([
    (ids[0], Point::new(0., 0.)),
    (ids[1], Point::new(2., 0.)),
  ]))?;
  let scene = depict::depict(&graph, &coordinates, &DepictOptions::default())?;
  assert!(
    scene
      .primitives()
      .iter()
      .any(|p| matches!(p, Primitive::Line(line) if line.start.distance(line.end) > 48.))
  );
  let missing = Layout2d::from_positions(BTreeMap::from([(ids[0], Point::new(0., 0.))]))?;
  assert_eq!(
    depict::depict(&graph, &missing, &DepictOptions::default()),
    Err(DepictError::MissingPosition(ids[1]))
  );
  let mut edited = graph.clone();
  edited.remove_atom(ids[1])?;
  assert_eq!(
    depict::depict(&edited, &coordinates, &DepictOptions::default()),
    Err(DepictError::UnexpectedPosition(ids[1]))
  );
  Ok(())
}

#[test]
fn unresolved_chemistry_should_not_be_silently_drawn_with_invented_h_or_charge() -> TestResult {
  let graph = read_smiles("O")?.into_graph();
  let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
  let options = DepictOptions {
    hydrogens: HydrogenMode::KnownOnly,
    ..Default::default()
  };
  assert!(matches!(
    depict::depict(&graph, &coordinates, &options),
    Err(DepictError::Hydrogens(_))
  ));
  let mut graph = MolecularGraph::new();
  let id = graph.add_atom(Atom::new(Element::Oxygen))?;
  let coordinates = Layout2d::from_positions(BTreeMap::from([(id, Point::new(0., 0.))]))?;
  assert_eq!(
    depict::depict(&graph, &coordinates, &DepictOptions::default()),
    Err(DepictError::UnknownFacts(id))
  );
  graph.replace_atom(
    id,
    Atom::new(Element::Oxygen)
      .with_formal_charge(Some(0))
      .with_declared_hydrogens(Some(0))
      .with_implicit_hydrogen_policy(ImplicitHydrogenPolicy::Fixed(0))
      .with_unpaired_electrons(Some(1)),
  )?;
  assert_eq!(
    depict::depict(&graph, &coordinates, &DepictOptions::default()),
    Err(DepictError::Radical(id))
  );
  Ok(())
}

/// Creates a text command without coupling export tests to chemistry or layout.
fn run(content: &str) -> Text {
  Text {
    font: FontFace::NotoSans,
    origin: Point::new(0., 0.),
    content: content.into(),
    font_size: 14.,
    width: 30.,
    color: Color::BLACK,
    source: None,
  }
}

#[test]
fn generic_svg_should_escape_text_and_preserve_transparency() -> TestResult {
  let scene = Drawing::new(vec![Primitive::Text(run("<&\"'>"))])?;
  let svg = svg::export(
    &scene,
    &SvgOptions {
      padding: 0.,
      background: None,
    },
  )?;
  assert!(svg.contains("&lt;&amp;&quot;&apos;&gt;"));
  assert!(!svg.contains("<rect"));
  assert!(svg.contains("<path "));
  assert!(!svg.contains("<text ") && !svg.contains("textLength"));
  Ok(())
}

#[test]
fn scene_bounds_should_include_strokes_and_text_envelopes() -> TestResult {
  let scene = Drawing::new(vec![Primitive::Line(Line {
    start: Point::new(0., 0.),
    end: Point::new(10., 0.),
    width: 2.,
    color: Color::BLACK,
    source: None,
  })])?;
  let bounds = scene.bounds().ok_or("missing bounds")?;
  assert_eq!(
    (bounds.min(), bounds.max()),
    (Point::new(-1., -1.), Point::new(11., 1.))
  );
  assert!(Drawing::new(vec![])?.bounds().is_none());
  let text = Drawing::new(vec![Primitive::Text(run("O"))])?;
  assert_eq!(
    text.bounds().ok_or("no text bounds")?.min().y,
    FontFace::NotoSans.measure("O", 14.)?.min.y
  );
  Ok(())
}

#[test]
fn nonfinite_geometry_invalid_text_and_invalid_options_should_be_rejected() -> TestResult {
  assert!(Bounds::from_points([Point::new(f64::NAN, 0.)]).is_err());
  assert!(Bounds::from_points([Point::new(-f64::MAX, 0.), Point::new(f64::MAX, 0.)]).is_err());
  assert!(matches!(
    Drawing::new(vec![Primitive::Text(run("\0"))]),
    Err(DrawingError::Text(0))
  ));
  assert!(
    Drawing::new(vec![Primitive::Text(Text {
      font_size: -1.,
      ..run("O")
    })])
    .is_err()
  );
  let empty = Drawing::new(vec![])?;
  for padding in [f64::NAN, f64::INFINITY, -1., f64::MAX] {
    assert!(
      svg::export(
        &empty,
        &SvgOptions {
          padding,
          background: None
        }
      )
      .is_err()
    );
  }
  let graph = read_smiles("CC")?.into_graph();
  let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
  assert_eq!(
    depict::depict(
      &graph,
      &coordinates,
      &DepictOptions {
        font_size: f64::NAN,
        ..Default::default()
      }
    ),
    Err(DepictError::Options)
  );
  let coincident = Layout2d::from_positions(graph.atoms().map(|(id, _)| (id, Point::new(0., 0.))).collect())?;
  assert!(matches!(
    depict::depict(&graph, &coincident, &DepictOptions::default()),
    Err(DepictError::OccludedBond(_))
  ));
  Ok(())
}

#[test]
fn long_chains_should_use_iterative_layout_without_false_crowding() -> TestResult {
  let graph = read_smiles(&"C".repeat(2_000))?.into_graph();
  let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
  assert_eq!(coordinates.positions().len(), 2_000);
  assert!(coordinates.diagnostics().is_empty());
  Ok(())
}

#[test]
fn irreducible_local_crowding_should_remain_a_visible_diagnostic() -> TestResult {
  // High-degree draft chemistry is permitted by the graph. Local layout must
  // neither pretend the crowded result is final nor silently delete attachments.
  let graph = read_smiles(&format!("C{}", "(C)".repeat(25)))?.into_graph();
  let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
  assert_eq!(coordinates.positions().len(), 26);
  assert!(!coordinates.diagnostics().is_empty());
  for diagnostic in coordinates.diagnostics() {
    match diagnostic {
      layout::LayoutDiagnostic::CrowdedAtoms { a, b } => {
        assert!(graph.atom(*a).is_some() && graph.atom(*b).is_some());
        assert!(
          coordinates
            .position(*a)
            .ok_or("missing center")?
            .distance(coordinates.position(*b).ok_or("missing center")?)
            < 0.55
        );
      }
      layout::LayoutDiagnostic::CrossingBonds { a, b } => {
        assert!(graph.bond(*a).is_some() && graph.bond(*b).is_some());
      }
      layout::LayoutDiagnostic::RefinementBudgetExhausted => {}
    }
  }
  Ok(())
}

#[test]
fn manual_coordinates_should_not_inherit_the_auto_models_cycle_limit() -> TestResult {
  let graph = read_smiles("C1CCCCC1C2CCCCC2")?.into_graph();
  let mut positions = BTreeMap::new();
  for (i, (id, _)) in graph.atoms().enumerate() {
    let ring = i / 6;
    let angle = (i % 6) as f64 * std::f64::consts::TAU / 6.;
    positions.insert(id, Point::new(ring as f64 * 4. + angle.cos(), angle.sin()));
  }
  let coordinates = Layout2d::from_positions(positions)?;
  let scene = depict::depict(&graph, &coordinates, &DepictOptions::default())?;
  assert!(!scene.primitives().is_empty());
  Ok(())
}

#[test]
fn supplied_unknown_orders_should_be_reported_before_hydrogen_model_failures() -> TestResult {
  let mut graph = read_smiles("CO")?.into_graph();
  let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
  let edge = graph.bonds().next().map(|(id, _)| id).ok_or("no bond")?;
  graph.set_bond_order(edge, BondOrder::Unknown)?;
  assert_eq!(
    depict::depict(&graph, &coordinates, &DepictOptions::default()),
    Err(DepictError::Bond(edge))
  );
  Ok(())
}

#[test]
fn hydrogen_atoms_cannot_be_labelled_as_hosts_of_additional_h() -> TestResult {
  let mut graph = read_smiles("[H]")?.into_graph();
  let (id, atom) = graph
    .atoms()
    .next()
    .map(|(id, atom)| (id, atom.clone()))
    .ok_or("no H")?;
  graph.replace_atom(id, atom.with_declared_hydrogens(Some(1)))?;
  let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
  assert_eq!(
    depict::depict(&graph, &coordinates, &DepictOptions::default()),
    Err(DepictError::Hydrogens(id))
  );
  Ok(())
}
