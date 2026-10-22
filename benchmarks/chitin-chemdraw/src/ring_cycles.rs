//! Bounded short-first cycle basis for auditable angle paths, not chemical SSSR.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// One allowance covers BFS, cycle storage and binary elimination across blocks.
pub(super) struct Budget(pub usize);

impl Budget {
  /// Charges before allocating or traversing; no partial basis is published.
  fn charge(&mut self, units: usize) -> Option<()> {
    self.0 = self.0.checked_sub(units)?;
    Some(())
  }
}

/// Chooses independent short cycles, retaining tree cycles for completeness.
///
/// Source ordinals determine BFS tie breaks, not the current drawing. Thus a
/// crossing layout and its repaired version use the same observation domain.
/// A shortest alternate path per edge is not an exhaustive minimum-cycle-basis
/// algorithm; fundamental cycles guarantee cycle-space coverage. `None` means
/// the budget was exhausted, never that the block contains no macrocycle.
pub(super) fn basis(
  atoms: &[usize],
  edges: &[[usize; 2]],
  rank: usize,
  budget: &mut Budget,
) -> Option<Vec<Vec<usize>>> {
  let n = atoms.iter().copied().max()? + 1;
  budget.charge(n + edges.len())?;
  let mut rows = vec![Vec::new(); n];
  for (edge, &[a, b]) in edges.iter().enumerate() {
    rows[a].push((b, edge));
    rows[b].push((a, edge));
  }
  for row in &mut rows {
    row.sort_unstable();
  }
  let mut candidates = BTreeSet::new();
  for (excluded, &[start, end]) in edges.iter().enumerate() {
    budget.charge(n)?;
    let mut parent = vec![usize::MAX; n];
    parent[start] = start;
    let mut queue = VecDeque::from([start]);
    while let Some(node) = queue.pop_front() {
      budget.charge(1 + rows[node].len())?;
      if node == end {
        break;
      }
      for &(other, edge) in &rows[node] {
        if edge != excluded && parent[other] == usize::MAX {
          parent[other] = node;
          queue.push_back(other);
        }
      }
    }
    if parent[end] == usize::MAX {
      return None;
    }
    let mut path = vec![end];
    while *path.last()? != start {
      budget.charge(1)?;
      path.push(parent[*path.last()?]);
    }
    candidates.insert(canonical(&path));
  }
  budget.charge(n)?;
  let mut parent = vec![usize::MAX; n];
  let mut depth = vec![0; n];
  parent[atoms[0]] = atoms[0];
  let mut queue = VecDeque::from([atoms[0]]);
  while let Some(node) = queue.pop_front() {
    budget.charge(1 + rows[node].len())?;
    for &(other, _) in &rows[node] {
      if parent[other] == usize::MAX {
        parent[other] = node;
        depth[other] = depth[node] + 1;
        queue.push_back(other);
      }
    }
  }
  for &[a, b] in edges {
    if parent[a] == b || parent[b] == a {
      continue;
    }
    let (mut a, mut b) = (a, b);
    let (mut left, mut right) = (vec![a], vec![b]);
    while a != b {
      budget.charge(1)?;
      if depth[a] >= depth[b] {
        a = parent[a];
        left.push(a);
      } else {
        b = parent[b];
        right.push(b);
      }
    }
    right.pop();
    left.extend(right.into_iter().rev());
    candidates.insert(canonical(&left));
  }
  let mut candidates: Vec<_> = candidates.into_iter().collect();
  candidates.sort_by(|a, b| a.len().cmp(&b.len()).then(a.cmp(b)));
  let lookup: BTreeMap<_, _> = edges
    .iter()
    .enumerate()
    .map(|(i, &[a, b])| ((a.min(b), a.max(b)), i))
    .collect();
  let width = edges.len().div_ceil(64);
  let mut pivots = BTreeMap::<usize, Vec<u64>>::new();
  let mut selected = Vec::new();
  for cycle in candidates {
    budget.charge(width + cycle.len())?;
    let mut bits = vec![0_u64; width];
    for (&a, &b) in cycle.iter().zip(cycle.iter().cycle().skip(1)) {
      let edge = lookup[&(a.min(b), a.max(b))];
      bits[edge / 64] ^= 1 << (edge % 64);
    }
    loop {
      budget.charge(width)?;
      let Some(word) = bits.iter().rposition(|&v| v != 0) else {
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
      return Some(selected);
    }
  }
  None
}

/// Selects one traversal independent of geometric rotation or cycle starting point.
fn canonical(path: &[usize]) -> Vec<usize> {
  let start = path
    .iter()
    .enumerate()
    .min_by_key(|(_, node)| *node)
    .map_or(0, |(i, _)| i);
  let n = path.len();
  let forward: Vec<_> = (0..n).map(|i| path[(start + i) % n]).collect();
  let reverse: Vec<_> = (0..n).map(|i| path[(start + n - i) % n]).collect();
  forward.min(reverse)
}
