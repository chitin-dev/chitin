//! Cage perspective and rigid dense-shell projections preserve source chemistry.

use chitin_chem::io::smiles::read_smiles;
use chitin_chemdraw::{
  depict, layout,
  scene::{Primitive, Source},
};
use std::collections::BTreeSet;

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// One supported spatial drawing and its expected source coverage.
struct Case {
  /// Stable diagnostic label in the shared cage corpus.
  name: &'static str,
  /// Original notation, including aromatic or explicit integer bond orders.
  source: &'static str,
  /// Every atom must have a position; virtual vertices cannot replace atoms.
  atoms: usize,
  /// Every edge must remain in the immutable chemical graph.
  bonds: usize,
}

/// Separate corpus because projected cage edges are deliberately not all unit length.
fn cases() -> Result<Vec<Case>, Box<dyn std::error::Error>> {
  let cases = include_str!("fixtures/cages.tsv")
    .lines()
    .filter(|line| !line.is_empty() && !line.starts_with('#'))
    .map(|line| {
      let fields: Vec<_> = line.split('\t').collect();
      let [name, source, atoms, bonds] = fields.as_slice() else {
        return Err("invalid cage fixture".into());
      };
      Ok(Case {
        name,
        source,
        atoms: atoms.parse()?,
        bonds: bonds.parse()?,
      })
    })
    .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
  if cases.is_empty() {
    return Err("empty cage fixture".into());
  }
  Ok(cases)
}

#[test]
fn projection_corpus_should_preserve_complete_source_coverage_and_export() -> TestResult {
  for Case {
    name,
    source,
    atoms,
    bonds,
  } in cases()?
  {
    let graph = read_smiles(source)?.into_graph();
    let before = graph.clone();
    assert_eq!((graph.atom_count(), graph.bond_count()), (atoms, bonds), "{name}");
    let positions = layout::generate(&graph, &Default::default())?;
    assert_eq!(positions.positions().len(), atoms, "{name}");
    assert!(
      positions.diagnostics().is_empty(),
      "{name}: {:?}",
      positions.diagnostics()
    );
    let drawing = depict::depict(&graph, &positions, &Default::default())?;
    let text = chitin_chemdraw::svg::export(&drawing, &Default::default())?;
    assert!(text.starts_with("<svg "), "{name}");
    assert_eq!(graph, before, "{name}");
  }
  Ok(())
}

#[test]
fn projection_corpus_should_be_independent_of_bond_insertion_and_endpoint_order() -> TestResult {
  for Case { name, source, .. } in cases()? {
    let graph = read_smiles(source)?.into_graph();
    let expected = layout::generate(&graph, &Default::default())?;
    let mut reordered = graph.clone();
    let edges: Vec<_> = graph
      .bonds()
      .map(|(id, bond)| (id, bond.atoms(), bond.order()))
      .collect();
    for &(id, _, _) in &edges {
      reordered.remove_bond(id)?;
    }
    for &(_, [a, b], order) in edges.iter().rev() {
      reordered.add_bond(b, a, order)?;
    }
    let actual = layout::generate(&reordered, &Default::default())?;
    for (&id, &point) in expected.positions() {
      assert!(
        point.distance(actual.position(id).ok_or("missing reordered atom")?) < 1e-8,
        "{name}: {id:?}"
      );
    }
  }
  Ok(())
}

#[test]
fn prismane_should_project_to_a_readable_perspective() -> TestResult {
  let graph = read_smiles("C12C3C1C1C2C31")?.into_graph();
  let coordinates = layout::generate(&graph, &Default::default())?;
  assert!(coordinates.diagnostics().is_empty(), "{:?}", coordinates.diagnostics());
  for (_, bond) in graph.bonds() {
    let [a, b] = bond.atoms();
    let first = coordinates.position(a).ok_or("missing first center")?;
    let second = coordinates.position(b).ok_or("missing second center")?;
    let length = first.distance(second);
    assert!(
      (0.4..2.).contains(&length),
      "projected bond length {length} at {a:?}/{b:?}"
    );
  }
  assert_eq!(coordinates, layout::generate(&graph, &Default::default())?);
  depict::depict(&graph, &coordinates, &Default::default())?;
  Ok(())
}

#[test]
fn substituted_cage_should_keep_its_branch_and_projection() -> TestResult {
  let graph = read_smiles("CC12C3C1C1C2C31")?.into_graph();
  let coordinates = layout::generate(&graph, &Default::default())?;
  assert!(coordinates.diagnostics().is_empty(), "{:?}", coordinates.diagnostics());
  assert_eq!(coordinates.positions().len(), graph.atom_count());
  assert_eq!(coordinates, layout::generate(&graph, &Default::default())?);
  depict::depict(&graph, &coordinates, &Default::default())?;
  Ok(())
}

