//! Serialization preserves supplied facts, not IDs, input spelling, or caches.

mod support;

use chitin_chem::io::smiles::{SmilesAtomWriteError, SmilesWriteError, read_smiles, write_smiles};
use chitin_chem::{Atom, BondOrder, Element, ImplicitHydrogenPolicy, Isotope, MolecularGraph};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Checks exact labelled-graph equivalence, source immutability, and deterministic output.
///
/// # Errors
/// Propagates codec/topology failures; assertions diagnose semantic mismatch.
fn round_trip(source: &str) -> TestResult {
  let parsed = read_smiles(source)?;
  let before = parsed.graph().clone();
  let text = write_smiles(parsed.graph())?;
  let reread = read_smiles(&text)?;
  assert!(
    support::isomorphic(parsed.graph(), reread.graph())?,
    "{source} -> {text}"
  );
  assert_eq!(parsed.graph(), &before);
  assert_eq!(write_smiles(parsed.graph())?, text);
  Ok(())
}

#[test]
fn supported_reader_facts_should_round_trip_without_sanitization() -> TestResult {
  for source in [
    "",
    "CCO",
    "C=O",
    "C#N",
    "CC(O)C",
    "CC(=O)(O)N",
    "C(C)(C)(C)(C)C",
    "C1CCCCC1",
    "C1=CC=CC=C1",
    "C12CCC(CC1)CC2",
    "C1CC2CCC1C2",
    "C1CCC2(CC1)CCCC2",
    "C=1CCCCC1",
    "C1CCCCC=1",
    "C1.C1",
    "C%01CCCCC1",
    "CCO.O",
    "C(.O)N",
    "C1CCCCC1C1CCCCC1",
    "ClCBr",
    "P",
    "S",
    "[Fe+2]",
    "[C]",
    "[CH0]",
    "[CH]",
    "[CH3]",
    "[CH4]",
    "[NH4+]",
    "[O-]",
    "[C+0]",
    "[C-99]",
    "[C+99]",
    "[CH9]",
    "[999C]",
    "[13CH4]",
    "[H][H]",
    "[2H]O[3H]",
    "[NH3][H]",
    "[O--].[Fe++]",
    "[13C](C)([H])[NH2]",
    "[Og]",
  ] {
    round_trip(source)?;
  }
  Ok(())
}

#[test]
fn all_bracket_elements_should_round_trip() -> TestResult {
  for element in Element::ALL {
    round_trip(&format!("[{element}]"))?;
  }
  Ok(())
}

#[test]
fn spelling_should_distinguish_inference_brackets_and_declared_h() -> TestResult {
  for (input, output) in [
    ("C", "C"),
    ("[C]", "[C]"),
    ("[CH0]", "[C]"),
    ("[CH4]", "[CH4]"),
    ("[CH1]", "[CH]"),
    ("[C+0]", "[C]"),
    ("[Fe++]", "[Fe+2]"),
    ("[O--]", "[O-2]"),
  ] {
    let graph = read_smiles(input)?.into_graph();
    assert_eq!(write_smiles(&graph)?, output);
  }
  Ok(())
}

#[test]
fn multiple_bond_branches_and_ring_orders_should_be_retained() -> TestResult {
  for source in ["C(=O)(#N)C", "C=1CCC#CC1", "C1CCC=1", "CC(C)(O)N"] {
    round_trip(source)?;
  }
  Ok(())
}

#[test]
fn writer_should_use_graph_facts_not_the_readers_original_source() -> TestResult {
  let mut graph = read_smiles("CCO")?.into_graph();
  let oxygen = graph
    .atoms()
    .find(|(_, a)| a.element() == Element::Oxygen)
    .map(|(id, _)| id)
    .ok_or("no oxygen")?;
  let atom = read_smiles("[O-]")?
    .graph()
    .atoms()
    .next()
    .map(|(_, a)| a.clone())
    .ok_or("no replacement")?;
  graph.replace_atom(oxygen, atom)?;
  let bond = graph.bonds().next().map(|(id, _)| id).ok_or("no bond")?;
  graph.set_bond_order(bond, BondOrder::Double)?;
  let text = write_smiles(&graph)?;
  assert!(text.contains("[O-]"));
  assert!(text.contains('='));
  assert!(support::isomorphic(&graph, read_smiles(&text)?.graph())?);
  Ok(())
}

