//! Detached double strokes must not look like additional skeleton connections.

use chitin_chem::{BondOrder, io::smiles::read_smiles};
use chitin_chemdraw::{
  depict::{self, BondColorStyle, ColorScheme, DepictOptions},
  geometry::Point,
  layout,
  scene::{Primitive, Source},
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Returns centerline distance, including proper or endpoint intersections.
fn segment_distance(a: Point, b: Point, c: Point, d: Point) -> f64 {
  let cross = |u: Point, v: Point| u.x * v.y - u.y * v.x;
  let sub = |u: Point, v: Point| Point::new(u.x - v.x, u.y - v.y);
  let u = sub(b, a);
  let v = sub(d, c);
  let det = cross(u, v);
  if det.abs() > 1e-10 {
    let t = cross(sub(c, a), v) / det;
    let s = cross(sub(c, a), u) / det;
    if (0. ..=1.).contains(&t) && (0. ..=1.).contains(&s) {
      return 0.;
    }
  }
  let to_segment = |p: Point, a: Point, b: Point| {
    let u = sub(b, a);
    let t = (((p.x - a.x) * u.x + (p.y - a.y) * u.y) / (u.x * u.x + u.y * u.y)).clamp(0., 1.);
    p.distance(Point::new(a.x + t * u.x, a.y + t * u.y))
  };
  to_segment(a, c, d)
    .min(to_segment(b, c, d))
    .min(to_segment(c, a, b))
    .min(to_segment(d, a, b))
}

#[test]
fn cyclic_double_auxiliaries_should_leave_visible_gaps_at_neighbor_edges() -> TestResult {
  for source in ["C12=C3C1C1C2C31", "C12=C3C4C1C5C2C3C45", "C1=CCCCC1"] {
    let graph = read_smiles(source)?.into_graph();
    let positions = layout::generate(&graph, &Default::default())?;
    for stroke_width in [0.8, 1.6, 2.4] {
      let options = DepictOptions {
        color_scheme: ColorScheme::Monochrome,
        bond_color_style: BondColorStyle::Uniform,
        stroke_width,
        ..Default::default()
      };
      let drawing = depict::depict(&graph, &positions, &options)?;
      for (id, bond) in graph.bonds().filter(|(_, bond)| bond.order() == BondOrder::Double) {
        let mut strokes: Vec<_> = drawing
          .primitives()
          .iter()
          .filter_map(|p| match p {
            Primitive::Line(line) if line.source == Some(Source::Bond(id)) => Some(line),
            _ => None,
          })
          .collect();
        assert_eq!(strokes.len(), 2, "{source}: double stroke coverage");
        strokes.sort_by(|a, b| a.start.distance(a.end).total_cmp(&b.start.distance(b.end)));
        let auxiliary = strokes[0];
        let primary = strokes[1];
        let [a, b] = bond.atoms();
        let expected_length = positions.positions()[&a].distance(positions.positions()[&b]) * options.bond_length;
        assert!((primary.start.distance(primary.end) - expected_length).abs() < 1e-8);
        for primitive in drawing.primitives() {
          let Primitive::Line(neighbor) = primitive else {
            continue;
          };
          let Some(Source::Bond(other)) = neighbor.source else {
            continue;
          };
          if other == id
            || !graph
              .bond(other)
              .is_some_and(|edge| edge.atoms().iter().any(|atom| bond.atoms().contains(atom)))
          {
            continue;
          }
          let distance = segment_distance(auxiliary.start, auxiliary.end, neighbor.start, neighbor.end);
          // Subtract both stroke radii: one full width of white space remains.
          assert!(
            distance >= 2. * stroke_width - 1e-8,
            "{source}: auxiliary {id:?} touches {other:?}, width={stroke_width}, distance={distance}"
          );
        }
      }
    }
  }
  Ok(())
}