#[test]
fn planar_fused_aromatic_should_keep_exact_polygons() -> TestResult {
  let graph = read_smiles("c1ccc2cc3ccccc3cc2c1")?.into_graph();
  let coordinates = layout::generate(&graph, &Default::default())?;
  assert!(coordinates.diagnostics().is_empty());
  for (_, bond) in graph.bonds() {
    let [a, b] = bond.atoms();
    let first = coordinates.position(a).ok_or("missing first center")?;
    let second = coordinates.position(b).ok_or("missing second center")?;
    assert!((first.distance(second) - 1.).abs() < 1e-8, "{a:?}/{b:?}");
  }
  Ok(())
}

#[test]
fn dodecahedrane_should_project_a_perspective_cage() -> TestResult {
  let graph = read_smiles("C12C3C4C5C1C6C7C2C8C3C9C4C1C5C6C2C7C8C9C12")?.into_graph();
  let coordinates = layout::generate(&graph, &Default::default())?;
  assert!(
    !coordinates
      .diagnostics()
      .iter()
      .any(|diagnostic| matches!(diagnostic, layout::LayoutDiagnostic::CrossingBonds { .. })),
    "{:?}",
    coordinates.diagnostics()
  );
  depict::depict(&graph, &coordinates, &Default::default())?;
  assert_eq!(coordinates, layout::generate(&graph, &Default::default())?);
  Ok(())
}

#[test]
fn fullerene_should_project_without_losing_aromatic_bonds() -> TestResult {
  let graph = read_smiles("c12c3c4c5c1c6c7c8c2c9c1c3c2c3c4c4c%10c5c5c6c6c7c7c%11c8c9c8c9c1c2c1c2c3c4c3c4c%10c5c5c6c6c7c7c%11c8c8c9c1c1c2c3c2c4c5c6c3c7c8c1c23")?.into_graph();
  let before = graph.clone();
  let layout = layout::generate(&graph, &Default::default())?;
  assert!(layout.diagnostics().is_empty(), "{:?}", layout.diagnostics());
  assert_eq!((graph.atom_count(), graph.bond_count()), (60, 90));
  assert_eq!(layout.positions().len(), 60);
  let options = depict::DepictOptions {
    color_scheme: depict::ColorScheme::Monochrome,
    bond_color_style: depict::BondColorStyle::Uniform,
    ..Default::default()
  };
  let drawing = depict::depict(&graph, &layout, &options)?;
  let visible: BTreeSet<_> = drawing
    .primitives()
    .iter()
    .filter_map(|primitive| match primitive {
      Primitive::Line(line) => match line.source {
        Some(Source::Bond(id)) => Some(id),
        _ => None,
      },
      _ => None,
    })
    .collect();
  assert_eq!(visible, graph.bonds().map(|(id, _)| id).collect());
  // An aromatic carbon in this source has one assigned double bond. All thirty
  // auxiliary strokes must survive the view and export; no circular shorthand
  // or background occlusion is allowed to conceal missing multiple bonds.
  assert_eq!(
    drawing
      .primitives()
      .iter()
      .filter(|p| matches!(p, Primitive::Line(_)))
      .count(),
    120
  );
  assert!(!layout.bond_crossings().is_empty());
  assert_eq!(layout.drawing_depths().len(), 60);
  assert_eq!(graph, before);
  Ok(())
}

#[test]
fn unsaturated_small_cages_should_preserve_source_double_bonds_and_stroke_counts() -> TestResult {
  for source in ["C12=C3C1C1C2C31", "C12=C3C4C1C5C2C3C45"] {
    let graph = read_smiles(source)?.into_graph();
    let before = graph.clone();
    let positions = layout::generate(&graph, &Default::default())?;
    assert!(positions.diagnostics().is_empty(), "{:?}", positions.diagnostics());
    let options = depict::DepictOptions {
      color_scheme: depict::ColorScheme::Monochrome,
      bond_color_style: depict::BondColorStyle::Uniform,
      ..Default::default()
    };
    let drawing = depict::depict(&graph, &positions, &options)?;
    for (id, bond) in graph
      .bonds()
      .filter(|(_, bond)| bond.order() == chitin_chem::BondOrder::Double)
    {
      let count = drawing
        .primitives()
        .iter()
        .filter(|p| matches!(p,Primitive::Line(line) if line.source == Some(Source::Bond(id))))
        .count();
      assert_eq!(count, 2, "double bond {id:?} was hidden or split");
      assert!(
        !positions
          .bond_crossings()
          .iter()
          .any(|crossing| crossing.front == id || crossing.back == id)
      );
      assert_eq!(bond.order(), chitin_chem::BondOrder::Double);
    }
    assert_eq!(graph, before);
    assert_eq!(positions, layout::generate(&graph, &Default::default())?);
  }
  Ok(())
}

