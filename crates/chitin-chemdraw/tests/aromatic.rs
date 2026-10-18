//! End-to-end aromatic drawings share domain assignment and preserve provenance.

use chitin_chem::{
  io::smiles::read_smiles,
  perception::{KekuleOptions, kekulize},
};
use chitin_chemdraw::{
  depict::{self, DepictOptions},
  layout::{self, LayoutError, LayoutOptions},
  scene::{Primitive, Source},
  svg,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn aromatic_drawings_should_equal_the_explicit_detached_representation() -> TestResult {
  for source in [
    "c1ccccc1",
    "n1ccccc1",
    "[nH]1cccc1",
    "Cn1cccc1",
    "o1cccc1",
    "s1cccc1",
    "c1ccc2ccccc2c1",
    "c1ccc2[nH]ccc2c1",
    "c1ccc2cc3ccccc3cc2c1",
    "c1ccccc1-c1ccccc1",
    "CC(=O)Oc1ccccc1C(=O)O",
    "[nH+]1ccccc1",
    "O=c1cccc[nH]1",
    "[13cH]1ccccc1",
    "c1ccccc1.O",
  ] {
    let graph = read_smiles(source)?.into_graph();
    let before = graph.clone();
    let resolved = kekulize(&graph, &KekuleOptions::default())?;
    let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
    assert!(
      coordinates.diagnostics().is_empty(),
      "{source}: {:?}",
      coordinates.diagnostics()
    );
    assert_eq!(
      coordinates,
      layout::generate(&resolved, &LayoutOptions::default())?,
      "{source}"
    );
    let drawing = depict::depict(&graph, &coordinates, &DepictOptions::default())?;
    assert_eq!(
      drawing,
      depict::depict(&resolved, &coordinates, &DepictOptions::default())?,
      "{source}"
    );
    let document = svg::export(&drawing, &svg::SvgOptions::default())?;
    assert!(!document.contains("NaN") && !document.contains("inf"));
    assert_eq!(graph, before);
    for (id, _) in graph.bonds() {
      assert!(
        drawing
          .primitives()
          .iter()
          .any(|p| matches!(p, Primitive::Line(line) if line.source == Some(Source::Bond(id)))),
        "missing {id}: {source}"
      );
    }
  }
  Ok(())
}

#[test]
fn pyridine_and_pyrrole_should_have_distinct_hydrogen_labels() -> TestResult {
  for (source, expected_h) in [("n1ccccc1", 0), ("[nH]1cccc1", 1), ("Cn1cccc1", 0)] {
    let graph = read_smiles(source)?.into_graph();
    let coordinates = layout::generate(&graph, &LayoutOptions::default())?;
    let drawing = depict::depict(&graph, &coordinates, &DepictOptions::default())?;
    assert_eq!(
      drawing
        .primitives()
        .iter()
        .filter(|p| matches!(p, Primitive::Text(text) if text.content == "H"))
        .count(),
      expected_h,
      "{source}"
    );
  }
  Ok(())
}

#[test]
fn aromatic_failures_should_be_typed_in_layout_and_manual_coordinate_depiction() -> TestResult {
  for source in ["c1cccc1", "b1ccccc1", "[se]1cccc1"] {
    let graph = read_smiles(source)?.into_graph();
    assert!(
      matches!(
        layout::generate(&graph, &LayoutOptions::default()),
        Err(LayoutError::Kekule(_))
      ),
      "{source}"
    );
    let template = read_smiles(if graph.atom_count() == 5 { "C1CCCC1" } else { "C1CCCCC1" })?.into_graph();
    let coordinates = layout::generate(&template, &LayoutOptions::default())?;
    assert!(
      matches!(
        depict::depict(&graph, &coordinates, &DepictOptions::default()),
        Err(depict::DepictError::Kekule(_))
      ),
      "{source}"
    );
  }
  Ok(())
}
