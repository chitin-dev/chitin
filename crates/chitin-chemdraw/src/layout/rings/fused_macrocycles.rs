//! Atomic assembly of one macrocycle with one edge-fused small polygon.
//!
//! The owner evaluates whole-block geometry and deferred stereo references.
//! Nonregular faces never masquerade as regular polygons in the general fused
//! solver, whose center/sector assumptions would be invalid at concave corners.

use std::{
  collections::{BTreeMap, BTreeSet},
  f64::consts::{PI, TAU},
};

use chitin_chem::{AtomId, BondId, DoubleBondRelation, MolecularGraph};

use super::{
  Error, Point, Work,
  constraints::Constraints,
  geometry::{contains, separated},
  macrocycles::Macrocycle,
  poses, score,
};
use crate::stereo_geometry::relation;

/// Two edge-sharing faces with candidate geometry owned at the block boundary.
pub(super) struct FusedMacrocycle {
  /// Nonregular face candidates and constraints that do not depend on the cap.
  macrocycle: Macrocycle,
  /// Big face in cyclic dense-row order.
  face: Vec<usize>,
  /// Small regular face in cyclic dense-row order.
  cap: Vec<usize>,
  /// Complete block adjacency for attachment gaps and crossing validation.
  neighbors: BTreeMap<usize, Vec<usize>>,
  /// Every block edge exactly once, in stable endpoint order.
  edges: Vec<[usize; 2]>,
  /// Both endpoints of the common boundary edge.
  shared: [usize; 2],
  /// Source constraints whose axis and both references belong to this block.
  checks: Vec<(BondId, [usize; 4], DoubleBondRelation)>,
}

impl FusedMacrocycle {
  /// Recognizes one large face fused to one 3–8-membered regular face.
  ///
  /// # Parameters
  /// - `graph`, `constraints`, `ids`: one immutable input and dense namespace.
  /// - `work`: shared discovery, constraint, and construction allowance.
  ///
  /// # Returns
  /// A block model, or `None` for a different face family. Reference atoms in
  /// the attached face are deferred until both faces have real coordinates.
  ///
  /// # Errors
  /// Propagates graph inconsistencies and work exhaustion, never partial state.
  pub(super) fn new(
    graph: &MolecularGraph,
    constraints: &Constraints,
    ids: &[AtomId],
    work: &mut Work,
  ) -> Result<Option<Self>, Error> {
    if constraints.faces.len() != 2 {
      return Ok(None);
    }
    let (a, b) = (&constraints.faces[0], &constraints.faces[1]);
    let (face, cap) = if a.len() >= 9 && (3..=8).contains(&b.len()) {
      (a, b)
    } else if b.len() >= 9 && (3..=8).contains(&a.len()) {
      (b, a)
    } else {
      return Ok(None);
    };
    work.charge(face.len() + cap.len())?;
    let common: Vec<_> = face.iter().filter(|row| cap.contains(row)).copied().collect();
    let Ok(shared) = <[usize; 2]>::try_from(common) else {
      return Ok(None);
    };
    let deferred = cap
      .iter()
      .filter(|row| !face.contains(row))
      .map(|&row| ids[row])
      .collect();
    let macrocycle = Macrocycle::with_deferred(graph, face, ids, &deferred, work)?;
    let mut edges = BTreeSet::new();
    for cycle in [face, cap] {
      for (&a, &b) in cycle.iter().zip(cycle.iter().cycle().skip(1)).take(cycle.len()) {
        edges.insert([a.min(b), a.max(b)]);
      }
    }
    let mut neighbors = BTreeMap::<usize, Vec<usize>>::new();
    for &[a, b] in &edges {
      neighbors.entry(a).or_default().push(b);
      neighbors.entry(b).or_default().push(a);
    }
    let slots: BTreeMap<_, _> = neighbors.keys().map(|&row| (ids[row], row)).collect();
    let mut checks = Vec::new();
    for (id, stereo) in graph.double_bond_stereos() {
      work.charge(1)?;
      let [a, b] = stereo.ends();
      let [ra, rb] = stereo.references();
      if let [Some(&a), Some(&b), Some(&ra), Some(&rb)] = [a, b, ra, rb].map(|id| slots.get(&id)) {
        checks.push((id, [a, b, ra, rb], stereo.relation()));
      }
    }
    Ok(Some(Self {
      macrocycle,
      face: face.clone(),
      cap: cap.clone(),
      neighbors,
      edges: edges.into_iter().collect(),
      shared,
      checks,
    }))
  }

