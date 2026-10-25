//! Fragment reflection choices align terminal groups and downstream connections.
//! Source attribution and redistribution terms are in licenses/coordinate-generation.txt.

use super::{
  Error, Work,
  fragments::Forest,
  math::{BOND, EPSILON, Vector},
  topology::Model,
};
use std::collections::BTreeMap;

/// Expresses local frames relative to the incoming atom and a leftward parent ray.
pub(super) fn normalize(
  frames: &mut [BTreeMap<usize, Vector>],
  model: &Model,
  forest: &Forest,
  work: &mut Work,
) -> Result<(), Error> {
  for (id, frame) in frames.iter_mut().enumerate() {
    let f = &forest.fragments[id];
    let origin = frame[&f.parent.map_or(f.atoms[0], |p| p.child_atom)];
    let (sine, cosine) = if let Some(p) = f.parent {
      let axis = (frame[&p.parent_atom] - origin).normalized();
      if axis.square() < EPSILON {
        return Err(Error::Invariant(model.atoms[p.child_atom].id));
      }
      (-axis.y, -axis.x)
    } else {
      (0., 1.)
    };
    work.charge(frame.len())?;
    for point in frame.values_mut() {
      *point = (*point - origin).rotate(sine, cosine);
    }
  }
  Ok(())
}

/// Computes downstream drawing lengths in reverse breadth-first order, without recursion.
pub(super) fn downstream(
  frames: &[BTreeMap<usize, Vector>],
  forest: &Forest,
  work: &mut Work,
) -> Result<Vec<f32>, Error> {
  work.charge(forest.fragments.len())?;
  let mut lengths = vec![0_f32; forest.fragments.len()];
  for &id in forest.order.iter().rev() {
    let f = &forest.fragments[id];
    let longest = f.children.iter().map(|&child| lengths[child]).fold(0., f32::max);
    let entry = f.parent.map_or(0., |p| frames[p.fragment][&p.child_atom].length());
    lengths[id] = longest + entry;
  }
  Ok(lengths)
}

/// Terminal edges include internal leaves, child connections and the incoming edge.
fn terminals(id: usize, model: &Model, forest: &Forest) -> Vec<usize> {
  let f = &forest.fragments[id];
  f.edges
    .iter()
    .copied()
    .filter(|&edge| model.edges[edge].ends.iter().any(|&a| model.neighbors[a].len() == 1))
    .chain(
      f.children
        .iter()
        .filter_map(|&child| forest.fragments[child].parent.map(|p| p.edge)),
    )
    .chain(f.parent.map(|p| p.edge))
    .collect()
}

/// Exposes oriented layout endpoints without changing the source bond's direction.
pub(super) fn oriented(edge: usize, model: &Model, forest: &Forest) -> [usize; 2] {
  let ends = model.edges[edge].ends;
  for owner in ends.map(|node| forest.owner[node]) {
    if let Some(p) = forest.fragments[owner].parent
      && p.edge == edge
    {
      return [p.parent_atom, p.child_atom];
    }
  }
  ends
}

fn terminal_weight(edge: usize, model: &Model) -> f32 {
  let mut weight = if model.edges[edge].order == 2 { 0.82 } else { 1. };
  if model.edges[edge]
    .ends
    .iter()
    .any(|&node| model.neighbors[node].len() == 1 && model.atoms[node].number != 6)
  {
    weight *= 0.9;
  }
  weight
}

/// Scores source plain/reflected terminal directions before placing any child atoms.
///
/// # Returns
/// Whether to reflect the child's entire local frame about its incoming axis.
/// Declared chemical descriptors are unaffected; depiction derives wedges later.
///
/// # Errors
/// Work exhaustion rejects this complete assembly proposal.
pub(super) struct Alignment<'a> {
  pub model: &'a Model,
  pub forest: &'a Forest,
  pub lengths: &'a [f32],
}

impl Alignment<'_> {
  pub(super) fn reflected(
    &self,
    id: usize,
    frame: &BTreeMap<usize, Vector>,
    parent_frame: &BTreeMap<usize, Vector>,
    points: &[Vector],
    rotation: [f32; 2],
    work: &mut Work,
  ) -> Result<bool, Error> {
    let (model, forest, lengths) = (self.model, self.forest, self.lengths);
    let [sine, cosine] = rotation;
    let Some(parent) = forest.fragments[id].parent else {
      return Ok(false);
    };
    let origin = (points[parent.parent_atom] + parent_frame[&parent.child_atom]) * 0.5;
    let get = |node: usize| parent_frame.get(&node).copied().unwrap_or(points[node]);
    let mut directions = Vec::new();
    for edge in terminals(parent.fragment, model, forest) {
      work.charge(1)?;
      let [a, b] = oriented(edge, model, forest);
      if forest.owner[b] == id {
        continue;
      }
      let ray = (origin - (get(a) + get(b)) * 0.5).normalized();
      let mut weight = terminal_weight(edge, model);
      if forest.owner[a] != parent.fragment || forest.owner[b] != parent.fragment {
        weight = lengths[forest.owner[b]] * 0.1;
        if forest.fragments[parent.fragment]
          .parent
          .is_some_and(|p| forest.owner[a] == p.fragment)
        {
          weight *= 100.;
        }
      }
      directions.push((ray, weight));
    }
    let mut best = 0.;
    let mut invert = false;
    for edge in terminals(id, model, forest) {
      let [a, b] = oriented(edge, model, forest);
      if forest.owner[a] != id {
        continue;
      }
      let ray = ((frame[&a] + frame[&b]) * 0.5 - Vector::new(-BOND * 0.5, 0.)).normalized();
      let plain = ray.rotate(sine, cosine);
      let flipped = Vector::new(ray.x, -ray.y).rotate(sine, cosine);
      let weight = if forest.owner[b] != id {
        lengths[forest.owner[b]] * 0.1
      } else {
        terminal_weight(edge, model)
      };
      for &(direction, priority) in &directions {
        work.charge(1)?;
        let score = |ray: Vector| {
          let dot = ray.dot(direction).max(0.);
          (dot * dot + if dot > 1. - EPSILON { 1000. } else { 0. }) * priority * weight
        };
        let normal = score(plain);
        if normal > best {
          best = normal;
          invert = false;
        }
        let mirrored = score(flipped);
        if mirrored > best {
          best = mirrored;
          invert = true;
        }
      }
    }
    Ok(invert)
  }
}
