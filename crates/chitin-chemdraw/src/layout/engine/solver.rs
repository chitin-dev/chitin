//! Bounded drawing-force integration with immutable fixed flags and atomic commit.
//! Source attribution and redistribution terms are in licenses/coordinate-generation.txt.

use crate::layout::{NativeFailureKind as Failure, NativeStage as Stage};

use super::{
  Error, Work,
  evaluation::Evaluator,
  interactions::Interaction,
  math::{EPSILON, Vector},
};

/// A completed solver pass is separate from whether convergence was observed.
#[derive(Debug)]
pub(super) struct Outcome {
  pub iterations: usize,
  pub minimum_energy: f32,
}

/// Integrates clipped drawing forces and commits the final complete trajectory.
///
/// # Parameters
/// - `points`, `fixed`: one dense snapshot and its fixed-coordinate mask.
/// - `interactions`: immutable source-ordered drawing terms.
/// - `iterations`, `work`: separate finite iteration and evaluation allowances.
///
/// # Returns
/// The source integrator's final coordinates. Its angular penalties and forces
/// are not a consistent potential/gradient pair, so minimum-penalty retention
/// can select the shortened terminal seed rather than its relaxed geometry.
/// Fixed forces are cleared with movable forces on every step.
///
/// # Errors
/// Invalid/nonfinite simulation input or exhausted work returns no partial update.
pub(super) fn minimize(
  points: &mut [Vector],
  fixed: &[bool],
  interactions: &[Interaction],
  iterations: usize,
  work: &mut Work,
) -> Result<Outcome, Error> {
  if points.len() != fixed.len() {
    return Err(Error::native(Stage::Simulation, Failure::InvalidSimulationDimensions));
  }
  if points.iter().any(|point| !point.finite()) {
    return Err(Error::native(Stage::Simulation, Failure::NonFiniteCoordinates));
  }
  work.charge(3 * points.len() + iterations)?;
  let mut trial = points.to_vec();
  let mut forces = vec![Vector::default(); points.len()];
  let mut history = vec![0.; iterations];
  let mut best_energy = f32::MAX;
  let mut evaluated = 0;
  let evaluator = Evaluator::new(interactions, work)?;
  for iteration in 0..iterations {
    work.charge(points.len())?;
    forces.fill(Vector::default());
    let energy = evaluator.score(&trial, &mut forces, work)?;
    if !energy.is_finite() {
      return Err(Error::native(Stage::Simulation, Failure::NonFiniteEnergy));
    }
    if energy < best_energy {
      best_energy = energy;
    }
    history[iteration] = energy;
    evaluated += 1;
    let displacement = step(&mut trial, &forces, fixed, 0.1);
    if displacement < 0.001 {
      break;
    }
    if iteration >= 200 && history[iteration - 100] - energy < 20. {
      break;
    }
  }
  points.copy_from_slice(&trial);
  Ok(Outcome {
    iterations: evaluated,
    minimum_energy: best_energy,
  })
}

/// Applies the source's 0.3 force multiplier and maximum Euclidean step length.
pub(super) fn step(points: &mut [Vector], forces: &[Vector], fixed: &[bool], limit: f32) -> f32 {
  let mut total = 0.;
  for ((point, &force), &fixed) in points.iter_mut().zip(forces).zip(fixed) {
    if fixed {
      continue;
    }
    let mut delta = force * 0.3;
    if !delta.finite() {
      delta = Vector::default();
    }
    let square = delta.square().max(EPSILON);
    if square > limit * limit {
      delta = delta * (limit / square.sqrt());
    }
    *point += delta;
    total += delta.square();
  }
  total
}
