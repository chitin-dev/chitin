//! Build a spanning forest, then emit its branches and non-tree edges.
//!
//! A traversal plan is not a canonical rank or chemical ring-perception result.
//! Dense indices are private temporary addresses; sparse stable IDs stay intact.

use std::collections::{BTreeMap, BTreeSet};

use crate::{AtomId, BondOrder, GraphError, MolecularGraph, StereoError, StereoLigand};

use super::{super::profile, SmilesWriteError as Error, atom};

/// One temporary dense vertex with its token and ordered serialization sites.
struct Node {
  /// Incoming tree neighbor, absent at a component root.
  parent: Option<usize>,
  /// Original sparse graph handle, used only for failure locations.
  id: AtomId,
  /// Attribute-checked spelling, independent of the spanning forest.
  token: String,
  /// DFS discovery order; each pair contains a child-node and tree-edge index.
  children: Vec<(usize, usize)>, // child node index, edge index
  /// Non-tree edge sites sorted with closings before openings at this vertex.
  rings: Vec<RingSite>,
}

/// Supported bond spelling addressed by temporary edge index, not a graph ID.
struct Edge {
  /// Dense endpoints used to recover the opposite ring-site ligand.
  atoms: [usize; 2],
  /// Single is explicit between aromatic atoms; other orders have their own symbol.
  symbol: &'static str,
  /// Canonical low-to-high handle direction, reversed at opposite emission sites.
  direction: Option<bool>,
}

impl Edge {
  /// Resolves a lexical direction from its actual traversal origin.
  fn symbol(&self, from: usize) -> &'static str {
    match self.direction {
      Some(slash) if slash ^ (from != self.atoms[0].min(self.atoms[1])) => "/",
      Some(_) => "\\",
      None => self.symbol,
    }
  }
}

/// One half of a non-tree edge's serialization, paired by temporary edge index.
struct RingSite {
  /// Index shared by exactly one opening and one later closing site.
  edge: usize,
  /// The ancestor endpoint opens the label; the descendant endpoint closes it.
  opening: bool,
}

/// Validated spanning forest consumed by emission, not a persistent graph cache.
///
/// Every vertex is reached by exactly one root/tree path. Every edge is either
/// a tree child or one paired non-tree edge. Child order matches DFS discovery,
/// so the opening ring site is emitted before its closing site. Dense indices
/// are scoped to this value and must never be installed as graph-local IDs.
pub(super) struct Plan {
  /// Vertices in input handle order, not in canonical or emission order.
  nodes: Vec<Node>,
  /// Supported edge symbols, shared by tree events and ring sites.
  edges: Vec<Edge>,
  /// One root for each connected component, visited in input handle order.
  roots: Vec<usize>,
}

/// Suspended DFS loop state, replacing recursive calls for long molecular chains.
struct Frame {
  /// Current dense vertex whose adjacency is being scanned.
  node: usize,
  /// First unprocessed position in that vertex's sorted adjacency list.
  next: usize,
}

/// Validates representability and plans a deterministic, iterative DFS forest.
///
/// # Parameters
/// - `graph`: borrowed input with all supported facts stored on its atoms/bonds.
///
/// # Returns
/// A private plan with encoded atoms, ordered tree children, and paired ring
/// sites. Atom IDs choose roots/neighbors but are never rewritten. Ordered-map
/// lookups and adjacency/site sorting bound planning by O((V + E) log(V + E))
/// time with O(V + E) temporary storage; this is not canonical labeling.
///
/// # Errors
/// Returns an atom/bond representability error or an inconsistent endpoint
/// reference before emission starts. Ring-label capacity is checked later,
/// because it depends on the lifetime of sites in actual output order.
pub(super) fn plan(graph: &MolecularGraph) -> Result<Plan, Error> {
  let mut nodes = Vec::with_capacity(graph.atom_count());
  let mut indices = BTreeMap::new();
  for (id, facts) in graph.atoms() {
    let token = atom::encode(facts, None).map_err(|reason| Error::Atom { atom: id, reason })?;
    indices.insert(id, nodes.len());
    nodes.push(Node {
      parent: None,
      id,
      token,
      children: Vec::new(),
      rings: Vec::new(),
    });
  }
  let mut edges = Vec::with_capacity(graph.bond_count());
  let directions = super::directional::assign(graph)?;
  let mut adjacency = vec![Vec::new(); nodes.len()];
  for (id, bond) in graph.bonds() {
    let aromatic_endpoints = bond
      .atoms()
      .into_iter()
      .all(|atom| graph.atom(atom).is_some_and(|atom| atom.is_aromatic()));
    let symbol = match bond.order() {
      BondOrder::Single if aromatic_endpoints => "-",
      BondOrder::Single => "",
      BondOrder::Double => "=",
      BondOrder::Triple => "#",
      BondOrder::Aromatic => ":",
      order => return Err(Error::Bond { bond: id, order }),
    };
    let [a, b] = bond.atoms();
    let a = *indices.get(&a).ok_or(GraphError::MissingAtom(a))?;
    let b = *indices.get(&b).ok_or(GraphError::MissingAtom(b))?;
    adjacency[a].push((b, edges.len()));
    adjacency[b].push((a, edges.len()));
    edges.push(Edge {
      symbol,
      atoms: [a, b],
      direction: directions.get(&id).copied(),
    });
  }
  // Neighbor order, not bond insertion/orientation, chooses DFS branches.
  for neighbors in &mut adjacency {
    neighbors.sort_unstable_by_key(|&(node, _)| node);
  }
  let mut visited = vec![false; nodes.len()];
  let mut seen_edges = vec![false; edges.len()];
  let mut roots = Vec::new();
  let mut stack = Vec::new();
  for root in 0..nodes.len() {
    if visited[root] {
      continue;
    }
    roots.push(root);
    visited[root] = true;
    stack.push(Frame { node: root, next: 0 });
    while let Some(frame) = stack.last_mut() {
      let node = frame.node;
      let Some(&(other, edge)) = adjacency[node].get(frame.next) else {
        stack.pop();
        continue;
      };
      frame.next += 1;
      // Each undirected edge appears twice in adjacency. Consume it only once,
      // independently of vertex visitation, to avoid pairing a tree edge as a ring.
      if seen_edges[edge] {
        continue;
      }
      seen_edges[edge] = true;
      if visited[other] {
        // Undirected DFS non-tree edges lead to ancestors: the ancestor is
        // emitted first. These edges need labels, not extra tree children.
        nodes[other].rings.push(RingSite { edge, opening: true });
        nodes[node].rings.push(RingSite { edge, opening: false });
      } else {
        visited[other] = true;
        nodes[other].parent = Some(node);
        nodes[node].children.push((other, edge));
        stack.push(Frame { node: other, next: 0 });
      }
    }
  }
  for node in &mut nodes {
    // Close before opening at the same atom so labels can be immediately reused.
    node.rings.sort_by_key(|site| (site.opening, site.edge));
  }
  // SMILES reference order is incoming, virtual H, ring sites, then children.
  // It is not sorted adjacency and may differ from the original source order.
  for index in 0..nodes.len() {
    let node = &nodes[index];
    let Some(stereo) = graph.tetrahedral(node.id) else {
      continue;
    };
    let mut ligands = Vec::new();
    if let Some(parent) = node.parent {
      ligands.push(StereoLigand::Atom(nodes[parent].id));
    }
    if stereo.ligands().contains(&StereoLigand::Hydrogen) {
      ligands.push(StereoLigand::Hydrogen);
    }
    for site in &node.rings {
      let [a, b] = edges[site.edge].atoms;
      ligands.push(StereoLigand::Atom(nodes[if a == index { b } else { a }].id));
    }
    for &(child, _) in &node.children {
      ligands.push(StereoLigand::Atom(nodes[child].id));
    }
    let ligands: [_; 4] = ligands.try_into().map_err(|_| StereoError::Coordination(node.id))?;
    let winding = stereo.reordered(ligands)?.winding();
    let facts = graph.atom(node.id).ok_or(GraphError::MissingAtom(node.id))?;
    nodes[index].token = atom::encode(facts, Some(winding)).map_err(|reason| Error::Atom {
      atom: nodes[index].id,
      reason,
    })?;
  }
  Ok(Plan { nodes, edges, roots })
}

