//! Topological large-cycle turns, independent of planar face validity and layout scores.
//!
//! Local angles remain measurable in crossing drawings; no interior-angle or
//! winding interpretation is assigned to those drawings. Shared local paths
//! are counted once even when they occur in multiple selected cycles.

use crate::{
  quality::{Metrics, Observation, State},
  ring_cycles::{self, Budget},
};
use anyhow::Result;
use chitin_chem::{BondOrder, MolecularGraph, perception::analyze_rings};
use chitin_chemdraw::{geometry::Point, layout::Layout2d};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const KEYS: [&str; 6] = [
  "macrocycle_angle_coverage",
  "macrocycle_angle_deviation_rmse_deg",
  "macrocycle_angle_deviation_p95_deg",
  "macrocycle_near_linear_fraction",
  "macrocycle_longest_near_linear_run",
  "macrocycle_longest_same_turn_run",
];
/// Near-straight screening threshold, not a chemical or graphical-standard limit.
const NEAR_LINEAR: f64 = 165.;

/// Local constraint strata; drawing angles are not molecular hybridization estimates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CenterClass {
  Path,
  Junction,
  Linear,
  SmallRing,
}

/// One unique four-atom path and its two signed planar turns, in degrees.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnSample {
  pub atoms: [usize; 4],
  pub turns: [f64; 2],
  pub classes: [CenterClass; 2],
  /// Smallest selected large basis cycle containing this path, not chemical ring size.
  pub cycle_size: usize,
  pub block_rank: usize,
}

/// Source-order scene centers and exact exported frame for a historical overlay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneFrame {
  pub positions: Vec<[f64; 2]>,
  pub view_box: [f64; 4],
}

/// Auditable, bounded large-cycle observations; absence in old runs is not zero.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MacrocycleGeometry {
  pub samples: Vec<TurnSample>,
  pub selected_large_cycles: usize,
  pub excluded_blocks: usize,
  pub unique_angles: usize,
  pub valid_angles: usize,
  pub scene: Option<SceneFrame>,
}

