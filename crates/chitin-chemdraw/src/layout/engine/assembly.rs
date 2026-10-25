//! Builds local fragment frames and composes oriented connection ports.
//! Source attribution and redistribution terms are in licenses/coordinate-generation.txt.

use super::{
  Error, Work,
  fragments::Forest,
  math::{BOND, Vector},
  topology::Model,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Aligned local frames remain immutable while search varies their discrete states.
pub(super) struct Assembly {
  pub frames: Vec<BTreeMap<usize, Vector>>,
  pub points: Vec<Vector>,
  pub actions: Vec<super::moves::Move>,
  pub movable: Vec<bool>,
}

/// Regenerates local non-ring geometry and rigidly assembles a fragment forest.
///
/// # Parameters
/// - `model`, `forest`: the same dense source namespace.
/// - `seed`: complete ring/template coordinates in fifty-unit drawing space.
/// - `work`: local frame and tree composition allowance.
///
/// # Returns
/// Complete coordinates, including every disconnected component. Port ghost
/// atoms guide placement but never enter the chemical graph or final snapshot.
///
/// # Errors
/// Missing frame anchors, zero-length connection rays and work exhaustion reject
/// this whole proposal rather than returning partially assembled components.
pub(super) fn prepare(model: &Model, forest: &Forest, seed: &[Vector], work: &mut Work) -> Result<Assembly, Error> {
  let rules = super::priority::Rules::new(model, work)?;
  let mut frames = Vec::with_capacity(forest.fragments.len());
  let mut scoped = Vec::with_capacity(forest.fragments.len());
  for id in 0..forest.fragments.len() {
    work.charge(model.atoms.len())?;
    let mut scopes = super::scopes::Scopes::new(model.atoms.len());
    frames.push(frame(id, model, forest, seed, &rules, &mut scopes, work)?);
    scoped.push(scopes.actions);
  }
  super::orientation::normalize(&mut frames, model, forest, work)?;
  let lengths = super::orientation::downstream(&frames, forest, work)?;
  let alignment = super::orientation::Alignment {
    model,
    forest,
    lengths: &lengths,
  };
  work.charge(model.atoms.len())?;
  let mut points = vec![Vector::default(); model.atoms.len()];
  let mut transformed = vec![BTreeMap::new(); forest.fragments.len()];
  for &id in &forest.order {
    let f = &forest.fragments[id];
    let (origin, target, sine, cosine) = if let Some(parent) = f.parent {
      if model.other(parent.edge, parent.parent_atom) != parent.child_atom {
        return Err(Error::Invariant(model.atoms[parent.child_atom].id));
      }
      let parent_frame = &transformed[parent.fragment];
      let target = *parent_frame
        .get(&parent.child_atom)
        .ok_or(Error::Invariant(model.atoms[parent.child_atom].id))?;
      let global_parent = points[parent.parent_atom];
      let origin = frames[id][&parent.child_atom];
      let local = frames[id][&parent.parent_atom] - origin;
      let global = global_parent - target;
      if local.length() == 0. || global.length() == 0. {
        return Err(Error::CoordinateRange);
      }
      let (a, b) = (local.normalized(), global.normalized());
      (origin, target, a.y * b.x - a.x * b.y, a.dot(b))
    } else {
      (Vector::default(), Vector::default(), 0., 1.)
    };
    if let Some(parent) = f.parent
      && alignment.reflected(
        id,
        &frames[id],
        &transformed[parent.fragment],
        &points,
        [sine, cosine],
        work,
      )?
    {
      for point in frames[id].values_mut() {
        point.y = -point.y;
      }
    }
    work.charge(frames[id].len())?;
    let world: BTreeMap<_, _> = frames[id]
      .iter()
      .map(|(&node, &point)| {
        (
          node,
          ((point - origin).rounded().rotate(sine, cosine) + target).rounded(),
        )
      })
      .collect();
    for &node in &f.atoms {
      points[node] = world[&node];
    }
    transformed[id] = world;
  }
  let mut actions = Vec::new();
  let mut movable = vec![false; model.atoms.len()];
  let mut defaults = super::moves::defaults(forest).into_iter().peekable();
  for &id in forest.order.iter().rev() {
    for action in &scoped[id] {
      match &action.kind {
        super::moves::Kind::ScaleAtoms { atoms, .. } | super::moves::Kind::InvertBond { atoms, .. } => {
          for &atom in atoms {
            movable[atom] = true;
          }
        }
        _ => {}
      }
    }
    while defaults.peek().is_some_and(|action| action.fragment == id) {
      if let Some(action) = defaults.next() {
        actions.push(action);
      }
    }
    actions.append(&mut scoped[id]);
  }
  Ok(Assembly {
    frames,
    points,
    actions,
    movable,
  })
}

#[cfg(test)]
pub(super) fn assemble(model: &Model, forest: &Forest, seed: &[Vector], work: &mut Work) -> Result<Vec<Vector>, Error> {
  Ok(prepare(model, forest, seed, work)?.points)
}

impl Assembly {
  /// Rebuilds a complete pose from immutable frames, applying states before composition.
  ///
  /// # Errors
  /// Invalid state vectors, absent ports and exhausted work reject the trial.
  pub(super) fn rebuild(
    &self,
    model: &Model,
    forest: &Forest,
    moves: &[super::moves::Move],
    states: &[u16],
    work: &mut Work,
  ) -> Result<Vec<Vector>, Error> {
    if moves.len() != states.len() {
      return Err(Error::CoordinateRange);
    }
    work.charge(model.atoms.len())?;
    let mut points = vec![Vector::default(); model.atoms.len()];
    let mut transformed = vec![BTreeMap::new(); forest.fragments.len()];
    for &id in &forest.order {
      let fragment = &forest.fragments[id];
      work.charge(self.frames[id].len() + moves.len())?;
      let mut local = self.frames[id].clone();
      if let Some(parent) = fragment.parent {
        local.remove(&parent.parent_atom);
      }
      for point in local.values_mut() {
        *point = point.rounded();
      }
      for (action, &state) in moves.iter().zip(states).filter(|(action, _)| action.fragment == id) {
        action.apply(state, &mut local, work)?;
      }
      let (target, angle) = if let Some(parent) = fragment.parent {
        let target = *transformed[parent.fragment]
          .get(&parent.child_atom)
          .ok_or(Error::Invariant(model.atoms[parent.child_atom].id))?;
        let ray: Vector = target - points[parent.parent_atom];
        (target, (-ray.y).atan2(ray.x))
      } else {
        (Vector::default(), 0.)
      };
      let world: BTreeMap<_, _> = local
        .into_iter()
        .map(|(a, p)| (a, (p.rotate(angle.sin(), angle.cos()) + target).rounded()))
        .collect();
      for &a in &fragment.atoms {
        points[a] = world[&a];
      }
      transformed[id] = world;
    }
    Ok(points)
  }
}

/// Builds a fragment plus one ghost endpoint for each of its external ports.
fn frame(
  id: usize,
  model: &Model,
  forest: &Forest,
  seed: &[Vector],
  rules: &super::priority::Rules,
  scopes: &mut super::scopes::Scopes,
  work: &mut Work,
) -> Result<BTreeMap<usize, Vector>, Error> {
  let f = &forest.fragments[id];
  work.charge(f.atoms.len() + f.ports.len())?;
  let ring_atoms: BTreeSet<_> = f.rings.iter().flat_map(|&i| model.cycles[i].iter().copied()).collect();
  let mut points = BTreeMap::new();
  let mut seen = BTreeSet::new();
  let mut pending = VecDeque::new();
  for &node in &f.atoms {
    if ring_atoms.contains(&node) {
      points.insert(node, seed[node]);
      seen.insert(node);
      pending.push_back(node);
    }
  }
  if pending.is_empty() {
    let start = f.parent.map_or(f.atoms[0], |p| p.child_atom);
    points.insert(start, Vector::default());
    seen.insert(start);
    pending.push_back(start);
    if let Some(parent) = f.parent {
      points.insert(parent.parent_atom, Vector::new(-BOND, 0.));
    }
  }
  while let Some(node) = pending.pop_front() {
    work.charge(1 + model.neighbors[node].len())?;
    let origin = points[&node];
    let (neighbors, mut ray, angles) = if ring_atoms.contains(&node) {
      ring_slots(node, model, &points, &ring_atoms, work)?
    } else {
      let mut neighbors = rules.neighbors(model, node, work)?;
      let mut ray = Vector::new(-BOND, 0.);
      if let Some(i) = neighbors.iter().position(|n| seen.contains(n)) {
        ray = points[&neighbors[i]] - origin;
        neighbors.rotate_left(i);
      }
      let count = neighbors.len();
      let angles = if count == 4 && !super::priority::cross(model, node) {
        vec![
          std::f32::consts::PI / 3.,
          std::f32::consts::PI / 2.,
          2. * std::f32::consts::PI / 3.,
          std::f32::consts::PI / 2.,
        ]
      } else {
        let division = if count == 2
          && model.neighbors[node]
            .iter()
            .map(|&(_, e)| usize::from(model.edges[e].order))
            .sum::<usize>()
            < 4
          && (model.atoms[node].number == 6 || neighbors.iter().any(|&n| !super::priority::cross(model, n)))
        {
          3
        } else {
          count
        };
        vec![std::f32::consts::TAU / division.max(1) as f32; count]
      };
      (neighbors, ray, angles)
    };
    for (next, angle) in neighbors.into_iter().zip(angles) {
      if seen.contains(&next) {
        continue;
      }
      ray = ray.rotate(angle.sin(), angle.cos());
      points.insert(next, (origin + ray).rounded());
      scopes.placed(node, next, work)?;
      if forest.owner[next] == id {
        seen.insert(next);
        pending.push_back(next);
      }
    }
    if !ring_atoms.contains(&node) {
      super::stereo::orient_neighbors(model, node, &seen, &mut points, work)?;
    }
    scopes.finish(node, id, model, forest, work)?;
  }
  if f.atoms.iter().any(|node| !points.contains_key(node)) {
    return Err(Error::Invariant(model.atoms[f.atoms[0]].id));
  }
  Ok(points)
}

/// Splits the largest permitted ring gap among non-ring neighbors.
fn ring_slots(
  node: usize,
  model: &Model,
  points: &BTreeMap<usize, Vector>,
  ring_atoms: &BTreeSet<usize>,
  work: &mut Work,
) -> Result<(Vec<usize>, Vector, Vec<f32>), Error> {
  let origin = points[&node];
  let mut existing = Vec::new();
  let mut outside = Vec::new();
  for &(next, edge) in &model.neighbors[node] {
    if model.edges[edge].cyclic && ring_atoms.contains(&next) {
      let point = points.get(&next).ok_or(Error::Invariant(model.atoms[next].id))?;
      let angle = (point.y - origin.y)
        .atan2(point.x - origin.x)
        .rem_euclid(std::f32::consts::TAU);
      existing.push((angle, next));
    } else {
      outside.push(next);
    }
  }
  existing.sort_by(|a, b| a.0.total_cmp(&b.0));
  if existing.is_empty() {
    return Err(Error::Invariant(model.atoms[node].id));
  }
  let mut best = (f32::NEG_INFINITY, 0., existing[0].1);
  for i in 0..existing.len() {
    let gap = (existing[(i + 1) % existing.len()].0 - existing[i].0).rem_euclid(std::f32::consts::TAU);
    let middle = existing[i].0 + gap * 0.5;
    let p = origin + Vector::new(BOND * 0.1, 0.).rotate((-middle).sin(), (-middle).cos());
    let mut inside = false;
    for ring in model
      .cycles
      .iter()
      .filter(|ring| ring.len() < 9 && ring.contains(&node))
    {
      work.charge(ring.len())?;
      if contains(p, ring, points) {
        inside = true;
        break;
      }
    }
    let score = gap
      * if gap > std::f32::consts::PI {
        10.
      } else if inside {
        0.2
      } else {
        1.
      };
    if score > best.0 {
      best = (score, gap, existing[i].1);
    }
  }
  let count = outside.len();
  Ok((
    outside,
    points[&best.2] - origin,
    vec![-best.1 / (count + 1) as f32; count],
  ))
}

/// Tests a small ring's actual polygon without assigning chemical inside/outside facts.
fn contains(p: Vector, ring: &[usize], points: &BTreeMap<usize, Vector>) -> bool {
  let mut inside = false;
  for i in 0..ring.len() {
    let (Some(&a), Some(&b)) = (points.get(&ring[i]), points.get(&ring[(i + 1) % ring.len()])) else {
      return false;
    };
    if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
      inside = !inside;
    }
  }
  inside
}
