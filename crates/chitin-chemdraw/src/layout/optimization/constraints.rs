//! Dense constraint snapshots and deterministic distance projection.

use super::{BOND_TOLERANCE, OptimizationError};
use crate::geometry::Point;
use crate::layout::quality::Budget;
use chitin_chem::{AtomId, BondId, BondOrder, DoubleBondRelation, MolecularGraph};
use std::{
  collections::{BTreeMap, BTreeSet, VecDeque},
  f64::consts::PI,
};

/// A local bond-angle reference; protected centers also constrain neighbor distance.
pub(super) struct Angle {
  /// Neighbor, center, neighbor dense rows.
  pub(super) rows: [usize; 3],
  /// Initial unsigned angle in radians.
  pub(super) reference: f64,
  /// Initial signed orientation of the two rays.
  orientation: f64,
  /// Multiple-bond, stereocenter, or small-ring geometry cannot flex.
  pub(super) hard: bool,
}

/// A distance equation projected independently of the objective function.
struct Distance {
  /// Dense endpoints; source bonds and protected angle chords share this model.
  ends: [usize; 2],
  /// Normalized target distance; source bonds always target one.
  target: f64,
}

/// A bounded neighbor chord equivalent to an angle between two unit bonds.
struct Interval {
  /// Neighbor endpoints around one flexible center.
  ends: [usize; 2],
  /// Minimum and maximum chord lengths, derived from initial angle bounds.
  limits: [f64; 2],
}

/// Complete component metadata with graph-local IDs at its boundary only.
pub(super) struct Model {
  /// Source IDs, in deterministic graph iteration order.
  pub(super) ids: Vec<AtomId>,
  /// Unit source edges, aligned with `bonds`.
  pub(super) edges: Vec<[usize; 2]>,
  /// Source handles for final diagnostics.
  pub(super) bonds: Vec<BondId>,
  /// Dense neighboring atoms.
  pub(super) rows: Vec<Vec<usize>>,
  /// Caller anchors and conservative small-ring template pins.
  pub(super) pinned: Vec<bool>,
  /// Angle references and hard/soft classification.
  pub(super) angles: Vec<Angle>,
  /// Vertices incident to multiple bonds or carrying supplied tetrahedral facts.
  protected: Vec<bool>,
  /// Graph-coordinate independent source double relations.
  stereo: Vec<([usize; 4], DoubleBondRelation)>,
  /// Unit bond and protected-angle chord equations, deduplicated by endpoint pair.
  distances: Vec<Distance>,
  /// Active soft-angle bounds, projected together with the distance equations.
  intervals: Vec<Interval>,
}

impl Model {
  /// Builds mandatory metadata without running any unbounded geometric search.
  ///
  /// # Errors
  /// Reports invalid graph references or unsupported integer bond orders.
  pub(super) fn topology(
    graph: &MolecularGraph,
    ids: &[AtomId],
    anchors: &BTreeSet<AtomId>,
  ) -> Result<Self, OptimizationError> {
    let indices: BTreeMap<_, _> = ids.iter().enumerate().map(|(i, &id)| (id, i)).collect();
    let mut edges = Vec::new();
    let mut bonds = Vec::new();
    let mut rows = vec![Vec::new(); ids.len()];
    let mut protected: Vec<_> = ids.iter().map(|&id| graph.tetrahedral(id).is_some()).collect();
    for (id, bond) in graph.bonds() {
      let [a, b] = bond.atoms();
      let (Some(&a), Some(&b)) = (indices.get(&a), indices.get(&b)) else {
        continue;
      };
      if !matches!(bond.order(), BondOrder::Single | BondOrder::Double | BondOrder::Triple) {
        return Err(OptimizationError::Bond(id));
      }
      rows[a].push(b);
      rows[b].push(a);
      edges.push([a, b]);
      bonds.push(id);
      if bond.order() != BondOrder::Single {
        protected[a] = true;
        protected[b] = true;
      }
    }
    for row in &mut rows {
      row.sort_unstable();
    }
    let stereo = graph
      .double_bond_stereos()
      .filter_map(|(_, s)| {
        let [a, b] = s.ends();
        let [ra, rb] = s.references();
        Some((
          [
            *indices.get(&a)?,
            *indices.get(&b)?,
            *indices.get(&ra)?,
            *indices.get(&rb)?,
          ],
          s.relation(),
        ))
      })
      .collect();
    Ok(Self {
      ids: ids.to_vec(),
      edges,
      bonds,
      rows,
      pinned: ids.iter().map(|id| anchors.contains(id)).collect(),
      angles: Vec::new(),
      protected,
      stereo,
      distances: Vec::new(),
      intervals: Vec::new(),
    })
  }