  /// Returns the largest face size for stable block seeding priority.
  pub(super) fn len(&self) -> usize {
    self.face.len()
  }

  /// Streams large-face candidates, assembles the cap, then scores whole blocks.
  ///
  /// # Parameters
  /// - `atoms`, `old`, `component`: global rows, fixed centers, and scoring scope.
  /// - `heading`: entry heading for an existing articulation in either face.
  /// - `work`, `source`: shared work allowance and failure locator.
  ///
  /// # Returns
  /// Complete positions and attachment directions, preserving one existing
  /// anchor exactly. Shared endpoints and every block edge stay unit length.
  ///
  /// # Errors
  /// Rejects multiple anchors, exhausted work, incompatible block geometry or
  /// an unsatisfied fully known stereo constraint. No partial face is committed.
  pub(super) fn embed(
    &self,
    atoms: &[usize],
    old: &[Option<Point>],
    component: &[usize],
    heading: f64,
    work: &mut Work,
    source: AtomId,
  ) -> Result<Vec<(usize, Point, f64)>, Error> {
    work.charge(old.len() + atoms.len())?;
    let anchors: Vec<_> = atoms.iter().copied().filter(|&row| old[row].is_some()).collect();
    if anchors.len() > 1 {
      return Err(Error::RingSystem {
        atom: source,
        cycles: 2,
      });
    }
    let anchor = anchors.first().copied();
    let blank = vec![None; old.len()];
    let mut best = None;
    let mut best_cost = f64::INFINITY;
    let mut failed_bond = None;
    self.macrocycle.visit(&blank, 0., work, source, |large, work| {
      work.charge(old.len() + 2 * large.len())?;
      let mut base = blank.clone();
      let directions: BTreeMap<_, _> = large.iter().map(|&(row, _, heading)| (row, heading)).collect();
      for &(row, point, _) in large {
        base[row] = Some(point);
      }
      for cap in poses(&self.cap, &base, 0., work)? {
        work.charge(old.len() + cap.points.len())?;
        let mut points = base.clone();
        for &(row, point) in &cap.points {
          points[row] = Some(base[row].unwrap_or(point));
        }
        if !self.valid(&points, work)? {
          continue;
        }
        work.charge(self.checks.len())?;
        if let Some(&(id, _, _)) = self.checks.iter().find(|(_, rows, expected)| {
          let [Some(a), Some(b), Some(ra), Some(rb)] = rows.map(|i| points[i]) else {
            return true;
          };
          relation([a, b, ra, rb]) != Some(*expected)
        }) {
          failed_bond = Some(id);
          continue;
        }
        let mut candidate = Vec::with_capacity(atoms.len());
        for &row in atoms {
          work.charge(1)?;
          let point = points[row].ok_or(Error::Invariant(source))?;
          let direction = if self.shared.contains(&row) {
            self.junction_heading(row, &points, work)?
          } else if let Some(&direction) = directions.get(&row) {
            direction
          } else {
            (point.y - cap.center.y).atan2(point.x - cap.center.x)
          };
          candidate.push((row, point, direction));
        }
        let origin = anchor.and_then(|row| points[row]).unwrap_or(Point::new(0., 0.));
        let target = anchor.and_then(|row| old[row]).unwrap_or(Point::new(0., 0.));
        let angle = anchor
          .and_then(|row| candidate.iter().find(|&&(id, _, _)| id == row))
          .map_or(0., |&(_, _, direction)| heading + PI - direction);
        for rotation in 0..if anchor.is_some() { 6 } else { 1 } {
          work.charge(old.len() + candidate.len())?;
          let angle = angle + rotation as f64 * PI / 3.;
          let (sin, cos) = angle.sin_cos();
          let mut global = old.to_vec();
          let transformed: Vec<_> = candidate
            .iter()
            .map(|&(row, p, direction)| {
              let (x, y) = (p.x - origin.x, p.y - origin.y);
              let point = old[row].unwrap_or(target.translated(x * cos - y * sin, x * sin + y * cos));
              global[row] = Some(point);
              (row, point, direction + angle)
            })
            .collect();
          let cost = score(atoms, &global, old, component, work)?;
          if cost + 1e-10 < best_cost {
            best_cost = cost;
            best = Some(transformed);
          }
        }
      }
      Ok(())
    })?;
    best.ok_or_else(|| {
      failed_bond.map_or(
        Error::RingSystem {
          atom: source,
          cycles: 2,
        },
        Error::DoubleBondStereo,
      )
    })
  }

