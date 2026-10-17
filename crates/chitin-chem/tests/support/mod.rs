//! Test-only exact labelled-graph isomorphism; not a production chemical matcher.
//!
//! These small-fixture checks compare every Atom attribute and edge order,
//! ignoring graph-local IDs and undirected endpoint orientation. Exhaustive
//! backtracking is deliberately kept out of the runtime API and large stress
//! tests; this is not canonicalization, stereo perception, or H normalization.

use chitin_chem::{Atom, BondOrder, GraphError, MolecularGraph};

/// Small-graph comparison view with dense indices unrelated to either graph's IDs.
struct Fixture<'a> {
  /// Borrowed attributes compared exactly, including known-versus-unknown facts.
  atoms: Vec<&'a Atom>,
  /// Symmetric edge-label matrix; `None` records a nonedge, not an unknown bond.
  edges: Vec<Vec<Option<BondOrder>>>,
  /// Degree-based candidate filter, never sufficient by itself for equivalence.
  degrees: Vec<usize>,
}

impl<'a> Fixture<'a> {
  /// Builds an O(V²) adjacency matrix solely for small-fixture assertions.
  ///
  /// # Returns
  /// A borrowed fact table and owned symmetric matrix with all input edge labels.
  ///
  /// # Errors
  /// Returns `GraphError` for an edge whose endpoint is absent from the graph.
  /// Large traversal stress tests must not use this quadratic-storage helper.
  fn new(graph: &'a MolecularGraph) -> Result<Self, GraphError> {
    let rows: Vec<_> = graph.atoms().collect();
    let indices: std::collections::BTreeMap<_, _> = rows.iter().enumerate().map(|(i, (id, _))| (*id, i)).collect();
    let mut edges = vec![vec![None; rows.len()]; rows.len()];
    let mut degrees = vec![0; rows.len()];
    for (_, bond) in graph.bonds() {
      let [a, b] = bond.atoms();
      let a = *indices.get(&a).ok_or(GraphError::MissingAtom(a))?;
      let b = *indices.get(&b).ok_or(GraphError::MissingAtom(b))?;
      edges[a][b] = Some(bond.order());
      edges[b][a] = Some(bond.order());
      degrees[a] += 1;
      degrees[b] += 1;
    }
    Ok(Self {
      atoms: rows.into_iter().map(|(_, atom)| atom).collect(),
      edges,
      degrees,
    })
  }
}

/// Searches a bijection preserving all atomic attributes and edge/nonedge labels.
///
/// # Parameters
/// - `left`, `right`: independently numbered small fixtures; no matching ID or
///   iteration-position relationship is assumed between their vertices.
///
/// # Returns
/// Whether an exact labelled undirected isomorphism exists, including for empty
/// and disconnected graphs. This performs no H normalization or stereo inference.
/// Candidate filtering and edge checks prune an otherwise exponential iterative
/// backtracking search; it is not a production matching or canonicalization API.
///
/// # Errors
/// Returns topology-reference failures while building either fixture matrix.
pub fn isomorphic(left: &MolecularGraph, right: &MolecularGraph) -> Result<bool, GraphError> {
  if left.atom_count() != right.atom_count() || left.bond_count() != right.bond_count() {
    return Ok(false);
  }
  let left = Fixture::new(left)?;
  let right = Fixture::new(right)?;
  let size = left.atoms.len();
  let candidates: Vec<Vec<_>> = (0..size)
    .map(|a| {
      (0..size)
        .filter(|&b| left.atoms[a] == right.atoms[b] && left.degrees[a] == right.degrees[b])
        .collect()
    })
    .collect();
  let mut order: Vec<_> = (0..size).collect();
  // Most constrained vertices go first to reduce symmetric candidate branching;
  // degree and attributes only filter candidates, never prove equivalence.
  order.sort_by_key(|&a| (candidates[a].len(), std::cmp::Reverse(left.degrees[a])));
  let mut mapping = vec![None; size];
  let mut used = vec![false; size];
  let mut choices = vec![0; size];
  let mut depth = 0;
  // At depth d, mapping/used describe exactly the first d vertices of order.
  // Each choices[d] cursor survives backtracking so failed candidates are not
  // retried; releasing a target restores the bijection invariant before retry.
  loop {
    if depth == size {
      return Ok(true);
    }
    let a = order[depth];
    let mut found = None;
    while let Some(&b) = candidates[a].get(choices[depth]) {
      choices[depth] += 1;
      if !used[b]
        && mapping
          .iter()
          .enumerate()
          .all(|(other, mapped)| mapped.is_none_or(|target| left.edges[a][other] == right.edges[b][target]))
      {
        found = Some(b);
        break;
      }
    }
    if let Some(b) = found {
      mapping[a] = Some(b);
      used[b] = true;
      depth += 1;
    } else {
      choices[depth] = 0;
      if depth == 0 {
        return Ok(false);
      }
      depth -= 1;
      if let Some(b) = mapping[order[depth]].take() {
        used[b] = false;
      }
    }
  }
}
