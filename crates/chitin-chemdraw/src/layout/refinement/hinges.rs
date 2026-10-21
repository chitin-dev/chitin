//! Whole-fragment reflection about two fixed attachment centers.

use super::{Budget, Model, Point, Quality};
use crate::layout::LayoutOptions;

/// Searches two-vertex separators without changing any source bond length.
///
/// # Parameters
/// - `model`, `points`: complete component topology and current geometry.
/// - `current`: fully measured score, replaced only with an accepted candidate.
/// - `options`, `budget`: bounded passes and shared optional search allowance.
///
/// # Returns
/// Whether the allowance was exhausted. Each candidate reflects a complete
/// component of the graph minus its two anchors. Every boundary bond therefore
/// ends on the reflection axis. Linear centers and supplied alkene relations
/// are protected; authored perspective cores are never reflected.
pub(super) fn improve(
  model: &Model,
  points: &mut Vec<Point>,
  current: &mut Quality,
  options: &LayoutOptions,
  budget: &mut Budget,
) -> bool {
  if (current.crossings.is_empty() && current.label_contacts == 0) || !model.perspective.is_empty() || budget.0 == 0 {
    return false;
  }
  let n = points.len();
  for _ in 0..options.refinement_passes {
    let mut changed = false;
    for a in 0..n {
      if model.rows[a].iter().filter(|&&edge| !model.edges[edge].movable).count() < 2 || !model.hinge_hosts[a] {
        continue;
      }
      for b in a + 1..n {
        if model.rows[b].iter().filter(|&&edge| !model.edges[edge].movable).count() < 2 || !model.hinge_hosts[b] {
          continue;
        }
        if !budget.charge(n) {
          return true;
        }
        let mut seen = vec![false; n];
        seen[a] = true;
        seen[b] = true;
        let axis = Point::new(points[b].x - points[a].x, points[b].y - points[a].y);
        let length = axis.x.hypot(axis.y);
        if length < 1e-10 {
          continue;
        }
        let (ux, uy) = (axis.x / length, axis.y / length);
        let mut fragments = Vec::new();
        for root in 0..n {
          if seen[root] {
            continue;
          }
          let mut fragment = Vec::new();
          let mut stack = vec![root];
          let mut touches = [false; 2];
          seen[root] = true;
          while let Some(node) = stack.pop() {
            if !budget.charge(1 + model.rows[node].len()) {
              return true;
            }
            fragment.push(node);
            for &edge in &model.rows[node] {
              for &other in &model.edges[edge].ends {
                if other == a {
                  touches[0] = true;
                }
                if other == b {
                  touches[1] = true;
                }
                if !seen[other] {
                  seen[other] = true;
                  stack.push(other);
                }
              }
            }
          }
          // One-ended branches already have a cheaper dedicated search. Moving
          // the entire remaining graph is just a redundant global reflection.
          if touches.iter().all(|&touch| touch) {
            fragments.push(fragment);
          }
        }
        if fragments.len() < 2 {
          continue;
        }
        for fragment in fragments {
          if !budget.charge(n + model.double_stereo.len()) {
            return true;
          }
          let mut candidate = points.clone();
          for &node in &fragment {
            let p = Point::new(points[node].x - points[a].x, points[node].y - points[a].y);
            let along = p.x * ux + p.y * uy;
            candidate[node] = points[a].translated(2. * along * ux - p.x, 2. * along * uy - p.y);
          }
          if !model.preserves_double_stereo(&candidate) {
            continue;
          }
          let Some(quality) = model.measure(&candidate, options.minimum_spacing, budget) else {
            return true;
          };
          if quality.crossings.len() <= current.crossings.len()
            && quality.crowds.len() <= current.crowds.len()
            && quality.cost + 1e-10 < current.cost
          {
            *points = candidate;
            *current = quality;
            changed = true;
            if current.clear() {
              return false;
            }
          }
        }
      }
    }
    if !changed {
      break;
    }
  }
  false
}

#[cfg(test)]
mod tests {
  use super::*;
  use chitin_chem::{io::smiles::read_smiles, perception::analyze_rings};
  type TestResult = Result<(), Box<dyn std::error::Error>>;

  /// Two ring paths cross, but can reflect rigidly around their two endpoints.
  fn crossed() -> Result<(Model, Vec<Point>, Quality), Box<dyn std::error::Error>> {
    let graph = read_smiles("C1CCCCC1")?.into_graph();
    let model = Model::new(
      &graph,
      graph.atoms().map(|(id, _)| id).collect(),
      analyze_rings(&graph)?.bridges(),
    );
    let points = vec![
      Point::new(-1., 0.),
      Point::new(-0.5, 1.),
      Point::new(0.5, 0.3),
      Point::new(1., 0.),
      Point::new(0.5, 1.),
      Point::new(-0.5, 0.3),
    ];
    let quality = model
      .measure(&points, 0.55, &mut Budget(usize::MAX))
      .ok_or("missing quality")?;
    Ok((model, points, quality))
  }

  #[test]
  fn separator_reflection_should_remove_crossings_without_stretching_any_edge() -> TestResult {
    let (model, mut points, mut quality) = crossed()?;
    let before = points.clone();
    assert!(!quality.crossings.is_empty());
    improve(
      &model,
      &mut points,
      &mut quality,
      &Default::default(),
      &mut Budget(1_000_000),
    );
    assert!(quality.crossings.is_empty());
    for edge in &model.edges {
      let [a, b] = edge.ends;
      assert!((points[a].distance(points[b]) - before[a].distance(before[b])).abs() < 1e-10);
    }
    Ok(())
  }

  #[test]
  fn interrupted_separator_search_should_not_publish_a_partial_trial() -> TestResult {
    let (model, mut points, mut quality) = crossed()?;
    let before = points.clone();
    assert!(improve(
      &model,
      &mut points,
      &mut quality,
      &Default::default(),
      &mut Budget(1)
    ));
    assert_eq!(points, before);
    Ok(())
  }

  #[test]
  fn protected_attachment_centers_should_not_be_used_as_hinges() -> TestResult {
    let (mut model, mut points, mut quality) = crossed()?;
    model.hinge_hosts.fill(false);
    let before = points.clone();
    improve(
      &model,
      &mut points,
      &mut quality,
      &Default::default(),
      &mut Budget(1_000_000),
    );
    assert_eq!(points, before);
    Ok(())
  }
}
