//! Mutable topology with opaque IDs and deterministic adjacency traversal.
//!
//! IDs are graph-local handles, not dense indices or reaction atom-map numbers.
//! Deletions do not renumber survivors or reuse handles. Cloning preserves the
//! handle namespace for a snapshot; unrelated graphs require an explicit mapping.

use std::{
  collections::{BTreeMap, BTreeSet},
  fmt,
};

use crate::{Atom, Bond, BondOrder, ImplicitHydrogenPolicy, StereoError, StereoLigand, TetrahedralStereo};

mod double_bond;

/// Generates distinct opaque handle types without exposing raw-ID construction.
macro_rules! graph_id {
  ($name:ident, $description:literal) => {
    #[doc = $description]
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct $name(u64);

    impl $name {
      /// Returns the graph-local handle value, not a dense table index.
      pub const fn value(self) -> u64 {
        self.0
      }
    }

    impl fmt::Display for $name {
      /// Formats the local numeric handle, not a global chemical identity.
      fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
      }
    }
  };
}

graph_id!(
  AtomId,
  "A stable atom handle scoped to its originating molecular graph."
);
graph_id!(
  BondId,
  "A stable bond handle scoped to its originating molecular graph."
);

/// A rejected graph operation; failed operations leave the graph unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum GraphError {
  /// An atom handle is not live in this graph.
  #[error("atom {0} does not exist in this graph")]
  MissingAtom(AtomId),
  /// A bond handle is not live in this graph.
  #[error("bond {0} does not exist in this graph")]
  MissingBond(BondId),
  /// A chemical edge cannot connect an atom to itself.
  #[error("atom {0} cannot be bonded to itself")]
  SelfBond(AtomId),
  /// One undirected edge already connects these endpoints.
  #[error("atoms {a} and {b} are already bonded")]
  DuplicateBond {
    /// First requested endpoint.
    a: AtomId,
    /// Second requested endpoint.
    b: AtomId,
  },
  /// The monotonic atom or bond counter cannot allocate another handle.
  #[error("graph identifier counter exhausted")]
  IdentifierExhausted,
}

/// One live atom and its symmetric, duplicate-free adjacency index.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Vertex {
  /// Supplied attributes; topology operations never sanitize them.
  atom: Atom,
  // One entry per neighboring atom simultaneously prevents parallel edges
  // and makes local traversal independent of deleted IDs and table compaction.
  neighbors: BTreeMap<AtomId, BondId>,
}

/// An editable simple, undirected chemical graph, possibly disconnected or empty.
///
/// Multiple bonds are represented by one edge's [`BondOrder`], not parallel
/// edges. Invalid valence and incomplete chemical facts remain editable. Private
/// storage prevents dangling endpoints, asymmetric adjacency, and ID mutation.
/// Iteration is deterministic by increasing handle, not chemically canonical.
/// This baseline uses ordered maps; it does not claim dense-array performance.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MolecularGraph {
  /// Live vertices only; removal does not compact their handles.
  atoms: BTreeMap<AtomId, Vertex>,
  /// One record per unordered endpoint pair, mirrored by vertex adjacency.
  bonds: BTreeMap<BondId, Bond>,
  /// Supplied ordered local configurations, invalidated by incident structural edits.
  tetrahedral: BTreeMap<AtomId, TetrahedralStereo>,
  /// Relative alkene configurations, invalidated by endpoint coordination edits.
  double_bond_stereo: BTreeMap<BondId, crate::DoubleBondStereo>,
  /// Next candidate handle; never decremented after deletion.
  next_atom: u64,
  /// Independent monotonic bond-handle namespace.
  next_bond: u64,
}

