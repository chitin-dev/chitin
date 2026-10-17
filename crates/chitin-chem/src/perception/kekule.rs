//! Bounded integer-bond assignment for supplied aromatic designations.
//!
//! This is not aromaticity perception or a general sanitization model. Input
//! flags remain authoritative; a detached representation serves integer-bond
//! consumers without installing inferred properties in the source graph.

use std::collections::{BTreeMap, BTreeSet};

use crate::{Atom, AtomId, BondId, BondOrder, Element, GraphError, ImplicitHydrogenPolicy, MolecularGraph};

use super::analyze_rings;

/// Resource limit for the restricted aromatic C/N/O/S assignment model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KekuleOptions {
  /// Shared work allowance for input traversal and iterative matching search.
  pub work_budget: usize,
}

impl Default for KekuleOptions {
  fn default() -> Self {
    Self { work_budget: 1_000_000 }
  }
}

/// Failure to construct a complete supported integer-bond representation.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum KekuleError {
  /// The work allowance must be nonzero.
  #[error("invalid Kekule options")]
  Options,
  /// The bounded search stopped without publishing any partial assignment.
  #[error("Kekule work budget exhausted")]
  BudgetExceeded,
  /// The atom's element, charge, H policy, radical state, or valence is unsupported.
  #[error("unsupported aromatic atomic state at {0}")]
  Atom(AtomId),
  /// An aromatic designation lies outside a cycle or has nonaromatic endpoints.
  #[error("invalid aromatic bond designation at {0}")]
  Bond(BondId),
  /// No assignment meets all participating atoms' double-bond requirements.
  #[error("no Kekule assignment at aromatic atom {0}")]
  NoAssignment(AtomId),
  /// An internal graph reference is inconsistent.
  #[error(transparent)]
  Topology(#[from] GraphError),
}

/// Shared allowance, including copied search states rather than only backtracks.
struct Work(usize);

impl Work {
  /// Charges work before allocation/traversal; exhaustion is never truncation.
  fn spend(&mut self, amount: usize) -> Result<(), KekuleError> {
    self.0 = self.0.checked_sub(amount).ok_or(KekuleError::BudgetExceeded)?;
    Ok(())
  }
}

/// Builds a detached C/N/O/S Kekulé representation without changing input facts.
///
/// # Parameters
/// - `graph`: supplied aromatic atom flags and aromatic/single/double bonds.
/// - `options`: explicit work limit shared across all disconnected fragments.
///
/// # Returns
/// A graph with the same handles, connectivity, isotope/charge/declaration and
/// radical knowledge. Aromatic flags are cleared and aromatic edges become
/// single/double. Aromatic heteroatoms receive fixed zero additional implicit H;
/// bare carbon retains its input inference policy. No uppercase ring is made
/// aromatic and no Hückel/electronic stability claim is made.
///
/// Neutral C/N/O/S and common bracket N(+1/-1) states are covered. Other aromatic
/// elements and charges remain explicit unsupported cases. Bracket H remains
/// declared H, never graph vertices or inferred radical knowledge.
///
/// # Errors
/// Returns unsupported atom/bond states, an unsatisfiable matching, or exhausted
/// work. No partially assigned graph escapes. Search is deterministic, iterative,
/// and potentially exponential; the budget bounds that worst case.
pub fn kekulize(graph: &MolecularGraph, options: &KekuleOptions) -> Result<MolecularGraph, KekuleError> {
  if options.work_budget == 0 {
    return Err(KekuleError::Options);
  }
  let mut work = Work(options.work_budget);
  work.spend(graph.atom_count().saturating_add(graph.bond_count()))?;
  let topology = analyze_rings(graph)?;
  let bridges: BTreeSet<_> = topology.bridges().iter().copied().collect();
  let mut ids = Vec::new();
  let mut indices = BTreeMap::new();
  let mut needed = Vec::new();
  for (id, atom) in graph.atoms().filter(|(_, atom)| atom.is_aromatic()) {
    indices.insert(id, ids.len());
    ids.push(id);
    needed.push(requirement(graph, id, atom)?);
  }
  let mut adjacency = vec![Vec::new(); ids.len()];
  let mut edges = Vec::new();
  for (id, bond) in graph.bonds().filter(|(_, bond)| bond.order() == BondOrder::Aromatic) {
    let [a, b] = bond.atoms();
    let (Some(a), Some(b)) = (indices.get(&a).copied(), indices.get(&b).copied()) else {
      return Err(KekuleError::Bond(id));
    };
    if bridges.contains(&id) {
      return Err(KekuleError::Bond(id));
    }
    adjacency[a].push((b, id));
    adjacency[b].push((a, id));
    edges.push(id);
  }
  // Even explicitly alternating lowercase input must designate cyclic atoms.
  let cyclic: BTreeSet<_> = topology
    .blocks()
    .iter()
    .filter(|block| block.cycle_rank() > 0)
    .flat_map(|block| block.atoms().iter().copied())
    .collect();
  for &id in &ids {
    if !cyclic.contains(&id) {
      return Err(KekuleError::Atom(id));
    }
  }
  let selected = match_edges(&ids, &adjacency, needed, &mut work)?;
  work.spend(graph.atom_count().saturating_add(graph.bond_count()))?;
  let mut result = graph.clone();
  for id in ids {
    let atom = graph.atom(id).ok_or(GraphError::MissingAtom(id))?;
    let mut resolved = atom.clone().with_aromatic(false);
    if atom.element() != Element::Carbon && atom.implicit_hydrogen_policy() == ImplicitHydrogenPolicy::Infer {
      resolved = resolved.with_implicit_hydrogens(Some(0));
    }
    result.replace_atom(id, resolved)?;
  }
  let selected: BTreeSet<_> = selected.into_iter().collect();
  for id in edges {
    result.set_bond_order(
      id,
      if selected.contains(&id) {
        BondOrder::Double
      } else {
        BondOrder::Single
      },
    )?;
  }
  Ok(result)
}

