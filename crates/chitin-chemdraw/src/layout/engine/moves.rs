//! Native discrete actions in normalized local drawing frames.
//! Source attribution and redistribution terms are in licenses/coordinate-generation.txt.

use super::{
  Error, Work,
  fragments::Forest,
  math::{BOND, Vector},
  search::Domain,
};
use std::collections::BTreeMap;

/// Fragment-local actions do not own or mutate chemical atoms or stereo labels.
#[derive(Clone, Debug)]
pub(super) enum Kind {
  Flip,
  ParentLength,
  Rotate,
  ScaleAtoms {
    pivot: usize,
    atoms: Vec<usize>,
  },
  InvertBond {
    pivot: usize,
    bound: usize,
    atoms: Vec<usize>,
  },
}

/// One source-ordered action, separate from its current search state.
pub(super) struct Move {
  pub fragment: usize,
  pub kind: Kind,
}

impl Move {
  pub(super) fn domain(&self) -> Domain {
    match &self.kind {
      Kind::Flip => Domain { states: 2, tier: 0 },
      Kind::ParentLength => Domain { states: 7, tier: 2 },
      Kind::Rotate => Domain { states: 5, tier: 3 },
      Kind::ScaleAtoms { .. } => Domain { states: 2, tier: 4 },
      Kind::InvertBond { .. } => Domain { states: 2, tier: 1 },
    }
  }

  pub(super) fn penalty(&self, state: u16) -> f32 {
    let rank = f32::from(state.div_ceil(2));
    match &self.kind {
      Kind::Flip => 0.,
      Kind::ParentLength => 200. * rank,
      Kind::Rotate => 400. * rank,
      Kind::ScaleAtoms { atoms, .. } => {
        if state == 0 {
          0.
        } else {
          50. * atoms.len() as f32
        }
      }
      Kind::InvertBond { .. } => {
        if state == 0 {
          0.
        } else {
          100.
        }
      }
    }
  }

  /// Applies a checked state to owned atoms and outgoing ghosts, not the parent ghost.
  pub(super) fn apply(&self, state: u16, frame: &mut BTreeMap<usize, Vector>, work: &mut Work) -> Result<(), Error> {
    if state >= self.domain().states {
      return Err(Error::CoordinateRange);
    }
    if state == 0 {
      return Ok(());
    }
    work.charge(match &self.kind {
      Kind::ScaleAtoms { atoms, .. } | Kind::InvertBond { atoms, .. } => atoms.len(),
      _ => frame.len(),
    })?;
    let rank = i32::from(state.div_ceil(2));
    match &self.kind {
      Kind::Flip => {
        for point in frame.values_mut() {
          point.y = -point.y;
        }
      }
      Kind::ParentLength => {
        let scale = 1.6_f64.powi(rank) as f32;
        let scale = if state.is_multiple_of(2) { 1. / scale } else { scale };
        let offset = BOND * (scale - 1.);
        for point in frame.values_mut() {
          point.x += offset;
        }
      }
      Kind::Rotate => {
        let angle = (std::f64::consts::PI / 180. * 15. * f64::from(rank)) as f32;
        let angle = if state.is_multiple_of(2) { -angle } else { angle };
        let origin = Vector::new(-BOND, 0.);
        for point in frame.values_mut() {
          *point = (((*point - origin).rotate(angle.sin(), angle.cos())) + origin).rounded();
        }
      }
      Kind::ScaleAtoms { pivot, atoms } => {
        validate_scope(frame, *pivot, atoms)?;
        for atom in atoms {
          let origin = frame[pivot];
          let point = frame.get_mut(atom).ok_or(Error::CoordinateRange)?;
          *point = ((*point - origin) * 0.4 + origin).rounded();
        }
      }
      Kind::InvertBond { pivot, bound, atoms } => {
        validate_scope(frame, *pivot, atoms)?;
        let origin = frame[pivot];
        let ray = *frame.get(bound).ok_or(Error::CoordinateRange)? - origin;
        let normal = Vector::new(ray.y, -ray.x);
        let a = origin + normal;
        let axis = (origin - normal) - a;
        if axis.square() <= super::math::EPSILON {
          return Err(Error::CoordinateRange);
        }
        for atom in atoms {
          let point = frame.get_mut(atom).ok_or(Error::CoordinateRange)?;
          let delta = *point - a;
          let parallel = (axis * delta.dot(axis)) / axis.square();
          *point = (a + parallel - (delta - parallel)).rounded();
        }
      }
    }
    Ok(())
  }
}

/// Checks every scope index before an action can mutate its scratch frame.
fn validate_scope(frame: &BTreeMap<usize, Vector>, pivot: usize, atoms: &[usize]) -> Result<(), Error> {
  if !frame.contains_key(&pivot) || atoms.iter().any(|atom| !frame.contains_key(atom)) {
    return Err(Error::CoordinateRange);
  }
  Ok(())
}

/// Enumerates reversed breadth-first fragments, retaining constructor action order.
pub(super) fn defaults(forest: &Forest) -> Vec<Move> {
  let mut result = Vec::new();
  for &fragment in forest.order.iter().rev() {
    if forest.fragments[fragment].parent.is_some() {
      result.push(Move {
        fragment,
        kind: Kind::Flip,
      });
    }
    result.push(Move {
      fragment,
      kind: Kind::ParentLength,
    });
    if forest.fragments[fragment].parent.is_some() {
      result.push(Move {
        fragment,
        kind: Kind::Rotate,
      });
    }
  }
  result
}
