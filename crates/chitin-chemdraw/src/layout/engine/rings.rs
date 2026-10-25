//! Central/side ring construction in source order, independent of tree assembly.
//! Source attribution and redistribution terms are in licenses/coordinate-generation.txt.

use super::{
  Error, Work,
  fragments::{Forest, Fragment},
  math::{BOND, Vector},
  topology::Model,
};
use std::collections::BTreeMap;

/// Rigid constraints and replaced geometry are independent provenance facts.
pub(super) struct Seeded {
  pub rigid: Vec<bool>,
  pub rebuilt: Vec<bool>,
  /// Temporary openings must be relaxed after the complete topology is restored.
  pub requires_minimization: bool,
}

/// Returns fusion partners in bond-incidence order, shared by assembly and orientation.
///
/// # Errors
/// Incidence and noncyclic connecting-bond inspections use the shared work allowance.
pub(super) fn fused_neighbors(model: &Model, fragment: &Fragment, work: &mut Work) -> Result<Vec<Vec<usize>>, Error> {
  let rings = &fragment.rings;
  let mut neighbors = vec![Vec::new(); rings.len()];
  for &atom in &fragment.atoms {
    work.charge(model.edges.len().saturating_mul(rings.len()))?;
    let mut incident = Vec::new();
    for edge in &model.edges {
      if !edge.ends.contains(&atom) {
        continue;
      }
      for (i, &ring) in rings.iter().enumerate() {
        let nodes = &model.cycles[ring];
        if edge.ends.iter().all(|a| nodes.contains(a)) && !incident.contains(&i) {
          incident.push(i);
        }
      }
    }
    for (i, &a) in incident.iter().enumerate() {
      for &b in &incident[i + 1..] {
        if !neighbors[a].contains(&b) {
          neighbors[a].push(b);
          neighbors[b].push(a);
        }
      }
    }
  }
  for edge in model.edges.iter().filter(|edge| !edge.cyclic && edge.order != 1) {
    work.charge(rings.len().saturating_mul(rings.len()))?;
    for (a, &ra) in rings
      .iter()
      .enumerate()
      .filter(|&(_, &r)| model.cycles[r].contains(&edge.ends[0]))
    {
      for (b, &rb) in rings
        .iter()
        .enumerate()
        .filter(|&(_, &r)| model.cycles[r].contains(&edge.ends[1]))
      {
        if ra != rb && !neighbors[a].contains(&b) {
          neighbors[a].push(b);
          neighbors[b].push(a);
        }
      }
    }
  }
  Ok(neighbors)
}

/// Builds native ring coordinates and marks any unconstructed atoms explicitly.
///
/// # Errors
/// Work exhaustion and inconsistent fusion anchors reject the complete proposal.
pub(super) fn seed(model: &Model, forest: &Forest, points: &mut [Vector], work: &mut Work) -> Result<Seeded, Error> {
  seed_with_policy(model, forest, points, false, 0, work)
}

/// Reconstructs temporary cut topologies with recursive forced-open macrocycles.
///
/// # Errors
/// Every nested opening shares the original work allowance and depth guard.
pub(super) fn seed_with_policy(
  model: &Model,
  forest: &Forest,
  points: &mut [Vector],
  force_open: bool,
  depth: usize,
  work: &mut Work,
) -> Result<Seeded, Error> {
  let mut seeded = Seeded {
    rigid: vec![false; model.atoms.len()],
    rebuilt: vec![false; model.atoms.len()],
    requires_minimization: false,
  };
  'fragments: for fragment in &forest.fragments {
    let rings = &fragment.rings;
    if rings.is_empty() {
      continue;
    }
    let neighbors = fused_neighbors(model, fragment, work)?;
    let mut macro_paths = BTreeMap::new();
    let mut removed = vec![false; rings.len()];
    let mut side = Vec::new();
    loop {
      let mut changed = false;
      for i in 0..rings.len() {
        if removed[i] || model.cycles[rings[i]].len() >= 9 {
          continue;
        }
        let connections: usize = neighbors[i]
          .iter()
          .filter(|&&j| !removed[j])
          .map(|&j| {
            let common = model.cycles[rings[i]]
              .iter()
              .filter(|a| model.cycles[rings[j]].contains(a))
              .count();
            1 + usize::from(model.cycles[rings[j]].len() >= 9)
              + usize::from(
                common > 3 || common == 3 && model.cycles[rings[i]].len() == 4 && model.cycles[rings[j]].len() == 4,
              )
          })
          .sum();
        if connections == 1 {
          removed[i] = true;
          side.push(i);
          changed = true;
        }
      }
      if !changed {
        break;
      }
    }
    let mut built = vec![false; rings.len()];
    let mut pending: Vec<_> = (0..rings.len()).filter(|&i| !removed[i]).collect();
    let central: Vec<_> = pending.iter().map(|&i| rings[i]).collect();
    if central.len() > 1
      && let Some(atoms) = super::templates::apply(model, &central, points, work)?
    {
      for atom in atoms {
        seeded.rigid[atom] = true;
      }
      for &i in &pending {
        built[i] = true;
      }
      pending.clear();
    } else if central.len() > 1 {
      let score = planarity(model, &central, work)?;
      if score >= 1000 {
        if score <= 200000 {
          let index = select_ring(&pending, rings, &neighbors, &built, model);
          open_fragment(model, fragment, rings[pending[index]], depth, points, &mut seeded, work)?;
        }
        continue;
      }
    }
    for &i in &pending {
      let ring = rings[i];
      if model.cycles[ring].len() < 9 {
        continue;
      }
      let path = if force_open {
        None
      } else {
        super::macrocycles::generate(model, &model.cycles[ring], work)?
      };
      if let Some(path) = path {
        macro_paths.insert(ring, path);
      } else if open_fragment(model, fragment, ring, depth, points, &mut seeded, work)? {
        continue 'fragments;
      } else {
        seeded.requires_minimization = true;
      }
      // With no eligible cut, the source's final seed is a regular polygon.
    }
    let construction = Construction {
      rings,
      neighbors: &neighbors,
      model,
      macro_paths: &macro_paths,
    };
    while !pending.is_empty() {
      let chosen = select_ring(&pending, rings, &neighbors, &built, model);
      let i = pending.remove(chosen);
      construction.build(i, &built, points, work)?;
      built[i] = true;
    }
    for i in side.into_iter().rev() {
      construction.build(i, &built, points, work)?;
      built[i] = true;
    }
    for &ring in rings {
      for &atom in &model.cycles[ring] {
        seeded.rebuilt[atom] = true;
      }
    }
  }
  Ok(seeded)
}

