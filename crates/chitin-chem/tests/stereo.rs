//! Relative configuration is invariant under SMILES traversal and graph handles.

mod support;

use chitin_chem::io::smiles::{read_smiles, write_smiles};
use chitin_chem::{BondOrder, StereoLigand, TetrahedralStereo, TetrahedralWinding};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn tetrahedral_facts_should_round_trip_through_an_independent_traversal() -> TestResult {
  for source in [
    "N[C@@](Br)(O)C",
    "N[C@](Br)(O)C",
    "N[C@H](O)C",
    "[C@H](F)(Cl)Br",
    "[C@@](F)(Cl)(Br)I",
    "[C@TH1](F)(Cl)(Br)I",
    "[C@TH2](F)(Cl)(Br)I",
    "F[C@]1(Cl)CCCC1",
    "F[C@]12CCCC1CCC2",
    "[C@]1(Br)(Cl)CCCC(F)C1",
    "CC1([C@@H](N2[C@H](S1)[C@@H](C2=O)NC(=O)Cc3ccccc3)C(=O)O)C",
    "[2H][C@@](F)(Cl)Br",
    "[13C@H](F)(Cl)Br",
    "O.N[C@H](C)C(=O)O",
  ] {
    let parsed = read_smiles(source)?;
    let before = parsed.graph().clone();
    let written = write_smiles(parsed.graph())?;
    let reread = read_smiles(&written)?;
    assert!(
      support::isomorphic(parsed.graph(), reread.graph())?,
      "{source} -> {written}"
    );
    assert_eq!(parsed.graph(), &before);
    assert_eq!(write_smiles(parsed.graph())?, written);
  }
  Ok(())
}

#[test]
fn reference_equivalent_spellings_should_preserve_configuration() -> TestResult {
  for (a, b) in [
    ("O[C@](Br)(C)N", "Br[C@](C)(O)N"),
    ("C[C@](Br)(N)O", "Br[C@](N)(C)O"),
    ("C[C@@](Br)(O)N", "Br[C@@](N)(O)C"),
    ("[C@@](C)(Br)(O)N", "[C@@](Br)(N)(O)C"),
    ("N[C@H](O)C", "N[C@@H](C)O"),
    ("N[C@H](O)C", "[C@@H](N)(O)C"),
    ("FC1C[C@](Br)(Cl)CCC1", "[C@]1(Br)(Cl)CCCC(F)C1"),
  ] {
    assert!(
      support::isomorphic(read_smiles(a)?.graph(), read_smiles(b)?.graph())?,
      "{a} != {b}"
    );
  }
  Ok(())
}

#[test]
fn enantiomers_and_missing_annotations_should_not_be_isomorphic() -> TestResult {
  for other in ["N[C@@H](O)C", "N[CH](O)C"] {
    assert!(!support::isomorphic(
      read_smiles("N[C@H](O)C")?.graph(),
      read_smiles(other)?.graph()
    )?);
  }
  Ok(())
}

#[test]
fn all_reference_permutations_should_invert_only_on_odd_parity() -> TestResult {
  let graph = read_smiles("[C@](F)(Cl)(Br)I")?.into_graph();
  let (_, descriptor) = graph.tetrahedral_centers().next().ok_or("missing stereo")?;
  let original = *descriptor;
  for a in 0..4 {
    for b in 0..4 {
      for c in 0..4 {
        for d in 0..4 {
          let p = [a, b, c, d];
          if (0..4).any(|i| p[i + 1..].contains(&p[i])) {
            continue;
          }
          let ligands = p.map(|i| original.ligands()[i]);
          let inversions = (0..4)
            .flat_map(|i| (i + 1..4).map(move |j| (i, j)))
            .filter(|&(i, j)| p[i] > p[j])
            .count();
          let changed = original.reordered(ligands)?;
          assert_eq!(
            changed.winding(),
            if inversions % 2 == 0 {
              original.winding()
            } else {
              original.winding().inverted()
            }
          );
          assert_eq!(changed.reordered(*original.ligands())?, original);
        }
      }
    }
  }
  Ok(())
}

#[test]
fn incident_edits_should_clear_stereo_but_failed_edits_should_leave_it_intact() -> TestResult {
  let mut graph = read_smiles("N[C@H](O)C")?.into_graph();
  let (center, stereo) = graph
    .tetrahedral_centers()
    .next()
    .map(|(id, s)| (id, *s))
    .ok_or("no stereo")?;
  let before = graph.clone();
  let mut invalid = *stereo.ligands();
  invalid[0] = StereoLigand::Atom(center);
  assert!(
    graph
      .set_tetrahedral(center, Some(TetrahedralStereo::new(invalid, stereo.winding())?))
      .is_err()
  );
  assert_eq!(graph, before);
  assert!(TetrahedralStereo::new([StereoLigand::Hydrogen; 4], TetrahedralWinding::Clockwise).is_err());
  let edge = graph.neighbors(center)?.next().map(|(_, edge)| edge).ok_or("no edge")?;
  graph.set_bond_order(edge, BondOrder::Double)?;
  assert!(graph.tetrahedral(center).is_none());
  assert!(graph.set_tetrahedral(center, Some(stereo)).is_err());
  Ok(())
}

