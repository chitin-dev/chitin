//! Restricted, closed-shell valence analysis, not RDKit-compatible sanitization.
//!
//! The named V1 model covers nonaromatic neutral H/B/C/N/O/F/Si/Cl/Br/I,
//! ammonium-like N(+1), and O(-1). It assumes the specified atomic radical
//! count is zero and only succeeds when its single target valence is satisfied.
//! Aromaticity, other charges/elements, radicals, and hypervalent models require
//! distinct future rules, not a fallback default valence.

use std::collections::BTreeMap;

use crate::{
  Atom, AtomId, BondId, BondOrder, Element, GraphError, HydrogenCounts, ImplicitHydrogenPolicy, MolecularGraph,
};

/// Explicit rule set used to interpret atomic input facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValenceModel {
  /// Restricted, saturated closed-shell main-group rules documented in this module.
  MainGroupClosedShellV1,
}

/// Atom-local result category; none of these statuses prevents graph editing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValenceStatus {
  /// All required facts are available and the chosen model is satisfied.
  Determined,
  /// Required input facts are missing; no implicit chemical defaults were used.
  Indeterminate,
  /// At least one supplied fact lies outside this model's coverage.
  Unsupported,
  /// Complete, supported input contradicts the chosen model's valence rule.
  Invalid,
}

/// A structured reason for an unresolved atom, retaining offending bond handles.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ValenceDiagnostic {
  /// Formal charge is unspecified, not established as neutral.
  UnknownFormalCharge,
  /// The unpaired-electron count is unspecified, not established as zero.
  UnknownRadicalState,
  /// Attached-H declaration is unspecified, not established as zero.
  UnknownDeclaredHydrogens,
  /// No fixed count or inference permission was supplied.
  UnknownHydrogenPolicy,
  /// Connectivity exists, but its bond-order contribution is unknown.
  UnknownBondOrder(BondId),
  /// The model defines no valence rules for this element.
  UnsupportedElement(Element),
  /// The element is covered but this charge state is not.
  UnsupportedCharge {
    /// Element carrying the unsupported charge.
    element: Element,
    /// Supplied formal charge.
    charge: i16,
  },
  /// Open-shell atoms require a separate rule set.
  UnsupportedRadical(u8),
  /// Aromatic labels require aromaticity-aware rules, not a numeric order of 1.5.
  AromaticBond(BondId),
  /// V1 does not attach declared or additional implicit H to an H vertex.
  HydrogenAsHost,
  /// The total valence exceeds the model's one supported target.
  ExcessValence {
    /// Known bond contributions, declared H, and any fixed implicit H.
    observed: u32,
    /// Maximum valence admitted by this rule set.
    target: u8,
  },
  /// A fixed H count leaves the closed-shell target unsatisfied.
  UnsatisfiedValence {
    /// Known valence including all fixed H.
    observed: u32,
    /// Required valence in this rule set.
    target: u8,
  },
  /// The graph's counts exceed this analysis representation.
  CountOverflow,
}

impl ValenceDiagnostic {
  fn status(&self) -> ValenceStatus {
    match self {
      Self::UnknownFormalCharge
      | Self::UnknownRadicalState
      | Self::UnknownDeclaredHydrogens
      | Self::UnknownHydrogenPolicy
      | Self::UnknownBondOrder(_) => ValenceStatus::Indeterminate,
      Self::ExcessValence { .. } | Self::UnsatisfiedValence { .. } => ValenceStatus::Invalid,
      _ => ValenceStatus::Unsupported,
    }
  }
}

/// Successfully determined local valence, not a whole-molecule validity claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AtomValence {
  /// Sum of incident bond orders and declared attached H; excludes implicit H.
  pub explicit_valence: u32,
  /// Explicit valence plus fixed or inferred implicit H.
  pub total_valence: u32,
  /// Separate neighbor, declared, and implicit hydrogen contributions.
  pub hydrogens: HydrogenCounts,
}

/// One immutable analysis row keyed to an input graph's atom handle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtomValenceAnalysis {
  status: ValenceStatus,
  value: Option<AtomValence>,
  diagnostics: Vec<ValenceDiagnostic>,
}

impl AtomValenceAnalysis {
  /// Returns the atom-local outcome category.
  pub const fn status(&self) -> ValenceStatus {
    self.status
  }
  /// Returns values only when analysis determined a supported, valid result.
  pub const fn value(&self) -> Option<&AtomValence> {
    self.value.as_ref()
  }
  /// Returns every detected blocker, not only the highest-priority category.
  pub fn diagnostics(&self) -> &[ValenceDiagnostic] {
    &self.diagnostics
  }

