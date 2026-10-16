//! Public graph contracts, including identity stability and failed edits.

use std::collections::BTreeSet;

use chitin_chem::{Atom, AtomId, BondOrder, Element, GraphError, MolecularGraph};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn carbon(graph: &mut MolecularGraph) -> Result<AtomId, GraphError> {
  graph.add_atom(Atom::new(Element::Carbon))
}

fn assert_consistent(graph: &MolecularGraph) -> TestResult {
  let mut degree_sum = 0;
  for (atom, _) in graph.atoms() {
    for (neighbor, edge) in graph.neighbors(atom)? {
      degree_sum += 1;
      assert!(graph.atom(neighbor).is_some());
      assert_eq!(graph.bond_between(neighbor, atom), Some(edge));
      let bond = graph.bond(edge).ok_or("adjacency references a missing bond")?;
      assert!(bond.atoms() == [atom, neighbor] || bond.atoms() == [neighbor, atom]);
    }
  }
  assert_eq!(degree_sum, graph.bond_count() * 2);
  for (id, bond) in graph.bonds() {
    let [a, b] = bond.atoms();
    assert_ne!(a, b);
    assert_eq!(graph.bond_between(a, b), Some(id));
    assert_eq!(graph.bond_between(b, a), Some(id));
  }
  let components = graph.connected_components();
  let flattened: Vec<_> = components.into_iter().flatten().collect();
  let unique: BTreeSet<_> = flattened.iter().copied().collect();
  assert_eq!(flattened.len(), graph.atom_count());
  assert_eq!(unique, graph.atoms().map(|(id, _)| id).collect());
  Ok(())
}

#[test]
fn empty_graph_should_have_no_components_or_edges() {
  let graph = MolecularGraph::new();
  assert!(graph.is_empty());
  assert_eq!(graph.atoms().len(), 0);
  assert_eq!(graph.bonds().len(), 0);
  assert!(graph.connected_components().is_empty());
}

#[test]
fn bonds_should_preserve_supplied_order_but_connect_in_both_directions() -> TestResult {
  let mut graph = MolecularGraph::new();
  let a = carbon(&mut graph)?;
  let b = carbon(&mut graph)?;
  let edge = graph.add_bond(b, a, BondOrder::Double)?;
  let bond = graph.bond(edge).ok_or("missing inserted bond")?;
  assert_eq!(bond.atoms(), [b, a]);
  assert_eq!(bond.order(), BondOrder::Double);
  assert_eq!(graph.neighbors(a)?.collect::<Vec<_>>(), [(b, edge)]);
  assert_eq!(graph.neighbors(b)?.collect::<Vec<_>>(), [(a, edge)]);
  assert_consistent(&graph)
}

#[test]
fn self_bonds_should_be_rejected_without_consuming_an_id() -> TestResult {
  let mut graph = MolecularGraph::new();
  let a = carbon(&mut graph)?;
  let before = graph.clone();
  assert_eq!(graph.add_bond(a, a, BondOrder::Single), Err(GraphError::SelfBond(a)));
  assert_eq!(graph, before);
  Ok(())
}

#[test]
fn reversed_duplicate_bonds_should_not_replace_existing_order() -> TestResult {
  let mut graph = MolecularGraph::new();
  let a = carbon(&mut graph)?;
  let b = carbon(&mut graph)?;
  graph.add_bond(a, b, BondOrder::Single)?;
  let before = graph.clone();
  assert_eq!(
    graph.add_bond(b, a, BondOrder::Triple),
    Err(GraphError::DuplicateBond { a: b, b: a })
  );
  assert_eq!(graph, before);
  Ok(())
}

#[test]
fn missing_endpoints_should_not_mutate_graph() -> TestResult {
  let mut graph = MolecularGraph::new();
  let a = carbon(&mut graph)?;
  let deleted = carbon(&mut graph)?;
  graph.remove_atom(deleted)?;
  let before = graph.clone();
  assert_eq!(
    graph.add_bond(a, deleted, BondOrder::Unknown),
    Err(GraphError::MissingAtom(deleted))
  );
  assert_eq!(
    graph.add_bond(deleted, a, BondOrder::Unknown),
    Err(GraphError::MissingAtom(deleted))
  );
  assert_eq!(graph, before);
  Ok(())
}

