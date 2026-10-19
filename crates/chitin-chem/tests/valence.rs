//! Chemical outcomes are local diagnostics; neither inference nor failure edits facts.

use chitin_chem::perception::{ValenceDiagnostic, ValenceModel, ValenceReport, ValenceStatus, analyze_valence};
use chitin_chem::{Atom, AtomId, BondOrder, Element, ImplicitHydrogenPolicy, Isotope, MolecularGraph};

type TestResult = Result<(), Box<dyn std::error::Error>>;
/// Explicit rule set shared by these fixtures, not an application-wide default.
const MODEL: ValenceModel = ValenceModel::MainGroupClosedShellV1;

/// Creates explicitly known neutral closed-shell facts with H inference enabled.
fn known_atom(element: Element) -> Atom {
  // These are explicit test inputs, not defaults of the domain model. Readers
  // and drawing tools will likewise need to establish their own input policies.
  Atom::new(element)
    .with_formal_charge(Some(0))
    .with_unpaired_electrons(Some(0))
    .with_declared_hydrogens(Some(0))
    .with_implicit_hydrogen_policy(ImplicitHydrogenPolicy::Infer)
}

/// Resolves a fixture's graph-local report row, failing rather than skipping assertions.
fn row(
  report: &ValenceReport,
  atom: AtomId,
) -> Result<&chitin_chem::perception::AtomValenceAnalysis, Box<dyn std::error::Error>> {
  report.atom(atom).ok_or_else(|| "missing analysis row".into())
}

#[test]
fn empty_graph_should_produce_an_empty_named_report() -> TestResult {
  let report = analyze_valence(&MolecularGraph::new(), MODEL)?;
  assert_eq!(report.model(), MODEL);
  assert_eq!(report.atoms().len(), 0);
  Ok(())
}

#[test]
fn supported_neutral_hosts_should_follow_the_explicit_v1_target_table() -> TestResult {
  for (element, hydrogens) in [
    (Element::Boron, 3),
    (Element::Carbon, 4),
    (Element::Nitrogen, 3),
    (Element::Oxygen, 2),
    (Element::Fluorine, 1),
    (Element::Silicon, 4),
    (Element::Chlorine, 1),
    (Element::Bromine, 1),
    (Element::Iodine, 1),
  ] {
    let mut graph = MolecularGraph::new();
    let id = graph.add_atom(known_atom(element))?;
    let before = graph.clone();
    let report = analyze_valence(&graph, MODEL)?;
    let result = row(&report, id)?;
    assert_eq!(result.status(), ValenceStatus::Determined, "{element}");
    assert_eq!(result.value().map(|v| v.hydrogens.implicit()), Some(hydrogens));
    assert_eq!(graph, before);
    // Inference must not silently turn a supplied policy into a fixed count.
    assert_eq!(graph.atom(id).and_then(Atom::implicit_hydrogens), None);
  }
  Ok(())
}

#[test]
fn ethanol_should_have_separate_carbon_and_oxygen_hydrogen_counts() -> TestResult {
  let mut graph = MolecularGraph::new();
  let a = graph.add_atom(known_atom(Element::Carbon))?;
  let b = graph.add_atom(known_atom(Element::Carbon))?;
  let o = graph.add_atom(known_atom(Element::Oxygen))?;
  graph.add_bond(a, b, BondOrder::Single)?;
  graph.add_bond(b, o, BondOrder::Single)?;
  let report = analyze_valence(&graph, MODEL)?;
  for (id, expected) in [(a, 3), (b, 2), (o, 1)] {
    assert_eq!(row(&report, id)?.value().map(|v| v.hydrogens.total()), Some(expected));
  }
  Ok(())
}

#[test]
fn double_and_triple_bonds_should_contribute_their_full_order() -> TestResult {
  for (element, order, expected) in [
    (Element::Oxygen, BondOrder::Double, 2),
    (Element::Nitrogen, BondOrder::Triple, 1),
  ] {
    let mut graph = MolecularGraph::new();
    let carbon = graph.add_atom(known_atom(Element::Carbon))?;
    let other = graph.add_atom(known_atom(element))?;
    graph.add_bond(carbon, other, order)?;
    let report = analyze_valence(&graph, MODEL)?;
    assert_eq!(
      row(&report, carbon)?.value().map(|v| v.hydrogens.implicit()),
      Some(expected)
    );
    assert_eq!(row(&report, other)?.value().map(|v| v.hydrogens.implicit()), Some(0));
  }
  Ok(())
}