#[test]
fn unsupported_coordination_and_stereo_classes_should_not_be_ignored() {
  for source in [
    "[C@]",
    "[C@H](F)Cl",
    "[C@H2](F)(Cl)Br",
    "[C@](F)(Cl)(Br)(I)N",
    "F[C@](Cl)=C",
    "[C@AL1](F)(Cl)(Br)I",
    "[C@SP1](F)(Cl)(Br)I",
    "[C@TH3](F)(Cl)(Br)I",
    "[C@@@](F)(Cl)(Br)I",
    "F/C=C/F",
  ] {
    assert!(read_smiles(source).is_err(), "{source}");
  }
}

#[test]
fn graph_edits_should_invalidate_only_the_affected_reference_frames() -> TestResult {
  let input = read_smiles("O.N[C@H](O)C")?.into_graph();
  let (center, descriptor) = input
    .tetrahedral_centers()
    .next()
    .map(|(id, s)| (id, *s))
    .ok_or("no stereo")?;
  let neighbor = input.neighbors(center)?.next().map(|(id, _)| id).ok_or("no neighbor")?;
  let edge = input.bond_between(center, neighbor).ok_or("no edge")?;
  for operation in 0..5 {
    let mut graph = input.clone();
    match operation {
      0 => {
        graph.remove_bond(edge)?;
      }
      1 => {
        graph.remove_atom(neighbor)?;
      }
      2 => {
        let atom = graph.atom(center).cloned().ok_or("no center")?;
        graph.replace_atom(center, atom)?;
      }
      3 => {
        let atom = graph.add_atom(chitin_chem::Atom::new(chitin_chem::Element::Fluorine))?;
        graph.add_bond(center, atom, BondOrder::Single)?;
      }
      _ => {
        graph.remove_atom(center)?;
      }
    }
    assert!(graph.tetrahedral(center).is_none());
  }
  let mut graph = input.clone();
  assert!(graph.add_bond(center, neighbor, BondOrder::Single).is_err());
  assert_eq!(graph, input);
  let distant = graph.atoms().next().map(|(id, _)| id).ok_or("no isolated atom")?;
  graph.remove_atom(distant)?;
  assert_eq!(graph.tetrahedral(center), Some(&descriptor));
  assert!(support::isomorphic(
    &graph,
    read_smiles(&write_smiles(&graph)?)?.graph()
  )?);
  Ok(())
}

#[test]
fn organic_valence_model_should_resolve_sulfur_without_widening_original_v1() -> TestResult {
  use chitin_chem::perception::{ValenceDiagnostic, ValenceModel, ValenceStatus, analyze_valence};
  for (source, target, hydrogens) in [
    ("S", 2, 2),
    ("CSC", 2, 0),
    ("CS(=O)C", 4, 0),
    ("CS(=O)(=O)C", 6, 0),
    ("P", 3, 3),
    ("CP(C)C", 3, 0),
    ("P(Cl)(Cl)(Cl)(Cl)Cl", 5, 0),
  ] {
    let graph = read_smiles(source)?.into_graph();
    let before = graph.clone();
    let id = graph
      .atoms()
      .find(|(_, atom)| {
        matches!(
          atom.element(),
          chitin_chem::Element::Sulfur | chitin_chem::Element::Phosphorus
        )
      })
      .map(|(id, _)| id)
      .ok_or("missing P/S")?;
    let report = analyze_valence(&graph, ValenceModel::SmilesOrganicV1)?;
    let value = report
      .atom(id)
      .and_then(|row| row.value())
      .ok_or("unresolved organic valence")?;
    assert_eq!(
      (value.total_valence, value.hydrogens.implicit()),
      (target, hydrogens),
      "{source}"
    );
    let old = analyze_valence(&graph, ValenceModel::MainGroupClosedShellV1)?;
    assert_eq!(old.atom(id).map(|row| row.status()), Some(ValenceStatus::Unsupported));
    assert!(old.atom(id).is_some_and(|row| {
      row
        .diagnostics()
        .iter()
        .any(|d| matches!(d, ValenceDiagnostic::UnsupportedElement(_)))
    }));
    assert_eq!(graph, before);
  }
  let excess = read_smiles("S(C)(C)(C)(C)(C)(C)C")?.into_graph();
  let report = analyze_valence(&excess, ValenceModel::SmilesOrganicV1)?;
  assert!(report.atoms().any(|(_, row)| row.status() == ValenceStatus::Invalid));
  let unresolved = read_smiles("[S]")?.into_graph();
  let report = analyze_valence(&unresolved, ValenceModel::SmilesOrganicV1)?;
  assert!(
    report
      .atoms()
      .all(|(_, row)| row.diagnostics().contains(&ValenceDiagnostic::UnknownRadicalState))
  );
  Ok(())
}
