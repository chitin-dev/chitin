//! Source-compatible directed-edge breadth-first ring discovery for layout only.
//! Source attribution and redistribution terms are in licenses/coordinate-generation.txt.

use super::{Error, Work, topology::Model};
use std::collections::{BTreeSet, VecDeque};

/// Cyclic atom traversal and closing BFS edge order serve different consumers.
pub(super) struct Cycle {
  pub atoms: Vec<usize>,
  pub edges: Vec<usize>,
}

/// Discovers one closed edge walk per source edge and deduplicates edge sets.
///
/// # Returns
/// Layout cycles in discovery order, not a minimum or independent chemical ring
/// basis. Saturated cages can retain more faces than the independent cycle rank.
///
/// # Errors
/// Work exhaustion or invalid face ordering rejects partial ring discovery.
pub(super) fn discover(model: &Model, work: &mut Work) -> Result<Vec<Cycle>, Error> {
  let mut result = Vec::new();
  let mut unique = BTreeSet::new();
  for root in 0..model.edges.len() {
    work.charge(3 * model.edges.len())?;
    let mut seen = vec![false; model.edges.len()];
    let mut parent = vec![None; model.edges.len()];
    let mut at_start = vec![true; model.edges.len()];
    seen[root] = true;
    let mut pending = VecDeque::from([root]);
    let mut closed = None;
    while let Some(edge) = pending.pop_front() {
      let pivot = model.edges[edge].ends[usize::from(at_start[edge])];
      work.charge(1 + model.neighbors[pivot].len())?;
      for &(_, next) in &model.neighbors[pivot] {
        if next == edge {
          continue;
        }
        if seen[next] {
          if next == root {
            closed = Some(edge);
          }
        } else {
          at_start[next] = model.edges[next].ends[1] != pivot;
          parent[next] = Some(edge);
          seen[next] = true;
          pending.push_back(next);
        }
      }
      if closed.is_some() {
        break;
      }
    }
    let Some(mut current) = closed else {
      continue;
    };
    let mut edges = Vec::new();
    loop {
      work.charge(1)?;
      edges.push(current);
      if let Some(previous) = parent[current] {
        current = previous;
      } else {
        break;
      }
    }
    let ordered_edges = edges.clone();
    edges.sort_unstable();
    if !unique.insert(edges.clone()) {
      continue;
    }
    let members: BTreeSet<_> = edges.iter().flat_map(|&e| model.edges[e].ends).collect();
    let Some(&start) = members.first() else {
      continue;
    };
    let mut ordered = vec![start];
    let mut used = BTreeSet::from([start]);
    while ordered.len() < members.len() {
      let last = ordered[ordered.len() - 1];
      work.charge(1 + model.neighbors[last].len())?;
      let next = model.neighbors[last]
        .iter()
        .map(|&(n, _)| n)
        .find(|n| members.contains(n) && !used.contains(n))
        .ok_or(Error::Invariant(model.atoms[last].id))?;
      used.insert(next);
      ordered.push(next);
    }
    if !model.neighbors[ordered[ordered.len() - 1]]
      .iter()
      .any(|&(node, _)| node == start)
    {
      return Err(Error::Invariant(model.atoms[start].id));
    }
    result.push(Cycle {
      atoms: ordered,
      edges: ordered_edges,
    });
  }
  Ok(result)
}