  /// Gathers a component snapshot from a previously checked global ID map.
  pub(super) fn initial(&self, points: &BTreeMap<AtomId, Point>) -> Vec<Point> {
    self.ids.iter().map(|id| points[id]).collect()
  }

  /// Classifies short rings and snapshots hard distances and bounded soft angles.
  ///
  /// Each edge is tested for a short alternate path. Witnesses pin existing
  /// small-ring templates; no cycle count is mistaken for a chemical ring size.
  /// Returns false before exceeding the shared preparation/search allowance.
  pub(super) fn prepare(
    &mut self,
    graph: &MolecularGraph,
    initial: &[Point],
    bridges: &BTreeSet<BondId>,
    budget: &mut Budget,
  ) -> bool {
    for (edge, &[start, end]) in self.edges.iter().enumerate() {
      if bridges.contains(&self.bonds[edge]) {
        continue;
      }
      if !budget.charge(self.ids.len()) {
        return false;
      }
      let mut previous = vec![None; self.ids.len()];
      let mut depth = vec![usize::MAX; self.ids.len()];
      let mut pending = VecDeque::from([start]);
      depth[start] = 0;
      while let Some(node) = pending.pop_front() {
        if !budget.charge(1 + self.rows[node].len()) {
          return false;
        }
        if depth[node] >= 7 || previous[end].is_some() {
          continue;
        }
        for &other in &self.rows[node] {
          if (node == start && other == end) || (node == end && other == start) || depth[other] != usize::MAX {
            continue;
          }
          depth[other] = depth[node] + 1;
          previous[other] = Some(node);
          pending.push_back(other);
        }
      }
      if previous[end].is_some() {
        let mut node = end;
        self.pinned[start] = true;
        self.protected[start] = true;
        while node != start {
          self.pinned[node] = true;
          self.protected[node] = true;
          let Some(parent) = previous[node] else {
            return false;
          };
          node = parent;
        }
      }
    }
    let mut pairs = BTreeMap::new();
    for &[a, b] in &self.edges {
      pairs.insert((a.min(b), a.max(b)), 1.);
    }
    for (center, row) in self.rows.iter().enumerate() {
      for (offset, &a) in row.iter().enumerate() {
        for &b in &row[offset + 1..] {
          if !budget.charge(1) {
            return false;
          }
          let reference = angle(initial, [a, center, b]);
          let orientation = cross(initial[a], initial[center], initial[b]);
          let hard = self.protected[center];
          if hard {
            pairs
              .entry((a.min(b), a.max(b)))
              .or_insert(initial[a].distance(initial[b]));
          } else {
            let low = reference.min(PI / 3.).max(reference - PI / 6.);
            let high = reference.max(5. * PI / 6.).min(reference + PI / 6.);
            self.intervals.push(Interval {
              ends: [a, b],
              limits: [2. * (low / 2.).sin(), 2. * (high / 2.).sin()],
            });
          }
          self.angles.push(Angle {
            rows: [a, center, b],
            reference,
            orientation,
            hard,
          });
        }
      }
      // Aromatic C/N supplied through the integer adapter still carry multiple
      // bond constraints. No aromaticity or stereogenicity is inferred here.
      if graph.atom(self.ids[center]).is_none() {
        return false;
      }
    }
    self.distances = pairs
      .into_iter()
      .map(|((a, b), target)| Distance { ends: [a, b], target })
      .collect();
    true
  }

