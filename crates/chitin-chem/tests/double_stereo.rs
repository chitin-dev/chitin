//! Directional spelling is relative to traversal, never an absolute E/Z label.

mod support;
use chitin_chem::io::smiles::{read_smiles, write_smiles};
use chitin_chem::{BondOrder, DoubleBondRelation as Relation, DoubleBondStereo, StereoError};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn detached_aromatic_assignment_should_preserve_adjacent_alkene_annotations() -> TestResult {
  use chitin_chem::perception::{KekuleOptions, kekulize};
  for source in ["c1ccccc1/C=C/O", "c1ccccc1/C=C\\O"] {
    let graph = read_smiles(source)?.into_graph();
    let before = graph.clone();
    let resolved = kekulize(&graph, &KekuleOptions::default())?;
    let source_facts: Vec<_> = graph.double_bond_stereos().collect();
    assert_eq!(source_facts, resolved.double_bond_stereos().collect::<Vec<_>>());
    assert_eq!(source_facts.len(), 1);
    assert_eq!(graph, before);
  }
  Ok(())
}

#[test]
fn contradictory_cyclic_direction_constraints_should_fail_without_emitting_text() -> TestResult {
  let mut graph = read_smiles("C1=CC=CC=C1")?.into_graph();
  let axes: Vec<_> = graph
    .bonds()
    .filter(|(_, bond)| bond.order() == BondOrder::Double)
    .map(|(id, bond)| (id, bond.atoms()))
    .collect();
  for (edge, ends) in axes {
    let mut references = Vec::new();
    for end in ends {
      references.push(
        graph
          .neighbors(end)?
          .find(|(_, id)| *id != edge)
          .map(|(atom, _)| atom)
          .ok_or("no reference")?,
      );
    }
    graph.set_double_bond_stereo(
      edge,
      Some(DoubleBondStereo::new(
        ends,
        [references[0], references[1]],
        Relation::SameSide,
      )),
    )?;
  }
  let before = graph.clone();
  assert!(matches!(
    write_smiles(&graph),
    Err(chitin_chem::io::smiles::SmilesWriteError::Stereo(
      StereoError::DirectionConflict(_)
    ))
  ));
  assert_eq!(graph, before);
  Ok(())
}

#[test]
fn writer_should_not_invent_configuration_after_removing_a_conjugated_annotation() -> TestResult {
  let mut graph = read_smiles("F/C=C/C=C/C=C/F")?.into_graph();
  let edge = graph
    .double_bond_stereos()
    .nth(1)
    .map(|(edge, _)| edge)
    .ok_or("no middle double")?;
  graph.set_double_bond_stereo(edge, None)?;
  assert!(
    matches!(write_smiles(&graph), Err(chitin_chem::io::smiles::SmilesWriteError::Stereo(StereoError::DirectionConflict(id))) if id == edge)
  );
  Ok(())
}

#[test]
fn reversing_atom_and_bond_insertion_should_preserve_configuration_not_slash_spelling() -> TestResult {
  let original = read_smiles("F/C(Cl)=C(/Br)[C@H](N)O")?.into_graph();
  let mut changed = chitin_chem::MolecularGraph::new();
  // Leave an ID gap, then reverse atom and edge creation independently.
  let dummy = changed.add_atom(chitin_chem::Atom::new(chitin_chem::Element::Carbon))?;
  changed.remove_atom(dummy)?;
  let atoms: Vec<_> = original.atoms().collect();
  let mut mapping = std::collections::BTreeMap::new();
  for &(id, atom) in atoms.iter().rev() {
    mapping.insert(id, changed.add_atom(atom.clone())?);
  }
  let bonds: Vec<_> = original.bonds().collect();
  for &(_, bond) in bonds.iter().rev() {
    let [a, b] = bond.atoms();
    changed.add_bond(mapping[&b], mapping[&a], bond.order())?;
  }
  for (_, stereo) in original.double_bond_stereos() {
    let ends = stereo.ends().map(|atom| mapping[&atom]);
    let references = stereo.references().map(|atom| mapping[&atom]);
    let edge = changed.bond_between(ends[0], ends[1]).ok_or("no axis")?;
    changed.set_double_bond_stereo(edge, Some(DoubleBondStereo::new(ends, references, stereo.relation())))?;
  }
  for (center, stereo) in original.tetrahedral_centers() {
    let ligands = stereo.ligands().map(|ligand| match ligand {
      chitin_chem::StereoLigand::Atom(atom) => chitin_chem::StereoLigand::Atom(mapping[&atom]),
      chitin_chem::StereoLigand::Hydrogen => chitin_chem::StereoLigand::Hydrogen,
    });
    changed.set_tetrahedral(
      mapping[&center],
      Some(chitin_chem::TetrahedralStereo::new(ligands, stereo.winding())?),
    )?;
  }
  assert!(support::isomorphic(&original, &changed)?);
  let written = write_smiles(&changed)?;
  assert!(
    support::isomorphic(&original, read_smiles(&written)?.graph())?,
    "{written}"
  );
  Ok(())
}