/// Commits only a complete, source-identity-preserving open-ring reconstruction.
fn open_fragment(
  model: &Model,
  fragment: &Fragment,
  ring: usize,
  depth: usize,
  points: &mut [Vector],
  seeded: &mut Seeded,
  work: &mut Work,
) -> Result<bool, Error> {
  if let Some(opened) = super::opening::generate(model, ring, depth, work)? {
    seeded.requires_minimization = true;
    points.copy_from_slice(&opened.points);
    for (target, source) in seeded.rigid.iter_mut().zip(opened.rigid) {
      *target |= source;
    }
    for &ring in &fragment.rings {
      for &atom in &model.cycles[ring] {
        seeded.rebuilt[atom] = true;
      }
    }
    return Ok(true);
  }
  Ok(false)
}

/// Central-ring priority preserves stable first-best ties in the pending order.
fn select_ring(pending: &[usize], rings: &[usize], neighbors: &[Vec<usize>], built: &[bool], model: &Model) -> usize {
  let mut chosen = 0;
  let mut highest = 0;
  for (position, &i) in pending.iter().enumerate() {
    let ring = &model.cycles[rings[i]];
    let common: usize = neighbors[i]
      .iter()
      .map(|&j| ring.iter().filter(|a| model.cycles[rings[j]].contains(a)).count())
      .sum();
    let score = usize::from(neighbors[i].iter().any(|&j| built[j])) * 100000
      + usize::from(ring.len() >= 9) * 1000
      + usize::from(ring.len() == 6) * 10
      + ring.len()
      + 40 * neighbors[i].len()
      + 15 * common;
    if position == 0 || score > highest {
      highest = score;
      chosen = position;
    }
  }
  chosen
}

/// Scores fused macrocycles, multiply shared edges and overloaded junction angles.
fn planarity(model: &Model, rings: &[usize], work: &mut Work) -> Result<usize, Error> {
  let mut score = 0;
  for &ring in rings {
    let nodes = &model.cycles[ring];
    if nodes.len() >= 9 {
      if super::opening::choose_edge(model, ring, work)?.is_none() {
        continue;
      }
      for (other, cycle) in model
        .cycles
        .iter()
        .enumerate()
        .filter(|&(other, cycle)| other != ring && cycle.len() >= 9)
      {
        work.charge(nodes.len() + cycle.len())?;
        if model.cycle_edges[ring]
          .iter()
          .any(|edge| model.cycle_edges[other].contains(edge))
          || nodes.iter().any(|atom| cycle.contains(atom))
        {
          score += 1000;
        }
      }
    }
    for &edge in &model.cycle_edges[ring] {
      work.charge(model.cycle_edges.iter().map(Vec::len).sum())?;
      let count = model.cycle_edges.iter().filter(|edges| edges.contains(&edge)).count();
      score += count.saturating_sub(2) * 1000;
    }
    for &atom in nodes {
      if model.neighbors[atom].len() <= 3 {
        continue;
      }
      work.charge(model.cycles.iter().map(Vec::len).sum())?;
      let angle: f32 = model
        .cycles
        .iter()
        .filter(|cycle| cycle.contains(&atom))
        .map(|cycle| (std::f64::consts::PI - std::f64::consts::TAU / cycle.len() as f64) as f32)
        .sum();
      if f64::from(angle) >= 1.99 * std::f64::consts::PI {
        score += 1000;
      }
    }
  }
  Ok(score)
}