  /// Projects all equations while leaving pinned centers untouched.
  ///
  /// Returns `None` for budget exhaustion, `Some(false)` for an infeasible or
  /// unconverged trial, and true only after a complete residual scan. A failed
  /// scratch trial is never returned as relaxed geometry.
  pub(super) fn project(&self, points: &mut [Point], passes: usize, budget: &mut Budget) -> Option<bool> {
    for _ in 0..passes {
      if !budget.charge(2 * (self.distances.len() + self.intervals.len())) {
        return None;
      }
      for equation in &self.distances {
        if !self.adjust(points, equation.ends, equation.target) {
          return Some(false);
        }
      }
      for interval in &self.intervals {
        let [a, b] = interval.ends;
        let target = points[a]
          .distance(points[b])
          .clamp(interval.limits[0], interval.limits[1]);
        if !self.adjust(points, interval.ends, target) {
          return Some(false);
        }
      }
      if self
        .distances
        .iter()
        .all(|d| (points[d.ends[0]].distance(points[d.ends[1]]) - d.target).abs() <= 1e-10)
        && self.intervals.iter().all(|d| {
          let length = points[d.ends[0]].distance(points[d.ends[1]]);
          length >= d.limits[0] - 1e-10 && length <= d.limits[1] + 1e-10
        })
      {
        return Some(true);
      }
    }
    Some(false)
  }

  /// Applies one distance correction with zero inverse weight for pinned rows.
  fn adjust(&self, points: &mut [Point], [a, b]: [usize; 2], target: f64) -> bool {
    let length = points[a].distance(points[b]);
    let error = length - target;
    if !length.is_finite() {
      return false;
    }
    if error.abs() <= 1e-11 {
      return true;
    }
    let wa = if self.pinned[a] { 0. } else { 1. };
    let wb = if self.pinned[b] { 0. } else { 1. };
    if wa + wb == 0. {
      return false;
    }
    let (ux, uy) = if length > 1e-12 {
      (
        (points[b].x - points[a].x) / length,
        (points[b].y - points[a].y) / length,
      )
    } else {
      (1., 0.)
    };
    let delta = error / (wa + wb);
    if wa > 0. {
      points[a] = points[a].translated(ux * delta, uy * delta);
    }
    if wb > 0. {
      points[b] = points[b].translated(-ux * delta, -uy * delta);
    }
    true
  }

  /// Verifies pins, source bonds, protected/soft angles, range and supplied stereo.
  ///
  /// # Parameters
  /// - `initial`: immutable stage-entry coordinates defining pins and angle limits.
  /// - `points`: complete projected trial in the identical dense namespace.
  /// - `budget`: shared allowance charged before any acceptance scan.
  ///
  /// # Returns
  /// `Some(true)` only for a complete feasible trial; false discards it and
  /// `None` signals exhaustion. Projection success alone is never acceptance.
  pub(super) fn valid(&self, initial: &[Point], points: &[Point], budget: &mut Budget) -> Option<bool> {
    if !budget.charge(points.len() + self.edges.len() + self.angles.len() + self.stereo.len()) {
      return None;
    }
    if points
      .iter()
      .enumerate()
      .any(|(i, p)| !p.is_finite() || p.x.abs() > 1e9 || p.y.abs() > 1e9 || (self.pinned[i] && *p != initial[i]))
    {
      return Some(false);
    }
    if self
      .edges
      .iter()
      .any(|&[a, b]| (points[a].distance(points[b]) - 1.).abs() > BOND_TOLERANCE)
    {
      return Some(false);
    }
    for constraint in &self.angles {
      let actual = angle(points, constraint.rows);
      if !actual.is_finite() {
        return Some(false);
      }
      if constraint.hard {
        let [a, c, b] = constraint.rows;
        if (actual.cos() - constraint.reference.cos()).abs() > 1e-8
          || (constraint.orientation.abs() > 1e-8
            && constraint.orientation * cross(points[a], points[c], points[b]) <= 0.)
        {
          return Some(false);
        }
      } else if (actual - constraint.reference).abs() > PI / 6. + 1e-8
        || actual < constraint.reference.min(PI / 3.) - 1e-8
        || actual > constraint.reference.max(5. * PI / 6.) + 1e-8
      {
        return Some(false);
      }
    }
    Some(
      self
        .stereo
        .iter()
        .all(|(rows, expected)| crate::stereo_geometry::relation(rows.map(|i| points[i])) == Some(*expected)),
    )
  }
}

