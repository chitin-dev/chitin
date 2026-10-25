//! Explicit coordination associations retain graph provenance and source facts.

use chitin_chem::{
  BondOrder,
  coordination::{CoordinationError, CoordinationSite},
  io::smiles::{read_smiles, write_smiles},
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn declaration_should_not_rewrite_bonds_charges_or_source_serialization() -> TestResult {
  let graph = read_smiles("O[Fe]O")?.into_graph();
  let before = graph.clone();
  let center = graph.atoms().nth(1).map(|(id, _)| id).ok_or("no center")?;
  let site = CoordinationSite::new(&graph, center, graph.neighbors(center)?.map(|(_, id)| id))?;
  assert_eq!(site.attachments().len(), 2);
  site.validate(&graph.clone())?;
  assert_eq!(graph, before);
  assert_eq!(write_smiles(&graph)?, write_smiles(&before)?);
  Ok(())
}

#[test]
fn declaration_should_reject_foreign_and_deleted_source_handles() -> TestResult {
  let graph = read_smiles("O[Fe]O")?.into_graph();
  let center = graph.atoms().nth(1).map(|(id, _)| id).ok_or("no center")?;
  let site = CoordinationSite::new(&graph, center, graph.neighbors(center)?.map(|(_, id)| id))?;
  let foreign = read_smiles("O[Fe]O")?.into_graph();
  assert!(matches!(site.validate(&foreign), Err(CoordinationError::Graph(_))));
  let mut edited = graph.clone();
  edited.remove_bond(site.attachments()[0].0)?;
  assert!(matches!(site.validate(&edited), Err(CoordinationError::Graph(_))));
  Ok(())
}

#[test]
fn declaration_should_reject_empty_duplicate_nonincident_and_changed_edges() -> TestResult {
  let mut graph = read_smiles("O[Fe]O.CC")?.into_graph();
  let center = graph.atoms().nth(1).map(|(id, _)| id).ok_or("no center")?;
  let edge = graph.neighbors(center)?.next().ok_or("no attachment")?.1;
  assert!(matches!(
    CoordinationSite::new(&graph, center, []),
    Err(CoordinationError::Empty)
  ));
  assert!(matches!(
    CoordinationSite::new(&graph, center, [edge, edge]),
    Err(CoordinationError::Duplicate(_))
  ));
  let other = graph.bonds().last().ok_or("no foreign component edge")?.0;
  assert!(matches!(
    CoordinationSite::new(&graph, center, [other]),
    Err(CoordinationError::Attachment(_))
  ));
  let site = CoordinationSite::new(&graph, center, [edge])?;
  graph.set_bond_order(edge, BondOrder::Double)?;
  assert!(matches!(site.validate(&graph), Err(CoordinationError::Attachment(_))));
  Ok(())
}
