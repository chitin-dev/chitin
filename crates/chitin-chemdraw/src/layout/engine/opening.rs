//! Detached ring opening, native fragment reconstruction and closure restoration.
//! Source attribution and redistribution terms are in licenses/coordinate-generation.txt.

use crate::layout::{NativeFailureKind as Failure, NativeStage as Stage};

use super::{
  Error, Work, assembly,
  fragments::Forest,
  interactions::Interaction,
  math::{BOND, Vector},
  refinement, rings,
  topology::Model,
};
use std::collections::BTreeMap;

/// Complete reconstructed geometry, mapped back to the original dense namespace.
pub(super) struct Opened {
  pub points: Vec<Vector>,
  pub rigid: Vec<bool>,
}

/// Opens one admissible source-ordered ring edge and rebuilds without old seeds.
///
/// # Parameters
/// - `model`, `ring`: immutable source snapshot and chosen layout face.
/// - `depth`: temporary opening depth; every recursion removes one cyclic edge.
/// - `work`: shared topology, assembly, search and closure allowance.
///
/// # Returns
/// Complete restored-topology coordinates, or `None` if the ring has no eligible
/// edge. All temporary atom/edge edits remain detached from the chemical graph.
///
/// # Errors
/// Exceeding the bounded opening depth, losing source identities or exhausting
/// required reconstruction work rejects the whole proposal.
pub(super) fn generate(model: &Model, ring: usize, depth: usize, work: &mut Work) -> Result<Option<Opened>, Error> {
  if depth >= 64 {
    return Err(Error::RingSearchBudgetExceeded);
  }
  let Some(edge) = choose_edge(model, ring, work)? else {
    return Ok(None);
  };
  let mut temporary = model.opened(edge, work)?;
  let forest = Forest::new(&temporary, work)?;
  let mut points = vec![Vector::default(); temporary.atoms.len()];
  let seeded = rings::seed_with_policy(&temporary, &forest, &mut points, true, depth + 1, work)?;
  if temporary.cycles.iter().flatten().any(|&atom| !seeded.rebuilt[atom]) {
    return Ok(None);
  }
  let assembly = assembly::prepare(&temporary, &forest, &points, work)?;
  temporary.restore(&model.edges[edge], model, work)?;
  let restored = temporary
    .edges
    .last()
    .ok_or(Error::native(Stage::RingOpening, Failure::MissingClosureEdge))?;
  let closure = Interaction::Stretch {
    atoms: restored.ends,
    target: BOND,
    tolerance: 0.,
    k: 1.,
  };
  let result = refinement::run_with_policy(
    &temporary,
    &forest,
    &assembly,
    &seeded.rigid,
    refinement::Relaxation::Deferred { closures: &[closure] },
    work,
  )?;
  work.charge(temporary.atoms.len())?;
  let indices: BTreeMap<_, _> = temporary
    .atoms
    .iter()
    .enumerate()
    .map(|(i, atom)| (atom.id, i))
    .collect();
  Ok(Some(Opened {
    points: model
      .atoms
      .iter()
      .map(|atom| result.points[indices[&atom.id]])
      .collect(),
    rigid: model.atoms.iter().map(|atom| seeded.rigid[indices[&atom.id]]).collect(),
  }))
}

/// Stable first-best cuts prefer less fused, less substituted ring connections.
pub(super) fn choose_edge(model: &Model, ring: usize, work: &mut Work) -> Result<Option<usize>, Error> {
  let macrocycle = model.cycles[ring].len() >= 9;
  let mut best = None;
  for &edge in &model.cycle_edges[ring] {
    let bond = &model.edges[edge];
    let [a, b] = bond.ends;
    work.charge(
      model.cycle_edges.iter().map(Vec::len).sum::<usize>() + model.neighbors[a].len() + model.neighbors[b].len(),
    )?;
    if macrocycle {
      if bond.order != 1 {
        continue;
      }
      let next_to_stereo = [a, b].iter().any(|&atom| {
        model.neighbors[atom].iter().any(|&(_, e)| {
          model.edges[e].order == 2
            && !model
              .cycle_edges
              .iter()
              .enumerate()
              .any(|(r, edges)| model.cycles[r].len() < 9 && edges.contains(&e))
        })
      });
      if next_to_stereo {
        continue;
      }
    }
    let score = model.cycle_edges.iter().filter(|edges| edges.contains(&edge)).count() * 10
      + model.neighbors[a].len()
      + model.neighbors[b].len();
    if best.is_none_or(|(_, previous)| score < previous) {
      best = Some((edge, score));
    }
  }
  Ok(best.map(|(edge, _)| edge))
}
