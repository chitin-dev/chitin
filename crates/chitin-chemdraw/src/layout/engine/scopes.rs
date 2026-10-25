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
  /// Alignment-only endpoint excluded from the fragment's movable frame.
  incoming_parent: Option<usize>,
}

impl Scopes {
  /// Captures the alignment-only incoming endpoint before registering movable atoms.
  pub(super) fn new(atoms: usize, incoming_parent: Option<usize>) -> Self {
    Self {
      actions: Vec::new(),
      registrations: vec![Vec::new(); atoms],
      incoming_parent,
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
    // Incoming ghosts only establish alignment. Outgoing ghosts remain movable
    // so a local action can carry its child attachment into the rebuilt pose.
    if self.incoming_parent == Some(atom) {
      return;
    }
    match &mut self.actions[action].kind {
      Kind::ScaleAtoms { atoms, .. } | Kind::InvertBond { atoms, .. } => atoms.push(atom),
      _ => return,
    }
    self.registrations[atom].push(action);
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::layout::engine::math::Vector;
  use std::collections::BTreeMap;
  type TestResult = Result<(), Box<dyn std::error::Error>>;

  #[test]
  fn incoming_ghost_should_not_enter_or_propagate_a_movable_scope() -> TestResult {
    let mut scopes = Scopes::new(5, Some(0));
    scopes.push(
      1,
      Kind::ScaleAtoms {
        pivot: 1,
        atoms: Vec::new(),
      },
      2,
    );
    let mut work = Work::new(100);
    scopes.placed(2, 0, &mut work)?;
    scopes.placed(2, 3, &mut work)?;
    scopes.placed(0, 4, &mut work)?;
    let Kind::ScaleAtoms { atoms, .. } = &scopes.actions[0].kind else {
      return Err("unexpected move kind".into());
    };
    assert_eq!(atoms, &[2, 3]);
    assert!(scopes.registrations[0].is_empty());
    // Row 3 represents an outgoing child port, retained even though the incoming
    // alignment endpoint 0 is absent from the actual movable coordinate frame.
    let mut frame = BTreeMap::from([
      (1, Vector::default()),
      (2, Vector::new(50., 0.)),
      (3, Vector::new(100., 0.)),
    ]);
    scopes.actions[0].apply(1, &mut frame, &mut work)?;
    assert_eq!(frame[&1], Vector::default());
    assert_eq!(frame[&2], Vector::new(20., 0.));
    assert_eq!(frame[&3], Vector::new(40., 0.));
    assert_eq!(scopes.actions[0].penalty(1), 100.);
    Ok(())
  }

  #[test]
  fn unexcluded_registration_should_preserve_duplicate_member_semantics() -> TestResult {
    let mut scopes = Scopes::new(4, None);
    scopes.push(
      0,
      Kind::ScaleAtoms {
        pivot: 0,
        atoms: Vec::new(),
      },
      1,
    );
    let mut work = Work::new(100);
    scopes.placed(1, 2, &mut work)?;
    scopes.placed(1, 2, &mut work)?;
    let Kind::ScaleAtoms { atoms, .. } = &scopes.actions[0].kind else {
      return Err("unexpected move kind".into());
    };
    assert_eq!(atoms, &[1, 2, 2]);
    Ok(())
  }
}