/// LIFO emission work; explicit delimiters avoid recursion for nested branches.
enum Event {
  /// Emit this vertex's token and ring sites before scheduling its children.
  Atom(usize),
  /// Emit a tree edge's supported order immediately before its child atom.
  Bond(usize, usize),
  /// Start a non-continuation child branch.
  Open,
  /// Close that branch after its entire subtree has been emitted.
  Close,
}

/// Emits a complete owned string; failure never publishes a prefix to a sink.
///
/// # Parameters
/// - `plan`: locally built forest whose site pairing and index invariants hold.
///
/// # Returns
/// Deterministic noncanonical text, with dots between roots, ring sites before
/// branches, and the last tree child used as the unparenthesized continuation.
/// Closures release labels immediately, including for reuse at the same atom.
/// The output/work stacks and label table are private to this call.
///
/// # Errors
/// Returns `RingLabelCapacity` only when this traversal needs more simultaneously
/// open labels than the shared profile permits, not when total cycle count is
/// large. `TraversalInvariant` indicates an internal unpaired closing site.
/// Either failure discards the owned prefix rather than returning partial text.
pub(super) fn emit(plan: Plan) -> Result<String, Error> {
  let mut text = String::new();
  let mut free: BTreeSet<_> = (0..profile::RING_LABEL_COUNT).collect();
  let mut labels = vec![None; plan.edges.len()];
  let mut stack = Vec::new();
  for (component, root) in plan.roots.iter().enumerate() {
    if component != 0 {
      text.push('.');
    }
    stack.push(Event::Atom(*root));
    while let Some(event) = stack.pop() {
      match event {
        Event::Open => text.push('('),
        Event::Close => text.push(')'),
        Event::Bond(edge, from) => text.push_str(plan.edges[edge].symbol(from)),
        Event::Atom(index) => {
          let node = &plan.nodes[index];
          text.push_str(&node.token);
          for site in &node.rings {
            let label = if site.opening {
              let label = free.pop_first().ok_or(Error::RingLabelCapacity { atom: node.id })?;
              labels[site.edge] = Some(label);
              text.push_str(plan.edges[site.edge].symbol(index));
              label
            } else {
              // Each closing site has one earlier opening by construction.
              let label = labels[site.edge]
                .take()
                .ok_or(Error::TraversalInvariant { atom: node.id })?;
              free.insert(label);
              label
            };
            if label >= 10 {
              text.push('%');
            }
            text.push_str(&label.to_string());
          }
          // Push child subtrees in reverse so LIFO execution retains planning
          // order. Each branch's closing event stays below its complete subtree,
          // while the last child needs neither an opening nor a closing event.
          for (position, &(child, edge)) in node.children.iter().enumerate().rev() {
            let branch = position + 1 != node.children.len();
            if branch {
              stack.push(Event::Close);
            }
            stack.push(Event::Atom(child));
            stack.push(Event::Bond(edge, index));
            if branch {
              stack.push(Event::Open);
            }
          }
        }
      }
    }
  }
  Ok(text)
}