  fn unresolved(diagnostics: Vec<ValenceDiagnostic>) -> Self {
    // Coverage failures take precedence over missing facts, but every reason is
    // retained. Invalid is used only after all required input is established.
    let status = if diagnostics
      .iter()
      .any(|reason| reason.status() == ValenceStatus::Invalid)
    {
      ValenceStatus::Invalid
    } else if diagnostics
      .iter()
      .any(|reason| reason.status() == ValenceStatus::Unsupported)
    {
      ValenceStatus::Unsupported
    } else {
      ValenceStatus::Indeterminate
    };
    Self {
      status,
      value: None,
      diagnostics,
    }
  }
}

/// Detached analysis snapshot; recompute after edits rather than reusing as a cache.
///
/// The report has no authority over the live graph. A determined result means
/// only that the local V1 valence rule is satisfied, not molecular stability,
/// aromaticity, stereochemistry, or suitability for a quantum calculation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValenceReport {
  model: ValenceModel,
  atoms: BTreeMap<AtomId, AtomValenceAnalysis>,
}

impl ValenceReport {
  /// Returns the rule set used for this snapshot.
  pub const fn model(&self) -> ValenceModel {
    self.model
  }
  /// Looks up a result by its input graph's stable atom ID.
  pub fn atom(&self, id: AtomId) -> Option<&AtomValenceAnalysis> {
    self.atoms.get(&id)
  }
  /// Visits results in stable atom-ID order, including unresolved atoms.
  pub fn atoms(&self) -> impl ExactSizeIterator<Item = (AtomId, &AtomValenceAnalysis)> + '_ {
    self.atoms.iter().map(|(&id, row)| (id, row))
  }
}

/// Analyzes every atom without modifying facts, hydrogens, topology, or IDs.
///
/// # Parameters
/// - `graph`: possibly disconnected or chemically incomplete input.
/// - `model`: explicit chemistry rule set; no ambient default is selected.
///
/// # Returns
/// One row per live atom. Missing facts, unsupported chemistry, and invalid
/// valence are report outcomes, not top-level errors. Unknown facts never become
/// zeros. Inference requires declared H, charge, radical count, and bond orders.
///
/// # Errors
/// Returns [`GraphError`] for inconsistent internal topology references. The
/// public graph mutation API prevents those states; chemical failures remain
/// local diagnostics so one problematic fragment does not hide other results.
///
/// # Examples
///
/// ```
/// use chitin_chem::{Atom, Element, ImplicitHydrogenPolicy, MolecularGraph};
/// use chitin_chem::perception::{ValenceModel, analyze_valence};
/// let mut graph = MolecularGraph::new();
/// let id = graph.add_atom(Atom::new(Element::Carbon)
///   .with_formal_charge(Some(0))
///   .with_unpaired_electrons(Some(0))
///   .with_declared_hydrogens(Some(0))
///   .with_implicit_hydrogen_policy(ImplicitHydrogenPolicy::Infer))?;
/// let report = analyze_valence(&graph, ValenceModel::MainGroupClosedShellV1)?;
/// let value = report.atom(id).and_then(|row| row.value());
/// assert_eq!(value.map(|v| v.hydrogens.implicit()), Some(4));
/// assert_eq!(graph.atom(id).and_then(Atom::implicit_hydrogens), None);
/// # Ok::<(), chitin_chem::GraphError>(())
/// ```
pub fn analyze_valence(graph: &MolecularGraph, model: ValenceModel) -> Result<ValenceReport, GraphError> {
  let atoms = graph
    .atoms()
    .map(|(id, atom)| analyze_atom(graph, id, atom, model).map(|row| (id, row)))
    .collect::<Result<_, _>>()?;
  Ok(ValenceReport { model, atoms })
}

