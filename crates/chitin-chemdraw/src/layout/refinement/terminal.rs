//! Reserved local hydroxyl clearance after whole-branch refinement.
//!
//! Only a terminal neutral hydroxyl on a saturated neutral carbon can move.
//! Rotation is about its carbon, not the terminal oxygen itself. Ring vertices,
//! carbonyl geometry, bond lengths, and source tetrahedral facts stay unchanged.

use std::{collections::BTreeMap, f64::consts::PI};

use chitin_chem::{AtomId, BondOrder, Element, MolecularGraph};

use super::{Budget, LayoutOptions, Model, Point, Quality, segment_distance};

/// A leaf whose placement can change without moving any skeleton vertex.
pub(super) struct Hydroxyl {
  /// Dense terminal oxygen row.
  pub(super) leaf: usize,
  /// Dense saturated carbon row; this is the fixed rotation pivot.
  host: usize,
  /// Other carbon ligands whose projected directions constrain the trial.
  neighbors: Vec<usize>,
}

/// Selects conservative candidates from supplied facts, never inferring a ring H.
///
/// # Returns
/// Source-ordered terminal O–C single bonds where O is neutral, closed-shell and
/// its carbon has 2–4 neighbors joined exclusively by single bonds. Unknown
/// charge/radical state, aromatic hosts and explicit oxygen H vertices are not
/// silently treated as this terminal-hydroxyl drawing case.
pub(super) fn collect(graph: &MolecularGraph, ids: &[AtomId], indices: &BTreeMap<AtomId, usize>) -> Vec<Hydroxyl> {
  let mut result = Vec::new();
  for (leaf, &id) in ids.iter().enumerate() {
    let Some(atom) = graph.atom(id) else { continue };
    if atom.element() != Element::Oxygen || atom.formal_charge() != Some(0) || atom.unpaired_electrons() != Some(0) {
      continue;
    }
    let Ok(mut row) = graph.neighbors(id) else { continue };
    if row.len() != 1 {
      continue;
    }
    let Some((host, edge)) = row.next() else { continue };
    if graph.bond(edge).is_none_or(|bond| bond.order() != BondOrder::Single) {
      continue;
    }
    let Some(carbon) = graph.atom(host) else { continue };
    let Ok(neighbors) = graph.neighbors(host) else { continue };
    if carbon.element() != Element::Carbon
      || carbon.is_aromatic()
      || carbon.formal_charge() != Some(0)
      || carbon.unpaired_electrons() != Some(0)
      || !(2..=4).contains(&neighbors.len())
    {
      continue;
    }
    let mut other = Vec::new();
    let mut saturated = true;
    for (neighbor, edge) in neighbors {
      saturated &= graph.bond(edge).is_some_and(|bond| bond.order() == BondOrder::Single);
      if neighbor != id {
        other.push(indices[&neighbor]);
      }
    }
    if saturated {
      result.push(Hydroxyl {
        leaf,
        host: indices[&host],
        neighbors: other,
      });
    }
  }
  result
}

impl Hydroxyl {
  /// Measures the unsigned angle to one other ligand in normalized drawing space.
  fn angle(&self, points: &[Point], neighbor: usize) -> f64 {
    let (o, c, n) = (points[self.leaf], points[self.host], points[neighbor]);
    (((o.x - c.x) * (n.x - c.x) + (o.y - c.y) * (n.y - c.y)) / (o.distance(c) * n.distance(c)))
      .clamp(-1., 1.)
      .acos()
  }

  /// Checks local congestion without building a full-component candidate index.
  fn congested(&self, model: &Model, points: &[Point], budget: &mut Budget) -> Option<bool> {
    if !budget.charge(points.len() + model.edges.len()) {
      return None;
    }
    let p = points[self.leaf];
    Some(
      points
        .iter()
        .enumerate()
        .any(|(i, &q)| i != self.leaf && i != self.host && p.distance(q) < 0.95)
        || model.edges.iter().any(|edge| {
          let [a, b] = edge.ends;
          a != self.leaf && b != self.leaf && segment_distance(p, points[a], points[b]) < 0.25
        }),
    )
  }

  /// Bounds movement against the original snapshot, so repeated passes cannot drift.
  ///
  /// Each candidate stays within 30° of its original ray and original ligand
  /// angles, and at least 60° from all other ligands. Saturated projection angles
  /// may flex; this exception never applies to multiple-bond/linear centers.
  fn permitted(&self, original: &[Point], candidate: &[Point]) -> bool {
    let c = original[self.host];
    let a = original[self.leaf];
    let b = candidate[self.leaf];
    let turn = (((a.x - c.x) * (b.x - c.x) + (a.y - c.y) * (b.y - c.y)) / (a.distance(c) * b.distance(c)))
      .clamp(-1., 1.)
      .acos();
    turn <= PI / 6. + 1e-8
      && self.neighbors.iter().all(|&neighbor| {
        let angle = self.angle(candidate, neighbor);
        angle >= PI / 3. - 1e-8 && (angle - self.angle(original, neighbor)).abs() <= PI / 6. + 1e-8
      })
  }
}