#[test]
fn ammonium_declaration_should_not_require_or_create_four_vertices() -> TestResult {
  let mut graph = MolecularGraph::new();
  let n = graph.add_atom(
    known_atom(Element::Nitrogen)
      .with_formal_charge(Some(1))
      .with_declared_hydrogens(Some(4))
      .with_implicit_hydrogens(Some(0)),
  )?;
  let before = graph.clone();
  let report = analyze_valence(&graph, MODEL)?;
  let value = row(&report, n)?.value().ok_or("ammonium did not resolve")?;
  assert_eq!((value.explicit_valence, value.total_valence), (4, 4));
  assert_eq!(
    (
      value.hydrogens.neighbors(),
      value.hydrogens.declared(),
      value.hydrogens.implicit(),
      value.hydrogens.total()
    ),
    (0, 4, 0, 4)
  );
  assert_eq!(graph, before);
  Ok(())
}

#[test]
fn nitrogen_with_four_hydrogen_vertices_should_not_add_more_hydrogens() -> TestResult {
  let mut graph = MolecularGraph::new();
  let n = graph.add_atom(known_atom(Element::Nitrogen).with_formal_charge(Some(1)))?;
  for _ in 0..4 {
    let h = graph.add_atom(known_atom(Element::Hydrogen))?;
    graph.add_bond(n, h, BondOrder::Single)?;
  }
  let report = analyze_valence(&graph, MODEL)?;
  let value = row(&report, n)?.value().ok_or("ammonium did not resolve")?;
  assert_eq!(
    (
      value.hydrogens.neighbors(),
      value.hydrogens.declared(),
      value.hydrogens.implicit(),
      value.hydrogens.total()
    ),
    (4, 0, 0, 4)
  );
  assert!(report.atoms().all(|(_, row)| row.status() == ValenceStatus::Determined));
  Ok(())
}

#[test]
fn mixed_hydrogen_sources_should_each_be_counted_once() -> TestResult {
  let mut graph = MolecularGraph::new();
  let c = graph.add_atom(known_atom(Element::Carbon).with_declared_hydrogens(Some(1)))?;
  let h = graph.add_atom(known_atom(Element::Hydrogen))?;
  graph.add_bond(c, h, BondOrder::Single)?;
  let report = analyze_valence(&graph, MODEL)?;
  let value = row(&report, c)?.value().ok_or("carbon did not resolve")?;
  assert_eq!((value.explicit_valence, value.total_valence), (2, 4));
  assert_eq!(
    (
      value.hydrogens.neighbors(),
      value.hydrogens.declared(),
      value.hydrogens.implicit(),
      value.hydrogens.total()
    ),
    (1, 1, 2, 4)
  );
  Ok(())
}

#[test]
fn isotope_labelled_hydrogen_vertices_should_count_as_neighbor_hydrogens() -> TestResult {
  let mut graph = MolecularGraph::new();
  let o = graph.add_atom(known_atom(Element::Oxygen))?;
  for mass in [2, 3] {
    let h = graph.add_atom(
      Atom::from_isotope(Isotope::new(Element::Hydrogen, mass)?)
        .with_formal_charge(Some(0))
        .with_unpaired_electrons(Some(0))
        .with_declared_hydrogens(Some(0))
        .with_implicit_hydrogens(Some(0)),
    )?;
    graph.add_bond(o, h, BondOrder::Single)?;
  }
  let report = analyze_valence(&graph, MODEL)?;
  assert_eq!(row(&report, o)?.value().map(|v| v.hydrogens.neighbors()), Some(2));
  assert!(report.atoms().all(|(_, row)| row.status() == ValenceStatus::Determined));
  Ok(())
}

#[test]
fn negative_oxygen_should_use_its_supported_charge_specific_target() -> TestResult {
  let mut graph = MolecularGraph::new();
  let c = graph.add_atom(known_atom(Element::Carbon))?;
  let o = graph.add_atom(known_atom(Element::Oxygen).with_formal_charge(Some(-1)))?;
  graph.add_bond(c, o, BondOrder::Single)?;
  let report = analyze_valence(&graph, MODEL)?;
  assert_eq!(
    row(&report, o)?
      .value()
      .map(|v| (v.total_valence, v.hydrogens.implicit())),
    Some((1, 0))
  );
  Ok(())
}

#[test]
fn unspecified_atomic_facts_should_be_reported_instead_of_defaulted() -> TestResult {
  let mut graph = MolecularGraph::new();
  let c = graph.add_atom(Atom::new(Element::Carbon))?;
  let before = graph.clone();
  let report = analyze_valence(&graph, MODEL)?;
  let result = row(&report, c)?;
  assert_eq!(result.status(), ValenceStatus::Indeterminate);
  assert_eq!(
    result.diagnostics(),
    [
      ValenceDiagnostic::UnknownFormalCharge,
      ValenceDiagnostic::UnknownRadicalState,
      ValenceDiagnostic::UnknownDeclaredHydrogens,
      ValenceDiagnostic::UnknownHydrogenPolicy
    ]
  );
  assert!(result.value().is_none());
  assert_eq!(graph, before);
  Ok(())
}

