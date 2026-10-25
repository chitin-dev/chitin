//! Traversal-time action scopes, separate from coordinates and search states.
//! Source attribution and redistribution terms are in licenses/coordinate-generation.txt.

use super::{
  Error, Work,
  fragments::Forest,
  moves::{Kind, Move},
  topology::Model,
};

/// Registration preserves creation order and repeated visits to outgoing ghosts.
pub(super) struct Scopes {
  pub actions: Vec<Move>,
  registrations: Vec<Vec<usize>>,
}

impl Scopes {
  pub(super) fn new(atoms: usize) -> Self {
    Self {
      actions: Vec::new(),
      registrations: vec![Vec::new(); atoms],
    }
  }

  /// Extends existing scopes only when a neighbor is newly positioned.
  pub(super) fn placed(&mut self, node: usize, next: usize, work: &mut Work) -> Result<(), Error> {
    work.charge(self.registrations[node].len())?;
    // Snapshot the length, not the list: registration may append to a node's list.
    let count = self.registrations[node].len();
    for index in 0..count {
      let action = self.registrations[node][index];
      self.add_atom(action, next);
    }
    Ok(())
  }

  /// Creates local actions after positioning this center's neighbors.
  ///
  /// # Errors
  /// Ring membership scans and scope allocations consume the work allowance.
  pub(super) fn finish(
    &mut self,
    node: usize,
    fragment: usize,
    model: &Model,
    forest: &Forest,
    work: &mut Work,
  ) -> Result<(), Error> {
    work.charge(model.cycles.iter().map(Vec::len).sum())?;
    let rings: Vec<_> = model.cycles.iter().filter(|ring| ring.contains(&node)).collect();
    let macrocycle = rings.len() == 1 && rings[0].len() >= 9 && model.neighbors[node].len() == 3;
    // Stereo-bond assignment is not migrated yet. Do not introduce an inversion
    // at supplied stereo centers or across the incoming parent boundary.
    let protected = model.atoms[node].stereochemical
      || model.neighbors[node]
        .iter()
        .any(|&(n, _)| model.atoms[n].stereochemical)
      || model.stereo.iter().any(|(atoms, _)| atoms.contains(&node));
    for &(next, _) in &model.neighbors[node] {
      work.charge(rings.iter().map(|ring| ring.len()).sum())?;
      if rings.iter().any(|ring| ring.contains(&next)) {
        continue;
      }
      if macrocycle && !protected && !forest.fragments[fragment].parent.is_some_and(|p| p.parent_atom == next) {
        self.push(
          fragment,
          Kind::InvertBond {
            pivot: node,
            bound: next,
            atoms: Vec::new(),
          },
          next,
        );
      }
    }
    for &(next, _) in &model.neighbors[node] {
      work.charge(rings.iter().map(|ring| ring.len()).sum())?;
      if forest.owner[next] == fragment && !rings.iter().any(|ring| ring.contains(&next)) {
        self.push(
          fragment,
          Kind::ScaleAtoms {
            pivot: node,
            atoms: Vec::new(),
          },
          next,
        );
      }
    }
    Ok(())
  }

  fn push(&mut self, fragment: usize, kind: Kind, atom: usize) {
    let index = self.actions.len();
    self.actions.push(Move { fragment, kind });
    self.add_atom(index, atom);
  }

  fn add_atom(&mut self, action: usize, atom: usize) {
    match &mut self.actions[action].kind {
      Kind::ScaleAtoms { atoms, .. } | Kind::InvertBond { atoms, .. } => atoms.push(atom),
      _ => return,
    }
    self.registrations[atom].push(action);
  }
}
