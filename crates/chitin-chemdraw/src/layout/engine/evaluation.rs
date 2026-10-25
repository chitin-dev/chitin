//! Conservative spatial pruning preserves interaction order, energies and forces.

use super::{Error, Work, interactions::Interaction, math::Vector};
use std::collections::BTreeMap;

struct Axis {
  ends: [usize; 2],
  radius: f64,
  terms: BTreeMap<usize, Vec<usize>>,
}

/// Immutable interaction incidence is reused across moving coordinate snapshots.
pub(super) struct Evaluator<'a> {
  terms: &'a [Interaction],
  ordinary: Vec<usize>,
  axes: Vec<Axis>,
}

impl<'a> Evaluator<'a> {
  /// Indexes finite-range terms without merging duplicate interactions.
  pub(super) fn new(terms: &'a [Interaction], work: &mut Work) -> Result<Self, Error> {
    work.charge(terms.len())?;
    let mut ordinary = Vec::new();
    let mut axes = BTreeMap::<[usize; 2], Axis>::new();
    for (index, term) in terms.iter().enumerate() {
      if let Interaction::Clash {
        atoms: [a, p, b],
        target_square,
        ..
      } = *term
      {
        let axis = axes.entry([a, b]).or_insert_with(|| Axis {
          ends: [a, b],
          radius: 0.,
          terms: BTreeMap::new(),
        });
        axis.radius = axis.radius.max(f64::from(target_square).sqrt());
        axis.terms.entry(p).or_default().push(index);
      } else {
        ordinary.push(index);
      }
    }
    Ok(Self {
      terms,
      ordinary,
      axes: axes.into_values().collect(),
    })
  }

  /// Evaluates every potentially nonzero term in its original accumulation order.
  ///
  /// # Errors
  /// Nonfinite coordinates and exhausted index/query work reject the evaluation.
  /// The broad phase may include extra terms, but never removes a nearby contact.
  pub(super) fn score(&self, points: &[Vector], forces: &mut [Vector], work: &mut Work) -> Result<f32, Error> {
    self.score_from(0., points, forces, work)
  }

  /// Continues an existing source-ordered sum without regrouping its additions.
  pub(super) fn score_from(
    &self,
    initial: f32,
    points: &[Vector],
    forces: &mut [Vector],
    work: &mut Work,
  ) -> Result<f32, Error> {
    if points.iter().any(|point| !point.finite()) {
      return Err(Error::CoordinateRange);
    }
    work.charge(points.len().saturating_mul(points.len().max(1).ilog2() as usize + 1))?;
    let mut order: Vec<_> = (0..points.len()).collect();
    order.sort_unstable_by(|&a, &b| points[a].x.total_cmp(&points[b].x).then(a.cmp(&b)));
    let mut selected = self.ordinary.clone();
    for axis in &self.axes {
      work.charge(1)?;
      let [a, b] = axis.ends.map(|atom| points[atom]);
      // Include roundoff at the segment-distance boundary rather than relying
      // on exact agreement between a square root and single-precision projection.
      let padding = axis.radius * 1.0001
        + 0.001
        + f64::from(a.x.abs().max(a.y.abs()).max(b.x.abs()).max(b.y.abs())) * f64::from(f32::EPSILON) * 8.;
      let left = f64::from(a.x.min(b.x)) - padding;
      let right = f64::from(a.x.max(b.x)) + padding;
      let bottom = f64::from(a.y.min(b.y)) - padding;
      let top = f64::from(a.y.max(b.y)) + padding;
      let start = order.partition_point(|&p| f64::from(points[p].x) < left);
      let end = order.partition_point(|&p| f64::from(points[p].x) <= right);
      work.charge(end - start)?;
      for &point in &order[start..end] {
        if (bottom..=top).contains(&f64::from(points[point].y))
          && let Some(indices) = axis.terms.get(&point)
        {
          selected.extend_from_slice(indices);
        }
      }
    }
    work.charge(selected.len())?;
    selected.sort_unstable();
    Ok(selected.into_iter().fold(initial, |energy, index| {
      energy + self.terms[index].score(points, forces)
    }))
  }
}
