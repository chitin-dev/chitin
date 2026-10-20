//! Camera-only dense-shell candidates: no planar solve or per-atom warping.

use super::{Error, POSES, Point, Pose, Work, cross, dot, normalize, sphere};
use chitin_chem::{BondId, BondOrder, perception::RingBlock};

/// Projects one complete spatial seed using rigid camera axes and uniform scale.
///
/// # Parameters
/// - `block`, `edges`, `bonds`, `orders`: matching source coverage and integer orders.
/// - `seed`: bounded drawing-only spatial coordinates, not a chemical conformer.
/// - `work`: shared model, view-search and validation allowance.
///
/// # Returns
/// Complete camera views retaining source depth and every bond. All intersections
/// must have an unambiguous interpolated depth order. Dense shells use depth cues
/// rather than cutting away crowded multiple-bond strokes during depiction.
///
/// # Errors
/// Exhaustion or invalid numeric geometry rejects an incomplete view.
pub(super) fn project(
  block: &RingBlock,
  edges: &[[usize; 2]],
  bonds: &[BondId],
  orders: &[BondOrder],
  seed: &[[f64; 3]],
  work: &mut Work,
) -> Result<Option<Vec<Pose>>, Error> {
  if block.atoms().len() != seed.len()
    || orders
      .iter()
      .any(|order| !matches!(order, BondOrder::Single | BondOrder::Double))
  {
    return Ok(None);
  }
  let mut candidates = Vec::new();
  // The seed consumed most of the shared allowance. Retain completed candidates
  // if a later optional camera scan is interrupted, reserving publication work.
  work.optional(10_000, |work| {
    for direction in sphere(192) {
      work.charge(seed.len() * seed.len() + edges.len() * 4)?;
      let reference = if direction[2].abs() > 0.9 {
        [0., 1., 0.]
      } else {
        [0., 0., 1.]
      };
      let right = normalize(cross(reference, direction))?;
      let up = cross(direction, right);
      let mut points: Vec<_> = seed.iter().map(|&p| Point::new(dot(p, right), dot(p, up))).collect();
      let mut depths: Vec<_> = seed.iter().map(|&p| dot(p, direction)).collect();
      let lengths: Vec<_> = edges.iter().map(|&[a, b]| points[a].distance(points[b])).collect();
      let shortest = lengths.iter().copied().fold(f64::INFINITY, f64::min);
      let longest = lengths.iter().copied().fold(0_f64, f64::max);
      let mut nearest = f64::INFINITY;
      for (i, &point) in points.iter().enumerate() {
        for &other in &points[i + 1..] {
          nearest = nearest.min(point.distance(other));
        }
      }
      if shortest < 1e-8 || nearest < 1e-8 || longest / shortest > 6. {
        continue;
      }
      let scale = (0.65 / shortest).max(0.6 / nearest);
      if !scale.is_finite() || scale > 8. {
        continue;
      }
      for point in &mut points {
        point.x *= scale;
        point.y *= scale;
      }
      for depth in &mut depths {
        *depth *= scale;
      }
      let quality = work.measure(&points, edges)?;
      if !quality.crowds.is_empty() {
        continue;
      }
      let mut crossings = Vec::new();
      let mut tied = false;
      for &(i, j) in &quality.crossings {
        let [a, b] = edges[i];
        let [c, d] = edges[j];
        let delta = |a: Point, b: Point| Point::new(a.x - b.x, a.y - b.y);
        let cross = |a: Point, b: Point| a.x * b.y - a.y * b.x;
        let u = delta(points[b], points[a]);
        let v = delta(points[d], points[c]);
        let offset = delta(points[c], points[a]);
        let determinant = cross(u, v);
        let t = cross(offset, v) / determinant;
        let s = cross(offset, u) / determinant;
        let first = depths[a] + t * (depths[b] - depths[a]);
        let second = depths[c] + s * (depths[d] - depths[c]);
        if (first - second).abs() < 0.08 * scale {
          tied = true;
          break;
        }
        crossings.push(if first > second {
          super::super::BondCrossing {
            front: bonds[i],
            back: bonds[j],
          }
        } else {
          super::super::BondCrossing {
            front: bonds[j],
            back: bonds[i],
          }
        });
      }
      if tied {
        continue;
      }
      let cost = longest / shortest + scale + quality.crossings.len() as f64 * 0.05;
      candidates.push((
        cost,
        Pose {
          points,
          crossings,
          depths,
        },
      ));
    }
    Ok(())
  })?;
  candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
  Ok((!candidates.is_empty()).then(|| candidates.into_iter().take(POSES).map(|(_, pose)| pose).collect()))
}
