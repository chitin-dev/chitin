//! Required alkene relations survive layout, refinement and SVG depiction.

use chitin_chem::{DoubleBondRelation, MolecularGraph, io::smiles::read_smiles};
use chitin_chemdraw::{
  depict,
  geometry::Point,
  layout::{self, Layout2d, LayoutError, LayoutOptions},
  svg,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Independently measures each supplied reference pair in a coordinate snapshot.
fn assert_relations(graph: &MolecularGraph, positions: &Layout2d) -> TestResult {
  for (id, stereo) in graph.double_bond_stereos() {
    let [a, b] = stereo.ends();
    let [ra, rb] = stereo.references();
    let point = |atom| positions.position(atom).ok_or("missing fixture position");
    let (a, b, ra, rb) = (point(a)?, point(b)?, point(ra)?, point(rb)?);
    let cross =
      |origin: Point, reference: Point| (b.x - a.x) * (reference.y - origin.y) - (b.y - a.y) * (reference.x - origin.x);
    let product = cross(a, ra) * cross(b, rb);
    assert!(product.abs() > 1e-8, "flattened bond {id}");
    assert_eq!(
      product > 0.,
      stereo.relation() == DoubleBondRelation::SameSide,
      "bond {id}"
    );
  }
  Ok(())
}

#[test]
fn automatic_layout_should_preserve_sides_with_and_without_quality_refinement() -> TestResult {
  for source in [
    "F/C=C/F",
    "F/C=C\\F",
    "C/C=C/C",
    "C/C=C\\C",
    "Cl/C(F)=C(Br)/I",
    "F/C=C/C=C\\Cl",
    "F/C=C/C=C/C=C/Br",
    "c1ccccc1/C=C\\O",
    "N/C=N/C",
    "[H]/C(Cl)=C(/F)Br",
    "F/C=C/[C@H](Cl)Br",
    "C1CCC(CC1)/C=C\\C2CCCCC2",
    "C/1=C/CCCCCC1",
  ] {
    let graph = read_smiles(source)?.into_graph();
    let before = graph.clone();
    for refinement_passes in [0, 4] {
      let options = LayoutOptions {
        refinement_passes,
        ..Default::default()
      };
      let positions = layout::generate(&graph, &options)?;
      assert_relations(&graph, &positions)?;
      for (_, bond) in graph.bonds() {
        let [a, b] = bond.atoms();
        assert!(
          (positions
            .position(a)
            .ok_or("no a")?
            .distance(positions.position(b).ok_or("no b")?)
            - 1.)
            .abs()
            < 0.001,
          "{source}"
        );
      }
      let drawing = depict::depict(&graph, &positions, &depict::DepictOptions::default())?;
      assert!(!svg::export(&drawing, &svg::SvgOptions::default())?.contains("NaN"));
      assert_eq!(layout::generate(&graph, &options)?, positions);
    }
    assert_eq!(before, graph);
  }
  Ok(())
}

#[test]
fn caller_coordinates_should_reject_inverted_or_collinear_geometry() -> TestResult {
  let graph = read_smiles("F/C=C/F")?.into_graph();
  let edge = graph
    .double_bond_stereos()
    .next()
    .map(|(edge, _)| edge)
    .ok_or("missing stereo")?;
  for y in [1., 0.] {
    let coordinates = Layout2d::from_positions(
      graph
        .atoms()
        .map(|(id, _)| id)
        .zip([
          Point::new(-0.5, 1.),
          Point::new(0., 0.),
          Point::new(1., 0.),
          Point::new(1.5, y),
        ])
        .collect(),
    )?;
    assert_eq!(
      depict::depict(&graph, &coordinates, &depict::DepictOptions::default()),
      Err(depict::DepictError::DoubleBondStereo(edge))
    );
  }
  Ok(())
}

#[test]
fn rotated_and_reflected_supplied_coordinates_should_preserve_reference_relations() -> TestResult {
  let graph = read_smiles("F/C=C\\Cl")?.into_graph();
  let positions = layout::generate(&graph, &LayoutOptions::default())?;
  for angle in [0_f64, 0.7, 2.1] {
    for sign in [-1., 1.] {
      let coordinates = Layout2d::from_positions(
        positions
          .positions()
          .iter()
          .map(|(&id, p)| {
            (
              id,
              Point::new(
                sign * p.x * angle.cos() - p.y * angle.sin(),
                sign * p.x * angle.sin() + p.y * angle.cos(),
              ),
            )
          })
          .collect(),
      )?;
      assert_relations(&graph, &coordinates)?;
      depict::depict(&graph, &coordinates, &depict::DepictOptions::default())?;
    }
  }
  Ok(())
}

#[test]
fn unsupported_cyclic_opposite_sides_should_fail_without_a_hidden_closure_backend() -> TestResult {
  let graph = read_smiles("C/1=C\\CCCCCC1")?.into_graph();
  let before = graph.clone();
  let edge = graph
    .double_bond_stereos()
    .next()
    .map(|(edge, _)| edge)
    .ok_or("missing stereo")?;
  assert_eq!(
    layout::generate(&graph, &Default::default()),
    Err(LayoutError::DoubleBondStereo(edge))
  );
  assert_eq!(graph, before);
  Ok(())
}

#[test]
fn required_stereo_budget_should_not_depend_on_optional_quality_passes() -> TestResult {
  let graph = read_smiles("F/C=C\\F.F/C=C/F")?.into_graph();
  for refinement_passes in [0, 4] {
    let options = LayoutOptions {
      stereo_budget: 1,
      refinement_passes,
      ..Default::default()
    };
    assert_eq!(
      layout::generate(&graph, &options),
      Err(LayoutError::StereoBudgetExceeded)
    );
  }
  Ok(())
}
