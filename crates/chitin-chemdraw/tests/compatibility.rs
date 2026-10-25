//! Versioned pipeline boundaries are exercised separately from visual quality.

#[path = "../../chitin-chem/tests/support/mod.rs"]
mod chemical_support;
#[path = "support/layout_cases.rs"]
mod layout_cases;

use chitin_chem::io::smiles::{SmilesErrorKind, read_smiles, write_smiles};
use chitin_chemdraw::{depict, layout, svg};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn visual_corpus_should_preserve_chemical_facts_through_independent_writing() -> TestResult {
  for case in layout_cases::load()? {
    let graph = read_smiles(case.source)?.into_graph();
    let written = write_smiles(&graph)?;
    let reread = read_smiles(&written)?;
    assert!(
      chemical_support::isomorphic(&graph, reread.graph())?,
      "{}: {written}",
      case.name
    );
    let positions = layout::generate(reread.graph(), &layout::LayoutOptions::default())?;
    let drawing = depict::depict(reread.graph(), &positions, &depict::DepictOptions::default())?;
    let document = svg::export(&drawing, &svg::SvgOptions::default())?;
    assert_eq!(document, svg::export(&drawing, &svg::SvgOptions::default())?);
    assert!(document.starts_with("<svg "), "{}", case.name);
    assert_eq!(
      graph.bond_count() + graph.connected_components().len() - graph.atom_count(),
      case.rank
    );
  }
  Ok(())
}

#[test]
fn unsupported_corpus_should_fail_at_the_declared_typed_boundary() -> TestResult {
  let mut names = std::collections::BTreeSet::new();
  for line in include_str!("fixtures/rejections.tsv")
    .lines()
    .filter(|line| !line.is_empty() && !line.starts_with('#'))
  {
    let fields: Vec<_> = line.split('\t').collect();
    let [name, source, boundary] = fields.as_slice() else {
      return Err("invalid rejection fixture".into());
    };
    assert!(names.insert(*name), "duplicate {name}");
    let observed = match read_smiles(source) {
      Err(error) => {
        assert!(error.span.end <= source.len(), "{name}");
        match error.kind {
          SmilesErrorKind::Unsupported(_) => "read_unsupported",
          SmilesErrorKind::Syntax(_) => "read_syntax",
          _ => "unexpected_read_failure",
        }
      }
      Ok(parsed) => {
        // A supported codec must still retain facts even when drawing refuses
        // the chemistry or geometry; successful parsing is not SVG support.
        let written = write_smiles(parsed.graph())?;
        assert!(chemical_support::isomorphic(
          parsed.graph(),
          read_smiles(&written)?.graph()
        )?);
        match layout::generate(parsed.graph(), &layout::LayoutOptions::default()) {
          Err(layout::LayoutError::DoubleBondStereo(_)) => "layout_stereo",
          Err(layout::LayoutError::Kekule(_)) => "layout_aromatic",
          _ => "unexpected_layout_result",
        }
      }
    };
    assert_eq!(observed, *boundary, "{name}: {source}");
  }
  assert!(!names.is_empty());
  Ok(())
}
