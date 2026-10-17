//! Short-cycle polygon constraints selected independently of chemical ring naming.
//!
//! All shortest alternate paths per edge supply small-cycle candidates; a single
//! spanning tree supplies a complete fallback cycle space. Binary elimination
//! selects independent candidates by length, not an exhaustive chemical ring set.

use super::Work;
use crate::layout::LayoutError as Error;
use chitin_chem::{AtomId, MolecularGraph, perception::RingBlock};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// A block's selected polygon cycles and shared-edge incidence graph.
pub(super) struct Constraints {
  /// Consecutive global dense vertices; cycle closure is implicit.
  pub(super) faces: Vec<Vec<usize>>,
  /// Polygon neighbors joined through exactly one shared skeleton edge.
  pub(super) neighbors: Vec<Vec<usize>>,
}

/// Dense block-local adjacency, separate from graph handles and component rows.
struct BlockGraph {
  /// Original handles used to locate invalid graph invariants.
  ids: Vec<AtomId>,
  /// Global layout rows corresponding to local vertices.
  global: Vec<usize>,
  /// Local edge endpoints in stable input bond order.
  edges: Vec<[usize; 2]>,
  /// Local neighbor and edge indices.
  rows: Vec<Vec<(usize, usize)>>,
}

impl Constraints {
  /// Selects a complete short-cycle basis and validates edge-fused incidence.
  ///
  /// # Parameters
  /// - `graph`, `block`, `indices`: identical immutable graph namespace.
  /// - `work`: shared accounting for discovery, elimination, and constraints.
  ///
  /// # Returns
  /// Layout polygons covering every block edge with independent cycle count
  /// E - V + 1. Adjacent polygons share one edge; single-vertex contacts are also
  /// allowed when the shared-edge graph remains connected. This is not SSSR,
  /// aromaticity, or a guaranteed minimum cycle basis for every graph family.
  ///
  /// # Errors
  /// Rejects bridged/shared-path or nonmanifold constraints, broken graph
  /// references, and exhausted work before any geometry is returned.
  pub(super) fn new(
    graph: &MolecularGraph,
    block: &RingBlock,
    indices: &BTreeMap<AtomId, usize>,
    work: &mut Work,
  ) -> Result<Self, Error> {
    work.charge(block.atoms().len() + block.bonds().len())?;
    let local: BTreeMap<_, _> = block.atoms().iter().enumerate().map(|(i, &id)| (id, i)).collect();
    let mut rows = vec![Vec::new(); local.len()];
    let mut edges = Vec::new();
    for &id in block.bonds() {
      let [a, b] = graph.bond(id).ok_or(chitin_chem::GraphError::MissingBond(id))?.atoms();
      let ends = [local[&a], local[&b]];
      rows[ends[0]].push((ends[1], edges.len()));
      rows[ends[1]].push((ends[0], edges.len()));
      edges.push(ends);
    }
    for row in &mut rows {
      row.sort_unstable();
    }
    let model = BlockGraph {
      ids: block.atoms().to_vec(),
      global: block.atoms().iter().map(|id| indices[id]).collect(),
      edges,
      rows,
    };
    let faces = model.basis(block.cycle_rank(), work)?;
    let unsupported = || Error::RingSystem {
      atom: block.atoms()[0],
      cycles: block.cycle_rank(),
    };
    let mut owners = BTreeMap::<(usize, usize), Vec<usize>>::new();
    let mut vertex_faces = vec![Vec::new(); model.ids.len()];
    for (i, face) in faces.iter().enumerate() {
      work.charge(face.len())?;
      for (&a, &b) in face.iter().zip(face.iter().cycle().skip(1)) {
        owners.entry(pair(a, b)).or_default().push(i);
        vertex_faces[a].push(i);
      }
    }
    if owners.len() != model.edges.len() || owners.values().any(|row| row.len() > 2) {
      return Err(unsupported());
    }
    let mut neighbors = vec![Vec::new(); faces.len()];
    let mut shared = BTreeSet::new();
    for row in owners.values() {
      if let &[a, b] = row.as_slice() {
        neighbors[a].push(b);
        neighbors[b].push(a);
        shared.insert(pair(a, b));
      }
    }
    let mut common = BTreeMap::<(usize, usize), usize>::new();
    for row in vertex_faces {
      for (i, &a) in row.iter().enumerate() {
        for &b in &row[i + 1..] {
          work.charge(1)?;
          *common.entry(pair(a, b)).or_default() += 1;
        }
      }
    }
    if common
      .iter()
      .any(|(pair, &count)| count > 2 || (count == 2 && !shared.contains(pair)))
    {
      return Err(unsupported());
    }
    let mut seen = vec![false; faces.len()];
    let mut pending = vec![0];
    seen[0] = true;
    while let Some(node) = pending.pop() {
      work.charge(1 + neighbors[node].len())?;
      for &other in &neighbors[node] {
        if !seen[other] {
          seen[other] = true;
          pending.push(other);
        }
      }
    }
    if seen.iter().any(|flag| !flag) {
      return Err(unsupported());
    }
    let faces = faces
      .into_iter()
      .map(|face| face.into_iter().map(|node| model.global[node]).collect())
      .collect();
    Ok(Self { faces, neighbors })
  }
}