impl MolecularGraph {
  /// Creates an empty graph with its own local ID namespace.
  pub fn new() -> Self {
    Self::default()
  }
  /// Returns the number of live atoms.
  pub fn atom_count(&self) -> usize {
    self.atoms.len()
  }
  /// Returns the number of live bonds.
  pub fn bond_count(&self) -> usize {
    self.bonds.len()
  }
  /// Returns whether the graph contains no atoms.
  pub fn is_empty(&self) -> bool {
    self.atoms.is_empty()
  }
  /// Returns an atom's attributes if its handle is live.
  pub fn atom(&self, id: AtomId) -> Option<&Atom> {
    self.atoms.get(&id).map(|vertex| &vertex.atom)
  }
  /// Returns a bond if its handle is live.
  pub fn bond(&self, id: BondId) -> Option<&Bond> {
    self.bonds.get(&id)
  }
  /// Visits live atoms in increasing handle order without exposing mutable IDs.
  pub fn atoms(&self) -> impl ExactSizeIterator<Item = (AtomId, &Atom)> + '_ {
    self.atoms.iter().map(|(&id, vertex)| (id, &vertex.atom))
  }
  /// Visits live bonds in increasing handle order.
  pub fn bonds(&self) -> impl ExactSizeIterator<Item = (BondId, &Bond)> + '_ {
    self.bonds.iter().map(|(&id, bond)| (id, bond))
  }

  /// Returns a supplied tetrahedral descriptor, not an inferred stereocenter.
  pub fn tetrahedral(&self, center: AtomId) -> Option<&TetrahedralStereo> {
    self.tetrahedral.get(&center)
  }
  /// Visits supplied tetrahedral centers in stable handle order.
  pub fn tetrahedral_centers(&self) -> impl ExactSizeIterator<Item = (AtomId, &TetrahedralStereo)> + '_ {
    self.tetrahedral.iter().map(|(&id, stereo)| (id, stereo))
  }

  /// Installs or clears an ordered tetrahedral fact after checking its references.
  ///
  /// # Parameters
  /// - `center`: a live atom in this graph's namespace.
  /// - `stereo`: four explicit neighbors, or three neighbors and one declared H.
  ///
  /// # Returns
  /// The previous descriptor. No CIP assignment or stereogenicity perception
  /// runs. Incident bond edits and center-attribute replacement clear this fact
  /// conservatively; consumers must reinstall it after validating their edit.
  ///
  /// # Errors
  /// Rejects stale/nonneighbor references, aromatic centers, multiple bonds,
  /// or inconsistent H/coordination. Failure leaves the graph unchanged.
  pub fn set_tetrahedral(
    &mut self,
    center: AtomId,
    stereo: Option<TetrahedralStereo>,
  ) -> Result<Option<TetrahedralStereo>, StereoError> {
    let atom = self.atom(center).ok_or(GraphError::MissingAtom(center))?;
    if let Some(stereo) = stereo {
      let mut expected: BTreeSet<_> = self.neighbors(center)?.map(|(id, _)| StereoLigand::Atom(id)).collect();
      if atom.declared_hydrogens() == Some(1) {
        expected.insert(StereoLigand::Hydrogen);
      }
      let supplied: BTreeSet<_> = stereo.ligands().iter().copied().collect();
      if atom.is_aromatic()
        || expected.len() != 4
        || expected != supplied
        || !matches!(atom.declared_hydrogens(), Some(0 | 1))
        || atom.implicit_hydrogen_policy() != ImplicitHydrogenPolicy::Fixed(0)
        || self
          .neighbors(center)?
          .any(|(_, id)| self.bond(id).is_none_or(|bond| bond.order() != BondOrder::Single))
      {
        return Err(StereoError::Coordination(center));
      }
      Ok(self.tetrahedral.insert(center, stereo))
    } else {
      Ok(self.tetrahedral.remove(&center))
    }
  }

  /// Adds supplied atomic attributes without inferring chemical properties.
  ///
  /// # Parameters
  /// - `atom`: input facts, including unknown or chemically implausible values.
  ///
  /// # Returns
  /// A fresh graph-local handle for an isolated vertex. The graph takes ownership
  /// of the supplied facts; no neighbors or inferred properties are added.
  ///
  /// # Errors
  /// Returns [`GraphError::IdentifierExhausted`] rather than reusing an old ID.
  pub fn add_atom(&mut self, atom: Atom) -> Result<AtomId, GraphError> {
    let id = AtomId(allocate_id(&mut self.next_atom)?);
    self.atoms.insert(
      id,
      Vertex {
        atom,
        neighbors: BTreeMap::new(),
      },
    );
    Ok(id)
  }

  /// Replaces atomic attributes while retaining the ID and all connectivity.
  ///
  /// # Parameters
  /// - `id`: live handle in this graph, not an index into atom iteration.
  /// - `atom`: replacement facts; no chemical validation is performed.
  ///
  /// # Returns
  /// The previous attributes, suitable for recording a future reversible edit.
  ///
  /// # Errors
  /// Returns [`GraphError::MissingAtom`] when `id` is not live.
  /// Successful replacement clears supplied stereo centered on `id`.
  pub fn replace_atom(&mut self, id: AtomId, atom: Atom) -> Result<Atom, GraphError> {
    let vertex = self.atoms.get_mut(&id).ok_or(GraphError::MissingAtom(id))?;
    self.tetrahedral.remove(&id);
    self.double_bond_stereo.retain(|_, stereo| !stereo.ends().contains(&id));
    Ok(std::mem::replace(&mut vertex.atom, atom))
  }

  /// Adds one bond between distinct, live atoms, preserving endpoint order.
  ///
  /// # Parameters
  /// - `a`, `b`: graph-local handles; either orientation denotes the same edge.
  /// - `order`: a supplied label, including unknown; no valence is inferred.
  ///
  /// # Returns
  /// A fresh stable bond handle. Existing edges are never silently overwritten.
  ///
  /// # Errors
  /// Rejects missing atoms, self-bonds, duplicate unordered pairs, or exhausted
  /// IDs. All checks precede mutation, including advancing the ID counter.
  pub fn add_bond(&mut self, a: AtomId, b: AtomId, order: BondOrder) -> Result<BondId, GraphError> {
    let vertex = self.atoms.get(&a).ok_or(GraphError::MissingAtom(a))?;
    if !self.atoms.contains_key(&b) {
      return Err(GraphError::MissingAtom(b));
    }
    if a == b {
      return Err(GraphError::SelfBond(a));
    }
    if vertex.neighbors.contains_key(&b) {
      return Err(GraphError::DuplicateBond { a, b });
    }
    let id = BondId(allocate_id(&mut self.next_bond)?);
    self.tetrahedral.remove(&a);
    self.tetrahedral.remove(&b);
    self.clear_double_stereo_at(&[a, b]);
    self.bonds.insert(id, Bond { atoms: [a, b], order });
    // Endpoints were checked before any writes. No user code can mutate the
    // graph between validation and these symmetric adjacency updates.
    if let Some(vertex) = self.atoms.get_mut(&a) {
      vertex.neighbors.insert(b, id);
    }
    if let Some(vertex) = self.atoms.get_mut(&b) {
      vertex.neighbors.insert(a, id);
    }
    Ok(id)
  }

  /// Changes only a bond's order and returns its previous label.
  ///
  /// # Errors
  /// Returns [`GraphError::MissingBond`] when `id` is not live.
  /// An actual order change clears stereo centered on either endpoint.
  pub fn set_bond_order(&mut self, id: BondId, order: BondOrder) -> Result<BondOrder, GraphError> {
    let bond = self.bonds.get_mut(&id).ok_or(GraphError::MissingBond(id))?;
    if bond.order != order {
      self
        .double_bond_stereo
        .retain(|_, stereo| !stereo.ends().iter().any(|end| bond.atoms.contains(end)));
      for atom in bond.atoms {
        self.tetrahedral.remove(&atom);
      }
    }
    Ok(std::mem::replace(&mut bond.order, order))
  }

  /// Removes one bond and both adjacency links, without changing atom IDs.
  ///
  /// # Parameters
  /// - `id`: live edge handle in this graph, independent of endpoint orientation.
  ///
  /// # Returns
  /// The removed edge with its original endpoint order and bond label. Both
  /// endpoints remain live, even when removal leaves them isolated.
  ///
  /// # Errors
  /// Returns [`GraphError::MissingBond`] when `id` is not live.
  pub fn remove_bond(&mut self, id: BondId) -> Result<Bond, GraphError> {
    let bond = self.bonds.remove(&id).ok_or(GraphError::MissingBond(id))?;
    let [a, b] = bond.atoms;
    self.clear_double_stereo_at(&[a, b]);
    self.tetrahedral.remove(&a);
    self.tetrahedral.remove(&b);
    if let Some(vertex) = self.atoms.get_mut(&a) {
      vertex.neighbors.remove(&b);
    }
    if let Some(vertex) = self.atoms.get_mut(&b) {
      vertex.neighbors.remove(&a);
    }
    Ok(bond)
  }

  /// Removes an atom and every incident bond, returning the atomic attributes.
  ///
  /// Surviving IDs remain unchanged. The returned atom alone is not a complete
  /// undo record: an edit layer must also capture incident bonds before deletion.
  ///
  /// # Parameters
  /// - `id`: live vertex to remove together with every incident edge.
  ///
  /// # Returns
  /// The removed input attributes; survivor IDs and allocation counters retain
  /// their previous values, so deleted handles are not available for reuse.
  ///
  /// # Errors
  /// Returns [`GraphError::MissingAtom`] when `id` is not live.
  pub fn remove_atom(&mut self, id: AtomId) -> Result<Atom, GraphError> {
    let vertex = self.atoms.remove(&id).ok_or(GraphError::MissingAtom(id))?;
    self.clear_double_stereo_at(&[id]);
    self.tetrahedral.remove(&id);
    for (neighbor, bond) in vertex.neighbors {
      self.clear_double_stereo_at(&[neighbor]);
      self.tetrahedral.remove(&neighbor);
      self.bonds.remove(&bond);
      if let Some(other) = self.atoms.get_mut(&neighbor) {
        other.neighbors.remove(&id);
      }
    }
    Ok(vertex.atom)
  }

  /// Finds the edge between two live atoms, regardless of endpoint orientation.
  pub fn bond_between(&self, a: AtomId, b: AtomId) -> Option<BondId> {
    self.atoms.get(&a)?.neighbors.get(&b).copied()
  }

  /// Visits `(neighbor atom, connecting bond)` pairs in neighbor-ID order.
  ///
  /// # Errors
  /// Returns [`GraphError::MissingAtom`] rather than treating a deleted handle
  /// as an isolated atom. Returned IDs belong to this graph's namespace.
  pub fn neighbors(&self, id: AtomId) -> Result<impl ExactSizeIterator<Item = (AtomId, BondId)> + '_, GraphError> {
    let vertex = self.atoms.get(&id).ok_or(GraphError::MissingAtom(id))?;
    Ok(vertex.neighbors.iter().map(|(&atom, &bond)| (atom, bond)))
  }

  /// Finds components using explicit edges only, including isolated atoms.
  ///
  /// # Returns
  /// Components ordered by their smallest atom ID, with each component sorted.
  /// Empty graphs return no components. Traversal is iterative to avoid stack
  /// overflow on long chains; unknown bond orders still establish connectivity.
  /// No proximity, valence, or chemical plausibility rules are applied.
  pub fn connected_components(&self) -> Vec<Vec<AtomId>> {
    let mut visited = BTreeSet::new();
    let mut components = Vec::new();
    for &root in self.atoms.keys() {
      if !visited.insert(root) {
        continue;
      }
      let mut pending = vec![root];
      let mut component = Vec::new();
      while let Some(id) = pending.pop() {
        component.push(id);
        if let Some(vertex) = self.atoms.get(&id) {
          for &neighbor in vertex.neighbors.keys() {
            // Mark on enqueue, not dequeue, so cycles cannot queue a vertex
            // repeatedly and the traversal retains O(V + E) edge visits.
            if visited.insert(neighbor) {
              pending.push(neighbor);
            }
          }
        }
      }
      component.sort_unstable();
      components.push(component);
    }
    components
  }
}

