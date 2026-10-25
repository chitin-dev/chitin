//! Test-only exact labelled-graph isomorphism; not a production chemical matcher.
//!
//! These small-fixture checks compare every Atom attribute and edge order,
//! ignoring graph-local IDs and undirected endpoint orientation. Exhaustive
//! backtracking is deliberately kept out of the runtime API and large stress
//! tests; this is not canonicalization, stereo perception, or H normalization.

use chitin_chem::{Atom, BondOrder, GraphError, MolecularGraph};

/// Small-graph comparison view with dense indices unrelated to either graph's IDs.
struct Fixture<'a> {
  /// Original graph for relative stereo comparison after a full bijection.
  graph: &'a MolecularGraph,
  /// Dense-to-stable identity map, scoped only to this test fixture.
  ids: Vec<chitin_chem::AtomId>,
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
      graph,
      ids: rows.iter().map(|(id, _)| *id).collect(),
      atoms: rows.into_iter().map(|(_, atom)| atom).collect(),
      edges,
      degrees,
    })
  }

  /// Checks relative winding only after a complete candidate bijection exists.
  fn stereo_matches(&self, other: &Self, mapping: &[Option<usize>]) -> bool {
    if !self.double_stereo_matches(other, mapping) {
      return false;
    }
    self.ids.iter().enumerate().all(|(index, &id)| {
      let Some(target) = mapping[index] else {
        return false;
      };
      match (self.graph.tetrahedral(id), other.graph.tetrahedral(other.ids[target])) {
        (None, None) => true,
        (Some(a), Some(b)) => {
          let mapped: Option<Vec<_>> = a
            .ligands()
            .iter()
            .map(|ligand| match ligand {
              chitin_chem::StereoLigand::Hydrogen => Some(chitin_chem::StereoLigand::Hydrogen),
              chitin_chem::StereoLigand::LonePair => Some(chitin_chem::StereoLigand::LonePair),
              chitin_chem::StereoLigand::Atom(id) => {
                let row = self.ids.iter().position(|candidate| candidate == id)?;
                Some(chitin_chem::StereoLigand::Atom(other.ids[mapping[row]?]))
              }
            })
            .collect();
          mapped
            .and_then(|items| items.try_into().ok())
            .and_then(|ligands| chitin_chem::TetrahedralStereo::new(ligands, a.winding()).ok())
            .and_then(|stereo| stereo.reordered(*b.ligands()).ok())
            .is_some_and(|stereo| stereo.winding() == b.winding())
        }
        _ => false,
      }
    })
  }

  /// Compares relative double-bond facts under endpoint and reference changes.
  fn double_stereo_matches(&self, other: &Self, mapping: &[Option<usize>]) -> bool {
    let mapped = |id| {
      let index = self.ids.iter().position(|&candidate| candidate == id)?;
      Some(other.ids[mapping[index]?])
    };
    self.graph.bonds().all(|(id, bond)| {
      let [Some(a), Some(b)] = bond.atoms().map(mapped) else {
        return false;
      };
      let Some(edge) = other.graph.bond_between(a, b) else {
        return false;
      };
      match (self.graph.double_bond_stereo(id), other.graph.double_bond_stereo(edge)) {
        (None, None) => true,
        (Some(left), Some(right)) => {
          let [Some(a), Some(_)] = left.ends().map(mapped) else {
            return false;
          };
          let [Some(ra), Some(rb)] = left.references().map(mapped) else {
            return false;
          };
          let references = if right.ends()[0] == a {
            right.references()
          } else {
            let [a, b] = right.references();
            [b, a]
          };
          let odd = (ra != references[0]) ^ (rb != references[1]);
          left.relation()
            == if odd {
              right.relation().inverted()
            } else {
              right.relation()
            }
        }
        _ => false,
      }
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
      if left.stereo_matches(&right, &mapping) {
        return Ok(true);
      }
      depth -= 1;
      if let Some(b) = mapping[order[depth]].take() {
        used[b] = false;
      }
      continue;
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
