//! Topology-derived chelate seeds over a detached covalent drawing graph.
//!
//! Short donor paths are geometric fragments, not assignments of coordination
//! stereochemistry. Ambiguous or overlapping paths decline this seed family;
//! the general bounded search remains available without changing source facts.

use super::{Budget, CoordinationLayoutError, Point};
use chitin_chem::{AtomId, MolecularGraph, coordination::CoordinationSite, perception::analyze_rings};
use std::{
  collections::{BTreeMap, VecDeque},
  f64::consts::{PI, TAU},
};

/// A complete proposal and the rigid rows retained during closure repair.
pub(super) struct Seed {
  /// Global dense coordinates, including the explicitly declared center.
  pub(super) points: Vec<Point>,
  /// Chelate interiors and attachment stubs must move as a group, not stretch.
  pub(super) fixed: Vec<bool>,
  /// Whether this trial explores a weak collective contraction before closure.
  pub(super) contract: bool,
}

/// Builds bounded chelate/scaffold proposals without using metal edges as rings.
///
/// Unique mutually nearest short donor paths define disjoint fragments. A simple
/// covalent macrocycle, when present, guides their cyclic order and the remaining
/// backbone. All coordinates are tentative until full-source closure validation.
/// Search exhaustion declines proposals rather than publishing partial geometry.
pub(super) fn seeds(
  ligand: &MolecularGraph,
  site: &CoordinationSite,
  ids: &[AtomId],
  base: &[Point],
  radius: f64,
  budget: &mut Budget,
) -> Result<Vec<Seed>, CoordinationLayoutError> {
  if !budget.charge(ids.len() + ligand.bond_count()) {
    return Ok(Vec::new());
  }
  let rows: BTreeMap<_, _> = ids.iter().enumerate().map(|(i, &id)| (id, i)).collect();
  let mut neighbors = vec![Vec::new(); ids.len()];
  for (_, bond) in ligand.bonds() {
    let [a, b] = bond.atoms().map(|id| rows[&id]);
    neighbors[a].push(b);
    neighbors[b].push(a);
  }
  let donors: Vec<_> = site.attachments().iter().map(|&(_, id)| rows[&id]).collect();
  let Some(paths) = chelates(&neighbors, &donors, budget) else {
    return Ok(Vec::new());
  };
  if paths.is_empty() {
    return Ok(Vec::new());
  }
  let topology = analyze_rings(ligand).map_err(super::CoordinationError::from)?;
  let ring = topology
    .blocks()
    .iter()
    .filter(|block| block.cycle_rank() == 1 && block.atoms().len() >= 9)
    .max_by_key(|block| block.atoms().len());
  let scaffold = ring.map(|block| cycle(&neighbors, &block.atoms().iter().map(|id| rows[id]).collect::<Vec<_>>()));
  let scaffold = scaffold.unwrap_or_default();
  let mut order: Vec<_> = paths
    .iter()
    .enumerate()
    .map(|(i, path)| {
      let target = nearest_scaffold(path, &scaffold, &neighbors);
      (
        target
          .and_then(|row| scaffold.iter().position(|&r| r == row))
          .unwrap_or(i),
        i,
      )
    })
    .collect();
  order.sort_unstable();
  let mut result = Vec::new();
  // Two traversals and two scaffold offsets search composition, not absolute
  // configurations. No wedges or metal-centered stereo are derived from them.
  for reverse in [false, true] {
    for offset in [0., PI / 6.] {
      if !budget.charge(ids.len() * (ids.len() + paths.len())) {
        return Ok(result);
      }
      let mut points = base.to_vec();
      let mut known = vec![false; ids.len()];
      let center = rows[&site.center()];
      points[center] = Point::new(0., 0.);
      known[center] = true;
      for (i, &row) in scaffold.iter().enumerate() {
        let theta = TAU * i as f64 / scaffold.len() as f64;
        let r = 1. / (2. * (PI / scaffold.len() as f64).sin());
        points[row] = Point::new(r * theta.cos(), r * theta.sin());
        known[row] = true;
      }
      let mut fixed = vec![false; ids.len()];
      fixed[center] = true;
      let mut valid = true;
      for (i, path) in paths.iter().enumerate() {
        let rank = order.iter().position(|&(_, index)| index == i).unwrap_or(i);
        let theta = TAU * rank as f64 / paths.len() as f64 + offset;
        let Some(local) = outline(path.len(), radius) else {
          valid = false;
          break;
        };
        let (sin, cos) = (theta + PI / 2.).sin_cos();
        for (j, &row) in path.iter().enumerate() {
          if known[row] {
            valid = false;
            break;
          }
          let p = local[if reverse { local.len() - 1 - j } else { j }];
          points[row] = Point::new(p.x * cos - p.y * sin, p.x * sin + p.y * cos);
          known[row] = true;
          fixed[row] = true;
        }
      }
      if !valid {
        continue;
      }
      // Preserve the vacant sector at three-connected chelate atoms. These
      // one-bond stubs carry the local drawing angle into flexible linkers.
      for path in &paths {
        for &row in &path[1..path.len() - 1] {
          let outside: Vec<_> = neighbors[row]
            .iter()
            .filter(|other| !path.contains(other))
            .copied()
            .collect();
          if outside.len() != 1 || known[outside[0]] {
            continue;
          }
          let other = outside[0];
          let p = addition(row, &neighbors, &points, &known);
          points[other] = p;
          known[other] = true;
          fixed[other] = true;
        }
      }
      complete(&neighbors, &mut points, &mut known);
      result.push(Seed {
        points,
        fixed,
        contract: true,
      });
    }
  }
  Ok(result)
}