  /// Requires opposite local interior sides and nonoverlapping face interiors.
  fn valid(&self, points: &[Option<Point>], work: &mut Work) -> Result<bool, Error> {
    work.charge(self.face.len() + self.cap.len())?;
    let polygon = |cycle: &[usize]| {
      cycle
        .iter()
        .map(|&row| points[row])
        .collect::<Option<Vec<_>>>()
        .ok_or(Error::CoordinateRange)
    };
    let large = polygon(&self.face)?;
    let small = polygon(&self.cap)?;
    let interior = |cycle: &[usize], polygon: &[Point]| {
      let area: f64 = polygon
        .iter()
        .zip(polygon.iter().cycle().skip(1))
        .take(polygon.len())
        .map(|(a, b)| a.x * b.y - a.y * b.x)
        .sum();
      let forward = cycle
        .iter()
        .zip(cycle.iter().cycle().skip(1))
        .take(cycle.len())
        .any(|(&a, &b)| [a, b] == self.shared);
      area.signum() * if forward { 1. } else { -1. }
    };
    if interior(&self.face, &large) * interior(&self.cap, &small) >= 0. {
      return Ok(false);
    }
    for (cycle, polygon, other) in [(&self.face, &large, &small), (&self.cap, &small, &large)] {
      for (i, &row) in cycle.iter().enumerate() {
        work.charge(other.len())?;
        if !self.shared.contains(&row) && contains(other, polygon[i]) {
          return Ok(false);
        }
      }
    }
    for (i, &[a, b]) in self.edges.iter().enumerate() {
      let a_point = points[a].ok_or(Error::CoordinateRange)?;
      let b_point = points[b].ok_or(Error::CoordinateRange)?;
      if (a_point.distance(b_point) - 1.).abs() > 1e-8 {
        return Ok(false);
      }
      for &[c, d] in &self.edges[i + 1..] {
        work.charge(1)?;
        if [a, b].iter().any(|row| [c, d].contains(row)) {
          continue;
        }
        if !separated(
          a_point,
          b_point,
          points[c].ok_or(Error::CoordinateRange)?,
          points[d].ok_or(Error::CoordinateRange)?,
        ) {
          return Ok(false);
        }
      }
    }
    Ok(true)
  }

