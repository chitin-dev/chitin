//! Bounded two-cut candidates following single-branch repair.
//!
//! Intermediate reflections may worsen clearance: only complete, fully scored
//! results can replace the input. This permits coordinated fragment movement
//! without introducing angle flex, bond stretching, or random initialization.

use super::{Budget, LayoutOptions, Model, Point, Quality};

/// Maximum contact-relevant bridge cuts considered per component.
const MAX_CUTS: usize = 24;
/// Maximum fully scored intermediate snapshots retained for the second move.
const BEAM_WIDTH: usize = 6;

/// A single-bond bridge and its rigidly movable side.
struct Cut {
  /// Dense moving-side atom rows, including the endpoint on the reflection axis.
  rows: Vec<usize>,
  /// Both axis endpoints remain fixed under this cut's reflection.
  axis: [usize; 2],
}

impl Cut {
  /// Reflects a complete snapshot, charging before allocation and validation.
  ///
  /// # Returns
  /// `None` on budget exhaustion; `Some(None)` for identity or stereo-invalid
  /// poses. Reflections preserve all bond lengths and unsigned attachment angles.
  /// Every axis is read from the current snapshot, including after another cut.
  fn reflect(&self, model: &Model, points: &[Point], budget: &mut Budget) -> Option<Option<Vec<Point>>> {
    if !budget.charge(points.len() + self.rows.len() + model.double_stereo.len()) {
      return None;
    }
    let [moving, fixed] = self.axis;
    let origin = points[moving];
    let dx = origin.x - points[fixed].x;
    let dy = origin.y - points[fixed].y;
    let length = dx.hypot(dy);
    if length < 1e-12 {
      return Some(None);
    }
    let (x, y) = (dx / length, dy / length);
    let mut candidate = points.to_vec();
    let mut changed = false;
    for &row in &self.rows {
      let p = points[row];
      let (dx, dy) = (p.x - origin.x, p.y - origin.y);
      let projection = dx * x + dy * y;
      candidate[row] = origin.translated(2. * projection * x - dx, 2. * projection * y - dy);
      changed |= candidate[row].distance(p) > 1e-10;
    }
    Some((changed && model.preserves_double_stereo(&candidate)).then_some(candidate))
  }
}

/// Fully evaluated intermediate pose; its score need not improve the input.
struct Seed {
  /// Source cut, so the second move cannot simply undo the first.
  cut: usize,
  /// Complete geometry, never a partially transformed subtree.
  points: Vec<Point>,
  /// Soft clearance orders exploration only, not final acceptance.
  cost: f64,
}

/// Searches combinations of two contact-relevant single-bond reflections.
///
/// # Parameters
/// - `model`, `points`, `current`: matching complete component geometry and score.
/// - `options`: unchanged diagnostic spacing and search enablement.
/// - `budget`: remaining shared refinement work; no new allowance is allocated.
///
/// # Returns
/// Whether work was exhausted. The best accepted complete pose survives early
/// termination. Each accepted result has nonincreasing hard-contact counts;
/// intermediate poses can cross a score barrier without being committed.
/// Perspective templates are excluded because their upright view is authored.
pub(super) fn improve(
  model: &Model,
  points: &mut Vec<Point>,
  current: &mut Quality,
  options: &LayoutOptions,
  budget: &mut Budget,
) -> bool {
  if current.clear() || !model.perspective.is_empty() || options.refinement_passes == 0 {
    return false;
  }
  let mut cuts = Vec::new();
  for (edge, source) in model.edges.iter().enumerate() {
    if !source.movable {
      continue;
    }
    let Some((rows, moving, fixed)) = model.branch(edge, budget) else {
      return true;
    };
    if rows.len() < 2 {
      continue;
    }
    if !budget.charge(rows.len() * (current.crowds.len() + current.atom_edges.len() * 3 + current.crossings.len() * 4))
    {
      return true;
    }
    if model.affects_contacts(&rows, current) {
      cuts.push(Cut {
        rows,
        axis: [moving, fixed],
      });
      if cuts.len() == MAX_CUTS {
        break;
      }
    }
  }
  if cuts.len() < 2 {
    return false;
  }
  if !budget.charge(points.len()) {
    return true;
  }
  // All trials share an immutable entry pose. Accepting one candidate must not
  // change the meaning of a later candidate or its stored intermediate pose.
  let initial = points.clone();
  let mut seeds = Vec::new();
  for (index, cut) in cuts.iter().enumerate() {
    let Some(candidate) = cut.reflect(model, &initial, budget) else {
      return true;
    };
    let Some(candidate) = candidate else { continue };
    let Some(score) = model.measure(&candidate, options.minimum_spacing, budget) else {
      return true;
    };
    let cost = score.cost;
    if score.improves(current) {
      points.clone_from(&candidate);
      *current = score;
      if current.clear() {
        return false;
      }
    }
    seeds.push(Seed {
      cut: index,
      points: candidate,
      cost,
    });
    // Stable ties retain source-bond order; no hash iteration or random seed
    // participates in choosing which intermediate poses receive further work.
    seeds.sort_by(|a, b| a.cost.total_cmp(&b.cost));
    seeds.truncate(BEAM_WIDTH);
  }
  for seed in seeds {
    for (index, cut) in cuts.iter().enumerate() {
      if index == seed.cut {
        continue;
      }
      let Some(candidate) = cut.reflect(model, &seed.points, budget) else {
        return true;
      };
      let Some(candidate) = candidate else { continue };
      let Some(score) = model.measure(&candidate, options.minimum_spacing, budget) else {
        return true;
      };
      if score.improves(current) {
        *points = candidate;
        *current = score;
        if current.clear() {
          return false;
        }
      }
    }
  }
  false
}

