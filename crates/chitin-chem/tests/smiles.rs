//! Reader contracts: lossless supported facts, strict failures, no sanitization.

use chitin_chem::io::smiles::{SmilesErrorKind, read_smiles};
use chitin_chem::perception::{ValenceDiagnostic, ValenceModel, ValenceStatus, analyze_valence};
use chitin_chem::{Atom, BondOrder, Element, ImplicitHydrogenPolicy};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn basic_topologies_should_have_the_expected_connectivity() -> TestResult {
  for (source, atoms, bonds, fragments) in [
    ("", 0, 0, 0),
    ("CCO", 3, 2, 1),
    ("C=O", 2, 1, 1),
    ("C#N", 2, 1, 1),
    ("CC(O)C", 4, 3, 1),
    ("C1CCCCC1", 6, 6, 1),
    ("CCO.O", 4, 2, 2),
    ("C1.C1", 2, 1, 1),
    ("C(.O)N", 3, 1, 2),
    ("ClCBr", 3, 2, 1),
    ("[H][H]", 2, 1, 1),
    ("C12CCC(CC1)CC2", 8, 9, 1),
  ] {
    let parsed = read_smiles(source)?;
    let graph = parsed.graph();
    assert_eq!(
      (
        graph.atom_count(),
        graph.bond_count(),
        graph.connected_components().len()
      ),
      (atoms, bonds, fragments),
      "{source}"
    );
  }
  Ok(())
}

#[test]
fn branches_should_restore_the_parent_and_keep_bond_orders() -> TestResult {
  let parsed = read_smiles("CC(=O)(O)N")?;
  let atoms: Vec<_> = parsed.graph().atoms().map(|(id, _)| id).collect();
  assert_eq!(parsed.graph().neighbors(atoms[1])?.len(), 4);
  let edge = parsed
    .graph()
    .bond_between(atoms[1], atoms[2])
    .ok_or("missing carbonyl bond")?;
  assert_eq!(parsed.graph().bond(edge).map(|b| b.order()), Some(BondOrder::Double));
  assert_eq!(parsed.graph().bond_between(atoms[3], atoms[4]), None);
  Ok(())
}

#[test]
fn ring_labels_should_support_reuse_aliases_and_orders_on_either_site() -> TestResult {
  for source in ["C1CCCCC1C1CCCCC1", "C%01CCCCC1", "C=1CCCCC1", "C1CCCCC=1", "C=1CCCCC=1"] {
    let parsed = read_smiles(source)?;
    assert_eq!(parsed.graph().connected_components().len(), 1, "{source}");
    let doubles = parsed
      .graph()
      .bonds()
      .filter(|(_, b)| b.order() == BondOrder::Double)
      .count();
    assert_eq!(doubles, usize::from(source.contains('=')), "{source}");
  }
  Ok(())
}

#[test]
fn bracket_atoms_should_preserve_isotope_hydrogen_and_charge_facts() -> TestResult {
  for (source, element, mass, h, charge) in [
    ("[13CH4]", Element::Carbon, Some(13), 4, 0),
    ("[NH4+]", Element::Nitrogen, None, 4, 1),
    ("[O-]", Element::Oxygen, None, 0, -1),
    ("[Fe+2]", Element::Iron, None, 0, 2),
    ("[Fe++]", Element::Iron, None, 0, 2),
    ("[O--]", Element::Oxygen, None, 0, -2),
    ("[2H]", Element::Hydrogen, Some(2), 0, 0),
    ("[3H]", Element::Hydrogen, Some(3), 0, 0),
    ("[Og]", Element::Oganesson, None, 0, 0),
  ] {
    let parsed = read_smiles(source)?;
    let (_, atom) = parsed.graph().atoms().next().ok_or("missing bracket atom")?;
    assert_eq!(
      (
        atom.element(),
        atom.isotope().map(|i| i.mass_number()),
        atom.declared_hydrogens(),
        atom.formal_charge()
      ),
      (element, mass, Some(h), Some(charge)),
      "{source}"
    );
    assert_eq!(atom.implicit_hydrogen_policy(), ImplicitHydrogenPolicy::Fixed(0));
    assert_eq!(atom.unpaired_electrons(), None);
  }
  Ok(())
}

#[test]
fn brackets_must_not_inherit_the_bare_atom_hydrogen_or_radical_policy() -> TestResult {
  let bare = read_smiles("C")?;
  let bracket = read_smiles("[C]")?;
  let explicit_zero = read_smiles("[CH0]")?;
  assert_eq!(bracket.graph(), explicit_zero.graph());
  let model = ValenceModel::MainGroupClosedShellV1;
  let bare_report = analyze_valence(bare.graph(), model)?;
  let bracket_report = analyze_valence(bracket.graph(), model)?;
  let (_, row) = bare_report.atoms().next().ok_or("missing bare result")?;
  assert_eq!(row.value().map(|v| v.hydrogens.implicit()), Some(4));
  let (_, row) = bracket_report.atoms().next().ok_or("missing bracket result")?;
  assert_eq!(row.status(), ValenceStatus::Indeterminate);
  assert_eq!(row.diagnostics(), &[ValenceDiagnostic::UnknownRadicalState]);
  Ok(())
}

