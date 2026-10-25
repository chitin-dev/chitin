//! Drawing-local substituent ranking with ghost atoms and ancestry tie handling.
//! Source attribution and redistribution terms are in licenses/coordinate-generation.txt.
//!
//! This is a coordinate convention, not a public chemical stereodescriptor API.

use super::{Error, Work, topology::Model};
use std::{cmp::Ordering, collections::BTreeSet};

/// Same-depth neighborhood, including nonexpanding multiple-bond ghosts.
struct Branch {
  atoms: Vec<(u8, Option<usize>)>,
  parent: usize,
  ancestors: Vec<usize>,
}

/// Each competitor has independent visitation, ancestry scores and medals.
struct Traversal {
  frontier: Vec<Branch>,
  scores: Vec<usize>,
  medals: Vec<Vec<usize>>,
  visited: Vec<usize>,
}

/// Returns the preferred nonaxis neighbor, preserving unresolved branch ties.
///
/// # Errors
/// Every expansion, stable ordering and ancestry comparison is bounded.
pub(super) fn first_neighbor(
  model: &Model,
  atom: usize,
  opposite: usize,
  work: &mut Work,
) -> Result<Option<usize>, Error> {
  let neighbors: Vec<_> = model.neighbors[atom]
    .iter()
    .filter_map(|&(a, _)| (a != opposite).then_some(a))
    .collect();
  match neighbors.as_slice() {
    [only] => Ok(Some(*only)),
    [a, b] => prefer(model, *a, *b, atom, work),
    _ => Ok(None),
  }
}

/// Compares detached branch traversals without assigning chemical stereo facts.
pub(super) fn prefer(
  model: &Model,
  a: usize,
  b: usize,
  center: usize,
  work: &mut Work,
) -> Result<Option<usize>, Error> {
  match model.atoms[a].number.cmp(&model.atoms[b].number) {
    Ordering::Greater => return Ok(Some(a)),
    Ordering::Less => return Ok(Some(b)),
    Ordering::Equal => {}
  }
  work.charge(6 * model.atoms.len())?;
  let mut left = Traversal::new(model, a, center);
  let mut right = Traversal::new(model, b, center);
  while !left.frontier.is_empty() || !right.frontier.is_empty() {
    left.order(model, work)?;
    right.order(model, work)?;
    for (l, r) in left.frontier.iter().zip(&right.frontier) {
      work.charge(l.ancestors.len() + l.atoms.len() + r.atoms.len())?;
      match compare(l, &left, r, &right, model, None) {
        Ordering::Less => return Ok(Some(a)),
        Ordering::Greater => return Ok(Some(b)),
        Ordering::Equal => {}
      }
    }
    match left.frontier.len().cmp(&right.frontier.len()) {
      Ordering::Greater => return Ok(Some(a)),
      Ordering::Less => return Ok(Some(b)),
      Ordering::Equal => {}
    }
    left.expand(model, work)?;
    right.expand(model, work)?;
  }
  Ok(None)
}

impl Traversal {
  fn new(model: &Model, atom: usize, center: usize) -> Self {
    let n = model.atoms.len();
    let mut visited = vec![0; n];
    visited[center] = 1;
    visited[atom] = 2;
    Self {
      frontier: vec![Branch {
        atoms: vec![(model.atoms[atom].number, Some(atom))],
        parent: center,
        ancestors: vec![center, atom],
      }],
      scores: vec![0; n],
      medals: vec![Vec::new(); n],
      visited,
    }
  }

