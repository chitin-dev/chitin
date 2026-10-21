//! Final clearance for terminal substituents on saturated ring carbons.
//!
//! A leaf group rotates rigidly about its host after backbone routing. No ring
//! vertex or growing subtree moves, so these trials cannot redirect later tree
//! placement. Geometry, angle-order and source alkene guards remain mandatory.

use std::f64::consts::PI;

use super::{Budget, LayoutOptions, Model, Point, Quality};

/// Clears complete terminal exit groups using only remaining shared search work.
///
/// # Parameters
/// - `model`, `points`, `current`: complete component topology, geometry and score.
/// - `options`, `budget`: existing pass limit and unspent refinement allowance.
///
/// # Returns
/// Whether work ran out. Only ±15°/±30° rotations from stage entry are tried;
/// multiple terminal neighbors move together to retain their mutual angles.
/// Accepted candidates cannot increase any hard-contact count. Ring/nonterminal
/// coordinates and bond lengths remain exact; no new stereo facts are assigned.
pub(super) fn improve(
  model: &Model,
  points: &mut Vec<Point>,
  current: &mut Quality,
  options: &LayoutOptions,
  budget: &mut Budget,
) -> bool {
  if current.clear() || !model.ring_ports.iter().any(|&eligible| eligible) {
    return false;
  }
  if !budget.charge(points.len()) {
    return true;
  }
  let original = points.clone();
  for _ in 0..options.refinement_passes {
    let mut changed = false;
    for (host, &eligible) in model.ring_ports.iter().enumerate() {
      if !eligible {
        continue;
      }
      if !budget.charge(model.rows[host].len()) {
        return true;
      }
      let leaves: Vec<_> = model.rows[host]
        .iter()
        .filter_map(|&edge| {
          let [a, b] = model.edges[edge].ends;
          let leaf = if a == host { b } else { a };
          (model.edges[edge].movable && model.rows[leaf].len() == 1).then_some(leaf)
        })
        .collect();
      if leaves.is_empty() {
        continue;
      }
      // Unrelated leaf groups cannot remove a reported hard contact. Keep the
      // scarce full-component measurements for exits that actually participate.
      let contacts = current.crowds.len() * 2 + current.atom_edges.len() * 3 + current.crossings.len() * 4;
      if !budget.charge(contacts.saturating_mul(leaves.len())) {
        return true;
      }
      if !model.affects_contacts(&leaves, current) {
        continue;
      }
      let mut best = None;
      let mut exhausted = false;
      for angle in [PI / 12., -PI / 12., PI / 6., -PI / 6.] {
        if !budget.charge(points.len() + leaves.len() + model.rows[host].len().pow(2) + model.double_stereo.len()) {
          exhausted = true;
          break;
        }
        let mut candidate = points.clone();
        let origin = original[host];
        let (sin, cos) = angle.sin_cos();
        for &leaf in &leaves {
          let (x, y) = (original[leaf].x - origin.x, original[leaf].y - origin.y);
          candidate[leaf] = origin.translated(cos * x - sin * y, sin * x + cos * y);
        }
        if !model.preserves_host_angles(host, &original, &candidate) || !model.preserves_double_stereo(&candidate) {
          continue;
        }
        let Some(quality) = model.measure(&candidate, options.minimum_spacing, budget) else {
          exhausted = true;
          break;
        };
        if quality.improves(best.as_ref().map_or(&*current, |(_, previous)| previous)) {
          best = Some((candidate, quality));
        }
      }
      if let Some((candidate, quality)) = best {
        *points = candidate;
        *current = quality;
        changed = true;
        if current.clear() {
          return false;
        }
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
  use chitin_chem::{io::smiles::read_smiles, perception::analyze_rings};

  use super::*;

  type TestResult = Result<(), Box<dyn std::error::Error>>;

  /// A substituted bridge with terminal exits initially grazing the outer face.
  fn crowded_bridge() -> Result<(Model, Vec<Point>), Box<dyn std::error::Error>> {
    let graph = read_smiles("C[C@@H]1C[C@@H]2CC[C@@]1(C)C2(C)C")?.into_graph();
    let rings = analyze_rings(&graph)?;
    let coordinates = crate::layout::generate(
      &graph,
      &LayoutOptions {
        refinement_passes: 0,
        ..Default::default()
      },
    )?;
    let ids = graph.atoms().map(|(id, _)| id).collect();
    let model = Model::new(&graph, ids, rings.bridges());
    let points = model
      .ids
      .iter()
      .map(|&id| coordinates.position(id).ok_or("missing atom"))
      .collect::<Result<_, _>>()?;
    Ok((model, points))
  }

  #[test]
  fn terminal_group_rotation_should_clear_contacts_without_moving_the_ring() -> TestResult {
    let (model, original) = crowded_bridge()?;
    let mut points = original.clone();
    let mut current = model
      .measure(&points, 0.55, &mut Budget(usize::MAX))
      .ok_or("measurement failed")?;
    let before = current.atom_edges.len();
    improve(
      &model,
      &mut points,
      &mut current,
      &LayoutOptions::default(),
      &mut Budget(1_000_000),
    );
    assert!(
      current.atom_edges.len() < before,
      "{before} -> {}",
      current.atom_edges.len()
    );
    for (row, edges) in model.rows.iter().enumerate() {
      if edges.len() > 1 {
        assert_eq!(points[row], original[row], "nonterminal row {row} moved");
      }
    }
    for edge in &model.edges {
      let [a, b] = edge.ends;
      assert!((points[a].distance(points[b]) - original[a].distance(original[b])).abs() < 1e-10);
    }
    Ok(())
  }

  #[test]
  fn exhausted_work_should_not_publish_partial_terminal_moves() -> TestResult {
    let (model, original) = crowded_bridge()?;
    let mut points = original.clone();
    let mut current = model
      .measure(&points, 0.55, &mut Budget(usize::MAX))
      .ok_or("measurement failed")?;
    assert!(improve(
      &model,
      &mut points,
      &mut current,
      &LayoutOptions::default(),
      &mut Budget(1)
    ));
    assert_eq!(points, original);
    Ok(())
  }
}
