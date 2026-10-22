//! Shared drawing conventions, distinct from immutable chemical facts.
//!
//! Equal lengths are preferred, not a chemical invariant. Complex ring blocks
//! and high-coordinate centers may need modest local distortion for clarity.
//! These conservative numeric bounds are engineering policy, not prescribed
//! standard values. They never authorize a change to connectivity or stereo.

use chitin_chem::{
  BondId, BondOrder, GraphError, MolecularGraph,
  perception::{RingBlock, RingTopology, analyze_rings},
};
use std::collections::BTreeSet;

/// Stage-entry length bounds; reference lengths never follow the latest winner.
#[derive(Clone, Copy)]
pub(crate) struct LengthLimit {
  /// Original length, including caller-authored or projected unequal geometry.
  reference: f64,
  /// Whether topology authorizes a local drawing exception.
  flexible: bool,
}

impl LengthLimit {
  /// Constructs a fixed reference with optional bounded drawing flexibility.
  pub(crate) fn new(reference: f64, flexible: bool) -> Self {
    Self { reference, flexible }
  }

  /// Returns the nearest admissible length without normalizing the reference.
  pub(crate) fn nearest(self, length: f64) -> f64 {
    if self.flexible {
      length.clamp(self.reference * 0.9, self.reference * 1.2)
    } else {
      self.reference
    }
  }

  /// Rejects collapsed/nonfinite bonds, including invalid stage-entry geometry.
  pub(crate) fn permits(self, length: f64) -> bool {
    self.reference.is_finite()
      && self.reference > 1e-10
      && length.is_finite()
      && length > 1e-10
      && (length - self.nearest(length)).abs() <= 1e-7 * self.reference.max(1.)
  }
}

/// Source-local exceptions reused by candidate construction and final admission.
pub(crate) struct LengthRules {
  /// Only eligible single bonds can depart from their captured reference.
  flexible: BTreeSet<BondId>,
}

impl LengthRules {
  /// Classifies drawing flexibility without modifying or inferring chemistry.
  ///
  /// # Parameters
  /// - `graph`: source namespace and supplied bond orders.
  ///
  /// # Returns
  /// Internal single bonds in complex/bridged blocks and single bonds at
  /// centers with more than four neighbors may vary. Terminal substituents do
  /// not inherit flexibility merely by belonging to a complex molecule.
  /// Aromatic and multiple bonds retain their reference geometry.
  ///
  /// # Errors
  /// Inconsistent source references propagate from topology perception.
  pub(crate) fn new(graph: &MolecularGraph) -> Result<Self, GraphError> {
    let topology = analyze_rings(graph)?;
    Self::from_topology(graph, &topology)
  }

  /// Reuses a topology snapshot when the admission stage already needs its angles.
  pub(crate) fn from_topology(graph: &MolecularGraph, topology: &RingTopology) -> Result<Self, GraphError> {
    let mut flexible = BTreeSet::new();
    for block in topology.blocks() {
      if exceptional_block(graph, block)? {
        flexible.extend(block.bonds().iter().copied());
      }
    }
    for (atom, _) in graph.atoms() {
      let neighbors = graph.neighbors(atom)?;
      if neighbors.len() > 4 {
        flexible.extend(neighbors.map(|(_, bond)| bond));
      }
    }
    flexible.retain(|&id| {
      graph.bond(id).is_some_and(|bond| {
        bond.order() == BondOrder::Single
          && bond
            .atoms()
            .iter()
            .all(|&id| graph.atom(id).is_some_and(|atom| !atom.is_aromatic()))
      })
    });
    Ok(Self { flexible })
  }

  /// Captures one immutable length reference using the source-local policy.
  pub(crate) fn limit(&self, bond: BondId, reference: f64) -> LengthLimit {
    LengthLimit::new(reference, self.flexible.contains(&bond))
  }
}

/// Distinguishes a true three-path bridge from an ordinary edge-fused bicycle.
///
/// In a rank-two block, directly adjacent degree-three junctions describe the
/// shared edge of two ordinary faces. Nonadjacent junctions require three paths
/// with internal vertices and qualify for a bridged drawing exception.
fn exceptional_block(graph: &MolecularGraph, block: &RingBlock) -> Result<bool, GraphError> {
  if block.cycle_rank() != 2 {
    return Ok(block.cycle_rank() >= 3);
  }
  let mut junctions = Vec::new();
  for &atom in block.atoms() {
    if graph
      .neighbors(atom)?
      .filter(|(_, bond)| block.bonds().contains(bond))
      .count()
      > 2
    {
      junctions.push(atom);
    }
  }
  let [a, b] = junctions.as_slice() else {
    return Ok(false);
  };
  Ok(!graph.neighbors(*a)?.any(|(other, _)| other == *b))
}

#[cfg(test)]
mod tests {
  use super::*;
  use chitin_chem::io::smiles::read_smiles;

  type TestResult = Result<(), Box<dyn std::error::Error>>;

  #[test]
  fn ordinary_and_aromatic_bonds_should_remain_fixed() -> TestResult {
    for source in ["CCCC", "C1CCC1", "C1CCC2CCCCC2C1", "c1ccc2cc3ccccc3cc2c1"] {
      let graph = read_smiles(source)?.into_graph();
      let rules = LengthRules::new(&graph)?;
      assert!(
        graph.bonds().all(|(id, _)| !rules.limit(id, 1.).permits(1.1)),
        "{source}"
      );
    }
    Ok(())
  }

  #[test]
  fn a_true_bicyclic_bridge_should_allow_internal_length_exceptions() -> TestResult {
    let graph = read_smiles("C1CC2CCC1C2")?.into_graph();
    let rules = LengthRules::new(&graph)?;
    assert!(graph.bonds().all(|(id, _)| rules.limit(id, 1.).permits(1.1)));
    Ok(())
  }

  #[test]
  fn complex_ring_exceptions_should_not_leak_into_substituents_or_multiple_bonds() -> TestResult {
    let graph = read_smiles("OC1=C2CC3CC1CC(C2)C3")?.into_graph();
    let rules = LengthRules::new(&graph)?;
    assert!(!rules.flexible.is_empty());
    assert!(
      graph
        .bonds()
        .filter(|(_, bond)| bond.order() != BondOrder::Single)
        .all(|(id, _)| !rules.limit(id, 1.).permits(1.1))
    );
    let terminal = graph.atoms().next().ok_or("missing atom")?.0;
    let (_, bond) = graph.neighbors(terminal)?.next().ok_or("missing bond")?;
    assert!(!rules.limit(bond, 1.).permits(1.1));
    Ok(())
  }

  #[test]
  fn bounds_should_be_finite_local_and_nonaccumulating() {
    let limit = LengthLimit::new(1.8, true);
    assert!(limit.permits(1.8 * 1.15));
    for length in [0., f64::NAN, f64::INFINITY, 1.8 * 0.89, 1.8 * 1.21] {
      assert!(!limit.permits(length));
    }
    assert!(!LengthLimit::new(0., true).permits(0.));
  }
}