/// Gives congested hydroxyls a bounded local search from reserved work.
///
/// # Parameters
/// - `model`, `points`, `current`: complete component geometry and its score.
/// - `options`, `budget`: existing pass limit and shared work allowance.
///
/// # Returns
/// Whether the budget was exhausted. Trials rotate a single O about its carbon
/// by ±15°/±30° from the initial ray. Only a strictly lower complete geometry
/// score without new proper crossings is committed. Failed/partial scoring never
/// publishes a candidate; label positions and chemical facts are not edited.
pub(super) fn improve(
  model: &Model,
  points: &mut Vec<Point>,
  current: &mut Quality,
  options: &LayoutOptions,
  budget: &mut Budget,
) -> bool {
  if model.hydroxyls.is_empty() {
    return false;
  }
  if !budget.charge(points.len()) {
    return true;
  }
  let original = points.clone();
  for _ in 0..options.refinement_passes {
    let mut changed = false;
    for hydroxyl in &model.hydroxyls {
      let Some(congested) = hydroxyl.congested(model, points, budget) else {
        return true;
      };
      if !congested {
        continue;
      }
      let origin = original[hydroxyl.host];
      let vector = Point::new(
        original[hydroxyl.leaf].x - origin.x,
        original[hydroxyl.leaf].y - origin.y,
      );
      let mut best = None;
      let mut best_cost = current.cost;
      let mut exhausted = false;
      for angle in [PI / 12., -PI / 12., PI / 6., -PI / 6.] {
        if !budget.charge(points.len() + hydroxyl.neighbors.len() + model.double_stereo.len()) {
          exhausted = true;
          break;
        }
        let (sin, cos) = angle.sin_cos();
        let mut candidate = points.clone();
        candidate[hydroxyl.leaf] = origin.translated(cos * vector.x - sin * vector.y, sin * vector.x + cos * vector.y);
        if !hydroxyl.permitted(&original, &candidate) || !model.preserves_double_stereo(&candidate) {
          continue;
        }
        let Some(quality) = model.measure(&candidate, options.minimum_spacing, budget) else {
          exhausted = true;
          break;
        };
        if quality.cost + 1e-10 < best_cost && quality.crossings.is_subset(&current.crossings) {
          best_cost = quality.cost;
          best = Some((candidate, quality));
        }
      }
      if let Some((candidate, quality)) = best {
        *points = candidate;
        *current = quality;
        changed = true;
      }
      if exhausted {
        return true;
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

  /// Two unit-length C–O branches occupying the same endpoint initially.
  fn folded() -> Result<(MolecularGraph, Model, Vec<Point>), Box<dyn std::error::Error>> {
    let graph = read_smiles("OCCO")?.into_graph();
    let ids = graph.atoms().map(|(id, _)| id).collect();
    let model = Model::new(&graph, ids, analyze_rings(&graph)?.bridges());
    let oxygen = Point::new(0., 3_f64.sqrt() / 2.);
    Ok((
      graph,
      model,
      vec![oxygen, Point::new(-0.5, 0.), Point::new(0.5, 0.), oxygen],
    ))
  }

  #[test]
  fn overlapping_hydroxyls_should_move_about_their_carbons_not_about_themselves() -> TestResult {
    let (graph, model, original) = folded()?;
    let before = graph.clone();
    let mut points = original.clone();
    let mut current = model
      .measure(&points, 0.55, &mut Budget(usize::MAX))
      .ok_or("no quality")?;
    assert!(!improve(
      &model,
      &mut points,
      &mut current,
      &LayoutOptions::default(),
      &mut Budget(1_000_000)
    ));
    assert!(points[0].distance(points[3]) > 0.95);
    assert!(current.crowds.is_empty());
    assert!(current.crossings.is_empty());
    assert_eq!(&points[1..3], &original[1..3]);
    for hydroxyl in &model.hydroxyls {
      assert!(hydroxyl.permitted(&original, &points));
      assert!((points[hydroxyl.leaf].distance(points[hydroxyl.host]) - 1.).abs() < 1e-8);
    }
    assert_eq!(graph, before);
    Ok(())
  }

  #[test]
  fn incomplete_local_scoring_should_never_commit_a_worse_candidate() -> TestResult {
    let (_, model, original) = folded()?;
    let baseline = model
      .measure(&original, 0.55, &mut Budget(usize::MAX))
      .ok_or("no quality")?;
    for allowance in [1, 8, 64, 256, 1024] {
      let mut points = original.clone();
      let mut current = model
        .measure(&points, 0.55, &mut Budget(usize::MAX))
        .ok_or("no quality")?;
      improve(
        &model,
        &mut points,
        &mut current,
        &LayoutOptions::default(),
        &mut Budget(allowance),
      );
      let final_quality = model
        .measure(&points, 0.55, &mut Budget(usize::MAX))
        .ok_or("no quality")?;
      assert!(final_quality.cost <= baseline.cost + 1e-10);
      assert!(final_quality.crossings.is_empty());
      assert_eq!(&points[1..3], &original[1..3]);
      for hydroxyl in &model.hydroxyls {
        assert!((points[hydroxyl.leaf].distance(points[hydroxyl.host]) - 1.).abs() < 1e-8);
      }
    }
    Ok(())
  }

  #[test]
  fn selection_should_exclude_carbonyl_aromatic_and_ring_oxygens() -> TestResult {
    for (source, expected) in [
      ("CC(=O)O", 0),
      ("Oc1ccccc1", 0),
      ("C1COCCO1", 0),
      ("COC", 0),
      ("OCCO", 2),
    ] {
      let graph = read_smiles(source)?.into_graph();
      let ids = graph.atoms().map(|(id, _)| id).collect();
      let model = Model::new(&graph, ids, analyze_rings(&graph)?.bridges());
      assert_eq!(model.hydroxyls.len(), expected, "{source}");
    }
    Ok(())
  }
}