#[test]
fn bracket_hydrogens_should_not_create_duplicate_vertices() -> TestResult {
  let parsed = read_smiles("[NH4+]")?;
  assert_eq!((parsed.graph().atom_count(), parsed.graph().bond_count()), (1, 0));
  // Establishing a radical state is an explicit caller/perception decision,
  // never a side effect of accepting the bracket grammar.
  let mut graph = parsed.into_graph();
  let (id, atom) = graph
    .atoms()
    .next()
    .map(|(id, a)| (id, a.clone()))
    .ok_or("missing nitrogen")?;
  graph.replace_atom(id, atom.with_unpaired_electrons(Some(0)))?;
  let report = analyze_valence(&graph, ValenceModel::MainGroupClosedShellV1)?;
  assert_eq!(
    report.atom(id).and_then(|r| r.value()).map(|v| v.hydrogens.total()),
    Some(4)
  );
  Ok(())
}

#[test]
fn reader_should_not_run_valence_validation_or_modify_inferred_properties() -> TestResult {
  for (source, expected) in [
    ("CCO", ValenceStatus::Determined),
    ("C(C)(C)(C)(C)C", ValenceStatus::Invalid),
    ("P", ValenceStatus::Unsupported),
  ] {
    let parsed = read_smiles(source)?;
    let before = parsed.graph().clone();
    let report = analyze_valence(parsed.graph(), ValenceModel::MainGroupClosedShellV1)?;
    assert_eq!(
      report.atoms().next().map(|(_, r)| r.status()),
      Some(expected),
      "{source}"
    );
    assert_eq!(parsed.graph(), &before);
    assert!(parsed.graph().atoms().all(|(_, a)| a.implicit_hydrogens().is_none()));
  }
  Ok(())
}

#[test]
fn all_element_symbols_should_be_readable_inside_brackets() -> TestResult {
  for element in Element::ALL {
    let source = format!("[{element}]");
    let parsed = read_smiles(&source)?;
    assert_eq!(parsed.graph().atoms().next().map(|(_, a)| a.element()), Some(element));
  }
  Ok(())
}

#[test]
fn unsupported_features_should_fail_instead_of_losing_information() {
  for source in ["C/C=C", "*", "[*]", "[C:1]", "C>>O", "C|foo|", "C~C", "C$C"] {
    assert!(
      matches!(
        read_smiles(source).map_err(|e| e.kind),
        Err(SmilesErrorKind::Unsupported(_))
      ),
      "{source}"
    );
  }
}

#[test]
fn malformed_input_should_not_return_partial_graphs() {
  for source in [
    "(",
    ")",
    "C()",
    "C(1)",
    "C(=1CC1)",
    "C(",
    "C(O",
    "C(O))",
    "C1CC",
    "C%",
    "C%1",
    "C%a1",
    "C%1a",
    "C1CCCCC=1=",
    "C=1CCCCC#1",
    "C11",
    "C1C1",
    "C12CC12",
    "C..O",
    "C.",
    ".C",
    "=C",
    "C=",
    "C==O",
    "C=(O)",
    "C(=)",
    "C(O)1CC1",
    "C(O)=1CC1",
    "[",
    "[]",
    "[C",
    "[CH",
    "[CH2",
  ] {
    assert!(read_smiles(source).is_err(), "unexpectedly accepted {source}");
  }
}

#[test]
fn whitespace_and_record_names_should_not_be_silently_discarded() {
  for source in [" C", "C ", "C\n", "C\tname", "CCO ethanol", "C\0O"] {
    assert!(
      matches!(read_smiles(source).map_err(|e| e.kind), Err(SmilesErrorKind::Syntax(_))),
      "{source:?}"
    );
  }
}

#[test]
fn errors_should_use_source_byte_ranges_and_preserve_causes() -> TestResult {
  for (source, span) in [
    ("CC=", 2..3),
    ("C(O", 1..2),
    ("C1CC", 1..2),
    ("C.", 2..2),
    ("C☃", 1..4),
    ("C[", 2..2),
    ("C%1", 3..3),
  ] {
    let failure = read_smiles(source).err().ok_or("expected read failure")?;
    assert_eq!(failure.span, span, "{source}");
    assert!(source.get(failure.span.clone()).is_some(), "{source}");
    assert!(!failure.to_string().is_empty());
  }
  assert!(matches!(
    read_smiles("C11").map_err(|e| e.kind),
    Err(SmilesErrorKind::Topology(_))
  ));
  assert!(matches!(
    read_smiles("[Xx]").map_err(|e| e.kind),
    Err(SmilesErrorKind::Element(_))
  ));
  assert!(matches!(
    read_smiles("[1C]").map_err(|e| e.kind),
    Err(SmilesErrorKind::Isotope(_))
  ));
  Ok(())
}