/// Shared immutable construction context; each ring edits only scratch geometry.
struct Construction<'a> {
  rings: &'a [usize],
  neighbors: &'a [Vec<usize>],
  model: &'a Model,
  macro_paths: &'a BTreeMap<usize, Vec<Vector>>,
}

impl Construction<'_> {
  fn build(&self, i: usize, built: &[bool], points: &mut [Vector], work: &mut Work) -> Result<(), Error> {
    let Self {
      rings,
      neighbors,
      model,
      macro_paths,
    } = self;
    let ring = &model.cycles[rings[i]];
    work.charge(ring.len())?;
    let mut parent: Option<usize> = None;
    let mut common = Vec::new();
    for &j in &neighbors[i] {
      if !built[j] {
        continue;
      }
      let other = &model.cycles[rings[j]];
      let mut shared: Vec<_> = ring.iter().copied().filter(|a| other.contains(a)).collect();
      shared.sort_unstable();
      if parent.is_some_and(|p| shared.len() < common.len() || other.len() < model.cycles[rings[p]].len()) {
        continue;
      }
      parent = Some(j);
      common = shared;
    }
    let increment = std::f32::consts::TAU / ring.len() as f32;
    let mut local = Vec::new();
    let mut next = Vector::default();
    for n in 0..ring.len() {
      local.push(next);
      let angle = increment * n as f32;
      next += Vector::new(
        (f64::from(angle).cos() * f64::from(BOND)) as f32,
        (-f64::from(angle).sin() * f64::from(BOND)) as f32,
      );
    }
    if let Some(path) = macro_paths.get(&rings[i]) {
      local.clone_from(path);
      // Already generated fusion anchors survive the macrocycle assignment.
      for (&atom, point) in ring.iter().zip(&mut local) {
        if rings
          .iter()
          .enumerate()
          .any(|(j, &r)| built[j] && model.cycles[r].contains(&atom))
        {
          *point = points[atom];
        }
      }
    }
    if let Some(parent) = parent {
      let other = &model.cycles[rings[parent]];
      if common.len() < 2 {
        let (pivot, parent_pivot, shift) = if let Some(&p) = common.first() {
          (p, p, false)
        } else {
          let edge = model
            .edges
            .iter()
            .find(|e| {
              e.order != 1 && e.ends.iter().any(|a| ring.contains(a)) && e.ends.iter().any(|a| other.contains(a))
            })
            .ok_or(Error::Invariant(model.atoms[ring[0]].id))?;
          let [a, b] = edge.ends;
          if ring.contains(&a) { (a, b, true) } else { (b, a, true) }
        };
        let local_pivot = local[ring
          .iter()
          .position(|&a| a == pivot)
          .ok_or(Error::Invariant(model.atoms[pivot].id))?];
        for point in &mut local {
          *point -= local_pivot;
        }
        let mut center = Vector::default();
        for &p in &local {
          center += p;
        }
        center = (center / ring.len() as f32).normalized();
        let mut mean = Vector::default();
        for &(n, _) in &model.neighbors[parent_pivot] {
          if other.contains(&n) {
            mean += points[n];
          }
        }
        mean = mean * 0.5;
        let direction = (points[parent_pivot] - mean).normalized();
        let pivot = points[parent_pivot] + if shift { direction * BOND } else { Vector::default() };
        for point in &mut local {
          *point = point.rotate(center.y, center.x).rotate(-direction.y, direction.x) + pivot;
        }
      } else {
        let a = common[0];
        let b = common[common.len() - 1];
        let slot = |node| {
          ring
            .iter()
            .position(|&id| id == node)
            .ok_or(Error::Invariant(model.atoms[node].id))
        };
        let (sa, sb) = (slot(a)?, slot(b)?);
        let mean = (points[a] + points[b]) * 0.5;
        let axis = (points[b] - points[a]).normalized();
        let align = |coords: &[Vector]| {
          let middle = (coords[sa] + coords[sb]) * 0.5;
          let v = (coords[sb] - coords[sa]).normalized();
          coords
            .iter()
            .map(|&p| (p - middle).rotate(v.y, v.x).rotate(-axis.y, axis.x) + mean)
            .collect::<Vec<_>>()
        };
        let normal = align(&local);
        let mirror = align(&local.iter().map(|p| Vector::new(p.x, -p.y)).collect::<Vec<_>>());
        let mut center = Vector::default();
        for &node in other {
          center += points[node];
        }
        center = center / other.len() as f32;
        let score = |coords: &[Vector]| {
          ring
            .iter()
            .zip(coords)
            .filter(|(node, _)| !common.contains(node))
            .map(|(_, &p)| (p - center).length())
            .sum::<f32>()
        };
        local = if score(&normal) < score(&mirror) {
          mirror
        } else {
          normal
        };
      }
    }
    for (&node, &point) in ring.iter().zip(&local) {
      points[node] = point.rounded();
    }
    Ok(())
  }
}
