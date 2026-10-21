//! Explicit coordination associations over source edges, independent of geometry.
//!
//! Ordinary single bonds are never automatically reclassified by element. This
//! detached declaration records a caller-supplied interpretation without editing
//! source orders, charges, hydrogen policies, or guessing coordination stereo.

use crate::{AtomId, BondId, BondOrder, GraphError, MolecularGraph};
use std::collections::BTreeSet;

/// A supplied acceptor and the donor attachments represented by existing edges.
///
/// Handles retain source provenance. Validate again after graph edits; clones of
/// the source graph remain compatible, independently created graphs do not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoordinationSite {
  /// Acceptor identity, not an oxidation-state or geometry assignment.
  center: AtomId,
  /// Stable source-edge identities paired with their donor endpoints.
  attachments: Vec<(BondId, AtomId)>,
}

/// Rejected explicit association; no source graph is ever modified.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CoordinationError {
  /// A source handle is absent or belongs to a different graph namespace.
  #[error(transparent)]
  Graph(#[from] GraphError),
  /// A declaration must include at least one attachment.
  #[error("coordination site has no attachments")]
  Empty,
  /// A source edge occurs more than once in the declaration.
  #[error("duplicate coordination attachment {0}")]
  Duplicate(BondId),
  /// This profile associates existing single edges only.
  #[error("coordination attachment {0} must be a single edge incident on its center")]
  Attachment(BondId),
}

impl CoordinationSite {
  /// Declares donor-to-center associations without changing source bond orders.
  ///
  /// # Parameters
  /// - `graph`: source namespace owning every supplied handle.
  /// - `center`: acceptor atom; its element alone never implies coordination.
  /// - `bonds`: existing incident single edges, in any order.
  ///
  /// # Returns
  /// A source-ordered detached declaration, with no inferred valence, geometry,
  /// ligand equivalence, oxidation state, or stereochemical configuration.
  ///
  /// # Errors
  /// Rejects missing/foreign handles, empty/duplicate edges, and edges not single
  /// or not incident on the center. All checks are read-only.
  pub fn new(
    graph: &MolecularGraph,
    center: AtomId,
    bonds: impl IntoIterator<Item = BondId>,
  ) -> Result<Self, CoordinationError> {
    graph.atom(center).ok_or(GraphError::MissingAtom(center))?;
    let mut seen = BTreeSet::new();
    let mut attachments = Vec::new();
    for id in bonds {
      if !seen.insert(id) {
        return Err(CoordinationError::Duplicate(id));
      }
      let bond = graph.bond(id).ok_or(GraphError::MissingBond(id))?;
      let [a, b] = bond.atoms();
      if bond.order() != BondOrder::Single || (a != center && b != center) {
        return Err(CoordinationError::Attachment(id));
      }
      attachments.push((id, if a == center { b } else { a }));
    }
    if attachments.is_empty() {
      return Err(CoordinationError::Empty);
    }
    attachments.sort_unstable();
    Ok(Self { center, attachments })
  }

  /// Returns the explicitly declared acceptor handle.
  pub const fn center(&self) -> AtomId {
    self.center
  }

  /// Returns source edges and their declared donor endpoints in stable order.
  pub fn attachments(&self) -> &[(BondId, AtomId)] {
    &self.attachments
  }

  /// Rechecks this declaration against an edited or cloned source graph.
  ///
  /// # Parameters
  /// - `graph`: source snapshot whose live identities and edge labels must match.
  ///
  /// # Returns
  /// Success only when every declared attachment retains its original endpoints.
  ///
  /// # Errors
  /// Reports missing/foreign handles or changed source-edge semantics. It never
  /// repairs the declaration or reinterprets an unrelated live entity.
  pub fn validate(&self, graph: &MolecularGraph) -> Result<(), CoordinationError> {
    let current = Self::new(graph, self.center, self.attachments.iter().map(|&(id, _)| id))?;
    if current.attachments != self.attachments {
      return Err(CoordinationError::Attachment(self.attachments[0].0));
    }
    Ok(())
  }
}
