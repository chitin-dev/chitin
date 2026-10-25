//! Connectivity refinement and stable breadth-first source ordering.
//! Source attribution and redistribution terms are in licenses/coordinate-generation.txt.

use super::{Error, Work, topology::Model};
use std::collections::VecDeque;

/// Computes atom/edge permutations without reversing edges or changing source IDs.
///
/// # Returns
/// Source-compatible cumulative connectivity scores followed by descending-score
/// breadth-first ordering. Equal scores retain the original traversal order.
/// Bond multiplicity does not enter the connectivity refinement.
///
/// # Errors
/// Work exhaustion rejects incomplete ordering. Fixed-width score arithmetic is
/// explicit and portable, including values beyond the signed integer boundary.
pub(super) fn order(model: &Model, work: &mut Work) -> Result<(Vec<usize>, Vec<usize>), Error> {
  let n = model.atoms.len();
  work.charge(4 * n + model.edges.len())?;
  let ends: Vec<_> = model.edges.iter().map(|edge| edge.ends).collect();
  let (mut scores, _) = connectivity(n, &ends, work)?;
  for (score, atom) in scores.iter_mut().zip(&model.atoms) {
    *score = score.wrapping_mul(100).wrapping_add(i32::from(atom.number));
  }
  let mut seen = vec![false; n];
  let mut edges_seen = vec![false; model.edges.len()];
  let mut atoms = Vec::new();
  let mut edges = Vec::new();
  while atoms.len() < n {
    work.charge(n)?;
    let mut start = None;
    for i in 0..n {
      if !seen[i] && start.is_none_or(|old| scores[i] > scores[old]) {
        start = Some(i);
      }
    }
    let Some(start) = start else {
      break;
    };
    seen[start] = true;
    let mut pending = VecDeque::from([start]);
    while let Some(node) = pending.pop_front() {
      atoms.push(node);
      let mut row = model.neighbors[node].clone();
      work.charge(row.len() * (1 + row.len().checked_ilog2().unwrap_or(0) as usize))?;
      row.sort_by(|a, b| scores[b.0].cmp(&scores[a.0]));
      for (other, edge) in row {
        if edges_seen[edge] {
          continue;
        }
        if !seen[other] {
          seen[other] = true;
          pending.push_back(other);
        }
        edges_seen[edge] = true;
        edges.push(edge);
      }
    }
  }
  Ok((atoms, edges))
}

/// Retains the cumulative connectivity refinement used by template identity tests.
pub(super) fn connectivity(n: usize, edges: &[[usize; 2]], work: &mut Work) -> Result<(Vec<i32>, usize), Error> {
  let mut scores = vec![1_i32; n];
  let mut next = vec![0_i32; n];
  let mut ties = n;
  let mut count = 0;
  if n < 2 {
    return Ok((scores, count));
  }
  loop {
    work.charge(edges.len() + n * (1 + n.checked_ilog2().unwrap_or(0) as usize))?;
    count += 1;
    for &[a, b] in edges {
      next[a] = next[a].wrapping_add(scores[b]);
      next[b] = next[b].wrapping_add(scores[a]);
    }
    let mut sorted = next.clone();
    sorted.sort_unstable();
    let current = sorted.windows(2).filter(|p| p[0] == p[1]).count();
    if current >= ties {
      break;
    }
    ties = current;
    scores.clone_from(&next);
  }
  Ok((scores, count))
}