  fn order(&mut self, model: &Model, work: &mut Work) -> Result<(), Error> {
    let mut frontier = std::mem::take(&mut self.frontier);
    let n = frontier.len();
    let depth = frontier.first().map_or(0, |branch| branch.ancestors.len());
    work.charge(n.saturating_mul(n).saturating_mul(depth + 5))?;
    frontier.sort_by(|a, b| compare(a, self, b, self, model, None));
    let equal = equal_neighbors(&frontier, self, model);
    let mut level = 0;
    for (i, branch) in frontier.iter().enumerate() {
      if equal[i] {
        level -= 1;
      }
      for &ancestor in &branch.ancestors {
        let medals = &mut self.medals[ancestor];
        medals.resize(medals.len().max(level + 1), 0);
        medals[level] += 1;
      }
      level += 1;
    }
    work.charge(model.atoms.len() + n * depth)?;
    // Indexed bit sets preserve precedence without undefined wide bit shifts.
    let mut friends = vec![BTreeSet::new(); model.atoms.len()];
    let mut selected = Vec::with_capacity(n);
    while !frontier.is_empty() {
      let mut best = 0;
      for i in 1..frontier.len() {
        if compare(&frontier[i], self, &frontier[best], self, model, Some(&friends)) == Ordering::Less {
          best = i;
        }
      }
      let branch = frontier.remove(best);
      for &ancestor in &branch.ancestors {
        friends[ancestor].insert(frontier.len());
      }
      selected.push(branch);
    }
    let equal = equal_neighbors(&selected, self, model);
    self.scores.fill(0);
    let mut score = 1;
    for (i, branch) in selected.iter().enumerate() {
      if equal[i] {
        score -= 1;
      }
      for &ancestor in &branch.ancestors {
        if self.scores[ancestor] == 0 {
          self.scores[ancestor] = score;
        }
      }
      score += 1;
    }
    for medals in &mut self.medals {
      medals.clear();
    }
    self.frontier = selected;
    Ok(())
  }

  fn expand(&mut self, model: &Model, work: &mut Work) -> Result<(), Error> {
    let mut next = Vec::new();
    for branch in &self.frontier {
      for &(_, atom) in &branch.atoms {
        let Some(atom) = atom else {
          continue;
        };
        work.charge(
          model.neighbors[atom].len() * 3 + branch.ancestors.len() + usize::from(model.atoms[atom].hydrogens),
        )?;
        let mut ancestors = branch.ancestors.clone();
        ancestors.push(atom);
        let mut atoms = Vec::new();
        for &(neighbor, edge) in &model.neighbors[atom] {
          let number = model.atoms[neighbor].number;
          if neighbor != branch.parent {
            let ghost =
              number == 1 || (self.visited[neighbor] != 0 && self.visited[neighbor] != self.visited[atom] + 1);
            atoms.push((number, (!ghost).then_some(neighbor)));
            if !ghost {
              self.visited[neighbor] = self.visited[atom] + 1;
            }
          }
          for _ in 1..model.edges[edge].order {
            atoms.push((number, None));
          }
        }
        atoms.extend(std::iter::repeat_n((1, None), usize::from(model.atoms[atom].hydrogens)));
        atoms.sort();
        atoms.reverse();
        next.push(Branch {
          atoms,
          parent: atom,
          ancestors,
        });
      }
    }
    self.frontier = next;
    Ok(())
  }
}

/// Group equality excludes current-round medals, unlike priority comparison.
fn equal_neighbors(frontier: &[Branch], state: &Traversal, model: &Model) -> Vec<bool> {
  std::iter::once(false)
    .take(frontier.len())
    .chain(frontier.windows(2).map(|pair| {
      let a = &pair[0];
      let b = &pair[1];
      a.ancestors
        .iter()
        .zip(&b.ancestors)
        .all(|(&a, &b)| model.atoms[a].number == model.atoms[b].number && state.scores[a] == state.scores[b])
        && a
          .atoms
          .iter()
          .map(|(number, _)| number)
          .eq(b.atoms.iter().map(|(number, _)| number))
    }))
    .collect()
}

/// Better branches sort first; friendship precedence is local to one traversal.
fn compare(
  a: &Branch,
  left: &Traversal,
  b: &Branch,
  right: &Traversal,
  model: &Model,
  friends: Option<&[BTreeSet<usize>]>,
) -> Ordering {
  for (&a, &b) in a.ancestors.iter().zip(&b.ancestors) {
    let friendship = friends.map_or(Ordering::Equal, |masks| {
      masks[b].iter().rev().cmp(masks[a].iter().rev())
    });
    let order = friendship
      .then_with(|| model.atoms[b].number.cmp(&model.atoms[a].number))
      .then_with(|| left.scores[a].cmp(&right.scores[b]))
      .then_with(|| right.medals[b].cmp(&left.medals[a]));
    if order != Ordering::Equal {
      return order;
    }
  }
  b.atoms
    .iter()
    .map(|(number, _)| number)
    .cmp(a.atoms.iter().map(|(number, _)| number))
}
