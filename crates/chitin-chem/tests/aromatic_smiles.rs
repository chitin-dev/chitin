//! Aromatic input facts, integer representation, and lossless codec boundaries.

mod support;

use chitin_chem::io::smiles::{SmilesErrorKind, read_smiles, write_smiles};
use chitin_chem::perception::{KekuleError, KekuleOptions, ValenceDiagnostic, ValenceModel, analyze_valence, kekulize};
use chitin_chem::{Atom, BondOrder, Element, ImplicitHydrogenPolicy};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn aromatic_designations_and_exact_bond_orders_should_round_trip() -> TestResult {
  for source in [
    "c1ccccc1",
    "c:1:c:c:c:c:c:1",
    "n1ccccc1",
    "[nH]1cccc1",
    "c1ccc2ccccc2c1",
    "c1ccc2[nH]ccc2c1",
    "c1ccccc1-c1ccccc1",
    "[13cH]1ccccc1",
    "[nH+]1ccccc1",
    "[n-]1cccc1",
    "o1cccc1",
    "s1cccc1",
    "p1ccccc1",
    "b1ccccc1",
    "[as]1ccccc1",
    "[se]1cccc1",
    "C:C",
    "c=1cc=cc=c1",
    "c1ccccc1.O",
    "c1.ccccc1",
  ] {
    let input = read_smiles(source)?;
    let before = input.graph().clone();
    let written = write_smiles(input.graph())?;
    let reread = read_smiles(&written)?;
    assert!(
      support::isomorphic(input.graph(), reread.graph())?,
      "{source} -> {written}"
    );
    assert_eq!(input.graph(), &before);
  }
  Ok(())
}

#[test]
fn only_omitted_bonds_between_aromatic_endpoints_should_default_to_aromatic() -> TestResult {
  for (source, singles, aromatics) in [
    ("c1ccccc1", 0, 6),
    ("Cc1ccccc1", 1, 6),
    ("c1ccccc1-c1ccccc1", 1, 12),
    ("c-1ccccc1", 1, 5),
    ("c1ccccc-1", 1, 5),
    ("c:1ccccc1", 0, 6),
  ] {
    let input = read_smiles(source)?;
    assert_eq!(
      input
        .graph()
        .bonds()
        .filter(|(_, b)| b.order() == BondOrder::Single)
        .count(),
      singles,
      "{source}"
    );
    assert_eq!(
      input
        .graph()
        .bonds()
        .filter(|(_, b)| b.order() == BondOrder::Aromatic)
        .count(),
      aromatics,
      "{source}"
    );
    let written = write_smiles(input.graph())?;
    assert!(
      support::isomorphic(input.graph(), read_smiles(&written)?.graph())?,
      "{source} -> {written}"
    );
  }
  Ok(())
}

#[test]
fn lowercase_tokens_should_preserve_source_spans_and_bracket_hydrogen_knowledge() -> TestResult {
  let source = "[13cH]1cc[nH]c1";
  let input = read_smiles(source)?;
  for (id, atom) in input.graph().atoms() {
    assert!(atom.is_aromatic());
    let token = &source[input.atom_span(id).ok_or("missing atom span")?];
    assert_eq!(
      atom.unpaired_electrons(),
      if token.starts_with('[') { None } else { Some(0) }
    );
    assert_eq!(
      atom.implicit_hydrogen_policy(),
      if token.starts_with('[') {
        ImplicitHydrogenPolicy::Fixed(0)
      } else {
        ImplicitHydrogenPolicy::Infer
      }
    );
  }
  Ok(())
}

#[test]
fn kekule_assignment_should_preserve_source_handles_facts_and_expected_double_counts() -> TestResult {
  for (source, doubles) in [
    ("c1ccccc1", 3),
    ("n1ccccc1", 3),
    ("[nH]1cccc1", 2),
    ("Cn1cccc1", 2),
    ("o1cccc1", 2),
    ("s1cccc1", 2),
    ("[nH+]1ccccc1", 3),
    ("[n-]1cccc1", 2),
    ("c1ccc2ccccc2c1", 5),
    ("c1ccc2[nH]ccc2c1", 4),
    ("c1ccccc1-c1ccccc1", 6),
    ("c1ccccc1.O", 3),
    ("O=c1cccc[nH]1", 3),
    ("c1=cc=cc=c1", 3),
  ] {
    let source = read_smiles(source)?.into_graph();
    let before = source.clone();
    let result = kekulize(&source, &KekuleOptions::default())?;
    assert_eq!(
      result.bonds().filter(|(_, b)| b.order() == BondOrder::Double).count(),
      doubles
    );
    assert_eq!(source, before);
    assert_eq!(kekulize(&source, &KekuleOptions::default())?, result);
    for (id, atom) in source.atoms() {
      let resolved = result.atom(id).ok_or("lost atom")?;
      let expected_policy = if atom.is_aromatic()
        && atom.element() != Element::Carbon
        && atom.implicit_hydrogen_policy() == ImplicitHydrogenPolicy::Infer
      {
        ImplicitHydrogenPolicy::Fixed(0)
      } else {
        atom.implicit_hydrogen_policy()
      };
      assert_eq!(
        resolved,
        &atom
          .clone()
          .with_aromatic(false)
          .with_implicit_hydrogen_policy(expected_policy)
      );
    }
    for (id, bond) in source.bonds() {
      let resolved = result.bond(id).ok_or("lost bond")?;
      assert_eq!(resolved.atoms(), bond.atoms());
      if bond.order() != BondOrder::Aromatic {
        assert_eq!(resolved.order(), bond.order());
      }
    }
  }
  Ok(())
}