#[cfg(test)]
mod tests {
  use super::*;
  use chitin_chem::{io::smiles::read_smiles, perception::analyze_rings};

  type TestResult = Result<(), Box<dyn std::error::Error>>;

  #[test]
  fn two_reflections_should_clear_a_contact_blocked_by_either_single_move() -> TestResult {
    // Two rigid arms and a stationary obstacle isolate a coordinated-move
    // barrier without requiring unrelated ring-placement choices in the test.
    let graph = read_smiles("CCC.CCC.C")?.into_graph();
    let ids = graph.atoms().map(|(id, _)| id).collect();
    let model = Model::new(&graph, ids, analyze_rings(&graph)?.bridges());
    let points = [
      Point::new(0., 0.),
      Point::new(1., -2.),
      Point::new(1., -3.),
      Point::new(2., 0.),
      Point::new(1.25, 2.),
      Point::new(1.25, 3.),
      Point::new(-0.5, 0.),
    ];
    let cuts = [
      Cut {
        rows: vec![0, 1],
        axis: [1, 2],
      },
      Cut {
        rows: vec![3, 4],
        axis: [4, 5],
      },
    ];
    let before = model
      .measure(&points, 0.55, &mut Budget(usize::MAX))
      .ok_or("measurement failed")?;
    for cut in &cuts {
      let candidate = cut
        .reflect(&model, &points, &mut Budget(usize::MAX))
        .flatten()
        .ok_or("reflection failed")?;
      let score = model
        .measure(&candidate, 0.55, &mut Budget(usize::MAX))
        .ok_or("measurement failed")?;
      assert!(!score.improves(&before));
    }
    let first = cuts[0]
      .reflect(&model, &points, &mut Budget(usize::MAX))
      .flatten()
      .ok_or("reflection failed")?;
    let combined = cuts[1]
      .reflect(&model, &first, &mut Budget(usize::MAX))
      .flatten()
      .ok_or("reflection failed")?;
    let score = model
      .measure(&combined, 0.55, &mut Budget(usize::MAX))
      .ok_or("measurement failed")?;
    assert!(score.clear());
    for edge in &model.edges {
      let [a, b] = edge.ends;
      assert!((points[a].distance(points[b]) - combined[a].distance(combined[b])).abs() < 1e-9);
    }
    Ok(())
  }

  #[test]
  fn exhaustion_should_keep_a_complete_nonworsening_pose() -> TestResult {
    let graph = read_smiles("CCCCCCC")?.into_graph();
    let ids = graph.atoms().map(|(id, _)| id).collect();
    let model = Model::new(&graph, ids, analyze_rings(&graph)?.bridges());
    let initial: Vec<_> = (0..7)
      .map(|i| {
        let angle = i as f64 * std::f64::consts::PI / 3.;
        Point::new(angle.cos(), angle.sin())
      })
      .collect();
    for allowance in [0, 1, 32, 128, 512, 2048] {
      let mut points = initial.clone();
      let before = model
        .measure(&points, 0.55, &mut Budget(usize::MAX))
        .ok_or("measurement failed")?;
      let mut current = model
        .measure(&points, 0.55, &mut Budget(usize::MAX))
        .ok_or("measurement failed")?;
      improve(
        &model,
        &mut points,
        &mut current,
        &LayoutOptions::default(),
        &mut Budget(allowance),
      );
      assert!(points == initial || current.improves(&before));
      for edge in &model.edges {
        let [a, b] = edge.ends;
        assert!((points[a].distance(points[b]) - initial[a].distance(initial[b])).abs() < 1e-9);
      }
    }
    Ok(())
  }
}
