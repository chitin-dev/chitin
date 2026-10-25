//! Immutable topology templates and bounded, source-ordered graph matching.
//! Source attribution and redistribution terms are in licenses/coordinate-generation.txt.

use crate::layout::{NativeFailureKind as Failure, NativeStage as Stage};

use super::ordering::connectivity;
use super::{
  Error, Work,
  math::{BOND, Vector, same_side},
  topology::Model,
};
use std::collections::BTreeMap;
mod data;

/// Templates encode drawing topology, not elemental or bond-order assignments.
struct Seed {
  points: &'static [[f32; 2]],
  edges: &'static [[usize; 2]],
}

/// Matches a complete central ring core and preserves source identity on output.
///
/// # Returns
/// The matched source atom rows, or `None` when no complete template is suitable.
/// Coordinates use the source's most-populated bond-length cluster normalization.
///
/// # Errors
/// Every score and backtracking expansion uses the shared finite work allowance.
pub(super) fn apply(
  model: &Model,
  rings: &[usize],
  points: &mut [Vector],
  work: &mut Work,
) -> Result<Option<Vec<usize>>, Error> {
  let mut atoms = Vec::new();
  for &ring in rings {
    let mut nodes = model.cycles[ring].clone();
    nodes.sort_unstable();
    for node in nodes {
      if !atoms.contains(&node) {
        atoms.push(node);
      }
    }
  }
  let n = atoms.len();
  let indices: BTreeMap<_, _> = atoms.iter().enumerate().map(|(i, &node)| (node, i)).collect();
  let edges: Vec<_> = model
    .edges
    .iter()
    .filter(|e| e.cyclic && e.ends.iter().all(|a| indices.contains_key(a)))
    .map(|e| e.ends.map(|a| indices[&a]))
    .collect();
  let (scores, iterations) = connectivity(n, &edges, work)?;
  for seed in data::SEEDS
    .iter()
    .filter(|seed| seed.points.len() == n && seed.edges.len() == edges.len())
  {
    let (other, count) = connectivity(n, seed.edges, work)?;
    if count != iterations {
      continue;
    }
    work.charge(2 * n * n)?;
    let mut source = vec![vec![false; n]; n];
    let mut target = source.clone();
    for &[a, b] in &edges {
      source[a][b] = true;
      source[b][a] = true;
    }
    for &[a, b] in seed.edges {
      target[a][b] = true;
      target[b][a] = true;
    }
    let mut mapping = vec![usize::MAX; n];
    let mut used = vec![false; n];
    let mut cursor = vec![0; n];
    let mut depth = 0;
    loop {
      work.charge(1)?;
      if depth == n {
        let valid = model
          .stereo
          .iter()
          .filter(|(ids, _)| ids.iter().all(|id| indices.contains_key(id)))
          .all(|(ids, expected)| {
            let [ra, a, b, rb] = ids.map(|id| {
              let [x, y] = seed.points[mapping[indices[&id]]];
              Vector::new(x, y)
            });
            same_side(ra, rb, a, b) == *expected
          });
        if valid {
          let coordinates = normalized(seed, work)?;
          for (i, &atom) in atoms.iter().enumerate() {
            points[atom] = coordinates[mapping[i]].rounded();
          }
          return Ok(Some(atoms));
        }
        depth -= 1;
        used[mapping[depth]] = false;
        continue;
      }
      let mut chosen = None;
      while cursor[depth] < n {
        let candidate = cursor[depth];
        cursor[depth] += 1;
        work.charge(1 + depth)?;
        if used[candidate] || scores[depth] != other[candidate] {
          continue;
        }
        if (0..depth).all(|previous| source[depth][previous] == target[candidate][mapping[previous]]) {
          chosen = Some(candidate);
          break;
        }
      }
      if let Some(candidate) = chosen {
        mapping[depth] = candidate;
        used[candidate] = true;
        depth += 1;
        if depth < n {
          cursor[depth] = 0;
        }
      } else if depth == 0 {
        break;
      } else {
        depth -= 1;
        used[mapping[depth]] = false;
      }
    }
  }
  Ok(None)
}

/// Uses the first representative in the most populated squared-length cluster.
fn normalized(seed: &Seed, work: &mut Work) -> Result<Vec<Vector>, Error> {
  // Template loading assigns coordinates before normalization. That assignment
  // quantizes the raw data; rounding only the final drawing changes cage shape.
  let raw: Vec<_> = seed.points.iter().map(|&[x, y]| Vector::new(x, y).rounded()).collect();
  let mut groups = Vec::<(f32, usize)>::new();
  for &[a, b] in seed.edges {
    work.charge(1 + groups.len())?;
    let distance = (raw[a] - raw[b]).square();
    if let Some((_, count)) = groups
      .iter_mut()
      .find(|(old, _)| distance * 0.9 < *old && distance * 1.1 > *old)
    {
      *count += 1;
    } else {
      groups.push((distance, 1));
    }
  }
  let mut best = 0;
  for i in 1..groups.len() {
    if groups[i].1 > groups[best].1 {
      best = i;
    }
  }
  let scale = groups.get(best).map_or(1., |(distance, _)| distance.sqrt());
  if scale <= 0. {
    return Err(Error::native(Stage::Template, Failure::InvalidTemplateScale));
  }
  Ok(raw.into_iter().map(|p| p / scale * BOND).collect())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn every_template_should_match_recorded_normalized_coordinates() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/templates.tsv"));
    let mut count = 0;
    for line in fixture.lines() {
      let mut fields = line.split_whitespace();
      let index: usize = fields.next().ok_or("missing seed index")?.parse()?;
      assert_eq!(index, count);
      let expected: Vec<f32> = fields.map(str::parse).collect::<Result<_, _>>()?;
      let actual = normalized(&data::SEEDS[index], &mut Work::new(1_000_000))?;
      assert_eq!(expected.len(), 2 * actual.len());
      for (i, point) in actual.iter().enumerate() {
        for (axis, actual) in [point.x / BOND, point.y / BOND].into_iter().enumerate() {
          let expected = expected[2 * i + axis];
          assert!(
            (actual - expected).abs() < 0.000002,
            "seed {index}, atom {i}, axis {axis}: {actual} != {expected}"
          );
        }
      }
      count += 1;
    }
    assert_eq!(count, data::SEEDS.len());
    Ok(())
  }
}