#[test]
fn deleting_atom_should_remove_only_incident_bonds_and_keep_surviving_ids() -> TestResult {
  let mut graph = MolecularGraph::new();
  let a = carbon(&mut graph)?;
  let b = carbon(&mut graph)?;
  let c = carbon(&mut graph)?;
  let d = carbon(&mut graph)?;
  let ab = graph.add_bond(a, b, BondOrder::Single)?;
  let bc = graph.add_bond(b, c, BondOrder::Double)?;
  let cd = graph.add_bond(c, d, BondOrder::Triple)?;
  graph.remove_atom(b)?;
  assert!(graph.atom(b).is_none());
  assert!(graph.bond(ab).is_none() && graph.bond(bc).is_none());
  assert_eq!(graph.bond(cd).map(|edge| edge.atoms()), Some([c, d]));
  assert_eq!(graph.atoms().map(|(id, _)| id).collect::<Vec<_>>(), [a, c, d]);
  assert_eq!(graph.connected_components(), [vec![a], vec![c, d]]);
  assert_consistent(&graph)
}

#[test]
fn deleted_atom_and_bond_handles_should_never_be_reused() -> TestResult {
  let mut graph = MolecularGraph::new();
  let a = carbon(&mut graph)?;
  let b = carbon(&mut graph)?;
  let first = graph.add_bond(a, b, BondOrder::Single)?;
  graph.remove_bond(first)?;
  let second = graph.add_bond(a, b, BondOrder::Double)?;
  graph.remove_atom(b)?;
  let c = carbon(&mut graph)?;
  let third = graph.add_bond(a, c, BondOrder::Unknown)?;
  assert!(a.value() < b.value() && b.value() < c.value());
  assert!(first.value() < second.value() && second.value() < third.value());
  assert!(graph.atom(b).is_none() && graph.bond(first).is_none() && graph.bond(second).is_none());
  assert_consistent(&graph)
}

#[test]
fn removing_bond_should_leave_both_atoms_isolated() -> TestResult {
  let mut graph = MolecularGraph::new();
  let a = carbon(&mut graph)?;
  let b = carbon(&mut graph)?;
  let edge = graph.add_bond(a, b, BondOrder::Aromatic)?;
  let removed = graph.remove_bond(edge)?;
  assert_eq!(removed.order(), BondOrder::Aromatic);
  assert_eq!(graph.connected_components(), [vec![a], vec![b]]);
  assert_eq!(graph.atom_count(), 2);
  assert_consistent(&graph)
}

#[test]
fn changing_atom_attributes_should_leave_connectivity_and_ids_untouched() -> TestResult {
  let mut graph = MolecularGraph::new();
  let a = carbon(&mut graph)?;
  let b = carbon(&mut graph)?;
  let edge = graph.add_bond(a, b, BondOrder::Single)?;
  let oxygen = Atom::new(Element::Oxygen).with_formal_charge(Some(-1));
  assert_eq!(graph.replace_atom(b, oxygen.clone())?, Atom::new(Element::Carbon));
  assert_eq!(graph.atom(b), Some(&oxygen));
  assert_eq!(graph.bond_between(a, b), Some(edge));
  assert_consistent(&graph)
}

#[test]
fn changing_bond_order_should_not_change_edge_identity_or_neighbors() -> TestResult {
  let mut graph = MolecularGraph::new();
  let a = carbon(&mut graph)?;
  let b = carbon(&mut graph)?;
  let edge = graph.add_bond(a, b, BondOrder::Unknown)?;
  assert_eq!(graph.set_bond_order(edge, BondOrder::Double)?, BondOrder::Unknown);
  assert_eq!(graph.bond(edge).map(|bond| bond.order()), Some(BondOrder::Double));
  assert_eq!(graph.neighbors(a)?.next(), Some((b, edge)));
  assert_consistent(&graph)
}

#[test]
fn deleted_handles_should_produce_errors_not_empty_neighbors_or_silent_edits() -> TestResult {
  let mut graph = MolecularGraph::new();
  let a = carbon(&mut graph)?;
  let b = carbon(&mut graph)?;
  let edge = graph.add_bond(a, b, BondOrder::Single)?;
  graph.remove_atom(b)?;
  let before = graph.clone();
  assert!(matches!(graph.neighbors(b), Err(GraphError::MissingAtom(id)) if id == b));
  assert_eq!(
    graph.replace_atom(b, Atom::new(Element::Oxygen)),
    Err(GraphError::MissingAtom(b))
  );
  assert_eq!(graph.remove_atom(b), Err(GraphError::MissingAtom(b)));
  assert_eq!(graph.remove_bond(edge), Err(GraphError::MissingBond(edge)));
  assert_eq!(
    graph.set_bond_order(edge, BondOrder::Double),
    Err(GraphError::MissingBond(edge))
  );
  assert_eq!(graph, before);
  Ok(())
}