/// Reserves one monotonic handle without changing the counter on overflow.
///
/// # Parameters
/// - `counter`: next unused value in one atom or bond namespace.
///
/// # Returns
/// The previous counter value, after successfully advancing the allocation cursor.
/// `u64::MAX` remains the exhausted sentinel rather than an allocatable handle.
///
/// # Errors
/// Returns `IdentifierExhausted` before any write if incrementing would overflow.
fn allocate_id(counter: &mut u64) -> Result<u64, GraphError> {
  let next = counter.checked_add(1).ok_or(GraphError::IdentifierExhausted)?;
  let id = *counter;
  *counter = next;
  Ok(id)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::Element;

  #[test]
  fn exhausted_atom_ids_should_fail_without_mutation() {
    let mut graph = MolecularGraph {
      next_atom: u64::MAX,
      ..Default::default()
    };
    let before = graph.clone();
    assert_eq!(
      graph.add_atom(Atom::new(Element::Carbon)),
      Err(GraphError::IdentifierExhausted)
    );
    assert_eq!(graph, before);
  }

  #[test]
  fn exhausted_bond_ids_should_not_leave_partial_adjacency() -> Result<(), GraphError> {
    let mut graph = MolecularGraph {
      next_bond: u64::MAX,
      ..Default::default()
    };
    let a = graph.add_atom(Atom::new(Element::Carbon))?;
    let b = graph.add_atom(Atom::new(Element::Oxygen))?;
    let before = graph.clone();
    assert_eq!(
      graph.add_bond(a, b, BondOrder::Single),
      Err(GraphError::IdentifierExhausted)
    );
    assert_eq!(graph, before);
    Ok(())
  }
}
