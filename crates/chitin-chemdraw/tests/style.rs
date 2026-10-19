//! Reference-style geometry invariants, not pixel identity with a browser renderer.

use chitin_chem::io::smiles::read_smiles;
use chitin_chem::{BondId, BondOrder};
use chitin_chemdraw::{
  depict::{DepictOptions, depict},
  font::{FontError, FontFace},
  geometry::Point,
  layout::{Layout2d, LayoutOptions, generate},
  scene::{Drawing, Line, Primitive, Source},
  svg::{SvgOptions, export},
};
use std::collections::BTreeMap;

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Tests physical stroke joins independently of endpoint paint segmentation.
fn geometry_options() -> DepictOptions {
  DepictOptions {
    bond_color_style: chitin_chemdraw::depict::BondColorStyle::Uniform,
    ..Default::default()
  }
}

/// Extracts only one bond's strokes, preserving renderer-independent provenance.
fn lines(drawing: &Drawing, id: BondId) -> Vec<&Line> {
  drawing
    .primitives()
    .iter()
    .filter_map(|p| match p {
      Primitive::Line(line) if line.source == Some(Source::Bond(id)) => Some(line),
      _ => None,
    })
    .collect()
}

/// Returns the geometric midpoint without reference to either graph endpoint order.
fn midpoint(line: &Line) -> Point {
  Point::new((line.start.x + line.end.x) / 2., (line.start.y + line.end.y) / 2.)
}
/// Projects a point onto an already normalized direction.
fn projection(point: Point, direction: Point) -> f64 {
  point.x * direction.x + point.y * direction.y
}

#[test]
fn ring_doubles_should_use_full_skeleton_and_short_interior_stroke_under_transforms() -> TestResult {
  for source in ["C1=CC=CC=C1", "C1=CC1", "C1=CCC1", "C1=CCCCCCC1"] {
    let graph = read_smiles(source)?.into_graph();
    let options = geometry_options();
    let original = generate(&graph, &LayoutOptions::default())?;
    for (angle, reflect) in [(0_f64, 1.), (0.7, 1.), (1.57, -1.)] {
      let positions: BTreeMap<_, _> = original
        .positions()
        .iter()
        .map(|(&id, p)| {
          (
            id,
            Point::new(
              p.x * angle.cos() - p.y * angle.sin() + 5.,
              reflect * (p.x * angle.sin() + p.y * angle.cos()) - 3.,
            ),
          )
        })
        .collect();
      let coordinates = Layout2d::from_positions(positions)?;
      let center = Point::new(
        coordinates
          .positions()
          .values()
          .map(|p| p.x * options.bond_length / graph.atom_count() as f64)
          .sum(),
        coordinates
          .positions()
          .values()
          .map(|p| -p.y * options.bond_length / graph.atom_count() as f64)
          .sum(),
      );
      let scene = depict(&graph, &coordinates, &options)?;
      for (id, bond) in graph.bonds().filter(|(_, b)| b.order() == BondOrder::Double) {
        let strokes = lines(&scene, id);
        assert_eq!(strokes.len(), 2);
        let baseline = strokes[0];
        let inside = strokes[1];
        assert!((baseline.start.distance(baseline.end) - options.bond_length).abs() < 1e-8);
        assert!(inside.start.distance(inside.end) <= options.short_bond_ratio * options.bond_length + 1e-8);
        assert!(midpoint(inside).distance(center) < midpoint(baseline).distance(center));
        let [a, b] = bond.atoms();
        let a = coordinates.position(a).ok_or("missing coordinate")?;
        let b = coordinates.position(b).ok_or("missing coordinate")?;
        let a = Point::new(a.x * options.bond_length, -a.y * options.bond_length);
        let b = Point::new(b.x * options.bond_length, -b.y * options.bond_length);
        assert!(baseline.start.distance(a).min(baseline.start.distance(b)) < 1e-8);
      }
    }
  }
  Ok(())
}

