//! Former corpus failures retain source identities, bond ink and alkene sides.

use chitin_chem::{BondOrder, DoubleBondRelation, io::smiles::read_smiles};
use chitin_chemdraw::{
  geometry::Point,
  layout::{self, LayoutDiagnostic, LayoutError, LayoutOptions},
  pipeline,
  scene::{Primitive, Source},
  svg,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn former_corpus_failures_should_generate_without_discarding_chemical_facts() -> TestResult {
  for line in include_str!("fixtures/coverage.tsv")
    .lines()
    .filter(|line| !line.starts_with('#'))
  {
    let (name, source) = line.split_once('\t').ok_or("invalid fixture")?;
    let graph = read_smiles(source)?.into_graph();
    let original = graph.clone();
    let result = pipeline::generate(&graph, &LayoutOptions::default(), &Default::default())
      .map_err(|error| format!("{name}: {error}"))?;
    assert_eq!(graph, original, "{name}");
    assert_eq!(result.layout.positions().len(), graph.atom_count(), "{name}");
    assert!(
      result.layout.positions().values().all(|point| point.is_finite()),
      "{name}"
    );
    for (id, bond) in graph.bonds() {
      let strokes = result
        .drawing
        .primitives()
        .iter()
        .filter(|primitive| match primitive {
          Primitive::Line(line) => line.source == Some(Source::Bond(id)),
          Primitive::Polygon(polygon) => polygon.source == Some(Source::Bond(id)),
          _ => false,
        })
        .count();
      let minimum = match bond.order() {
        BondOrder::Double => 2,
        BondOrder::Triple => 3,
        _ => 1,
      };
      assert!(strokes >= minimum, "{name}: missing bond-order ink for {id}");
    }
    for (id, stereo) in graph.double_bond_stereos() {
      let [a, b] = stereo.ends();
      let [ra, rb] = stereo.references();
      let point = |id| result.layout.position(id).ok_or("missing atom");
      let (a, b, ra, rb) = (point(a)?, point(b)?, point(ra)?, point(rb)?);
      let cross = |origin: Point, reference: Point| {
        (b.x - a.x) * (reference.y - origin.y) - (b.y - a.y) * (reference.x - origin.x)
      };
      let product = cross(a, ra) * cross(b, rb);
      assert!(product.abs() > 1e-8, "{name}: collinear {id}");
      assert_eq!(
        product > 0.,
        stereo.relation() == DoubleBondRelation::SameSide,
        "{name}: {id}"
      );
    }
    let document = svg::export(&result.drawing, &Default::default())?;
    assert!(document.starts_with("<svg ") && !document.contains("NaN"), "{name}");
  }
  Ok(())
}

#[test]
fn fallback_should_be_reproducible_and_obey_its_independent_allowance() -> TestResult {
  let graph = read_smiles("C/1=C\\C2CCC1CC2")?.into_graph();
  let options = LayoutOptions {
    ring_search_budget: 1,
    refinement_passes: 0,
    ..Default::default()
  };
  let result = layout::generate(&graph, &options)?;
  assert!(result.diagnostics().contains(&LayoutDiagnostic::ConstraintClosure));
  assert_eq!(result, layout::generate(&graph, &options)?);
  assert!(result.bond_crossings().is_empty());
  assert!(result.drawing_depths().is_empty());
  assert_eq!(
    layout::generate(
      &graph,
      &LayoutOptions {
        fallback_budget: 1,
        ..options
      }
    ),
    Err(LayoutError::FallbackBudgetExceeded)
  );
  Ok(())
}
