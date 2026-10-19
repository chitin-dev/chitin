//! Ring topology contracts do not depend on valence, aromaticity, or coordinates.

use chitin_chem::{Atom, BondOrder, Element, io::smiles::read_smiles, perception::analyze_rings};
use std::collections::BTreeSet;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn blocks_should_partition_edges_and_distinguish_spiro_from_fused() -> TestResult {
  for (source, ranks, bridges) in [
    ("", vec![], 0),
    ("C.C", vec![], 0),
    ("CCC", vec![0, 0], 2),
    ("C1CC1.CC", vec![0, 1], 1),
    ("C1CCCCC1C2CCCCC2", vec![0, 1, 1], 1),
    ("C1CCC2(CC1)CCCC2", vec![1, 1], 0),
    ("C1CCC2CCCCC2C1", vec![2], 0),
    ("C1CC2CCC1C2", vec![2], 0),
  ] {
    let graph = read_smiles(source)?.into_graph();
    let before = graph.clone();
    let report = analyze_rings(&graph)?;
    let mut actual: Vec<_> = report.blocks().iter().map(|block| block.cycle_rank()).collect();
    actual.sort_unstable();
    assert_eq!(actual, ranks, "{source}");
    assert_eq!(report.bridges().len(), bridges, "{source}");
    let edges: Vec<_> = report
      .blocks()
      .iter()
      .flat_map(|block| block.bonds())
      .copied()
      .collect();
    assert_eq!(edges.len(), graph.bond_count());
    assert_eq!(
      edges.into_iter().collect::<BTreeSet<_>>(),
      graph.bonds().map(|(id, _)| id).collect()
    );
    assert_eq!(report, analyze_rings(&graph)?);
    assert_eq!(graph, before);
  }
  Ok(())
}

#[test]
fn articulation_should_handle_dfs_roots_and_shared_atoms() -> TestResult {
  for (source, count) in [
    ("C", 0),
    ("CC", 0),
    ("CCC", 1),
    ("C(C)(C)C", 1),
    ("C1CC1", 0),
    ("C1CCC2(CC1)CCCC2", 1),
  ] {
    let graph = read_smiles(source)?.into_graph();
    assert_eq!(analyze_rings(&graph)?.articulation_atoms().len(), count, "{source}");
  }
  Ok(())
}

#[test]
fn sparse_handles_and_unknown_bonds_should_not_change_topological_membership() -> TestResult {
  let mut graph = read_smiles("C1CC1.C")?.into_graph();
  let removed = graph.atoms().last().map(|(id, _)| id).ok_or("missing atom")?;
  graph.remove_atom(removed)?;
  let added = graph.add_atom(Atom::new(Element::Iron))?;
  let anchor = graph.atoms().next().map(|(id, _)| id).ok_or("missing anchor")?;
  let bridge = graph.add_bond(anchor, added, BondOrder::Unknown)?;
  let report = analyze_rings(&graph)?;
  assert_eq!(report.bridges(), &BTreeSet::from([bridge]));
  assert!(report.articulation_atoms().contains(&anchor));
  Ok(())
}

#[test]
fn long_chain_should_not_require_recursive_dfs() -> TestResult {
  let graph = read_smiles(&"C".repeat(12000))?.into_graph();
  let report = analyze_rings(&graph)?;
  assert_eq!(report.bridges().len(), 11999);
  assert_eq!(report.articulation_atoms().len(), 11998);
  Ok(())
}
