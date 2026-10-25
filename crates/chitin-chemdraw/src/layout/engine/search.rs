//! Deterministic tiered discrete search; geometry and evaluation are caller-owned.
//! Source attribution and redistribution terms are in licenses/coordinate-generation.txt.

use super::{Error, Work};
use std::collections::{BTreeMap, BTreeSet};

/// One finite coordinate variable, visited in its source insertion order.
#[derive(Clone, Copy)]
pub(super) struct Domain {
  pub states: u16,
  pub tier: u8,
}

/// The best fully evaluated state remains valid even when optional search ends.
pub(super) struct Outcome {
  pub states: Vec<u16>,
  pub score: f32,
  pub evaluations: usize,
  pub exhausted: bool,
}

/// Grows strictly improving solution frontiers through tiers zero, three and six.
///
/// # Parameters
/// - `domains`: source-ordered variables; state zero is the initial assembly.
/// - `evaluate`: rebuilds a complete candidate and returns its discrete score.
/// - `limit`: maximum newly scored states, excluding cache hits.
/// - `work`: shared optional evaluation allowance, separate from the iteration cap.
///
/// # Returns
/// A complete champion, with lexical state-vector tie-breaking. Up to seven
/// frontier states grow per round, matching the inclusive source beam limit.
/// Budget exhaustion never publishes an unevaluated or partially rebuilt state.
///
/// # Errors
/// Invalid domains, nonfinite scores and non-budget evaluator failures propagate.
/// At least the initial candidate must be evaluated successfully.
pub(super) fn run(
  domains: &[Domain],
  limit: usize,
  work: &mut Work,
  mut evaluate: impl FnMut(&[u16], &mut Work) -> Result<f32, Error>,
) -> Result<Outcome, Error> {
  if limit == 0 || domains.iter().any(|d| d.states == 0) {
    return Err(Error::CoordinateRange);
  }
  work.charge(domains.len())?;
  let initial = vec![0; domains.len()];
  let score = evaluate(&initial, work)?;
  if !score.is_finite() {
    return Err(Error::CoordinateRange);
  }
  let mut cache = BTreeMap::from([(initial.clone(), score)]);
  let mut frontier = cache.clone();
  let mut seen = BTreeSet::new();
  let mut best_score = score;
  let mut tier = 0;
  let mut exhausted = false;
  'rounds: for round in 0..100 {
    let old = std::mem::take(&mut frontier);
    let mut sorted: Vec<_> = old.iter().collect();
    sorted.sort_by(|(a, sa), (b, sb)| sa.total_cmp(sb).then(a.cmp(b)));
    let previous_best = best_score;
    for &(state, _) in sorted.iter().take(7) {
      for (i, domain) in domains.iter().enumerate().filter(|(_, d)| d.tier <= tier) {
        for step in 1..domain.states {
          let mut next = state.clone();
          next[i] = ((u32::from(next[i]) + u32::from(step)) % u32::from(domain.states)) as u16;
          if seen.contains(&next) {
            continue;
          }
          if let Err(error) = work.charge(1 + next.len()) {
            if matches!(error, Error::RingSearchBudgetExceeded) {
              exhausted = true;
              break 'rounds;
            }
            return Err(error);
          }
          let score = if let Some(&score) = cache.get(&next) {
            score
          } else {
            if cache.len() >= limit {
              exhausted = true;
              break 'rounds;
            }
            let score = match evaluate(&next, work) {
              Ok(score) if score.is_finite() => score,
              Ok(_) => return Err(Error::CoordinateRange),
              Err(Error::RingSearchBudgetExceeded) => {
                exhausted = true;
                break 'rounds;
              }
              Err(error) => return Err(error),
            };
            cache.insert(next.clone(), score);
            score
          };
          seen.insert(next.clone());
          best_score = best_score.min(score);
          if score < previous_best {
            frontier.insert(next, score);
          }
        }
      }
    }
    if frontier.is_empty() && tier < 5 {
      tier += 3;
      frontier = old;
    }
    if frontier.is_empty() {
      break;
    }
    if round == 99 {
      exhausted = true;
    }
  }
  let (states, &score) = cache
    .iter()
    .min_by(|(a, sa), (b, sb)| sa.total_cmp(sb).then(a.cmp(b)))
    .ok_or(Error::CoordinateRange)?;
  Ok(Outcome {
    states: states.clone(),
    score,
    evaluations: cache.len(),
    exhausted,
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn tier_growth_should_retain_champions_and_not_rescore_cached_states() -> Result<(), Error> {
    let domains = [
      Domain { states: 2, tier: 0 },
      Domain { states: 3, tier: 3 },
      Domain { states: 2, tier: 4 },
    ];
    let mut seen = BTreeSet::new();
    let result = run(&domains, 100, &mut Work::new(10000), |state, _| {
      assert!(seen.insert(state.to_vec()));
      Ok(100. - 10. * f32::from(state[0]) - 20. * f32::from(state[1]) - 30. * f32::from(state[2]))
    })?;
    assert_eq!(result.states, [1, 2, 1]);
    assert_eq!(result.score, 20.);
    assert_eq!(result.evaluations, seen.len());
    assert!(!result.exhausted);
    Ok(())
  }

  #[test]
  fn exhausted_evaluation_should_not_replace_a_complete_champion() -> Result<(), Error> {
    let result = run(
      &[Domain { states: 3, tier: 0 }],
      100,
      &mut Work::new(100),
      |state, _| match state[0] {
        0 => Ok(100.),
        1 => Ok(50.),
        _ => Err(Error::RingSearchBudgetExceeded),
      },
    )?;
    assert_eq!(result.states, [1]);
    assert_eq!(result.score, 50.);
    assert!(result.exhausted);
    Ok(())
  }

  #[test]
  fn equal_scores_should_use_lexical_state_order_and_a_hard_evaluation_limit() -> Result<(), Error> {
    let result = run(&[Domain { states: 4, tier: 0 }], 2, &mut Work::new(100), |_, _| Ok(1.))?;
    assert_eq!(result.states, [0]);
    assert_eq!(result.evaluations, 2);
    assert!(result.exhausted);
    Ok(())
  }
}
