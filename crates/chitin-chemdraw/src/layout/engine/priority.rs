//! Local drawing priorities are separate from chemical stereochemical ranking.
//! Source attribution and redistribution terms are in licenses/coordinate-generation.txt.

use super::{Error, Work, topology::Model};
use std::collections::VecDeque;

/// Requests a cross-shaped drawing at P/S and densely tetracoordinated neighborhoods.
pub(super) fn cross(model: &Model, atom: usize) -> bool {
  matches!(model.atoms[atom].number, 15 | 16)
    || model.neighbors[atom]
      .iter()
      .filter(|&&(n, _)| model.neighbors[n].len() > 3)
      .count()
      > 2
}

/// Fusion-interior flags are geometry-generation facts, not chemical atom properties.
pub(super) struct Rules {
  inner: Vec<bool>,
}

impl Rules {
  /// Captures inner shared atoms before branch construction changes any coordinates.
  ///
  /// # Errors
  /// Ring-pair and neighbor inspections consume the shared work allowance.
  pub(super) fn new(model: &Model, work: &mut Work) -> Result<Self, Error> {
    let mut inner = vec![false; model.atoms.len()];
    for (i, a) in model.cycles.iter().enumerate() {
      for b in &model.cycles[i + 1..] {
        work.charge(a.len() + b.len())?;
        let shared: Vec<_> = a.iter().copied().filter(|n| b.contains(n)).collect();
        if shared.len() <= 2 {
          continue;
        }
        for atom in shared {
          work.charge(model.neighbors[atom].len().saturating_mul(a.len() + b.len()))?;
          if !model.neighbors[atom]
            .iter()
            .any(|&(n, _)| a.contains(&n) != b.contains(&n))
          {
            inner[atom] = true;
          }
        }
      }
    }
    Ok(Self { inner })
  }

  /// Places the two lowest-weight substituents in the source's outer slots.
  ///
  /// Reachable branch size dominates drawing preference. Bond multiplicity,
  /// fused interiors add explicit source tie-breakers. Input tetrahedral facts
  /// are not authored wedge constraints in this automatic-coordinate stage.
  /// This ordering does not assign CIP R/S or E/Z descriptors.
  pub(super) fn neighbors(&self, model: &Model, center: usize, work: &mut Work) -> Result<Vec<usize>, Error> {
    let original = &model.neighbors[center];
    if original.len() != 4 {
      return Ok(original.iter().map(|&(n, _)| n).collect());
    }
    let mut weighted = Vec::with_capacity(4);
    for &(neighbor, bond) in original {
      work.charge(model.atoms.len())?;
      let mut visited = vec![false; model.atoms.len()];
      visited[center] = true;
      visited[neighbor] = true;
      let mut queue = VecDeque::from([neighbor]);
      let mut count = 0;
      while let Some(atom) = queue.pop_front() {
        work.charge(1 + model.neighbors[atom].len())?;
        count += 1;
        for &(next, _) in &model.neighbors[atom] {
          if !visited[next] {
            visited[next] = true;
            queue.push_back(next);
          }
        }
      }
      let mut weight = count as f32;
      if model.edges[bond].order == 2 {
        weight -= 0.25;
        if model.atoms[center].number == 16 {
          weight += 2000.;
        }
      }
      if model
        .cycles
        .iter()
        .any(|ring| ring.contains(&center) && ring.contains(&neighbor))
      {
        weight += 500.;
      }
      match model.atoms[neighbor].number {
        6 => weight += 0.5,
        1 => weight -= 0.5,
        _ => {}
      }
      if self.inner[neighbor] && !self.inner[center] {
        weight -= 2000.;
      }
      if cross(model, center) && model.neighbors[neighbor].len() > 1 {
        weight += 200.;
      }
      if model.neighbors[neighbor]
        .iter()
        .any(|&(_, e)| model.edges[e].order == 2)
      {
        weight += 100.;
      }
      weighted.push((neighbor, weight));
    }
    let pop_smallest = |items: &mut Vec<(usize, f32)>| {
      let mut best = 0;
      for i in 1..items.len() {
        if items[i].1 < items[best].1 {
          best = i;
        }
      }
      items.remove(best).0
    };
    let first = pop_smallest(&mut weighted);
    let second = pop_smallest(&mut weighted);
    let mut result: Vec<_> = weighted.into_iter().map(|(n, _)| n).collect();
    if matches!(model.atoms[center].number, 15 | 16) {
      result.insert(1, second);
    } else {
      result.push(second);
    }
    result.insert(0, first);
    Ok(result)
  }
}