/// Adds local-angle coverage and conditional distortion without changing geometry.
///
/// The short-first independent basis is topology-defined and stable across
/// layout changes; it is not SSSR or an exhaustive enumeration of all cycles.
/// Ordinary path centers use the nearest of 108, 120, 900/7 and 135 degrees;
/// constrained small-ring and junction centers remain visible but unscored.
/// Linear centers are visible but excluded from near-straight failure flags.
/// Work/sample exhaustion makes aggregate results unavailable, never a passing
/// partial zero. Perspective drawings are outside this planar policy.
///
/// # Parameters
/// - `graph`, `layout`: immutable source namespace and validated complete centers.
/// - `perspective`: excludes layouts with explicit camera depth/crossing order.
/// - `output`: receives independent scalars and auditable local path records.
///
/// # Errors
/// Inconsistent source topology propagates; incomplete discovery is recorded as
/// unavailable measurements rather than a chemistry or rendering failure.
pub fn measure(graph: &MolecularGraph, layout: &Layout2d, perspective: bool, output: &mut Metrics) -> Result<()> {
  if perspective {
    unavailable(output, State::NotApplicable, "planar turns exclude perspective");
    return Ok(());
  }
  let topology = analyze_rings(graph)?;
  let ids: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
  let ordinals: BTreeMap<_, _> = ids.iter().enumerate().map(|(i, &id)| (id, i)).collect();
  let points: Vec<_> = ids.iter().map(|id| layout.positions()[id]).collect();
  let mut budget = Budget(1_000_000);
  let mut details = MacrocycleGeometry::default();
  let mut angles = BTreeMap::new();
  let mut pairs = BTreeSet::new();
  let mut errors = Vec::new();
  let mut near = 0;
  let mut scored = 0;
  let mut near_run = 0;
  let mut same_run = 0;
  for block in topology
    .blocks()
    .iter()
    .filter(|block| block.atoms().len() >= 9 && block.cycle_rank() > 0)
  {
    let atoms: Vec<_> = block.atoms().iter().map(|id| ordinals[id]).collect();
    let edges: Vec<_> = block
      .bonds()
      .iter()
      .filter_map(|id| graph.bond(*id))
      .map(|b| b.atoms().map(|id| ordinals[&id]))
      .collect();
    let Some(cycles) = ring_cycles::basis(&atoms, &edges, block.cycle_rank(), &mut budget) else {
      details.excluded_blocks += 1;
      continue;
    };
    let small: BTreeSet<_> = cycles
      .iter()
      .filter(|c| c.len() <= 8)
      .flat_map(|c| (0..c.len()).map(|i| angle_key(c[(i + c.len() - 1) % c.len()], c[i], c[(i + 1) % c.len()])))
      .collect();
    let mut degrees = BTreeMap::<usize, usize>::new();
    for &[a, b] in &edges {
      *degrees.entry(a).or_default() += 1;
      *degrees.entry(b).or_default() += 1;
    }
    for cycle in cycles.iter().filter(|c| c.len() >= 9) {
      if details.samples.len() + cycle.len() > 8192 {
        details.excluded_blocks += 1;
        break;
      }
      details.selected_large_cycles += 1;
      let n = cycle.len();
      let mut values = Vec::new();
      let mut classes = Vec::new();
      for i in 0..n {
        let [a, c, b] = [cycle[(i + n - 1) % n], cycle[i], cycle[(i + 1) % n]];
        let key = angle_key(a, c, b);
        let orders: Vec<_> = graph
          .neighbors(ids[c])?
          .filter_map(|(_, id)| graph.bond(id).map(|b| b.order()))
          .collect();
        let class = if orders.contains(&BondOrder::Triple) || orders == [BondOrder::Double, BondOrder::Double] {
          CenterClass::Linear
        } else if degrees[&c] > 2 || orders.len() > 4 {
          CenterClass::Junction
        } else if small.contains(&key) {
          CenterClass::SmallRing
        } else {
          CenterClass::Path
        };
        let turn = turn(points[a], points[c], points[b]);
        if let std::collections::btree_map::Entry::Vacant(entry) = angles.entry(key) {
          entry.insert(turn);
          if let Some(value) = turn.filter(|_| class == CenterClass::Path) {
            let angle = 180. - value.abs();
            errors.push(
              [108., 120., 900. / 7., 135.]
                .into_iter()
                .map(|target: f64| (angle - target).abs())
                .fold(f64::INFINITY, f64::min),
            );
            near += usize::from(angle >= NEAR_LINEAR);
            scored += 1;
          }
        }
        values.push(turn);
        classes.push(class);
      }
      let ordinary: Vec<_> = values
        .iter()
        .zip(&classes)
        .map(|(turn, class)| turn.filter(|_| *class == CenterClass::Path))
        .collect();
      near_run = near_run.max(longest_run(
        &ordinary
          .iter()
          .map(|v| i8::from(v.is_some_and(|v| 180. - v.abs() >= NEAR_LINEAR)))
          .collect::<Vec<_>>(),
      ));
      same_run = same_run.max(longest_run(
        &ordinary
          .iter()
          .map(|v| {
            v.map_or(0, |v| {
              if v.abs() < 1e-8 {
                0
              } else if v > 0. {
                1
              } else {
                -1
              }
            })
          })
          .collect::<Vec<_>>(),
      ));
      for i in 0..n {
        let mut atoms = [cycle[(i + n - 1) % n], cycle[i], cycle[(i + 1) % n], cycle[(i + 2) % n]];
        let mut turns = [values[i], values[(i + 1) % n]];
        let mut local_classes = [classes[i], classes[(i + 1) % n]];
        let mut reverse = atoms;
        reverse.reverse();
        if reverse < atoms {
          atoms = reverse;
          turns = [turns[1].map(|t| -t), turns[0].map(|t| -t)];
          local_classes.reverse();
        }
        if let [Some(a), Some(b)] = turns
          && pairs.insert(atoms)
        {
          details.samples.push(TurnSample {
            atoms,
            turns: [a, b],
            classes: local_classes,
            cycle_size: n,
            block_rank: block.cycle_rank(),
          });
        }
      }
    }
  }
  details.unique_angles = angles.len();
  details.valid_angles = angles.values().filter(|v| v.is_some()).count();
  if details.excluded_blocks > 0 {
    unavailable(
      output,
      State::NotImplemented,
      "large-cycle discovery/sample allowance exhausted; local records are partial",
    );
  } else if details.valid_angles < details.unique_angles {
    unavailable(
      output,
      State::NotImplemented,
      "collapsed bonds prevent complete large-cycle angle/run evaluation",
    );
    output.measurements.insert(
      KEYS[0].into(),
      Observation::measured(
        details.valid_angles as f64 / details.unique_angles as f64,
        details.unique_angles,
        None,
      ),
    );
  } else {
    output.measurements.insert(
      KEYS[0].into(),
      if angles.is_empty() {
        Observation::unavailable(State::NotApplicable, 0, "no selected large basis cycles")
      } else {
        Observation::measured(details.valid_angles as f64 / angles.len() as f64, angles.len(), None)
      },
    );
    output
      .measurements
      .insert(KEYS[1].into(), Observation::rmse(&errors, 15.));
    errors.sort_by(f64::total_cmp);
    output.measurements.insert(
      KEYS[2].into(),
      if errors.is_empty() {
        Observation::unavailable(State::NotApplicable, 0, "no valid ordinary large-cycle angles")
      } else {
        Observation::measured(
          errors[(errors.len() * 95).div_ceil(100).saturating_sub(1)],
          errors.len(),
          None,
        )
      },
    );
    output
      .measurements
      .insert(KEYS[3].into(), Observation::rate(near, scored));
    for (key, value) in [(KEYS[4], near_run), (KEYS[5], same_run)] {
      output.measurements.insert(
        key.into(),
        if scored == 0 {
          Observation::unavailable(State::NotApplicable, 0, "no valid ordinary large-cycle angles")
        } else {
          Observation::measured(value as f64, details.selected_large_cycles, None)
        },
      );
    }
  }
  output.macrocycle_geometry = Some(details);
  Ok(())
}