  /// Chooses an unoccupied junction gap outside both actual face polygons.
  fn junction_heading(&self, row: usize, points: &[Option<Point>], work: &mut Work) -> Result<f64, Error> {
    let origin = points[row].ok_or(Error::CoordinateRange)?;
    let mut angles = self.neighbors[&row]
      .iter()
      .map(|&id| {
        let p = points[id].ok_or(Error::CoordinateRange)?;
        Ok((p.y - origin.y).atan2(p.x - origin.x))
      })
      .collect::<Result<Vec<_>, Error>>()?;
    angles.sort_by(f64::total_cmp);
    let mut best = (0., 0.);
    for i in 0..angles.len() {
      let gap = (angles[(i + 1) % angles.len()] - angles[i]).rem_euclid(TAU);
      let direction = angles[i] + gap / 2.;
      let probe = origin.translated(1e-5 * direction.cos(), 1e-5 * direction.sin());
      let mut inside = false;
      for cycle in [&self.face, &self.cap] {
        work.charge(cycle.len())?;
        let polygon = cycle
          .iter()
          .map(|&row| points[row])
          .collect::<Option<Vec<_>>>()
          .ok_or(Error::CoordinateRange)?;
        inside |= contains(&polygon, probe);
      }
      if !inside && gap > best.0 {
        best = (gap, direction);
      }
    }
    if best.0 == 0. {
      return Err(Error::CoordinateRange);
    }
    Ok(best.1)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use chitin_chem::{io::smiles::read_smiles, perception::analyze_rings};
  type TestResult = Result<(), Box<dyn std::error::Error>>;

  #[test]
  fn entry_in_either_face_should_retain_the_anchor_bit_for_bit() -> TestResult {
    let graph = read_smiles("C1CCCCCCCCCCC2CCCCC12")?.into_graph();
    let ids: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
    let indices: BTreeMap<_, _> = ids.iter().enumerate().map(|(row, &id)| (id, row)).collect();
    let report = analyze_rings(&graph)?;
    let block = report
      .blocks()
      .iter()
      .find(|block| block.cycle_rank() == 2)
      .ok_or("no fused block")?;
    let mut work = Work::new(2_000_000);
    let constraints = Constraints::new(&graph, block, &indices, &mut work)?;
    let model = FusedMacrocycle::new(&graph, &constraints, &ids, &mut work)?.ok_or("unrecognized block")?;
    let atoms: Vec<_> = (0..ids.len()).collect();
    let large = model
      .face
      .iter()
      .find(|row| !model.shared.contains(row))
      .copied()
      .ok_or("no free large vertex")?;
    let small = model
      .cap
      .iter()
      .find(|row| !model.shared.contains(row))
      .copied()
      .ok_or("no free small vertex")?;
    for row in [large, small, model.shared[0]] {
      let point = Point::new(27.125, -3.75);
      let mut old = vec![None; ids.len()];
      old[row] = Some(point);
      let placed = model.embed(&atoms, &old, &atoms, 0.25, &mut Work::new(2_000_000), ids[0])?;
      assert_eq!(
        placed.iter().find(|&&(id, _, _)| id == row).map(|&(_, p, _)| p),
        Some(point)
      );
      let points: BTreeMap<_, _> = placed.iter().map(|&(id, p, _)| (id, p)).collect();
      for &[a, b] in &model.edges {
        assert!((points[&a].distance(points[&b]) - 1.).abs() < 1e-8);
      }
    }
    Ok(())
  }

  #[test]
  fn two_fixed_anchors_should_be_rejected_without_moving_either() -> TestResult {
    let graph = read_smiles("C1CCCCCCCCCCC2CCCCC12")?.into_graph();
    let ids: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
    let indices: BTreeMap<_, _> = ids.iter().enumerate().map(|(row, &id)| (id, row)).collect();
    let report = analyze_rings(&graph)?;
    let block = report
      .blocks()
      .iter()
      .find(|block| block.cycle_rank() == 2)
      .ok_or("no fused block")?;
    let mut work = Work::new(2_000_000);
    let constraints = Constraints::new(&graph, block, &indices, &mut work)?;
    let model = FusedMacrocycle::new(&graph, &constraints, &ids, &mut work)?.ok_or("unrecognized block")?;
    let atoms: Vec<_> = (0..ids.len()).collect();
    let mut old = vec![None; ids.len()];
    old[0] = Some(Point::new(0., 0.));
    old[1] = Some(Point::new(1., 0.));
    let before = old.clone();
    assert!(matches!(
      model.embed(&atoms, &old, &atoms, 0., &mut work, ids[0]),
      Err(Error::RingSystem { cycles: 2, .. })
    ));
    assert_eq!(old, before);
    Ok(())
  }
}
