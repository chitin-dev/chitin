//! Default-backend contracts use fixed reference coordinates, not retired model choices.

use chitin_chem::io::smiles::read_smiles;
use chitin_chemdraw::{
  depict,
  layout::{self, LayoutOptions},
  pipeline, svg,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn default_entry_should_match_all_completed_reference_coordinates() -> TestResult {
  let mut checked = 0;
  for row in include_str!("fixtures/coordinates.tsv").lines() {
    let (status, row) = row.split_once('\t').ok_or("missing reference status")?;
    if status == "pending" {
      continue;
    }
    assert_eq!(status, "matched");
    let (source, row) = row.split_once('\t').ok_or("missing reference coordinates")?;
    let expected: Vec<f64> = row.split_whitespace().map(str::parse).collect::<Result<_, _>>()?;
    let graph = read_smiles(source)?.into_graph();
    let before = graph.clone();
    let result = layout::generate(&graph, &LayoutOptions::default())?;
    assert_eq!(expected.len(), graph.atom_count() * 2);
    for (i, (id, _)) in graph.atoms().enumerate() {
      let p = result.position(id).ok_or("missing source coordinate")?;
      assert!(
        (p.x - expected[2 * i]).hypot(p.y - expected[2 * i + 1]) < 0.001,
        "{source}: {id}"
      );
    }
    assert_eq!(result, layout::generate(&graph, &LayoutOptions::default())?);
    assert_eq!(before, graph);
    checked += 1;
  }
  assert_eq!(checked, 40);
  Ok(())
}

#[test]
fn exhausted_native_work_should_not_invoke_a_coverage_backend() -> TestResult {
  let graph = read_smiles("C1CCC2CCCCC2C1")?.into_graph();
  let before = graph.clone();
  let options = LayoutOptions {
    work_budget: 1,
    ..Default::default()
  };
  assert!(matches!(
    layout::generate(&graph, &options),
    Err(layout::LayoutError::RingSearchBudgetExceeded)
  ));
  assert!(matches!(
    pipeline::generate(&graph, &options, &Default::default()),
    Err(pipeline::RenderError::Layout(
      layout::LayoutError::RingSearchBudgetExceeded
    ))
  ));
  assert_eq!(graph, before);
  Ok(())
}

#[test]
fn default_pipeline_without_label_search_should_use_exactly_the_native_geometry() -> TestResult {
  for source in [
    "CCO",
    "CC(=O)OC1=CC=CC=C1C(=O)O",
    "F/C=C/F",
    "C12C3C4C1C5C2C3C45",
    "C1C2CC3CC1CC(C2)C3",
    "[Na+].[Cl-]",
  ] {
    let graph = read_smiles(source)?.into_graph();
    let options = LayoutOptions {
      refinement_passes: 0,
      ..Default::default()
    };
    let coordinates = layout::generate(&graph, &options)?;
    let result = pipeline::generate(&graph, &options, &Default::default())?;
    assert_eq!(coordinates, result.layout);
    let drawing = depict::depict(&graph, &coordinates, &Default::default())?;
    assert_eq!(result.drawing, drawing);
    assert!(svg::export(&drawing, &Default::default())?.starts_with("<svg "));
  }
  Ok(())
}