/// Finds unique mutual nearest donors, rejecting shared or ambiguous fragments.
fn chelates(neighbors: &[Vec<usize>], donors: &[usize], budget: &mut Budget) -> Option<Vec<Vec<usize>>> {
  let mut choices = Vec::new();
  for &root in donors {
    let mut previous = vec![usize::MAX; neighbors.len()];
    let mut distance = vec![usize::MAX; neighbors.len()];
    let mut ways = vec![0_usize; neighbors.len()];
    let mut queue = VecDeque::from([root]);
    distance[root] = 0;
    ways[root] = 1;
    while let Some(row) = queue.pop_front() {
      if !budget.charge(1 + neighbors[row].len()) {
        return None;
      }
      if distance[row] >= 4 {
        continue;
      }
      for &other in &neighbors[row] {
        if distance[other] == usize::MAX {
          previous[other] = row;
          distance[other] = distance[row] + 1;
          queue.push_back(other);
        }
        if distance[other] == distance[row] + 1 {
          ways[other] = (ways[other] + ways[row]).min(2);
        }
      }
    }
    let best = donors.iter().filter(|&&id| id != root).map(|&id| distance[id]).min()?;
    let matches: Vec<_> = donors
      .iter()
      .filter(|&&id| id != root && distance[id] == best)
      .copied()
      .collect();
    if !(2..=4).contains(&best) || matches.len() != 1 {
      return None;
    }
    if ways[matches[0]] != 1 {
      return None;
    }
    let mut path = vec![matches[0]];
    while *path.last()? != root {
      path.push(previous[*path.last()?]);
    }
    path.reverse();
    choices.push(path);
  }
  let mut paths = Vec::new();
  let mut used = vec![false; neighbors.len()];
  for path in &choices {
    if path[0] > *path.last()? {
      continue;
    }
    if !choices
      .iter()
      .any(|other| other.iter().copied().eq(path.iter().rev().copied()))
    {
      return None;
    }
    if path.iter().any(|&row| used[row]) {
      return None;
    }
    for &row in path {
      used[row] = true;
    }
    paths.push(path.clone());
  }
  (paths.len() * 2 == donors.len()).then_some(paths)
}

/// Orders a simple block by connectivity, never by source atom ordinal.
fn cycle(neighbors: &[Vec<usize>], ring: &[usize]) -> Vec<usize> {
  let mut result = vec![ring[0]];
  let mut previous = usize::MAX;
  while result.len() < ring.len() {
    let row = result[result.len() - 1];
    let Some(&next) = neighbors[row]
      .iter()
      .find(|&&n| n != previous && ring.contains(&n) && !result.contains(&n))
    else {
      return Vec::new();
    };
    result.push(next);
    previous = row;
  }
  result
}

/// Locates the nearest backbone attachment in the detached covalent topology.
fn nearest_scaffold(path: &[usize], scaffold: &[usize], neighbors: &[Vec<usize>]) -> Option<usize> {
  let mut seen = vec![false; neighbors.len()];
  let mut queue: VecDeque<_> = path.iter().copied().collect();
  for &row in path {
    seen[row] = true;
  }
  while let Some(row) = queue.pop_front() {
    if scaffold.contains(&row) {
      return Some(row);
    }
    for &other in &neighbors[row] {
      if !seen[other] {
        seen[other] = true;
        queue.push_back(other);
      }
    }
  }
  None
}

/// Closes a short 120-degree donor path onto a center at the requested distance.
fn outline(n: usize, radius: f64) -> Option<Vec<Point>> {
  let mut points = vec![Point::new(0., 0.)];
  for i in 0..n - 1 {
    let theta = (i as f64 - (n - 2) as f64 / 2.) * PI / 3.;
    let p = points[i].translated(theta.cos(), theta.sin());
    points.push(p);
  }
  let half = points[n - 1].x / 2.;
  if half >= radius {
    return None;
  }
  let height = (radius * radius - half * half).sqrt();
  for p in &mut points {
    *p = p.translated(-half, -height);
  }
  Some(points)
}