#[test]
fn markers_should_resolve_to_reference_sides() -> TestResult {
  for (source, relation) in [("F/C=C/F", Relation::OppositeSides), ("F/C=C\\F", Relation::SameSide)] {
    let graph = read_smiles(source)?.into_graph();
    let (_, stereo) = graph.double_bond_stereos().next().ok_or("missing configuration")?;
    assert_eq!(stereo.relation(), relation, "{source}");
  }
  Ok(())
}

#[test]
fn equivalent_branch_and_reverse_spellings_should_match() -> TestResult {
  for (left, right) in [
    ("F/C=C/Cl", "Cl/C=C/F"),
    ("F/C=C/Cl", "F\\C=C\\Cl"),
    ("F/C=C/Cl", "C(\\F)=C/Cl"),
    ("F/C(Cl)=C/Br", "Cl/C(F)=C\\Br"),
    ("F/C(/Cl)=C/Br", "F/C(Cl)=C/Br"),
  ] {
    assert!(
      support::isomorphic(read_smiles(left)?.graph(), read_smiles(right)?.graph())?,
      "{left} != {right}"
    );
  }
  Ok(())
}

#[test]
fn opposite_and_absent_configurations_should_not_match() -> TestResult {
  let graph = read_smiles("F/C=C/Cl")?;
  for other in ["F/C=C\\Cl", "FC=CCl"] {
    assert!(!support::isomorphic(graph.graph(), read_smiles(other)?.graph())?);
  }
  Ok(())
}

#[test]
fn independent_writer_should_preserve_conjugated_branched_and_ring_directions() -> TestResult {
  for source in [
    "F/C=C/F",
    "F/C=C\\F",
    "Cl/C(F)=C(Br)/I",
    "F/C=C/C=C\\Cl",
    "F/C=C/C=C/C=C/Br",
    "c1ccccc1/C=C\\O",
    "N/C=N/C",
    "[H]/C(Cl)=C(/F)Br",
    "F/C=C/[C@H](Cl)Br",
    "C/1=C/CCCCCC1",
    "C1=C/CCCCCC/1",
    "C/1=C/CCCCCC\\1",
  ] {
    let graph = read_smiles(source)?.into_graph();
    let before = graph.clone();
    let written = write_smiles(&graph)?;
    let reread = read_smiles(&written)?;
    assert!(support::isomorphic(&graph, reread.graph())?, "{source} -> {written}");
    assert_eq!(graph, before);
    assert_eq!(written, write_smiles(&graph)?);
  }
  Ok(())
}

#[test]
fn malformed_and_partial_directions_should_report_source_locations() {
  for source in ["C/C", "F/C=C", "FC=C/F", "F/C(\\Cl)=C/F", "F/C=C=C/F", "C/1=C/CCCCCC/1"] {
    let Err(error) = read_smiles(source) else {
      panic!("accepted {source}");
    };
    assert!(
      error.span.start < error.span.end && error.span.end <= source.len(),
      "{source}: {error:?}"
    );
  }
}

#[test]
fn failed_stereo_installation_should_leave_graph_unchanged() -> TestResult {
  let mut graph = read_smiles("F/C=C/Cl")?.into_graph();
  let (edge, stereo) = graph
    .double_bond_stereos()
    .next()
    .map(|(id, value)| (id, *value))
    .ok_or("missing stereo")?;
  let before = graph.clone();
  assert_eq!(
    graph.set_double_bond_stereo(
      edge,
      Some(DoubleBondStereo::new(stereo.ends(), stereo.ends(), stereo.relation()))
    ),
    Err(StereoError::DoubleBond(edge))
  );
  assert_eq!(graph, before);
  let [a, b] = stereo.ends();
  assert!(graph.add_bond(a, b, BondOrder::Single).is_err());
  assert_eq!(graph, before);
  Ok(())
}

#[test]
fn endpoint_edits_should_invalidate_but_remote_attribute_edits_should_preserve_stereo() -> TestResult {
  let original = read_smiles("F/C=C/Cl")?.into_graph();
  let (edge, stereo) = original
    .double_bond_stereos()
    .next()
    .map(|(id, value)| (id, *value))
    .ok_or("missing stereo")?;
  let a = stereo.ends()[0];
  let reference = stereo.references()[0];
  let incident = original.bond_between(a, reference).ok_or("missing edge")?;
  for operation in 0..6 {
    let mut graph = original.clone();
    match operation {
      0 => {
        graph.remove_bond(incident)?;
      }
      1 => {
        graph.remove_atom(reference)?;
      }
      2 => {
        graph.remove_atom(a)?;
      }
      3 => {
        graph.set_bond_order(edge, BondOrder::Single)?;
      }
      4 => {
        graph.replace_atom(a, graph.atom(a).cloned().ok_or("missing atom")?)?;
      }
      _ => {
        let new = graph.add_atom(chitin_chem::Atom::new(chitin_chem::Element::Carbon))?;
        graph.add_bond(a, new, BondOrder::Single)?;
      }
    }
    assert!(graph.double_bond_stereo(edge).is_none());
  }
  let mut graph = original;
  graph.replace_atom(reference, graph.atom(reference).cloned().ok_or("missing reference")?)?;
  graph.set_bond_order(incident, BondOrder::Single)?;
  assert_eq!(graph.double_bond_stereo(edge), Some(&stereo));
  Ok(())
}
