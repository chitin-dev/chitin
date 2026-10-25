//! Discrete fragment search precedes continuous cleanup; neither owns source chemistry.
//! Source attribution and redistribution terms are in licenses/coordinate-generation.txt.

use crate::layout::{NativeFailureKind as Failure, NativeStage as Stage};

use super::{
  Error, Work, assembly::Assembly, clashes, constraints, evaluation::Evaluator, fragments::Forest,
  interactions::Interaction, math::Vector, search, solver, topology::Model,
};

/// Completed geometry carries optional discrete-search provenance separately.
pub(super) struct Refined {
  pub points: Vec<Vector>,
  pub limited_search: Option<usize>,
}

/// Temporary cut graphs search discrete states but defer force integration.
#[derive(Clone, Copy)]
pub(super) enum Relaxation<'a> {
  Deferred { closures: &'a [Interaction] },
  Complete { required: bool },
}

/// Scores the geometry-dependent terms without evaluating stretch/bend forces.
#[cfg(test)]
pub(super) fn discrete(
  model: &Model,
  forest: &Forest,
  points: &[Vector],
  terms: &[Interaction],
  movable: &[bool],
  work: &mut Work,
) -> Result<f32, Error> {
  let terms = discrete_terms(forest, terms, movable, work)?;
  let mut scratch = vec![Vector::default(); points.len()];
  let initial = clashes::score(model, forest, points, work)?;
  Evaluator::new(&terms, work)?.score_from(initial, points, &mut scratch, work)
}

/// Resolves immutable fragment incidence once, not for every search candidate.
fn discrete_terms(
  forest: &Forest,
  terms: &[Interaction],
  movable: &[bool],
  work: &mut Work,
) -> Result<Vec<Interaction>, Error> {
  let mut result = Vec::new();
  for &term in terms {
    work.charge(1)?;
    let include = match term {
      Interaction::Clash { atoms: [a, p, b], .. } => {
        movable[a]
          || movable[p]
          || movable[b]
          || (forest.owner[p] != forest.owner[a] && forest.owner[p] != forest.owner[b])
      }
      Interaction::Alkene { .. } => true,
      _ => false,
    };
    if include {
      result.push(term);
    }
  }
  Ok(result)
}

/// Optimizes the complete assembly, retaining the initial state as a search champion.
///
/// # Returns
/// Fully evaluated coordinates. Optional search consumes at most half the
/// remaining work; a budget-limited search still returns its best complete state.
/// Scoped actions are recorded during local traversal, not inferred from cut bonds.
///
/// # Errors
/// Native geometry/scoring errors and required cleanup budget exhaustion propagate.
pub(super) fn run(
  model: &Model,
  forest: &Forest,
  assembly: &Assembly,
  rigid: &[bool],
  required: bool,
  work: &mut Work,
) -> Result<Refined, Error> {
  run_with_policy(model, forest, assembly, rigid, Relaxation::Complete { required }, work)
}

/// Searches restored closure springs without integrating temporary cut graphs.
///
/// # Errors
/// Additional springs share the finite scoring allowance. Force integration is
/// permitted only by the complete-topology policy and uses the same allowance.
pub(super) fn run_with_policy(
  model: &Model,
  forest: &Forest,
  assembly: &Assembly,
  rigid: &[bool],
  relaxation: Relaxation<'_>,
  work: &mut Work,
) -> Result<Refined, Error> {
  let mut points = assembly.points.clone();
  let extra = match relaxation {
    Relaxation::Deferred { closures } => closures,
    Relaxation::Complete { .. } => &[],
  };
  let mut limited_search = None;
  let terms = constraints::build(model, &points, rigid, work)?;
  let terms = discrete_terms(forest, &terms, &assembly.movable, work)?;
  let evaluator = Evaluator::new(&terms, work)?;
  let peptide = super::backbone::torsions(model, work)?;
  let score_pose = |points: &[Vector], work: &mut Work| -> Result<f32, Error> {
    work.charge(extra.len() + peptide.len() + points.len())?;
    let mut forces = vec![Vector::default(); points.len()];
    let initial = clashes::score(model, forest, points, work)?;
    Ok(
      evaluator.score_from(initial, points, &mut forces, work)?
        + peptide
          .iter()
          .chain(extra)
          .map(|term| term.score(points, &mut forces))
          .sum::<f32>(),
    )
  };
  let mut score = score_pose(&points, work)?;
  if score >= 10. {
    let actions = &assembly.actions;
    let domains: Vec<_> = actions.iter().map(|action| action.domain()).collect();
    let constant = forest
      .fragments
      .iter()
      .filter(|f| f.chain && f.parent.is_some_and(|parent| forest.fragments[parent.fragment].chain))
      .count() as f32
      * 10.;
    let reserve = work.remaining() / 2;
    let best = work.optional(reserve, |work| {
      search::run(&domains, 10_001, work, |states, work| {
        let candidate = assembly.rebuild(model, forest, actions, states, work)?;
        let penalty = actions
          .iter()
          .zip(states)
          .map(|(action, &state)| action.penalty(state))
          .sum::<f32>();
        Ok(score_pose(&candidate, work)? + penalty + constant)
      })
    })?;
    if let Some(best) = best {
      if best.exhausted {
        limited_search = Some(best.evaluations);
      }
      // Search state zero and the submitted assembly must use identical frames.
      // Re-evaluate before admission so roundoff cannot publish a worse pose.
      if best.states.iter().any(|&state| state != 0) && best.score <= score + constant {
        let candidate = assembly.rebuild(model, forest, actions, &best.states, work)?;
        let candidate_score = score_pose(&candidate, work)?;
        if candidate_score < score {
          points = candidate;
          score = candidate_score;
        }
      }
    }
  }
  let parity = model
    .cycles
    .iter()
    .any(|ring| ring.len() >= 9 && !ring.len().is_multiple_of(2));
  let required = matches!(relaxation, Relaxation::Complete { required: true });
  // Terminal shortening is a solver seed, never a complete rollback state.
  let before = points.clone();
  if score >= 10. {
    super::cleanup::terminals(model, &mut points, work)?;
  }
  if matches!(relaxation, Relaxation::Complete { .. }) && (score >= 10. || parity || required) {
    // Required force setup replaces search-only backbone/closure springs.
    let terms = constraints::build(model, &points, rigid, work)?;
    let fixed = vec![false; points.len()];
    work.charge(points.len())?;
    let preserve_stereo = super::stereo::supplied_relations_satisfied(model, &before);
    let outcome = solver::minimize(&mut points, &fixed, &terms, 1000, work)?;
    if outcome.iterations > 0 && !outcome.minimum_energy.is_finite() {
      return Err(Error::native(Stage::Simulation, Failure::NonFiniteEnergy));
    }
    if preserve_stereo && !super::stereo::supplied_relations_satisfied(model, &points) {
      points = before;
    }
  }
  Ok(Refined { points, limited_search })
}
