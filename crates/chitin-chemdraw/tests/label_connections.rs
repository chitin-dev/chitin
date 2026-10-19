//! Moving a printed element must not leave an unlabeled chemical bond endpoint.

use chitin_chem::{Element, io::smiles::read_smiles};
use chitin_chemdraw::{
  depict,
  geometry::Point,
  layout::Layout2d,
  scene::{Primitive, Source},
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn crowded_label_shifts_should_keep_incident_bonds_clipped() -> TestResult {
  let graph = read_smiles("OCO")?.into_graph();
  let before = graph.clone();
  let options = depict::DepictOptions::default();
  for span in [0.01, 0.05, 0.1, 0.2] {
    for height in [-1., 1.] {
      let positions = graph
        .atoms()
        .zip([Point::new(-span, 0.), Point::new(0., height), Point::new(span, 0.)])
        .map(|((id, _), point)| (id, point))
        .collect();
      let coordinates = Layout2d::from_positions(positions)?;
      let drawing = depict::depict(&graph, &coordinates, &options)?;
      for (atom, _) in graph.atoms().filter(|(_, atom)| atom.element() == Element::Oxygen) {
        let point = coordinates.position(atom).ok_or("missing oxygen")?;
        let anchor = Point::new(point.x * options.bond_length, -point.y * options.bond_length);
        let mut incident = 0;
        for primitive in drawing.primitives() {
          let Primitive::Line(line) = primitive else { continue };
          let Some(Source::Bond(bond)) = line.source else {
            continue;
          };
          if graph.bond(bond).is_some_and(|bond| bond.atoms().contains(&atom)) {
            incident += 1;
            assert!(
              line.start.distance(anchor) > 1e-6 && line.end.distance(anchor) > 1e-6,
              "unclipped endpoint at {atom:?}, span={span}, height={height}"
            );
          }
        }
        assert!(incident > 0, "missing incident bond for {atom:?}");
      }
    }
  }
  assert_eq!(graph, before);
  Ok(())
}