/// Returns the vacant local sector opposite already placed covalent neighbors.
fn addition(row: usize, neighbors: &[Vec<usize>], points: &[Point], known: &[bool]) -> Point {
  let origin = points[row];
  let mut ray = Point::new(0., 0.);
  for &other in &neighbors[row] {
    if known[other] {
      let q = points[other];
      let d = origin.distance(q).max(1e-8);
      ray = ray.translated((origin.x - q.x) / d, (origin.y - q.y) / d);
    }
  }
  let d = ray.x.hypot(ray.y).max(1e-8);
  origin.translated(ray.x / d, ray.y / d)
}

/// Grows unplaced branches from known ring and fragment anchors in BFS order.
fn complete(neighbors: &[Vec<usize>], points: &mut [Point], known: &mut [bool]) {
  let mut queue: VecDeque<_> = known
    .iter()
    .enumerate()
    .filter_map(|(i, &set)| set.then_some(i))
    .collect();
  while let Some(row) = queue.pop_front() {
    let children: Vec<_> = neighbors[row].iter().filter(|&&other| !known[other]).copied().collect();
    let parents: Vec<_> = neighbors[row].iter().filter(|&&other| known[other]).copied().collect();
    let origin = points[row];
    let heading = if parents.len() == 1 {
      (origin.y - points[parents[0]].y).atan2(origin.x - points[parents[0]].x)
    } else {
      let next = addition(row, neighbors, points, known);
      (next.y - origin.y).atan2(next.x - origin.x)
    };
    let offsets = if parents.len() == 1 {
      super::super::angles::offsets(
        children.len(),
        true,
        if row.is_multiple_of(2) { PI / 3. } else { -PI / 3. },
        false,
      )
    } else {
      vec![0.; children.len()]
    };
    for (&other, offset) in children.iter().zip(offsets) {
      let theta = heading + offset;
      points[other] = origin.translated(theta.cos(), theta.sin());
      known[other] = true;
      queue.push_back(other);
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use chitin_chem::{Element, io::smiles::read_smiles};
  type TestResult = Result<(), Box<dyn std::error::Error>>;

  #[test]
  fn chelate_outlines_should_keep_unit_edges_and_declared_attachment_distances() -> TestResult {
    for n in 3..=5 {
      let points = outline(n, 1.4).ok_or("missing outline")?;
      assert!((points[0].distance(Point::new(0., 0.)) - 1.4).abs() < 1e-10);
      assert!((points[n - 1].distance(Point::new(0., 0.)) - 1.4).abs() < 1e-10);
      assert!(
        points
          .windows(2)
          .all(|pair| (pair[0].distance(pair[1]) - 1.).abs() < 1e-10)
      );
    }
    Ok(())
  }

  #[test]
  fn ambiguous_donor_paths_should_not_create_an_arbitrary_rigid_fragment() {
    let rows = vec![vec![1, 2], vec![0, 3], vec![0, 3], vec![1, 2]];
    assert!(chelates(&rows, &[0, 3], &mut Budget(1000)).is_none());
  }

  #[test]
  fn fragment_discovery_should_decline_work_before_exhausting_its_allowance() {
    assert!(chelates(&[vec![1], vec![0]], &[0, 1], &mut Budget(1)).is_none());
  }

  #[test]
  fn disconnected_bidentate_ligands_should_generate_rigid_source_handle_preserving_seeds() -> TestResult {
    let graph = read_smiles("O1CCO[Fe]12OCCO2")?.into_graph();
    let before = graph.clone();
    let center = graph
      .atoms()
      .find(|(_, atom)| atom.element() == Element::Iron)
      .map(|(id, _)| id)
      .ok_or("missing center")?;
    let site = CoordinationSite::new(&graph, center, graph.neighbors(center)?.map(|(_, edge)| edge))?;
    let ids: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
    let rows: BTreeMap<_, _> = ids.iter().enumerate().map(|(i, &id)| (id, i)).collect();
    let mut ligand = graph.clone();
    ligand.remove_atom(center)?;
    let initial = crate::layout::generate(&ligand, &Default::default())?;
    let base: Vec<_> = ids
      .iter()
      .map(|&id| initial.position(id).unwrap_or(Point::new(0., 0.)))
      .collect();
    let candidates = seeds(&ligand, &site, &ids, &base, 1.4, &mut Budget(1_000_000))?;
    assert!(!candidates.is_empty());
    for candidate in candidates {
      assert!(candidate.fixed.iter().all(|&fixed| fixed));
      for (id, bond) in graph.bonds() {
        let [a, b] = bond.atoms().map(|id| rows[&id]);
        let length = if site.attachments().iter().any(|&(edge, _)| edge == id) {
          1.4
        } else {
          1.
        };
        assert!((candidate.points[a].distance(candidate.points[b]) - length).abs() < 1e-10);
      }
    }
    assert_eq!(graph, before);
    Ok(())
  }
}
