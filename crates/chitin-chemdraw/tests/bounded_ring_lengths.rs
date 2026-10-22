//! Controlled drawing exceptions never weaken source coverage or bond semantics.

use chitin_chem::{BondOrder, io::smiles::read_smiles, perception::analyze_rings};
use chitin_chemdraw::{layout::LayoutOptions, pipeline};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn complex_closure_should_use_only_local_bounded_single_bond_exceptions() -> TestResult {
  for line in include_str!("fixtures/bounded_ring_lengths.tsv")
    .lines()
    .filter(|line| !line.starts_with('#'))
  {
    let (name, source) = line.split_once('\t').ok_or("invalid fixture")?;
    let graph = read_smiles(source)?.into_graph();
    let before = graph.clone();
    let topology = analyze_rings(&graph)?;
    let result = pipeline::generate(&graph, &LayoutOptions::default(), &Default::default())?;
    assert_eq!(result.layout.positions().len(), graph.atom_count(), "{name}");
    let mut exceptions = 0;
    for (id, bond) in graph.bonds() {
      let [a, b] = bond.atoms();
      let length = result
        .layout
        .position(a)
        .ok_or("missing atom")?
        .distance(result.layout.position(b).ok_or("missing atom")?);
      let eligible = bond.order() == BondOrder::Single
        && topology
          .blocks()
          .iter()
          .any(|block| block.cycle_rank() >= 3 && block.bonds().contains(&id));
      if eligible {
        assert!((0.9 - 1e-7..=1.2 + 1e-7).contains(&length), "{name}: {length}");
        exceptions += usize::from((length - 1.).abs() > 1e-6);
      } else {
        assert!((length - 1.).abs() < 1e-7, "{name}: noneligible edge {length}");
      }
    }
    assert!(exceptions > 0, "{name}: fixture no longer exercises bounded closure");
    assert!(result.layout.bond_crossings().is_empty(), "{name}");
    assert!(result.layout.drawing_depths().is_empty(), "{name}");
    assert_eq!(graph, before, "{name}");
    assert_eq!(
      result,
      pipeline::generate(&graph, &LayoutOptions::default(), &Default::default())?,
      "{name}"
    );
  }
  Ok(())
}