#[test]
fn bare_aromatic_heteroatoms_should_not_gain_implicit_hydrogens() -> TestResult {
  let source = read_smiles("n1ccccc1")?.into_graph();
  let resolved = kekulize(&source, &KekuleOptions::default())?;
  let report = analyze_valence(&resolved, ValenceModel::MainGroupClosedShellV1)?;
  for (id, atom) in resolved.atoms() {
    let value = report
      .atom(id)
      .and_then(|row| row.value())
      .ok_or("unresolved aromatic H")?;
    assert_eq!(value.hydrogens.implicit(), u8::from(atom.element() == Element::Carbon));
  }
  Ok(())
}

#[test]
fn unsupported_or_unsatisfiable_aromatic_chemistry_should_not_return_partial_assignments() -> TestResult {
  for source in [
    "c1cccc1",
    "c1cc1",
    "c",
    "cc",
    "C:C",
    "b1ccccc1",
    "p1ccccc1",
    "[se]1cccc1",
    "[cH2]1ccccc1",
    "[oH]1cccc1",
    "[n+3]1ccccc1",
  ] {
    let input = read_smiles(source)?.into_graph();
    let before = input.clone();
    assert!(kekulize(&input, &KekuleOptions::default()).is_err(), "{source}");
    assert_eq!(input, before);
    assert!(support::isomorphic(
      &input,
      read_smiles(&write_smiles(&input)?)?.graph()
    )?);
  }
  Ok(())
}

#[test]
fn budgets_should_be_shared_and_errors_should_be_distinct_from_no_assignment() -> TestResult {
  let input = read_smiles("c1ccccc1")?.into_graph();
  assert_eq!(
    kekulize(&input, &KekuleOptions { work_budget: 0 }),
    Err(KekuleError::Options)
  );
  assert_eq!(
    kekulize(&input, &KekuleOptions { work_budget: 12 }),
    Err(KekuleError::BudgetExceeded)
  );
  assert!(matches!(
    kekulize(read_smiles("c1cccc1")?.graph(), &KekuleOptions::default()),
    Err(KekuleError::NoAssignment(_))
  ));
  let budget = KekuleOptions { work_budget: 250 };
  assert!(kekulize(&input, &budget).is_ok());
  let fragments = read_smiles(&["c1ccccc1"; 10].join("."))?.into_graph();
  assert_eq!(kekulize(&fragments, &budget), Err(KekuleError::BudgetExceeded));
  Ok(())
}

#[test]
fn sparse_source_handles_and_even_antiaromatic_notation_should_not_be_renumbered_or_perceived() -> TestResult {
  let mut graph = read_smiles("O.c1ccc1")?.into_graph();
  let first = graph.atoms().next().map(|(id, _)| id).ok_or("missing leading atom")?;
  graph.remove_atom(first)?;
  let result = kekulize(&graph, &KekuleOptions::default())?;
  assert_eq!(
    result.atoms().map(|(id, _)| id).collect::<Vec<_>>(),
    graph.atoms().map(|(id, _)| id).collect::<Vec<_>>()
  );
  assert_eq!(
    result
      .bonds()
      .filter(|(_, bond)| bond.order() == BondOrder::Double)
      .count(),
    2
  );
  assert!(support::isomorphic(
    &graph,
    read_smiles(&write_smiles(&graph)?)?.graph()
  )?);
  Ok(())
}

#[test]
fn nonaromatic_valence_model_should_not_ignore_aromatic_atomic_flags() -> TestResult {
  let input = read_smiles("c1=cc=cc=c1")?.into_graph();
  let report = analyze_valence(&input, ValenceModel::MainGroupClosedShellV1)?;
  assert!(
    report
      .atoms()
      .all(|(_, row)| row.diagnostics().contains(&ValenceDiagnostic::AromaticAtom))
  );
  assert!(!Atom::new(Element::Carbon).is_aromatic());
  Ok(())
}

#[test]
fn malformed_lowercase_symbols_and_unsupported_stereo_should_still_fail() {
  for source in ["[cl]", "[si]", "se1cccc1", "as1ccccc1", "[nHH]", "c:1ccccc-1"] {
    assert!(read_smiles(source).is_err(), "{source}");
  }
  for source in ["[nH:1]1cccc1", "N[C@AL1](C)C(=O)O", "C/C=C\\C"] {
    assert!(
      matches!(
        read_smiles(source).map_err(|e| e.kind),
        Err(SmilesErrorKind::Unsupported(_))
      ),
      "{source}"
    );
  }
}
