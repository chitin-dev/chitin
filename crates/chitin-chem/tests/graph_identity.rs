//! Provenance boundaries for independent graphs, snapshots, and parsed molecules.

use std::collections::{BTreeSet, HashSet};

use chitin_chem::{
  Atom, BondOrder, DoubleBondStereo, Element, GraphError, MolecularGraph, StereoError, io::smiles::read_smiles,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn independent_graphs_should_reject_foreign_handles_without_mutation() -> TestResult {
  let mut source = MolecularGraph::new();
  let foreign = source.add_atom(Atom::new(Element::Carbon))?;
  let neighbor = source.add_atom(Atom::new(Element::Carbon))?;
  let foreign_bond = source.add_bond(foreign, neighbor, BondOrder::Single)?;
  let mut target = MolecularGraph::default();
  let local = target.add_atom(Atom::new(Element::Oxygen))?;
  let local_neighbor = target.add_atom(Atom::new(Element::Carbon))?;
  let local_bond = target.add_bond(local, local_neighbor, BondOrder::Single)?;
  let before = target.clone();

  // Equal diagnostic serials must not imply equal identity in any key container.
  assert_eq!(foreign.value(), local.value());
  assert_eq!(foreign_bond.value(), local_bond.value());
  assert_eq!(BTreeSet::from([foreign, local]).len(), 2);
  assert_eq!(HashSet::from([foreign, local]).len(), 2);
  assert_eq!(BTreeSet::from([foreign_bond, local_bond]).len(), 2);
  assert_eq!(HashSet::from([foreign_bond, local_bond]).len(), 2);
  assert!(target.atom(foreign).is_none());
  assert!(target.bond(foreign_bond).is_none());
  assert!(target.bond_between(foreign, local_neighbor).is_none());
  assert!(matches!(target.neighbors(foreign), Err(GraphError::MissingAtom(id)) if id == foreign));
  assert_eq!(
    target.replace_atom(foreign, Atom::new(Element::Nitrogen)),
    Err(GraphError::MissingAtom(foreign))
  );
  assert_eq!(target.remove_atom(foreign), Err(GraphError::MissingAtom(foreign)));
  assert_eq!(
    target.add_bond(local, foreign, BondOrder::Single),
    Err(GraphError::MissingAtom(foreign))
  );
  assert_eq!(
    target.add_bond(foreign, local, BondOrder::Single),
    Err(GraphError::MissingAtom(foreign))
  );
  assert_eq!(
    target.remove_bond(foreign_bond),
    Err(GraphError::MissingBond(foreign_bond))
  );
  assert_eq!(
    target.set_bond_order(foreign_bond, BondOrder::Double),
    Err(GraphError::MissingBond(foreign_bond))
  );
  assert_eq!(
    target.set_tetrahedral(foreign, None),
    Err(StereoError::Topology(GraphError::MissingAtom(foreign)))
  );
  assert_eq!(
    target.set_double_bond_stereo(foreign_bond, None),
    Err(StereoError::Topology(GraphError::MissingBond(foreign_bond)))
  );
  assert_eq!(target, before);
  Ok(())
}

#[test]
fn divergent_clones_should_preserve_old_handles_and_allocate_distinct_new_handles() -> TestResult {
  let original = read_smiles("CC")?.into_graph();
  let [a, b]: [_; 2] = original
    .atoms()
    .map(|(id, _)| id)
    .collect::<Vec<_>>()
    .try_into()
    .map_err(|_| "expected two atoms")?;
  let mut left = original.clone();
  let mut right = original.clone();
  let left_new = left.add_atom(Atom::new(Element::Oxygen))?;
  let right_new = right.add_atom(Atom::new(Element::Nitrogen))?;
  let left_bond = left.add_bond(a, left_new, BondOrder::Single)?;
  let right_bond = right.add_bond(a, right_new, BondOrder::Single)?;
  assert_ne!(left_new, right_new);
  assert_ne!(left_bond, right_bond);
  assert!(right.atom(left_new).is_none() && left.atom(right_new).is_none());
  assert!(right.bond(left_bond).is_none() && left.bond(right_bond).is_none());
  left.remove_atom(b)?;
  assert!(right.atom(b).is_some() && original.atom(b).is_some());
  // Removing branch-only additions restores snapshot equality despite allocation.
  left.remove_atom(left_new)?;
  right.remove_atom(right_new)?;
  assert_eq!(right, original);
  Ok(())
}

#[test]
fn empty_clones_should_share_provenance_before_the_first_insertion() -> TestResult {
  let mut left = MolecularGraph::new();
  let mut right = left.clone();
  let a = left.add_atom(Atom::new(Element::Carbon))?;
  let b = right.add_atom(Atom::new(Element::Carbon))?;
  assert_ne!(a, b);
  assert!(left.atom(b).is_none() && right.atom(a).is_none());
  Ok(())
}

#[test]
fn independent_parses_should_not_accept_each_others_stereo_references() -> TestResult {
  let source = read_smiles("N[C@H](F)Cl")?.into_graph();
  let mut target = read_smiles("N[C@H](F)Cl")?.into_graph();
  let (foreign_center, descriptor) = source.tetrahedral_centers().next().ok_or("missing stereo")?;
  let (local_center, _) = target.tetrahedral_centers().next().ok_or("missing stereo")?;
  let before = target.clone();
  assert_ne!(foreign_center, local_center);
  assert_eq!(
    target.set_tetrahedral(local_center, Some(*descriptor)),
    Err(StereoError::Coordination(local_center))
  );
  assert_eq!(target, before);
  Ok(())
}

#[test]
fn foreign_alkene_ligands_should_not_resolve_to_local_neighbors() -> TestResult {
  let source = read_smiles("F/C=C/F")?.into_graph();
  let mut target = read_smiles("F/C=C/F")?.into_graph();
  let (_, foreign) = source.double_bond_stereos().next().ok_or("missing stereo")?;
  let (bond, local) = target.double_bond_stereos().next().ok_or("missing stereo")?;
  let mixed = DoubleBondStereo::new(local.ends(), foreign.references(), local.relation());
  let before = target.clone();
  assert_eq!(
    target.set_double_bond_stereo(bond, Some(mixed)),
    Err(StereoError::DoubleBond(bond))
  );
  assert_eq!(target, before);
  Ok(())
}

#[test]
fn dropped_graph_namespaces_should_not_be_recycled() -> TestResult {
  let foreign = {
    let mut graph = MolecularGraph::new();
    graph.add_atom(Atom::new(Element::Carbon))?
  };
  for _ in 0..64 {
    let mut graph = MolecularGraph::new();
    let local = graph.add_atom(Atom::new(Element::Carbon))?;
    assert_ne!(local, foreign);
    assert!(graph.atom(foreign).is_none());
  }
  Ok(())
}

#[test]
fn concurrent_clone_edits_should_allocate_unique_handles() -> TestResult {
  // Begin before namespace initialization to exercise concurrent first inserts.
  let original = MolecularGraph::new();
  let results = std::thread::scope(|scope| {
    let workers: Vec<_> = (0..8)
      .map(|_| {
        let mut graph = original.clone();
        scope.spawn(move || -> Result<_, GraphError> {
          let anchor = graph.add_atom(Atom::new(Element::Carbon))?;
          let mut atoms = Vec::new();
          let mut bonds = Vec::new();
          for _ in 0..64 {
            let atom = graph.add_atom(Atom::new(Element::Carbon))?;
            atoms.push(atom);
            bonds.push(graph.add_bond(anchor, atom, BondOrder::Single)?);
          }
          Ok((atoms, bonds))
        })
      })
      .collect();
    workers.into_iter().map(|worker| worker.join()).collect::<Vec<_>>()
  });
  let mut atoms = HashSet::new();
  let mut bonds = HashSet::new();
  for result in results {
    let (new_atoms, new_bonds) = result.map_err(|_| "allocation worker panicked")??;
    for atom in new_atoms {
      assert!(atoms.insert(atom));
    }
    for bond in new_bonds {
      assert!(bonds.insert(bond));
    }
  }
  assert_eq!(atoms.len(), 512);
  assert_eq!(bonds.len(), 512);
  Ok(())
}