#[test]
fn saturated_dense_shell_should_have_complete_repeatable_separated_geometry() -> TestResult {
  let graph = read_smiles("C12C3C4C5C1C6C7C8C2C9C1C3C2C3C4C4C%10C5C5C6C6C7C7C%11C8C9C8C9C1C2C1C2C3C4C3C4C%10C5C5C6C6C7C7C%11C8C8C9C1C1C2C3C2C4C5C6C3C7C8C1C23")?.into_graph();
  let positions = layout::generate(&graph, &Default::default())?;
  assert!(positions.diagnostics().is_empty(), "{:?}", positions.diagnostics());
  assert_eq!(positions.positions().len(), 60);
  let points: Vec<_> = positions.positions().values().copied().collect();
  for (i, &point) in points.iter().enumerate() {
    for &other in &points[i + 1..] {
      assert!(point.distance(other) >= 0.6 - 1e-9);
    }
  }
  depict::depict(&graph, &positions, &Default::default())?;
  assert_eq!(positions, layout::generate(&graph, &Default::default())?);
  Ok(())
}

#[test]
fn dense_projection_should_retain_spatial_bond_lengths_without_planar_warping() -> TestResult {
  for case in cases()?.into_iter().filter(|case| case.atoms == 60) {
    let graph = read_smiles(case.source)?.into_graph();
    let positions = layout::generate(&graph, &Default::default())?;
    let mut shortest = f64::INFINITY;
    let mut longest = 0_f64;
    for (_, bond) in graph.bonds() {
      let [a, b] = bond.atoms();
      let xy = positions
        .position(a)
        .ok_or("missing atom")?
        .distance(positions.position(b).ok_or("missing atom")?);
      let dz = positions.drawing_depths().get(&a).ok_or("missing camera depth")?
        - positions.drawing_depths().get(&b).ok_or("missing camera depth")?;
      let length = xy.hypot(dz);
      shortest = shortest.min(length);
      longest = longest.max(length);
    }
    // Recovering z must restore the near-uniform seed lengths. A harmonic
    // flattening followed by arbitrary depth attachment cannot pass this check.
    assert!(
      longest / shortest < 1.25,
      "{}: spatial length ratio {}",
      case.name,
      longest / shortest
    );
  }
  Ok(())
}

#[test]
fn dense_depth_cues_should_preserve_transparency_and_all_source_strokes() -> TestResult {
  let case = cases()?
    .into_iter()
    .find(|case| case.name.contains("fullerene"))
    .ok_or("missing fullerene")?;
  let graph = read_smiles(case.source)?.into_graph();
  let positions = layout::generate(&graph, &Default::default())?;
  let options = depict::DepictOptions {
    color_scheme: depict::ColorScheme::Monochrome,
    bond_color_style: depict::BondColorStyle::Uniform,
    foreground: chitin_chemdraw::scene::Color {
      red: 25,
      green: 50,
      blue: 75,
      alpha: 160,
    },
    ..Default::default()
  };
  let drawing = depict::depict(&graph, &positions, &options)?;
  let mut alpha = BTreeSet::new();
  for primitive in drawing.primitives() {
    let Primitive::Line(line) = primitive else {
      return Err("spatial depth invented a non-bond primitive".into());
    };
    assert_eq!((line.color.red, line.color.green, line.color.blue), (25, 50, 75));
    assert!(line.color.alpha > 0 && line.color.alpha <= 160);
    alpha.insert(line.color.alpha);
  }
  assert!(alpha.len() > 10, "front/back depth cues were lost");
  // Caller-supplied xy positions have no authority to infer drawing depth.
  let flat = layout::Layout2d::from_positions(positions.positions().clone())?;
  assert!(flat.drawing_depths().is_empty());
  let flat_drawing = depict::depict(&graph, &flat, &options)?;
  assert!(
    flat_drawing
      .primitives()
      .iter()
      .all(|p| matches!(p, Primitive::Line(line) if line.color.alpha == 160))
  );
  Ok(())
}

#[test]
fn dense_crossings_should_order_depth_at_the_actual_intersection() -> TestResult {
  for case in cases()?.into_iter().filter(|case| case.atoms == 60) {
    let graph = read_smiles(case.source)?.into_graph();
    let positions = layout::generate(&graph, &Default::default())?;
    assert!(!positions.bond_crossings().is_empty());
    for crossing in positions.bond_crossings() {
      let front = graph.bond(crossing.front).ok_or("missing front bond")?.atoms();
      let back = graph.bond(crossing.back).ok_or("missing back bond")?.atoms();
      let [a, b, c, d] = [front[0], front[1], back[0], back[1]].map(|id| positions.positions()[&id]);
      let u = [b.x - a.x, b.y - a.y];
      let v = [d.x - c.x, d.y - c.y];
      let offset = [c.x - a.x, c.y - a.y];
      let cross = |u: [f64; 2], v: [f64; 2]| u[0] * v[1] - u[1] * v[0];
      let t = cross(offset, v) / cross(u, v);
      let s = cross(offset, u) / cross(u, v);
      assert!((0. ..1.).contains(&t) && (0. ..1.).contains(&s));
      let depths = positions.drawing_depths();
      let front_z = depths[&front[0]] + t * (depths[&front[1]] - depths[&front[0]]);
      let back_z = depths[&back[0]] + s * (depths[&back[1]] - depths[&back[0]]);
      assert!(front_z > back_z + 1e-9, "{}: reversed depth order", case.name);
    }
  }
  Ok(())
}