#[test]
fn sparse_ids_and_permuted_atom_and_bond_insertion_should_not_change_round_trip_semantics() -> TestResult {
  let original = read_smiles("C1C(O)C(N)CC1.[13CH3][O-]")?;
  let rows: Vec<_> = original.graph().atoms().collect();
  let mut graph = MolecularGraph::new();
  let mut mapping = std::collections::BTreeMap::new();
  for &(id, atom) in rows.iter().rev() {
    let hole = graph.add_atom(Atom::new(Element::Helium))?;
    graph.remove_atom(hole)?;
    mapping.insert(id, graph.add_atom(atom.clone())?);
  }
  let bonds: Vec<_> = original.graph().bonds().collect();
  for &(_, bond) in bonds.iter().rev() {
    let [a, b] = bond.atoms();
    graph.add_bond(mapping[&b], mapping[&a], bond.order())?;
  }
  assert_ne!(&graph, original.graph());
  assert!(support::isomorphic(&graph, original.graph())?);
  let text = write_smiles(&graph)?;
  assert!(support::isomorphic(&graph, read_smiles(&text)?.graph())?);
  Ok(())
}

#[test]
fn matcher_should_compare_labelled_topology_not_counts_or_input_positions() -> TestResult {
  for (left, right, expected) in [
    ("CCCC", "CC(C)C", false),
    ("CCCO", "CC(O)C", false),
    ("C1CCCCC1", "C1CC1.C1CC1", false),
    ("[13CH4]", "[12CH4]", false),
    ("[C+]", "[C-]", false),
    ("[CH3]", "[CH4]", false),
    ("[C]", "C", false),
    ("C=CC", "CC=C", true),
    ("CCO", "OCC", true),
    ("C1CC1.O", "O.C1CC1", true),
    ("C#N", "C=N", false),
  ] {
    let equivalent = support::isomorphic(read_smiles(left)?.graph(), read_smiles(right)?.graph())?;
    // Reversing the same labelled path really is equivalent; the matcher must
    // search correspondences, not assume atom positions already line up.
    assert_eq!(equivalent, expected, "{left} vs {right}");
  }
  Ok(())
}

/// Creates representable bracket facts without guessing a closed-shell state.
fn bracket_atom() -> Atom {
  Atom::new(Element::Carbon)
    .with_formal_charge(Some(0))
    .with_declared_hydrogens(Some(0))
    .with_implicit_hydrogen_policy(ImplicitHydrogenPolicy::Fixed(0))
}

#[test]
fn missing_or_unrepresentable_facts_should_be_errors_not_defaults() -> TestResult {
  use SmilesAtomWriteError as Reason;
  let base = bracket_atom();
  for (atom, expected) in [
    (base.clone().with_formal_charge(None), Reason::UnknownFormalCharge),
    (
      base.clone().with_declared_hydrogens(None),
      Reason::UnknownDeclaredHydrogens,
    ),
    (
      base
        .clone()
        .with_implicit_hydrogen_policy(ImplicitHydrogenPolicy::Unspecified),
      Reason::UnknownHydrogenPolicy,
    ),
    (
      base
        .clone()
        .with_implicit_hydrogen_policy(ImplicitHydrogenPolicy::Infer),
      Reason::ImplicitInference,
    ),
    (
      base.clone().with_implicit_hydrogens(Some(1)),
      Reason::FixedImplicitHydrogens(1),
    ),
    (
      base.clone().with_unpaired_electrons(Some(0)),
      Reason::ExplicitRadicalState(0),
    ),
    (
      base.clone().with_unpaired_electrons(Some(2)),
      Reason::ExplicitRadicalState(2),
    ),
    (
      base.clone().with_declared_hydrogens(Some(10)),
      Reason::HydrogenCountOutOfRange(10),
    ),
    (
      base.clone().with_formal_charge(Some(100)),
      Reason::ChargeOutOfRange(100),
    ),
    (
      base.clone().with_formal_charge(Some(i16::MIN)),
      Reason::ChargeOutOfRange(i16::MIN),
    ),
    (
      Atom::from_isotope(Isotope::new(Element::Carbon, 1000)?)
        .with_formal_charge(Some(0))
        .with_declared_hydrogens(Some(0))
        .with_implicit_hydrogens(Some(0)),
      Reason::IsotopeOutOfRange(1000),
    ),
    (
      Atom::new(Element::Hydrogen)
        .with_formal_charge(Some(0))
        .with_declared_hydrogens(Some(1))
        .with_implicit_hydrogens(Some(0)),
      Reason::HydrogenAsHost,
    ),
  ] {
    let mut graph = MolecularGraph::new();
    let id = graph.add_atom(atom)?;
    let before = graph.clone();
    assert_eq!(
      write_smiles(&graph),
      Err(SmilesWriteError::Atom {
        atom: id,
        reason: expected
      })
    );
    assert_eq!(graph, before);
  }
  Ok(())
}

