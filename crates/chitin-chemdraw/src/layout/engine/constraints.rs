//! Source drawing interactions are assembled separately from the simulation loop.
//! Source attribution and redistribution terms are in licenses/coordinate-generation.txt.

use super::{
  Error, Work,
  interactions::Interaction,
  math::{BOND, Vector, same_side},
  topology::Model,
};

/// Builds source-ordered atom-edge, stretch, bend and declared alkene terms.
///
/// # Errors
/// Pair enumeration and ring inspection consume the caller's finite work budget.
pub(super) fn build(
  model: &Model,
  points: &[Vector],
  rigid: &[bool],
  work: &mut Work,
) -> Result<Vec<Interaction>, Error> {
  let mut result = Vec::new();
  let mut component = vec![0; model.atoms.len()];
  for (i, nodes) in model.components.iter().enumerate() {
    for &node in nodes {
      component[node] = i;
    }
  }
  for (node, atom) in model.atoms.iter().enumerate() {
    for edge in &model.edges {
      work.charge(1)?;
      let [a, b] = edge.ends;
      if (rigid[node] && rigid[a] && rigid[b])
        || node == a
        || node == b
        || component[node] != component[a]
        || model.neighbors[node].iter().any(|&(other, _)| other == a || other == b)
      {
        continue;
      }
      let mut scale = 0.8;
      if atom.number == 6 && atom.charge == 0 {
        scale -= 0.1;
      }
      if [a, b]
        .iter()
        .all(|&i| model.atoms[i].number == 6 && model.atoms[i].charge == 0)
      {
        scale -= 0.1;
      }
      result.push(Interaction::Clash {
        atoms: [a, node, b],
        target_square: (BOND * scale) * (BOND * scale),
        k: 1.,
        k2: 0.1,
      });
    }
  }
  for edge in &model.edges {
    let [a, b] = edge.ends;
    work.charge(model.cycles.len())?;
    let small = model
      .cycles
      .iter()
      .any(|ring| ring.len() < 9 && ring.contains(&a) && ring.contains(&b));
    result.push(Interaction::Stretch {
      atoms: edge.ends,
      target: if rigid[a] && rigid[b] {
        (points[a] - points[b]).length()
      } else {
        BOND
      },
      tolerance: 0.,
      k: if small { 5. } else { 0.1 },
    });
  }
  for (node, row) in model.neighbors.iter().enumerate() {
    if row.len() < 2 {
      continue;
    }
    work.charge(row.len())?;
    let center = points[node];
    let reference = points[row[0].0] - center;
    let mut ordered: Vec<_> = row
      .iter()
      .map(|&(next, _)| {
        let ray = points[next] - center;
        let angle = (reference.x * ray.y - reference.y * ray.x)
          .atan2(reference.dot(ray))
          .rem_euclid(std::f32::consts::TAU);
        (angle, next)
      })
      .collect();
    ordered.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let base = if row.len() == 2 {
      if row
        .iter()
        .map(|&(_, e)| usize::from(model.edges[e].order))
        .sum::<usize>()
        > 3
      {
        180.
      } else {
        120.
      }
    } else {
      360. / row.len() as f32
    };
    let mut terms = Vec::new();
    let mut rings = Vec::new();
    let mut ordinary = Vec::new();
    let mut inverted = false;
    for i in 0..ordered.len() {
      if ordered.len() == 2 && i == 1 {
        continue;
      }
      let a = ordered[i].1;
      let b = ordered[(i + ordered.len() - 1) % ordered.len()].1;
      let mut target = base;
      let mut k = 1.;
      let mut in_ring = false;
      work.charge(model.cycles.len())?;
      if let Some(ring) = model
        .cycles
        .iter()
        .filter(|ring| ring.contains(&a) && ring.contains(&node) && ring.contains(&b))
        .min_by_key(|ring| ring.len())
      {
        if ring.len() < 9 {
          let mut extra = 0;
          for other in model.cycles.iter().filter(|other| other.len() < 9 && *other != ring) {
            work.charge(ring.len() + other.len())?;
            let common = other.iter().filter(|n| ring.contains(n)).count();
            if common > 2 {
              extra += other.len() - common;
            }
          }
          target = 180. - 360. / (ring.len() + extra) as f32;
          k = 100.;
          in_ring = true;
        } else {
          if row.len() == 3
            && let Some((_, other)) = ordered.iter().find(|&&(_, other)| other != a && other != b)
            && same_side(points[b], points[*other], points[a], points[node])
          {
            inverted = true;
          }
          let fused = row.len() > 2
            && row
              .iter()
              .all(|&(other, _)| model.cycles.iter().any(|r| r.contains(&node) && r.contains(&other)));
          in_ring = fused || inverted;
        }
      }
      if rigid[a] && rigid[node] && rigid[b] {
        let v1 = points[a] - points[node];
        let v2 = points[b] - points[node];
        target = (v1.dot(v2) / (v1.square().sqrt() * v2.square().sqrt()).max(super::math::EPSILON))
          .clamp(-1., 1.)
          .acos()
          .to_degrees();
      }
      if in_ring {
        rings.push(terms.len());
      } else {
        ordinary.push(terms.len());
      }
      terms.push(Interaction::Bend {
        atoms: [a, node, b],
        target,
        k,
        k2: 0.05,
      });
    }
    if rings.len() != 1 || ordinary.len() != 2 {
      inverted = false;
    }
    if !rings.is_empty() {
      let mut sum: f32 = rings
        .iter()
        .map(|&i| match terms[i] {
          Interaction::Bend { target, .. } => target,
          _ => 0.,
        })
        .sum();
      if inverted {
        sum = 360. - sum;
      }
      let target = (360. - sum) / ordinary.len().max(1) as f32;
      for &i in &ordinary {
        if let Interaction::Bend {
          target: ref mut value, ..
        } = terms[i]
        {
          *value = target;
        }
      }
    } else if ordinary.len() == 4 {
      let angle = |index: usize| {
        let a = points[ordered[index].1] - center;
        let b = points[ordered[(index + 3) % 4].1] - center;
        (a.dot(b) / (a.square().sqrt() * b.square().sqrt()).max(super::math::EPSILON))
          .clamp(-1., 1.)
          .acos()
      };
      let mut biggest = 0;
      for i in 1..4 {
        if angle(i) > angle(biggest) {
          biggest = i;
        }
      }
      let targets = if super::priority::cross(model, node) {
        [90.; 4]
      } else {
        [120., 90., 60., 90.]
      };
      for (offset, target) in targets.into_iter().enumerate() {
        if let Interaction::Bend {
          target: ref mut value, ..
        } = terms[(biggest + offset) % 4]
        {
          *value = target;
        }
      }
    }
    result.extend(terms);
  }
  for &(atoms, same_side) in &model.stereo {
    result.push(Interaction::Alkene { atoms, same_side });
  }
  Ok(result)
}