impl BlockGraph {
  /// Finds independent short cycles, retaining a spanning-tree fallback for rank completeness.
  ///
  /// Unicyclic blocks take one linear walk. Multi-cycle blocks enumerate tied
  /// shortest paths under the shared budget, then reduce edge vectors over GF(2).
  /// Candidate storage and all repeated searches are charged, including long paths.
  fn basis(&self, rank: usize, work: &mut Work) -> Result<Vec<Vec<usize>>, Error> {
    let mut candidates = BTreeSet::new();
    if rank == 1 {
      self.shortest(0, &mut candidates, work)?;
    } else {
      for edge in 0..self.edges.len() {
        self.shortest(edge, &mut candidates, work)?;
      }
      self.fundamental(&mut candidates, work)?;
    }
    let mut candidates: Vec<_> = candidates.into_iter().collect();
    candidates.sort_by(|a, b| a.len().cmp(&b.len()).then(a.cmp(b)));
    let lookup: BTreeMap<_, _> = self
      .edges
      .iter()
      .enumerate()
      .map(|(i, &[a, b])| (pair(a, b), i))
      .collect();
    let width = self.edges.len().div_ceil(64);
    let mut pivots = BTreeMap::<usize, Vec<u64>>::new();
    let mut selected = Vec::new();
    for cycle in candidates {
      work.charge(width + cycle.len())?;
      let mut bits = vec![0_u64; width];
      for (&a, &b) in cycle.iter().zip(cycle.iter().cycle().skip(1)) {
        let edge = lookup[&pair(a, b)];
        bits[edge / 64] ^= 1 << (edge % 64);
      }
      loop {
        work.charge(width)?;
        let Some(word) = bits.iter().rposition(|&value| value != 0) else {
          break;
        };
        let pivot = word * 64 + 63 - bits[word].leading_zeros() as usize;
        if let Some(row) = pivots.get(&pivot) {
          for (value, &other) in bits.iter_mut().zip(row) {
            *value ^= other;
          }
        } else {
          pivots.insert(pivot, bits);
          selected.push(cycle);
          break;
        }
      }
      if selected.len() == rank {
        return Ok(selected);
      }
    }
    Err(Error::Invariant(self.ids[0]))
  }

  /// Enumerates all shortest alternate paths around one edge, without recursive DFS.
  ///
  /// Distances are discovered only through the target layer. Sparse maps avoid
  /// clearing an entire block-sized array for every small-ring search. Tied path
  /// multiplicity can be exponential, so every expansion and stored cycle is charged.
  fn shortest(&self, excluded: usize, candidates: &mut BTreeSet<Vec<usize>>, work: &mut Work) -> Result<(), Error> {
    let [start, end] = self.edges[excluded];
    let mut distance = BTreeMap::from([(end, 0_usize)]);
    let mut queue = VecDeque::from([end]);
    while let Some(node) = queue.pop_front() {
      let depth = distance[&node];
      if distance.get(&start).is_some_and(|&limit| depth >= limit) {
        break;
      }
      work.charge(1 + self.rows[node].len())?;
      for &(other, edge) in &self.rows[node] {
        if edge != excluded && !distance.contains_key(&other) {
          distance.insert(other, depth + 1);
          queue.push_back(other);
        }
      }
    }
    if !distance.contains_key(&start) {
      return Err(Error::Invariant(self.ids[start]));
    }
    let mut path = vec![start];
    let mut stack = vec![(start, 0)];
    while let Some((node, cursor)) = stack.last_mut() {
      work.charge(1)?;
      if *node == end {
        work.charge(path.len())?;
        candidates.insert(normalize(&path));
        stack.pop();
        path.pop();
      } else if let Some(&(other, edge)) = self.rows[*node].get(*cursor) {
        *cursor += 1;
        if edge != excluded && distance.get(&other).is_some_and(|&depth| depth + 1 == distance[node]) {
          path.push(other);
          stack.push((other, 0));
        }
      } else {
        stack.pop();
        path.pop();
      }
    }
    Ok(())
  }

  /// Adds one fundamental cycle per non-tree edge, ensuring candidates span the cycle space.
  ///
  /// Parent paths are joined at their common ancestor; the closed edge is not a
  /// duplicate path vertex. Long paths and adjacency visits consume the same budget.
  fn fundamental(&self, candidates: &mut BTreeSet<Vec<usize>>, work: &mut Work) -> Result<(), Error> {
    work.charge(self.ids.len())?;
    let mut parent = vec![usize::MAX; self.ids.len()];
    let mut depth = vec![0; self.ids.len()];
    parent[0] = 0;
    let mut queue = VecDeque::from([0]);
    while let Some(node) = queue.pop_front() {
      work.charge(1 + self.rows[node].len())?;
      for &(other, _) in &self.rows[node] {
        if parent[other] == usize::MAX {
          parent[other] = node;
          depth[other] = depth[node] + 1;
          queue.push_back(other);
        }
      }
    }
    for &[a, b] in &self.edges {
      if parent[a] == b || parent[b] == a {
        continue;
      }
      let (mut u, mut v) = (a, b);
      let mut left = vec![a];
      let mut right = vec![b];
      while u != v {
        work.charge(1)?;
        if depth[u] >= depth[v] {
          u = parent[u];
          left.push(u);
        } else {
          v = parent[v];
          right.push(v);
        }
      }
      right.pop();
      right.reverse();
      left.extend(right);
      work.charge(left.len())?;
      candidates.insert(normalize(&left));
    }
    Ok(())
  }
}

/// Returns a stable unordered pair key for an edge or an incident face pair.
fn pair(a: usize, b: usize) -> (usize, usize) {
  (a.min(b), a.max(b))
}

/// Canonicalizes traversal rotation and reversal, not chemical atom identities.
fn normalize(cycle: &[usize]) -> Vec<usize> {
  let start = cycle
    .iter()
    .enumerate()
    .min_by_key(|&(_, node)| node)
    .map_or(0, |(i, _)| i);
  let forward: Vec<_> = (0..cycle.len()).map(|i| cycle[(start + i) % cycle.len()]).collect();
  let backward: Vec<_> = (0..cycle.len())
    .map(|i| cycle[(start + cycle.len() - i) % cycle.len()])
    .collect();
  forward.min(backward)
}