#[test]
fn each_missing_fact_should_block_inference_independently() -> TestResult {
  for (atom, reason) in [
    (
      known_atom(Element::Carbon).with_formal_charge(None),
      ValenceDiagnostic::UnknownFormalCharge,
    ),
    (
      known_atom(Element::Carbon).with_unpaired_electrons(None),
      ValenceDiagnostic::UnknownRadicalState,
    ),
    (
      known_atom(Element::Carbon).with_declared_hydrogens(None),
      ValenceDiagnostic::UnknownDeclaredHydrogens,
    ),
    (
      known_atom(Element::Carbon).with_implicit_hydrogens(None),
      ValenceDiagnostic::UnknownHydrogenPolicy,
    ),
  ] {
    let mut graph = MolecularGraph::new();
    let id = graph.add_atom(atom)?;
    let report = analyze_valence(&graph, MODEL)?;
    assert_eq!(row(&report, id)?.status(), ValenceStatus::Indeterminate);
    assert_eq!(row(&report, id)?.diagnostics(), [reason]);
  }
  Ok(())
}

#[test]
fn unknown_order_should_not_be_assumed_to_be_a_single_bond() -> TestResult {
  let mut graph = MolecularGraph::new();
  let a = graph.add_atom(known_atom(Element::Carbon))?;
  let b = graph.add_atom(known_atom(Element::Oxygen))?;
  let bond = graph.add_bond(a, b, BondOrder::Unknown)?;
  let report = analyze_valence(&graph, MODEL)?;
  for id in [a, b] {
    assert_eq!(row(&report, id)?.status(), ValenceStatus::Indeterminate);
    assert_eq!(
      row(&report, id)?.diagnostics(),
      [ValenceDiagnostic::UnknownBondOrder(bond)]
    );
  }
  Ok(())
}

#[test]
fn aromaticity_metals_radicals_and_unsupported_charges_should_not_use_fallback_rules() -> TestResult {
  for (atom, reason) in [
    (
      known_atom(Element::Iron),
      ValenceDiagnostic::UnsupportedElement(Element::Iron),
    ),
    (
      known_atom(Element::Phosphorus),
      ValenceDiagnostic::UnsupportedElement(Element::Phosphorus),
    ),
    (
      known_atom(Element::Sulfur),
      ValenceDiagnostic::UnsupportedElement(Element::Sulfur),
    ),
    (
      known_atom(Element::Carbon).with_formal_charge(Some(1)),
      ValenceDiagnostic::UnsupportedCharge {
        element: Element::Carbon,
        charge: 1,
      },
    ),
    (
      known_atom(Element::Carbon).with_unpaired_electrons(Some(1)),
      ValenceDiagnostic::UnsupportedRadical(1),
    ),
  ] {
    let mut graph = MolecularGraph::new();
    let id = graph.add_atom(atom)?;
    let report = analyze_valence(&graph, MODEL)?;
    assert_eq!(row(&report, id)?.status(), ValenceStatus::Unsupported);
    assert_eq!(row(&report, id)?.diagnostics(), [reason]);
    assert!(row(&report, id)?.value().is_none());
  }
  let mut graph = MolecularGraph::new();
  let a = graph.add_atom(known_atom(Element::Carbon))?;
  let b = graph.add_atom(known_atom(Element::Carbon))?;
  let edge = graph.add_bond(a, b, BondOrder::Aromatic)?;
  let report = analyze_valence(&graph, MODEL)?;
  assert_eq!(row(&report, a)?.diagnostics(), [ValenceDiagnostic::AromaticBond(edge)]);
  assert_eq!(row(&report, a)?.status(), ValenceStatus::Unsupported);
  Ok(())
}

#[test]
fn unsupported_status_should_preserve_simultaneous_missing_fact_diagnostics() -> TestResult {
  let mut graph = MolecularGraph::new();
  let id = graph.add_atom(Atom::new(Element::Iron))?;
  let report = analyze_valence(&graph, MODEL)?;
  assert_eq!(row(&report, id)?.status(), ValenceStatus::Unsupported);
  assert!(
    row(&report, id)?
      .diagnostics()
      .contains(&ValenceDiagnostic::UnknownFormalCharge)
  );
  assert!(
    row(&report, id)?
      .diagnostics()
      .contains(&ValenceDiagnostic::UnsupportedElement(Element::Iron))
  );
  Ok(())
}