fn analyze_atom(
  graph: &MolecularGraph,
  id: AtomId,
  atom: &Atom,
  model: ValenceModel,
) -> Result<AtomValenceAnalysis, GraphError> {
  let mut diagnostics = Vec::new();
  if atom.formal_charge().is_none() {
    diagnostics.push(ValenceDiagnostic::UnknownFormalCharge);
  }
  match atom.unpaired_electrons() {
    None => diagnostics.push(ValenceDiagnostic::UnknownRadicalState),
    Some(0) => {}
    Some(count) => diagnostics.push(ValenceDiagnostic::UnsupportedRadical(count)),
  }
  if atom.declared_hydrogens().is_none() {
    diagnostics.push(ValenceDiagnostic::UnknownDeclaredHydrogens);
  }
  if atom.implicit_hydrogen_policy() == ImplicitHydrogenPolicy::Unspecified {
    diagnostics.push(ValenceDiagnostic::UnknownHydrogenPolicy);
  }
  let target = match target_valence(atom, model) {
    Ok(target) => Some(target),
    Err(reason) => {
      if !diagnostics.contains(&reason) {
        diagnostics.push(reason);
      }
      None
    }
  };
  let mut bond_sum = 0_u32;
  let mut neighbor_hydrogens = 0_u32;
  for (neighbor, bond_id) in graph.neighbors(id)? {
    let bond = graph.bond(bond_id).ok_or(GraphError::MissingBond(bond_id))?;
    let neighbor_atom = graph.atom(neighbor).ok_or(GraphError::MissingAtom(neighbor))?;
    if neighbor_atom.element() == Element::Hydrogen {
      match neighbor_hydrogens.checked_add(1) {
        Some(count) => neighbor_hydrogens = count,
        None => diagnostics.push(ValenceDiagnostic::CountOverflow),
      }
    }
    let order = match bond.order() {
      BondOrder::Unknown => {
        diagnostics.push(ValenceDiagnostic::UnknownBondOrder(bond_id));
        continue;
      }
      BondOrder::Aromatic => {
        diagnostics.push(ValenceDiagnostic::AromaticBond(bond_id));
        continue;
      }
      BondOrder::Single => 1,
      BondOrder::Double => 2,
      BondOrder::Triple => 3,
    };
    match bond_sum.checked_add(order) {
      Some(value) => bond_sum = value,
      None => diagnostics.push(ValenceDiagnostic::CountOverflow),
    }
  }
  // Missing options each installed their own diagnostic above. Arithmetic
  // starts only after all required facts are present, never through None -> 0.
  match (target, atom.declared_hydrogens()) {
    (Some(target), Some(declared)) if diagnostics.is_empty() => {
      Ok(complete_atom(atom, bond_sum, neighbor_hydrogens, declared, target))
    }
    _ => Ok(AtomValenceAnalysis::unresolved(diagnostics)),
  }
}

fn target_valence(atom: &Atom, model: ValenceModel) -> Result<u8, ValenceDiagnostic> {
  match model {
    ValenceModel::MainGroupClosedShellV1 => main_group_target(atom),
  }
}

fn main_group_target(atom: &Atom) -> Result<u8, ValenceDiagnostic> {
  let neutral = match atom.element() {
    Element::Hydrogen | Element::Fluorine | Element::Chlorine | Element::Bromine | Element::Iodine => 1,
    Element::Boron | Element::Nitrogen => 3,
    Element::Carbon | Element::Silicon => 4,
    Element::Oxygen => 2,
    element => return Err(ValenceDiagnostic::UnsupportedElement(element)),
  };
  let charge = atom.formal_charge().ok_or(ValenceDiagnostic::UnknownFormalCharge)?;
  match (atom.element(), charge) {
    (_, 0) => Ok(neutral),
    (Element::Nitrogen, 1) => Ok(4),
    (Element::Oxygen, -1) => Ok(1),
    (element, charge) => Err(ValenceDiagnostic::UnsupportedCharge { element, charge }),
  }
}

fn complete_atom(atom: &Atom, bond_sum: u32, neighbors: u32, declared: u8, target: u8) -> AtomValenceAnalysis {
  let Some(explicit_valence) = bond_sum.checked_add(u32::from(declared)) else {
    return AtomValenceAnalysis::unresolved(vec![ValenceDiagnostic::CountOverflow]);
  };
  let implicit = match atom.implicit_hydrogen_policy() {
    ImplicitHydrogenPolicy::Fixed(count) => count,
    ImplicitHydrogenPolicy::Infer if explicit_valence <= u32::from(target) => {
      // V1 targets are at most four, so this subtraction is bounded by u8.
      (u32::from(target) - explicit_valence) as u8
    }
    ImplicitHydrogenPolicy::Infer => 0,
    ImplicitHydrogenPolicy::Unspecified => {
      return AtomValenceAnalysis::unresolved(vec![ValenceDiagnostic::UnknownHydrogenPolicy]);
    }
  };
  if atom.element() == Element::Hydrogen && (declared > 0 || implicit > 0) {
    return AtomValenceAnalysis::unresolved(vec![ValenceDiagnostic::HydrogenAsHost]);
  }
  let Some(total_valence) = explicit_valence.checked_add(u32::from(implicit)) else {
    return AtomValenceAnalysis::unresolved(vec![ValenceDiagnostic::CountOverflow]);
  };
  if total_valence != u32::from(target) {
    let issue = if total_valence > u32::from(target) {
      ValenceDiagnostic::ExcessValence {
        observed: total_valence,
        target,
      }
    } else {
      ValenceDiagnostic::UnsatisfiedValence {
        observed: total_valence,
        target,
      }
    };
    return AtomValenceAnalysis::unresolved(vec![issue]);
  }
  AtomValenceAnalysis {
    status: ValenceStatus::Determined,
    value: Some(AtomValence {
      explicit_valence,
      total_valence,
      hydrogens: HydrogenCounts {
        neighbors,
        declared,
        implicit,
      },
    }),
    diagnostics: Vec::new(),
  }
}