#[test]
fn numeric_overflow_should_be_reported_without_wrapping() {
  for source in [
    "[1000C]",
    "[65536C]",
    "[CH10]",
    "[CH256]",
    "[C+100]",
    "[C-100]",
    "[C+999999999999999999999999]",
  ] {
    assert!(
      matches!(
        read_smiles(source).map_err(|e| e.kind),
        Err(SmilesErrorKind::NumberOutOfRange(_))
      ),
      "{source}"
    );
  }
}

#[test]
fn numeric_boundaries_should_be_representable() -> TestResult {
  for (source, charge) in [("[C+]", 1), ("[C+0]", 0), ("[C-0]", 0), ("[C+99]", 99), ("[C-99]", -99)] {
    let parsed = read_smiles(source)?;
    assert_eq!(
      parsed.graph().atoms().next().and_then(|(_, a)| a.formal_charge()),
      Some(charge)
    );
  }
  let isotope = read_smiles("[999C]")?;
  assert_eq!(
    isotope
      .graph()
      .atoms()
      .next()
      .and_then(|(_, a)| a.isotope())
      .map(|i| i.mass_number()),
    Some(999)
  );
  let hydrogen = read_smiles("[CH9]")?;
  assert_eq!(
    hydrogen
      .graph()
      .atoms()
      .next()
      .and_then(|(_, a)| a.declared_hydrogens()),
    Some(9)
  );
  Ok(())
}

#[test]
fn hydrogen_atoms_cannot_declare_attached_hydrogens() {
  for source in ["[HH]", "[HH0]", "[2HH1]"] {
    assert!(read_smiles(source).is_err());
  }
}

#[test]
fn source_maps_should_cover_atoms_ordinary_bonds_and_both_ring_sites() -> TestResult {
  let source = "[13CH2]=1CCCCC1.O";
  let parsed = read_smiles(source)?;
  assert_eq!(parsed.source(), source);
  for (id, atom) in parsed.graph().atoms() {
    let span = parsed.atom_span(id).ok_or("missing atom span")?;
    assert_eq!(
      &source[span],
      if atom.isotope().is_some() {
        "[13CH2]"
      } else if atom.element() == Element::Oxygen {
        "O"
      } else {
        "C"
      }
    );
  }
  for (id, bond) in parsed.graph().bonds() {
    let sites = parsed.bond_spans(id).ok_or("missing bond spans")?;
    if bond.order() == BondOrder::Double {
      let text: Vec<_> = sites.iter().map(|span| &source[span.clone()]).collect();
      assert_eq!(text, vec!["=1", "1"]);
    } else {
      assert_eq!(sites.len(), 1);
      assert!(sites[0].is_empty());
    }
  }
  let explicit = read_smiles("C-O")?;
  let (id, _) = explicit.graph().bonds().next().ok_or("missing single bond")?;
  let spans = explicit.bond_spans(id).ok_or("missing explicit bond site")?;
  assert_eq!(spans.len(), 1);
  assert_eq!(spans.first(), Some(&(1..2)));
  Ok(())
}

#[test]
fn deep_branches_should_be_processed_without_recursion() -> TestResult {
  let depth = 5_000;
  let source = format!("C{}{}", "(C".repeat(depth), ")".repeat(depth));
  let parsed = read_smiles(&source)?;
  assert_eq!(parsed.graph().atom_count(), depth + 1);
  Ok(())
}

#[test]
fn arbitrary_short_inputs_should_return_errors_or_consistent_source_maps_without_panicking() -> TestResult {
  // Deterministic mutation corpus, without a random-number or fuzz dependency.
  // It checks robustness, not chemical equivalence or full SMILES conformance.
  let alphabet = ["C", "N", "1", "=", ".", "(", ")", "[", "]", "%", "é"];
  for a in alphabet {
    for b in alphabet {
      for c in alphabet {
        let source = format!("{a}{b}{c}");
        match read_smiles(&source) {
          Ok(parsed) => {
            assert_eq!(parsed.graph().atoms().len(), parsed.graph().atom_count());
            for (id, _) in parsed.graph().atoms() {
              let span = parsed.atom_span(id).ok_or("missing atom span")?;
              assert!(source.get(span).is_some());
              for (neighbor, bond) in parsed.graph().neighbors(id)? {
                assert!(parsed.graph().atom(neighbor).is_some());
                assert!(parsed.graph().bond(bond).is_some());
              }
            }
          }
          Err(failure) => {
            assert!(source.get(failure.span).is_some());
          }
        }
      }
    }
  }
  Ok(())
}

#[test]
fn bare_organic_atoms_should_carry_explicit_reader_defaults() -> TestResult {
  let parsed = read_smiles("BCNOFPSClBrI")?;
  for (_, atom) in parsed.graph().atoms() {
    assert_eq!(atom.formal_charge(), Some(0));
    assert_eq!(atom.declared_hydrogens(), Some(0));
    assert_eq!(atom.unpaired_electrons(), Some(0));
    assert_eq!(atom.implicit_hydrogen_policy(), ImplicitHydrogenPolicy::Infer);
    assert_eq!(Atom::implicit_hydrogens(atom), None);
  }
  Ok(())
}
