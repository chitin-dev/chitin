//! Projected descent with backtracking and best-complete-result acceptance.

use super::{OptimizationOptions, Outcome, constraints::Model, objective};
use crate::{
  geometry::Point,
  layout::quality::{Budget, Quality},
};

/// Searches private projected states, publishing only a verified improvement.
///
/// # Parameters
/// - `model`, `points`: immutable constraints and a complete component snapshot.
/// - `initial_quality`: fully measured shared geometry score at entry.
/// - `options`, `budget`: iteration/projection limits and shared work accounting.
/// - `outcome`: telemetry updated even when the original geometry is retained.
///
/// # Returns
/// Best complete geometry in place. No new crossing pair, changed pin or source
/// double relation is accepted. The final shared geometry score cannot worsen;
/// private intermediate search states can have a worse geometry-only score if
/// their combined crossing-depth/angle objective strictly decreases.
pub(super) fn improve(
  model: &Model,
  points: &mut Vec<Point>,
  initial_quality: Quality,
  options: &OptimizationOptions,
  budget: &mut Budget,
  outcome: &mut Outcome,
) {
  if !budget.charge(2 * points.len()) {
    outcome.exhausted = true;
    return;
  }
  let initial = points.clone();
  let mut current_points = points.clone();
  let Some(mut current) = objective::evaluate(model, &current_points, budget) else {
    outcome.exhausted = budget.0 == 0;
    return;
  };
  let mut best_total = current.total;
  for _ in 0..options.iterations {
    outcome.iterations += 1;
    let magnitude = current.gradient.iter().map(|g| g.x.hypot(g.y)).fold(0., f64::max);
    if magnitude < 1e-9 {
      break;
    }
    let mut accepted = None;
    for attempt in 0..10 {
      if !budget.charge(points.len()) {
        outcome.exhausted = true;
        return;
      }
      let step = 0.1 / magnitude * 0.5_f64.powi(attempt);
      let mut candidate: Vec<_> = current_points
        .iter()
        .zip(&current.gradient)
        .map(|(p, g)| p.translated(-step * g.x, -step * g.y))
        .collect();
      match model.project(&mut candidate, options.projection_passes, budget) {
        None => {
          outcome.exhausted = true;
          return;
        }
        Some(false) => continue,
        Some(true) => {}
      }
      match model.valid(&initial, &candidate, budget) {
        None => {
          outcome.exhausted = true;
          return;
        }
        Some(false) => continue,
        Some(true) => {}
      }
      if !budget.charge(candidate.len()) {
        outcome.exhausted = true;
        return;
      }
      if candidate.iter().zip(&current_points).any(|(a, b)| a.distance(*b) > 0.2) {
        continue;
      }
      let Some(evaluation) = objective::evaluate(model, &candidate, budget) else {
        outcome.exhausted = budget.0 == 0;
        return;
      };
      if evaluation.total + 1e-10 < current.total && evaluation.quality.crossings.is_subset(&initial_quality.crossings)
      {
        accepted = Some((candidate, evaluation));
        break;
      }
    }
    let Some((candidate, evaluation)) = accepted else {
      break;
    };
    if evaluation.total + 1e-10 < best_total && evaluation.quality.cost <= initial_quality.cost {
      if !budget.charge(candidate.len()) {
        outcome.exhausted = true;
        return;
      }
      best_total = evaluation.total;
      outcome.after = evaluation.quality.cost;
      outcome.improved = true;
      *points = candidate.clone();
    }
    current_points = candidate;
    current = evaluation;
  }
}