#[test]
fn terminal_imine_should_keep_axis_join_and_use_short_side_stroke() -> TestResult {
  let graph = read_smiles("CCC=N")?.into_graph();
  let options = geometry_options();
  let coordinates = generate(&graph, &LayoutOptions::default())?;
  let scene = depict(&graph, &coordinates, &options)?;
  let (id, bond) = graph
    .bonds()
    .find(|(_, b)| b.order() == BondOrder::Double)
    .ok_or("no double")?;
  let strokes = lines(&scene, id);
  assert_eq!(strokes.len(), 2);
  let carbon = bond
    .atoms()
    .into_iter()
    .find(|&id| {
      graph
        .atom(id)
        .is_some_and(|a| a.element() == chitin_chem::Element::Carbon)
    })
    .ok_or("no carbon")?;
  let p = coordinates.position(carbon).ok_or("no position")?;
  let p = Point::new(p.x * options.bond_length, -p.y * options.bond_length);
  assert!(strokes[0].start.distance(p).min(strokes[0].end.distance(p)) < 1e-8);
  assert!(strokes[1].start.distance(strokes[1].end) < strokes[0].start.distance(strokes[0].end));
  Ok(())
}

#[test]
fn triple_label_ends_should_share_an_axial_clearance_and_reference_spacing() -> TestResult {
  let graph = read_smiles("CCC#N")?.into_graph();
  let options = geometry_options();
  let coordinates = generate(&graph, &LayoutOptions::default())?;
  let scene = depict(&graph, &coordinates, &options)?;
  let (id, _) = graph
    .bonds()
    .find(|(_, b)| b.order() == BondOrder::Triple)
    .ok_or("no triple")?;
  let strokes = lines(&scene, id);
  assert_eq!(strokes.len(), 3);
  let axis = strokes[1];
  let length = axis.start.distance(axis.end);
  let direction = Point::new(
    (axis.end.x - axis.start.x) / length,
    (axis.end.y - axis.start.y) / length,
  );
  let normal = Point::new(-direction.y, direction.x);
  for line in &strokes {
    assert!((projection(line.start, direction) - projection(axis.start, direction)).abs() < 1e-8);
    assert!((projection(line.end, direction) - projection(axis.end, direction)).abs() < 1e-8);
  }
  assert!(
    ((projection(midpoint(strokes[0]), normal) - projection(midpoint(axis), normal)).abs()
      - options.bond_spacing / 1.5)
      .abs()
      < 1e-8
  );
  Ok(())
}

#[test]
fn centered_carbonyl_strokes_should_meet_real_neighbor_rays_at_both_axis_orientations() -> TestResult {
  let options = geometry_options();
  for source in ["CC(=O)O", "OC(=O)C"] {
    let graph = read_smiles(source)?.into_graph();
    let original = generate(&graph, &LayoutOptions::default())?;
    for reflect in [1., -1.] {
      let coordinates = Layout2d::from_positions(
        original
          .positions()
          .iter()
          .map(|(&id, p)| (id, Point::new(p.x * reflect, p.y)))
          .collect(),
      )?;
      let scene = depict(&graph, &coordinates, &options)?;
      let (edge, bond) = graph
        .bonds()
        .find(|(_, b)| b.order() == BondOrder::Double)
        .ok_or("no carbonyl")?;
      let carbon = bond
        .atoms()
        .into_iter()
        .find(|&id| graph.neighbors(id).is_ok_and(|n| n.len() > 1))
        .ok_or("no carbon")?;
      let c = coordinates.position(carbon).ok_or("no point")?;
      let c = Point::new(c.x * options.bond_length, -c.y * options.bond_length);
      for line in lines(&scene, edge) {
        let joint = if line.start.distance(c) < line.end.distance(c) {
          line.start
        } else {
          line.end
        };
        let mut meets_ray = false;
        for (neighbor, id) in graph.neighbors(carbon)? {
          if id == edge {
            continue;
          }
          let n = coordinates.position(neighbor).ok_or("no neighbor")?;
          let n = Point::new(n.x * options.bond_length, -n.y * options.bond_length);
          let v = Point::new(n.x - c.x, n.y - c.y);
          let j = Point::new(joint.x - c.x, joint.y - c.y);
          if (v.x * j.y - v.y * j.x).abs() < 1e-7 && projection(j, v) >= 0. {
            meets_ray = true;
          }
        }
        assert!(meets_ray, "detached carbonyl leg for {source}: {joint:?}");
      }
    }
  }
  Ok(())
}