/// Computes an unsigned angle using normalized rays, including linear centers.
pub(super) fn angle(points: &[Point], [a, c, b]: [usize; 3]) -> f64 {
  let (u, v) = (points[a], points[b]);
  let p = points[c];
  (((u.x - p.x) * (v.x - p.x) + (u.y - p.y) * (v.y - p.y)) / (u.distance(p) * v.distance(p)))
    .clamp(-1., 1.)
    .acos()
}

/// Signed cross product of two outgoing rays at the center.
fn cross(a: Point, c: Point, b: Point) -> f64 {
  (a.x - c.x) * (b.y - c.y) - (a.y - c.y) * (b.x - c.x)
}

#[cfg(test)]
mod tests {
  use super::*;
  use chitin_chem::{io::smiles::read_smiles, perception::analyze_rings};
  type TestResult = Result<(), Box<dyn std::error::Error>>;

  #[test]
  fn hard_multiple_bond_angles_should_not_be_traded_for_clearance() -> TestResult {
    for (source, points, center, leaf) in [
      (
        "CC(=O)C",
        vec![
          Point::new(-1., 0.),
          Point::new(0., 0.),
          Point::new(0.5, 3_f64.sqrt() / 2.),
          Point::new(0.5, -3_f64.sqrt() / 2.),
        ],
        1,
        2,
      ),
      (
        "C#CC",
        vec![Point::new(-1., 0.), Point::new(0., 0.), Point::new(1., 0.)],
        1,
        2,
      ),
    ] {
      let graph = read_smiles(source)?.into_graph();
      let ids: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
      let mut model = Model::topology(&graph, &ids, &BTreeSet::new())?;
      assert!(model.prepare(
        &graph,
        &points,
        analyze_rings(&graph)?.bridges(),
        &mut Budget(1_000_000)
      ));
      let mut moved = points.clone();
      let p = points[leaf];
      let c = points[center];
      let (sin, cos) = (PI / 12.).sin_cos();
      moved[leaf] = c.translated(
        (p.x - c.x) * cos - (p.y - c.y) * sin,
        (p.x - c.x) * sin + (p.y - c.y) * cos,
      );
      assert_eq!(model.valid(&points, &moved, &mut Budget(1_000_000)), Some(false));
    }
    Ok(())
  }

  #[test]
  fn small_cycle_template_rows_should_be_fixed_without_freezing_macrocycles() -> TestResult {
    for (source, expected) in [("C1CCCCC1", true), ("C1CCCCCCCCCCC1", false)] {
      let graph = read_smiles(source)?.into_graph();
      let ids: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
      let n = ids.len();
      let radius = 1. / (2. * (PI / n as f64).sin());
      let points: Vec<_> = (0..n)
        .map(|i| {
          let angle = 2. * PI * i as f64 / n as f64;
          Point::new(radius * angle.cos(), radius * angle.sin())
        })
        .collect();
      let mut model = Model::topology(&graph, &ids, &BTreeSet::new())?;
      assert!(model.prepare(
        &graph,
        &points,
        analyze_rings(&graph)?.bridges(),
        &mut Budget(1_000_000)
      ));
      assert!(model.pinned.iter().all(|&p| p == expected));
    }
    Ok(())
  }
}