#[test]
fn inference_should_only_be_spelled_bare_when_every_reader_default_matches() -> TestResult {
  let bare = read_smiles("C")?
    .graph()
    .atoms()
    .next()
    .map(|(_, a)| a.clone())
    .ok_or("no bare atom")?;
  for atom in [
    bare.clone().with_formal_charge(Some(1)),
    bare.clone().with_declared_hydrogens(Some(1)),
    bare.clone().with_unpaired_electrons(None),
    bare.with_unpaired_electrons(Some(1)),
    Atom::from_isotope(Isotope::new(Element::Carbon, 13)?)
      .with_formal_charge(Some(0))
      .with_declared_hydrogens(Some(0))
      .with_unpaired_electrons(Some(0))
      .with_implicit_hydrogen_policy(ImplicitHydrogenPolicy::Infer),
  ] {
    let mut graph = MolecularGraph::new();
    let atom = graph.add_atom(atom)?;
    assert_eq!(
      write_smiles(&graph),
      Err(SmilesWriteError::Atom {
        atom,
        reason: SmilesAtomWriteError::ImplicitInference
      })
    );
  }
  Ok(())
}

#[test]
fn unknown_bonds_should_not_be_replaced_with_single_bonds() -> TestResult {
  let order = BondOrder::Unknown;
  let mut graph = read_smiles("CC")?.into_graph();
  let bond = graph.bonds().next().map(|(id, _)| id).ok_or("no bond")?;
  graph.set_bond_order(bond, order)?;
  assert_eq!(write_smiles(&graph), Err(SmilesWriteError::Bond { bond, order }));
  Ok(())
}

/// Creates a dense, deliberately nonphysical carbon graph for ring-label tests.
///
/// # Parameters
/// - `size`: vertex count for the complete simple graph, not a chemical valence.
///
/// # Returns
/// Bare-reader-compatible atoms connected by every unordered single-bond pair.
/// Chemistry is intentionally invalid at high degree: serialization must not
/// replace graph preservation with an accidental sanitization gate.
///
/// # Errors
/// Propagates fixture-reader failures or graph allocation/connectivity errors.
fn complete_graph(size: usize) -> Result<MolecularGraph, Box<dyn std::error::Error>> {
  let atom = read_smiles("C")?
    .graph()
    .atoms()
    .next()
    .map(|(_, a)| a.clone())
    .ok_or("no atom")?;
  let mut graph = MolecularGraph::new();
  let ids = (0..size)
    .map(|_| graph.add_atom(atom.clone()))
    .collect::<Result<Vec<_>, _>>()?;
  for a in 0..size {
    for b in a + 1..size {
      graph.add_bond(ids[a], ids[b], BondOrder::Single)?;
    }
  }
  Ok(graph)
}

#[test]
fn dense_graphs_should_use_percent_labels_and_retain_every_edge() -> TestResult {
  let graph = complete_graph(8)?;
  let text = write_smiles(&graph)?;
  assert!(text.contains('%'));
  assert!(support::isomorphic(&graph, read_smiles(&text)?.graph())?);
  Ok(())
}