/// Normalizes scene strokes for testing geometry independently of edge IDs/order.
fn geometry(drawing: &Drawing) -> Vec<[f64; 4]> {
  let mut values: Vec<_> = drawing
    .primitives()
    .iter()
    .filter_map(|p| {
      if let Primitive::Line(l) = p {
        Some([l.start.x, l.start.y, l.end.x, l.end.y])
      } else {
        None
      }
    })
    .collect();
  values.sort_by(|a, b| {
    a.iter()
      .zip(b)
      .find_map(|(x, y)| {
        let order = x.total_cmp(y);
        if order.is_eq() { None } else { Some(order) }
      })
      .unwrap_or(std::cmp::Ordering::Equal)
  });
  values
}

#[test]
fn reversing_all_source_bond_endpoints_should_not_change_stroke_geometry() -> TestResult {
  for source in ["CCC=N", "CCC#N", "CC(=O)O", "C1=CC=CC=C1"] {
    let mut graph = read_smiles(source)?.into_graph();
    let options = geometry_options();
    let coordinates = generate(&graph, &LayoutOptions::default())?;
    let before = geometry(&depict(&graph, &coordinates, &options)?);
    let edges: Vec<_> = graph.bonds().map(|(id, b)| (id, b.atoms(), b.order())).collect();
    for (id, [a, b], order) in edges {
      graph.remove_bond(id)?;
      graph.add_bond(b, a, order)?;
    }
    let after = geometry(&depict(&graph, &coordinates, &options)?);
    assert_eq!(before.len(), after.len());
    for (a, b) in before.iter().zip(&after) {
      for (x, y) in a.iter().zip(b) {
        assert!((x - y).abs() < 1e-8, "{source}");
      }
    }
  }
  Ok(())
}

#[test]
fn label_advances_should_be_measured_and_svg_should_not_require_installed_fonts() -> TestResult {
  let font = FontFace::NotoSans;
  assert!(font.measure("III", 17.6)?.advance < font.measure("WWW", 17.6)?.advance);
  assert!(matches!(
    font.measure("\u{10ffff}", 17.6),
    Err(FontError::MissingGlyph(_))
  ));
  let graph = read_smiles("OCC")?.into_graph();
  let coordinates = generate(&graph, &LayoutOptions::default())?;
  let scene = depict(&graph, &coordinates, &geometry_options())?;
  let o = scene
    .primitives()
    .iter()
    .find_map(|p| {
      if let Primitive::Text(t) = p {
        (t.content == "O").then_some(t)
      } else {
        None
      }
    })
    .ok_or("no O")?;
  let h = scene
    .primitives()
    .iter()
    .find_map(|p| {
      if let Primitive::Text(t) = p {
        (t.content == "H").then_some(t)
      } else {
        None
      }
    })
    .ok_or("no H")?;
  assert!(h.origin.x < o.origin.x);
  let svg = export(&scene, &SvgOptions::default())?;
  assert!(svg.contains("<path ") && svg.contains("aria-label=\"O\""));
  assert!(!svg.contains("font-family") && !svg.contains("textLength") && !svg.contains("<text "));
  Ok(())
}

#[test]
fn separated_rings_connected_by_a_double_bridge_should_not_be_misclassified_as_one_ring_edge() -> TestResult {
  let graph = read_smiles("C1CCCCC1=C2CCCCC2")?.into_graph();
  let positions = graph
    .atoms()
    .enumerate()
    .map(|(i, (id, _))| {
      let angle = if i < 6 {
        (i as f64 - 5.) * std::f64::consts::TAU / 6.
      } else {
        std::f64::consts::PI + (i - 6) as f64 * std::f64::consts::TAU / 6.
      };
      (id, Point::new(angle.cos() + if i < 6 { 0. } else { 3. }, angle.sin()))
    })
    .collect();
  let scene = depict(&graph, &Layout2d::from_positions(positions)?, &geometry_options())?;
  let (id, _) = graph
    .bonds()
    .find(|(_, b)| b.order() == BondOrder::Double)
    .ok_or("no bridge")?;
  assert_eq!(lines(&scene, id).len(), 2);
  Ok(())
}
