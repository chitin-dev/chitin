//! Graph-theoretic ring boundaries, independent of aromaticity and coordinates.
//!
//! Vertex-biconnected blocks separate spiro rings at their shared articulation
//! atom. Fused/bridged cycles remain in one block. No cycle basis is presented as
//! a catalogue of all chemical rings; reports belong to one immutable snapshot.

use crate::{AtomId, BondId, GraphError, MolecularGraph};
use std::collections::{BTreeMap, BTreeSet};

/// A maximal vertex-biconnected edge block; a bridge is a one-edge block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RingBlock {
  /// Live handles in ascending order; articulation atoms can occur in several blocks.
  atoms: Vec<AtomId>,
  /// Live edge handles in ascending order; each graph edge belongs to one block.
  bonds: Vec<BondId>,
}

impl RingBlock {
  /// Returns this block's atom handles in stable order.
  pub fn atoms(&self) -> &[AtomId] {
    &self.atoms
  }
  /// Returns this block's bond handles in stable order.
  pub fn bonds(&self) -> &[BondId] {
    &self.bonds
  }
  /// Returns E - V + 1, not a chemical/aromatic ring count.
  pub fn cycle_rank(&self) -> usize {
    self.bonds.len() + 1 - self.atoms.len()
  }
}

/// Detached block decomposition without assigning chemical ring identities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RingTopology {
  /// All edge blocks, including bridges; isolated atoms have no edge block.
  blocks: Vec<RingBlock>,
  /// Edges belonging to no cycle, irrespective of bond order.
  bridges: BTreeSet<BondId>,
  /// Atoms whose removal increases their component's connected-component count.
  articulation_atoms: BTreeSet<AtomId>,
}

impl RingTopology {
  /// Returns edge-disjoint blocks ordered by their smallest bond handle.
  pub fn blocks(&self) -> &[RingBlock] {
    &self.blocks
  }
  /// Returns all edges outside cycles; this does not imply chemical rotatability.
  pub fn bridges(&self) -> &BTreeSet<BondId> {
    &self.bridges
  }
  /// Returns atoms joining multiple edge blocks, including acyclic branch points.
  pub fn articulation_atoms(&self) -> &BTreeSet<AtomId> {
    &self.articulation_atoms
  }
}

/// Decomposes an immutable graph using iterative low-link DFS and an edge stack.
///
/// # Parameters
/// - `graph`: possibly empty/disconnected simple graph; unknown chemical facts
///   and bond orders are accepted because this operation is purely topological.
///
/// # Returns
/// Every live edge in exactly one block, all bridges, and articulation atoms.
/// Spiro rings share an articulation atom but not a block. Work/storage are
/// O(V + E) after ordered handle lookup; sorting adds logarithmic overhead.
/// Recompute after edits. No recursion, coordinates, or property-cache mutation.
///
/// # Errors
/// Propagates missing endpoints if graph structural invariants are violated.
pub fn analyze_rings(graph: &MolecularGraph) -> Result<RingTopology, GraphError> {
  let ids: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
  let indices: BTreeMap<_, _> = ids.iter().enumerate().map(|(i, &id)| (id, i)).collect();
  let mut rows = vec![Vec::new(); ids.len()];
  let mut endpoints = BTreeMap::new();
  for (edge, bond) in graph.bonds() {
    let [a, b] = bond.atoms();
    let a = *indices.get(&a).ok_or(GraphError::MissingAtom(a))?;
    let b = *indices.get(&b).ok_or(GraphError::MissingAtom(b))?;
    rows[a].push((b, edge));
    rows[b].push((a, edge));
    endpoints.insert(edge, bond.atoms());
  }
  let mut discovery = vec![usize::MAX; ids.len()];
  let mut low = vec![0; ids.len()];
  let mut parent: Vec<Option<(usize, BondId)>> = vec![None; ids.len()];
  let mut children = vec![0; ids.len()];
  let mut pending_edges = Vec::new();
  let mut blocks = Vec::new();
  let mut bridges = BTreeSet::new();
  let mut articulation_atoms = BTreeSet::new();
  let mut tick = 0;
  for root in 0..ids.len() {
    if discovery[root] != usize::MAX {
      continue;
    }
    discovery[root] = tick;
    low[root] = tick;
    tick += 1;
    let mut stack = vec![(root, 0)];
    while let Some((node, cursor)) = stack.last_mut() {
      let node = *node;
      if let Some(&(other, edge)) = rows[node].get(*cursor) {
        *cursor += 1;
        if parent[node].is_some_and(|(_, incoming)| edge == incoming) {
          continue;
        }
        if discovery[other] == usize::MAX {
          children[node] += 1;
          parent[other] = Some((node, edge));
          pending_edges.push(edge);
          discovery[other] = tick;
          low[other] = tick;
          tick += 1;
          stack.push((other, 0));
        } else if discovery[other] < discovery[node] {
          // Only ancestor-facing back edges enter the stack, once per edge.
          pending_edges.push(edge);
          low[node] = low[node].min(discovery[other]);
        }
      } else {
        stack.pop();
        if let Some((previous, edge)) = parent[node] {
          low[previous] = low[previous].min(low[node]);
          if low[node] > discovery[previous] {
            bridges.insert(edge);
          }
          if low[node] >= discovery[previous] {
            if parent[previous].is_some() {
              articulation_atoms.insert(ids[previous]);
            }
            let mut bonds = Vec::new();
            let mut atoms = BTreeSet::new();
            while let Some(candidate) = pending_edges.pop() {
              atoms.extend(endpoints[&candidate]);
              bonds.push(candidate);
              if candidate == edge {
                break;
              }
            }
            bonds.sort_unstable();
            blocks.push(RingBlock {
              atoms: atoms.into_iter().collect(),
              bonds,
            });
          }
        } else if children[node] > 1 {
          articulation_atoms.insert(ids[node]);
        }
      }
    }
  }
  blocks.sort_unstable_by_key(|block| block.bonds[0]);
  Ok(RingTopology {
    blocks,
    bridges,
    articulation_atoms,
  })
}