#[test]
fn components_should_include_cycles_unknown_bonds_and_isolated_atoms() -> TestResult {
  let mut graph = MolecularGraph::new();
  let ids: Vec<_> = (0..7).map(|_| carbon(&mut graph)).collect::<Result<_, _>>()?;
  for (a, b) in [(0, 2), (2, 4), (4, 0), (1, 3), (3, 6)] {
    graph.add_bond(ids[a], ids[b], BondOrder::Unknown)?;
  }
  assert_eq!(
    graph.connected_components(),
    [vec![ids[0], ids[2], ids[4]], vec![ids[1], ids[3], ids[6]], vec![ids[5]]]
  );
  assert_consistent(&graph)
}

#[test]
fn adjacency_should_be_deterministic_despite_bond_insertion_order() -> TestResult {
  let mut graph = MolecularGraph::new();
  let center = carbon(&mut graph)?;
  let a = carbon(&mut graph)?;
  let b = carbon(&mut graph)?;
  graph.add_bond(center, b, BondOrder::Single)?;
  graph.add_bond(center, a, BondOrder::Single)?;
  assert_eq!(
    graph.neighbors(center)?.map(|(atom, _)| atom).collect::<Vec<_>>(),
    [a, b]
  );
  Ok(())
}

#[test]
fn graph_should_accept_incomplete_chemistry_without_inference() -> TestResult {
  let mut graph = MolecularGraph::new();
  let center = graph.add_atom(Atom::new(Element::Hydrogen))?;
  for _ in 0..5 {
    let neighbor = carbon(&mut graph)?;
    graph.add_bond(center, neighbor, BondOrder::Triple)?;
  }
  let atom = graph.atom(center).ok_or("missing atom")?;
  assert_eq!((atom.formal_charge(), atom.implicit_hydrogens()), (None, None));
  assert_eq!(graph.neighbors(center)?.len(), 5);
  assert_consistent(&graph)
}

#[test]
fn graph_clones_should_preserve_snapshot_ids_without_sharing_mutable_state() -> TestResult {
  let mut original = MolecularGraph::new();
  let a = carbon(&mut original)?;
  let b = carbon(&mut original)?;
  let edge = original.add_bond(a, b, BondOrder::Single)?;
  let mut copy = original.clone();
  copy.remove_atom(a)?;
  assert!(original.atom(a).is_some() && original.bond(edge).is_some());
  assert!(copy.atom(a).is_none() && copy.bond(edge).is_none());
  assert_consistent(&original)?;
  assert_consistent(&copy)
}

#[test]
fn long_chains_should_use_iterative_component_traversal() -> TestResult {
  let mut graph = MolecularGraph::new();
  let mut previous = carbon(&mut graph)?;
  for _ in 1..10_000 {
    let next = carbon(&mut graph)?;
    graph.add_bond(previous, next, BondOrder::Single)?;
    previous = next;
  }
  let components = graph.connected_components();
  assert_eq!(components.len(), 1);
  assert_eq!(components[0].len(), 10_000);
  Ok(())
}

#[test]
fn mixed_edits_should_preserve_all_topology_invariants() -> TestResult {
  let mut graph = MolecularGraph::new();
  // Deterministic operation coverage without a random-data dependency. Failed
  // additions and stale handles must be as harmless as successful removals.
  let mut seed = 0x12_34_56_78_u64;
  let mut historical_atoms = Vec::new();
  let mut historical_bonds = Vec::new();
  for step in 0..500 {
    seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
    let index = (seed >> 32) as usize;
    match step % 5 {
      0 => historical_atoms.push(carbon(&mut graph)?),
      1 | 2 if !historical_atoms.is_empty() => {
        let a = historical_atoms[index % historical_atoms.len()];
        let b = historical_atoms[(index / 7) % historical_atoms.len()];
        let before = graph.clone();
        match graph.add_bond(a, b, BondOrder::Single) {
          Ok(id) => historical_bonds.push(id),
          Err(_) => assert_eq!(graph, before),
        }
      }
      3 if !historical_atoms.is_empty() => {
        let _ = graph.remove_atom(historical_atoms[index % historical_atoms.len()]);
      }
      4 if !historical_bonds.is_empty() => {
        let _ = graph.remove_bond(historical_bonds[index % historical_bonds.len()]);
      }
      _ => {}
    }
    assert_consistent(&graph)?;
  }
  Ok(())
}