/// Marks every aggregate channel unavailable without fabricating a zero denominator.
fn unavailable(output: &mut Metrics, state: State, reason: &str) {
  for key in KEYS {
    output
      .measurements
      .insert(key.into(), Observation::unavailable(state, 0, reason));
  }
}

/// Unordered ligand pair around one center, shared by overlapping basis cycles.
fn angle_key(a: usize, c: usize, b: usize) -> [usize; 3] {
  [a.min(b), c, a.max(b)]
}

/// Signed angle between successive unit rays; collapsed source bonds stay missing.
fn turn(a: Point, c: Point, b: Point) -> Option<f64> {
  let (l, r) = (a.distance(c), b.distance(c));
  if l <= 0. || r <= 0. {
    return None;
  }
  let (ux, uy, vx, vy) = ((c.x - a.x) / l, (c.y - a.y) / l, (b.x - c.x) / r, (b.y - c.y) / r);
  let value = (ux * vy - uy * vx).atan2(ux * vx + uy * vy).to_degrees();
  value.is_finite().then_some(value)
}

/// Longest circular contiguous run of the same nonzero category, capped at n.
fn longest_run(values: &[i8]) -> usize {
  let (mut longest, mut run, mut previous) = (0, 0, 0);
  for &value in values.iter().cycle().take(values.len() * 2) {
    run = if value == 0 {
      0
    } else if value == previous {
      run + 1
    } else {
      1
    };
    previous = value;
    longest = longest.max(run.min(values.len()));
  }
  longest
}

#[cfg(test)]
mod tests;