/// Determines whether one additional internal double bond is required.
///
/// Aromatic bonds contribute one sigma bond here, never a fictitious order 1.5.
/// Explicit multiple bonds consume the atom's pi slot before matching. Missing
/// facts, unknown bonds, radicals, and unlisted charges are not guessed.
fn requirement(graph: &MolecularGraph, id: AtomId, atom: &Atom) -> Result<bool, KekuleError> {
  let fail = || KekuleError::Atom(id);
  let charge = atom.formal_charge().ok_or_else(fail)?;
  let declared = atom.declared_hydrogens().ok_or_else(fail)?;
  let fixed = match atom.implicit_hydrogen_policy() {
    ImplicitHydrogenPolicy::Fixed(count) => count,
    ImplicitHydrogenPolicy::Infer => 0,
    ImplicitHydrogenPolicy::Unspecified => return Err(fail()),
  };
  if atom.unpaired_electrons().is_some_and(|count| count != 0) {
    return Err(fail());
  }
  let neighbors = graph.neighbors(id)?;
  if !(2..=3).contains(&neighbors.len()) {
    return Err(fail());
  }
  if neighbors.len() + usize::from(declared) + usize::from(fixed) > 3 {
    return Err(fail());
  }
  let mut sum = u32::from(declared) + u32::from(fixed);
  let mut multiple = 0;
  for (_, edge) in neighbors {
    let order = graph.bond(edge).ok_or(GraphError::MissingBond(edge))?.order();
    sum += match order {
      BondOrder::Single | BondOrder::Aromatic => 1,
      BondOrder::Double => {
        multiple += 1;
        2
      }
      _ => return Err(fail()),
    };
  }
  let target = match (atom.element(), charge) {
    (Element::Carbon, 0) => 4,
    (Element::Nitrogen, 0) => 3,
    (Element::Nitrogen, 1) => 4,
    (Element::Nitrogen, -1) | (Element::Oxygen | Element::Sulfur, 0) => 2,
    _ => return Err(fail()),
  };
  if sum > target
    || multiple > 1
    || (multiple != 0 && sum != target && !(atom.element() == Element::Carbon && sum + 1 == target))
    || (atom.element() == Element::Carbon && multiple == 0 && sum >= target)
  {
    return Err(fail());
  }
  let needs = sum < target && multiple == 0;
  // Only carbon may fill the final remaining valence with inferred H. Bare
  // aromatic heteroatoms must not acquire an accidental H from a valence deficit.
  if atom.element() != Element::Carbon && target - sum > 1 {
    return Err(fail());
  }
  Ok(needs)
}

/// One suspended iterative matching branch; source handles remain outside it.
struct State {
  /// Unmatched atoms each require exactly one remaining double bond.
  needed: Vec<bool>,
  /// Fully selected edges on this branch, not an exposed partial assignment.
  selected: Vec<BondId>,
}

/// Finds an exact matching of required pi slots with bounded iterative search.
///
/// A smallest-remaining-domain vertex is expanded first. Nonrequired atoms can
/// never receive a double bond. Every copied state is charged before allocation.
fn match_edges(
  ids: &[AtomId],
  adjacency: &[Vec<(usize, BondId)>],
  needed: Vec<bool>,
  work: &mut Work,
) -> Result<Vec<BondId>, KekuleError> {
  let mut stack = vec![State {
    needed,
    selected: Vec::new(),
  }];
  while let Some(state) = stack.pop() {
    work.spend(ids.len().saturating_add(adjacency.iter().map(Vec::len).sum::<usize>()))?;
    let node = (0..ids.len()).filter(|&node| state.needed[node]).min_by_key(|&node| {
      adjacency[node]
        .iter()
        .filter(|&&(other, _)| state.needed[other])
        .count()
    });
    let Some(node) = node else {
      return Ok(state.selected);
    };
    for &(other, bond) in adjacency[node].iter().rev().filter(|&&(other, _)| state.needed[other]) {
      work.spend(ids.len().saturating_add(state.selected.len()).saturating_add(1))?;
      let mut next = State {
        needed: state.needed.clone(),
        selected: state.selected.clone(),
      };
      next.needed[node] = false;
      next.needed[other] = false;
      next.selected.push(bond);
      stack.push(next);
    }
  }
  // An empty requirement set succeeds in the loop, so failure has a source atom.
  Err(KekuleError::NoAssignment(ids[0]))
}