#[test]
fn excess_valence_should_be_a_local_diagnostic_and_not_prevent_further_editing() -> TestResult {
  let mut graph = MolecularGraph::new();
  let c = graph.add_atom(known_atom(Element::Carbon))?;
  let mut last = None;
  for _ in 0..5 {
    let h = graph.add_atom(known_atom(Element::Hydrogen))?;
    last = Some(graph.add_bond(c, h, BondOrder::Single)?);
  }
  let before = graph.clone();
  let report = analyze_valence(&graph, MODEL)?;
  assert_eq!(row(&report, c)?.status(), ValenceStatus::Invalid);
  assert_eq!(
    row(&report, c)?.diagnostics(),
    [ValenceDiagnostic::ExcessValence { observed: 5, target: 4 }]
  );
  assert_eq!(graph, before);
  graph.remove_bond(last.ok_or("missing test bond")?)?;
  assert_eq!(
    row(&analyze_valence(&graph, MODEL)?, c)?.status(),
    ValenceStatus::Determined
  );
  Ok(())
}

#[test]
fn fixed_hydrogen_counts_should_not_be_overwritten_to_fix_a_conflict() -> TestResult {
  for (fixed, diagnostic) in [
    (0, ValenceDiagnostic::UnsatisfiedValence { observed: 0, target: 4 }),
    (5, ValenceDiagnostic::ExcessValence { observed: 5, target: 4 }),
  ] {
    let mut graph = MolecularGraph::new();
    let id = graph.add_atom(known_atom(Element::Carbon).with_implicit_hydrogens(Some(fixed)))?;
    let report = analyze_valence(&graph, MODEL)?;
    assert_eq!(row(&report, id)?.status(), ValenceStatus::Invalid);
    assert_eq!(row(&report, id)?.diagnostics(), [diagnostic]);
    assert_eq!(graph.atom(id).and_then(Atom::implicit_hydrogens), Some(fixed));
  }
  Ok(())
}

#[test]
fn hydrogen_vertices_should_not_silently_become_hosts_for_hidden_hydrogens() -> TestResult {
  let mut graph = MolecularGraph::new();
  let h = graph.add_atom(known_atom(Element::Hydrogen))?;
  let report = analyze_valence(&graph, MODEL)?;
  assert_eq!(row(&report, h)?.status(), ValenceStatus::Unsupported);
  assert_eq!(row(&report, h)?.diagnostics(), [ValenceDiagnostic::HydrogenAsHost]);
  Ok(())
}

#[test]
fn disconnected_bad_fragments_should_not_hide_determined_atoms() -> TestResult {
  let mut graph = MolecularGraph::new();
  let carbon = graph.add_atom(known_atom(Element::Carbon))?;
  let unknown = graph.add_atom(Atom::new(Element::Oxygen))?;
  let metal = graph.add_atom(known_atom(Element::Iron))?;
  let report = analyze_valence(&graph, MODEL)?;
  assert_eq!(report.atoms().len(), 3);
  assert_eq!(row(&report, carbon)?.status(), ValenceStatus::Determined);
  assert_eq!(row(&report, unknown)?.status(), ValenceStatus::Indeterminate);
  assert_eq!(row(&report, metal)?.status(), ValenceStatus::Unsupported);
  Ok(())
}

#[test]
fn recomputation_should_observe_edits_without_changing_previous_snapshots() -> TestResult {
  let mut graph = MolecularGraph::new();
  let c = graph.add_atom(known_atom(Element::Carbon))?;
  let deleted = graph.add_atom(known_atom(Element::Oxygen))?;
  graph.remove_atom(deleted)?;
  let first = analyze_valence(&graph, MODEL)?;
  graph.replace_atom(c, known_atom(Element::Nitrogen))?;
  let second = analyze_valence(&graph, MODEL)?;
  assert_eq!(row(&first, c)?.value().map(|v| v.hydrogens.implicit()), Some(4));
  assert_eq!(row(&second, c)?.value().map(|v| v.hydrogens.implicit()), Some(3));
  assert!(second.atom(deleted).is_none());
  assert_eq!(second.atoms().map(|(id, _)| id).collect::<Vec<_>>(), [c]);
  Ok(())
}

#[test]
fn legacy_fixed_count_api_and_policy_api_should_not_store_conflicting_states() {
  let atom = known_atom(Element::Carbon).with_implicit_hydrogens(Some(3));
  assert_eq!(atom.implicit_hydrogen_policy(), ImplicitHydrogenPolicy::Fixed(3));
  let infer = atom.with_implicit_hydrogen_policy(ImplicitHydrogenPolicy::Infer);
  assert_eq!(infer.implicit_hydrogens(), None);
  assert_eq!(
    infer.with_implicit_hydrogens(None).implicit_hydrogen_policy(),
    ImplicitHydrogenPolicy::Unspecified
  );
}