#[test]
fn total_cycle_count_should_not_limit_reusable_ring_labels() -> TestResult {
  let graph = complete_graph(20)?;
  assert!(graph.bond_count() - graph.atom_count() + 1 > 100);
  let text = write_smiles(&graph)?;
  assert!(support::isomorphic(&graph, read_smiles(&text)?.graph())?);
  let source = "C1CC1".repeat(200);
  let graph = read_smiles(&source)?.into_graph();
  let reread = read_smiles(&write_smiles(&graph)?)?;
  assert_eq!((reread.graph().atom_count(), reread.graph().bond_count()), (600, 799));
  Ok(())
}

#[test]
fn too_many_simultaneously_open_labels_should_return_a_typed_error() -> TestResult {
  let graph = complete_graph(21)?;
  let before = graph.clone();
  assert!(
    matches!(write_smiles(&graph), Err(SmilesWriteError::RingLabelCapacity { atom }) if graph.atom(atom).is_some())
  );
  assert_eq!(graph, before);
  Ok(())
}

#[test]
fn long_chains_and_deep_branches_should_not_use_recursive_writer_stacks() -> TestResult {
  for (source, max_degree) in [
    ("C".repeat(10_000), 2),
    (format!("C{}{}", "(C".repeat(5_000), ")".repeat(5_000)), 2),
    // Every first child continues a deep subtree and every last child is a
    // leaf, so emission must retain thousands of pending closing parentheses.
    (format!("{}C{}", "C(".repeat(5_000), ")C".repeat(5_000)), 3),
  ] {
    let graph = read_smiles(&source)?.into_graph();
    let text = write_smiles(&graph)?;
    let reread = read_smiles(&text)?;
    assert_eq!(reread.graph().atom_count(), graph.atom_count());
    assert_eq!(reread.graph().bond_count(), graph.bond_count());
    assert_eq!(reread.graph().connected_components().len(), 1);
    assert!(
      reread
        .graph()
        .atoms()
        .all(|(id, _)| reread.graph().neighbors(id).is_ok_and(|n| n.len() <= max_degree))
    );
  }
  Ok(())
}

#[test]
fn every_four_atom_edge_order_assignment_should_round_trip() -> TestResult {
  let base = read_smiles("C.N.[13CH2].[O-]")?;
  let ids: Vec<_> = base.graph().atoms().map(|(id, _)| id).collect();
  let pairs = [(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)];
  // Six unordered pairs, each absent/single/double/triple: 4^6 assignments.
  // This deliberately includes valence-invalid drafts: the writer is not a
  // sanitizer, and topology/H annotations must still survive unchanged.
  for assignment in 0..4_usize.pow(pairs.len() as u32) {
    let mut graph = base.graph().clone();
    let mut bits = assignment;
    for (a, b) in pairs {
      let order = match bits % 4 {
        1 => Some(BondOrder::Single),
        2 => Some(BondOrder::Double),
        3 => Some(BondOrder::Triple),
        _ => None,
      };
      bits /= 4;
      if let Some(order) = order {
        graph.add_bond(ids[a], ids[b], order)?;
      }
    }
    let text = write_smiles(&graph)?;
    assert!(
      support::isomorphic(&graph, read_smiles(&text)?.graph())?,
      "assignment {assignment}: {text}"
    );
  }
  Ok(())
}

#[test]
fn matcher_should_distinguish_a_known_radical_state_from_unknown() -> TestResult {
  let original = read_smiles("[CH4]")?.into_graph();
  let (id, atom) = original.atoms().next().ok_or("no atom")?;
  let mut known = original.clone();
  known.replace_atom(id, atom.clone().with_unpaired_electrons(Some(0)))?;
  assert!(!support::isomorphic(&original, &known)?);
  Ok(())
}

#[test]
fn failure_in_a_later_component_should_not_publish_a_partial_result() -> TestResult {
  let mut graph = read_smiles("CCO")?.into_graph();
  let invalid = graph.add_atom(Atom::new(Element::Neon))?;
  let before = graph.clone();
  assert_eq!(
    write_smiles(&graph),
    Err(SmilesWriteError::Atom {
      atom: invalid,
      reason: SmilesAtomWriteError::UnknownFormalCharge,
    })
  );
  assert_eq!(graph, before);
  Ok(())
}
