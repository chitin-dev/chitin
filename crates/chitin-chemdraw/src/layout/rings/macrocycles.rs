//! Closed unit-edge outlines for isolated macrocyclic blocks.
//!
//! Hexagonal cell boundaries provide visible chain bends without breaking ring
//! closure. Cyclic index offsets and traversal directions are candidates, not
//! chemical assignments: supplied double-bond relations are hard constraints.

use std::{
  collections::{BTreeMap, BTreeSet},
  f64::consts::PI,
};

use chitin_chem::{AtomId, BondId, DoubleBondRelation, MolecularGraph};

use super::{Error, Point, Work, score, side};
use crate::stereo_geometry::relation;

mod caps;

/// Integer axial cell coordinates and doubled lattice vertex coordinates.
type Pair = (i32, i32);

/// Neighbor offsets, ordered around a hexagon to detect connected shared edges.
const DIRECTIONS: [Pair; 6] = [(1, 0), (0, 1), (-1, 1), (-1, 0), (0, -1), (1, -1)];

/// One ring-local reference relation after converting exocyclic references.
struct Requirement {
  /// Source bond for reporting an unsatisfied, rather than ignored, constraint.
  bond: BondId,
  /// Axis endpoints and their respective adjacent ring atoms in local slots.
  slots: [usize; 4],
  /// Relation between the two ring neighbors, not necessarily source references.
  expected: DoubleBondRelation,
}

/// Immutable topology/constraint view, separate from temporary geometric trials.
pub(super) struct Macrocycle {
  /// Global dense rows in cyclic traversal order.
  face: Vec<usize>,
  /// Number of external attachments at each ring atom.
  attachments: Vec<usize>,
  /// Required configurations whose axis is an edge of this simple ring.
  requirements: Vec<Requirement>,
}

impl Macrocycle {
  /// Captures ring-local constraints without changing graph facts or handles.
  ///
  /// # Parameters
  /// - `face`, `ids`: cyclic dense rows and the global source handle namespace.
  /// - `work`: shared allowance for topology and stereo inspection.
  ///
  /// # Returns
  /// An independent model for one unicyclic block of at least 12 atoms.
  /// Exocyclic stereo references invert the corresponding ring-neighbor side.
  ///
  /// # Errors
  /// Propagates graph inconsistencies and work exhaustion before storing geometry.
  pub(super) fn new(graph: &MolecularGraph, face: &[usize], ids: &[AtomId], work: &mut Work) -> Result<Self, Error> {
    Self::with_deferred(graph, face, ids, &BTreeSet::new(), work)
  }

  /// Defers references in an attached face until the complete fused block exists.
  ///
  /// A neighboring face is not a freely placed substituent: its reference can
  /// lie on either side at a concave junction. Its actual positions must be
  /// checked by the fused owner, not inferred by inverting a ring-neighbor side.
  pub(super) fn with_deferred(
    graph: &MolecularGraph,
    face: &[usize],
    ids: &[AtomId],
    deferred: &BTreeSet<AtomId>,
    work: &mut Work,
  ) -> Result<Self, Error> {
    work.charge(face.len())?;
    let slots: BTreeMap<_, _> = face.iter().enumerate().map(|(i, &row)| (ids[row], i)).collect();
    let mut attachments = Vec::with_capacity(face.len());
    for &row in face {
      let neighbors = graph.neighbors(ids[row])?;
      work.charge(1 + neighbors.len())?;
      attachments.push(neighbors.filter(|(id, _)| !slots.contains_key(id)).count());
    }
    let mut requirements = Vec::new();
    for (bond, stereo) in graph.double_bond_stereos() {
      work.charge(1)?;
      let [a, b] = stereo.ends();
      let (Some(&a), Some(&b)) = (slots.get(&a), slots.get(&b)) else {
        continue;
      };
      let n = face.len();
      let (ra, rb) = if (a + 1) % n == b {
        ((a + n - 1) % n, (b + 1) % n)
      } else if (b + 1) % n == a {
        ((a + 1) % n, (b + n - 1) % n)
      } else {
        continue;
      };
      let [source_a, source_b] = stereo.references();
      if deferred.contains(&source_a) || deferred.contains(&source_b) {
        continue;
      }
      let invert = (source_a != ids[face[ra]]) != (source_b != ids[face[rb]]);
      let expected = match (stereo.relation(), invert) {
        (DoubleBondRelation::SameSide, false) | (DoubleBondRelation::OppositeSides, true) => {
          DoubleBondRelation::SameSide
        }
        _ => DoubleBondRelation::OppositeSides,
      };
      requirements.push(Requirement {
        bond,
        slots: [a, b, ra, rb],
        expected,
      });
    }
    Ok(Self {
      face: face.to_vec(),
      attachments,
      requirements,
    })
  }

  /// Returns this ring's vertex count for stable block seeding priority.
  pub(super) fn len(&self) -> usize {
    self.face.len()
  }

  /// Scores closed outline permutations and rigid placements atomically.
  ///
  /// # Parameters
  /// - `old`, `component`: global positions and the only rows allowed in scoring.
  /// - `heading`: entry direction for an existing articulation anchor.
  /// - `work`: shared allowance, including every candidate snapshot and scan.
  /// - `source`: source locator if no complete geometry can be constructed.
  ///
  /// # Returns
  /// Complete centers and local attachment headings, retaining an existing anchor
  /// exactly. No angle springs, partial closures, or soft stereo penalties apply.
  ///
  /// # Errors
  /// Reports exhausted work, unsupported multiple anchors, or an unsatisfied
  /// double-bond relation. Candidate enumeration is bounded, not exhaustive.
  pub(super) fn embed(
    &self,
    old: &[Option<Point>],
    component: &[usize],
    heading: f64,
    work: &mut Work,
    source: AtomId,
  ) -> Result<Vec<(usize, Point, f64)>, Error> {
    let mut best = None;
    let mut best_cost = f64::INFINITY;
    self.visit(old, heading, work, source, |candidate, work| {
      work.charge(old.len() + candidate.len())?;
      let mut points = old.to_vec();
      for &(row, point, _) in candidate {
        points[row] = Some(point);
      }
      let cost = score(&self.face, &points, old, component, work)? + self.attachment_cost(candidate);
      if cost + 1e-10 < best_cost {
        work.charge(candidate.len())?;
        best_cost = cost;
        best = Some(candidate.to_vec());
      }
      Ok(())
    })?;
    best.ok_or(Error::RingSystem {
      atom: source,
      cycles: 1,
    })
  }

  /// Streams complete rigid candidates to a consumer without retaining all poses.
  ///
  /// # Parameters
  /// - `old`, `heading`: global known centers and optional entry direction.
  /// - `work`, `source`: shared allowance and typed failure locator.
  /// - `visit`: receives a temporary complete ring and the same work allowance.
  ///
  /// # Returns
  /// Success after visiting every candidate satisfying local constraints. This
  /// permits a fused owner to test another face before committing the block.
  ///
  /// # Errors
  /// Rejects multiple anchors, exhausted work, or a constraint set for which no
  /// complete candidate exists. Consumer failures propagate unchanged.
  pub(super) fn visit(
    &self,
    old: &[Option<Point>],
    heading: f64,
    work: &mut Work,
    source: AtomId,
    mut visit: impl FnMut(&[(usize, Point, f64)], &mut Work) -> Result<(), Error>,
  ) -> Result<(), Error> {
    let n = self.len();
    work.charge(n)?;
    let anchors: Vec<_> = self
      .face
      .iter()
      .enumerate()
      .filter(|&(_, &row)| old[row].is_some())
      .map(|(i, _)| i)
      .collect();
    if anchors.len() > 1 {
      return Err(Error::RingSystem {
        atom: source,
        cycles: 1,
      });
    }
    let anchor = anchors.first().copied();
    let mut emitted = false;
    let mut failed_bond = None;
    for outline in outlines(n, work)? {
      for reverse in [false, true] {
        for offset in 0..n {
          work.charge(n + self.requirements.len())?;
          let local: Vec<_> = (0..n)
            .map(|i| outline[(offset + if reverse { n - i } else { i }) % n])
            .collect();
          if let Some(failed) = self
            .requirements
            .iter()
            .find(|r| relation(r.slots.map(|i| local[i])) != Some(r.expected))
          {
            failed_bond = Some(failed.bond);
            continue;
          }
          let attachment_heading = |i: usize| {
            let prev = local[(i + n - 1) % n];
            let next = local[(i + 1) % n];
            // Choose the unoccupied 120-degree sector, not the topological
            // exterior normal. At a concave carbon the latter would put a
            // substituent only 60 degrees from both ring bonds and invert an
            // exocyclic double-bond reference relative to its ring neighbor.
            (2. * local[i].y - prev.y - next.y).atan2(2. * local[i].x - prev.x - next.x)
          };
          let origin = anchor.map_or(Point::new(0., 0.), |i| local[i]);
          let target = anchor.and_then(|i| old[self.face[i]]).unwrap_or(Point::new(0., 0.));
          // A free attachment slot points back toward the parent, whereas
          // entry heading points away from it.
          let base = anchor.map_or(0., |i| heading + PI - attachment_heading(i));
          for rotation in 0..if anchor.is_some() { 6 } else { 1 } {
            work.charge(n)?;
            let angle = base + rotation as f64 * PI / 3.;
            let (sin, cos) = angle.sin_cos();
            let points: Vec<_> = self
              .face
              .iter()
              .enumerate()
              .map(|(i, &row)| {
                let (x, y) = (local[i].x - origin.x, local[i].y - origin.y);
                (
                  row,
                  old[row].unwrap_or(target.translated(x * cos - y * sin, x * sin + y * cos)),
                  attachment_heading(i) + angle,
                )
              })
              .collect();
            visit(&points, work)?;
            emitted = true;
          }
        }
      }
    }
    if emitted {
      Ok(())
    } else {
      Err(failed_bond.map_or(
        Error::RingSystem {
          atom: source,
          cycles: 1,
        },
        Error::DoubleBondStereo,
      ))
    }
  }

  /// Penalizes attachment-bearing concave corners independently of rigid pose.
  fn attachment_cost(&self, points: &[(usize, Point, f64)]) -> f64 {
    let n = points.len();
    let area: f64 = (0..n)
      .map(|i| {
        let a = points[i].1;
        let b = points[(i + 1) % n].1;
        a.x * b.y - a.y * b.x
      })
      .sum();
    (0..n)
      .filter(|&i| area * side(points[(i + n - 1) % n].1, points[i].1, points[(i + 1) % n].1) < 0.)
      .map(|i| self.attachments[i] as f64 * 0.1)
      .sum()
  }
}

/// Returns up to two simple lattice boundaries of the requested exact size.
///
/// Compact growth adds cells sharing one or two consecutive edges. Each step
/// increases perimeter by four or two; isolated neighbor runs cannot close a
/// hole or pinch the boundary. A strip with an optional triangular end cap
/// guarantees a second construction for every supported even size. All
/// construction visits consume ring work.
fn outlines(n: usize, work: &mut Work) -> Result<Vec<Vec<Point>>, Error> {
  if !n.is_multiple_of(2) {
    return caps::generate(n, work);
  }
  let mut cells = BTreeSet::from([(0, 0)]);
  let mut perimeter = 6;
  while perimeter < n {
    work.charge(6 * cells.len())?;
    let frontier: BTreeSet<_> = cells
      .iter()
      .flat_map(|&(x, y)| DIRECTIONS.map(|(dx, dy)| (x + dx, y + dy)))
      .filter(|cell| !cells.contains(cell))
      .collect();
    let mut best = None;
    for cell in frontier {
      work.charge(6)?;
      let neighbors = DIRECTIONS.map(|(dx, dy)| cells.contains(&(cell.0 + dx, cell.1 + dy)));
      let count = neighbors.iter().filter(|&&present| present).count();
      let starts = (0..6).filter(|&i| neighbors[i] && !neighbors[(i + 5) % 6]).count();
      if !(1..=2).contains(&count) || starts != 1 || perimeter + 6 - 2 * count > n {
        continue;
      }
      // Axial squared radius is exact and keeps tie-breaking independent of
      // floating-point map iteration or platform-dependent hash seeds.
      let (x, y) = (i64::from(cell.0), i64::from(cell.1));
      let key = (x * x + x * y + y * y, cell);
      if best.is_none_or(|(old, _)| key < old) {
        best = Some((key, (cell, count)));
      }
    }
    let Some((_, (cell, count))) = best else { break };
    cells.insert(cell);
    perimeter += 6 - 2 * count;
  }
  let mut result = Vec::new();
  if perimeter == n
    && let Some(points) = boundary(&cells, n, work)?
  {
    result.push(points);
  }
  work.charge(n)?;
  let capped = n.is_multiple_of(4);
  let count = if capped { (n - 12) / 4 + 2 } else { (n - 6) / 4 + 1 };
  let mut strip: BTreeSet<_> = (0..count).map(|i| (i as i32, 0)).collect();
  if capped {
    strip.insert((0, 1));
  }
  if (strip != cells || result.is_empty())
    && let Some(points) = boundary(&strip, n, work)?
  {
    result.push(points);
  }
  // Keep lattice candidates first so unconstrained even rings retain their
  // established geometry. End caps add different turn patterns for stereo.
  result.extend(caps::generate(n, work)?);
  Ok(result)
}

/// Cancels shared oriented edges, then verifies one complete simple boundary.
///
/// Integer vertices prove exact connectivity before conversion to unit-space
/// floats. A duplicate outgoing edge, hole, or extra loop rejects the candidate;
/// neither a closing edge nor a repeated atom is silently discarded.
fn boundary(cells: &BTreeSet<Pair>, n: usize, work: &mut Work) -> Result<Option<Vec<Point>>, Error> {
  work.charge(6 * cells.len() + n)?;
  let mut edges = BTreeSet::new();
  for &(x, y) in cells {
    let corners = [(1, 1), (0, 2), (-1, 1), (-1, -1), (0, -2), (1, -1)].map(|(dx, dy)| (2 * x + y + dx, 3 * y + dy));
    for i in 0..6 {
      let edge = (corners[i], corners[(i + 1) % 6]);
      if !edges.remove(&(edge.1, edge.0)) {
        edges.insert(edge);
      }
    }
  }
  if edges.len() != n {
    return Ok(None);
  }
  let mut next = BTreeMap::new();
  for (a, b) in edges {
    if next.insert(a, b).is_some() {
      return Ok(None);
    }
  }
  let Some((&start, _)) = next.first_key_value() else {
    return Ok(None);
  };
  let mut node = start;
  let mut points = Vec::with_capacity(n);
  for _ in 0..n {
    points.push(Point::new(
      f64::from(node.0) * 3_f64.sqrt() / 2.,
      f64::from(node.1) / 2.,
    ));
    let Some(other) = next.remove(&node) else {
      return Ok(None);
    };
    node = other;
  }
  Ok((node == start && next.is_empty()).then_some(points))
}

#[cfg(test)]
mod tests {
  use super::*;
  use chitin_chem::io::smiles::read_smiles;

  type TestResult = Result<(), Box<dyn std::error::Error>>;

  #[test]
  fn boundary_candidates_should_cover_even_sizes_without_repeated_vertices() -> TestResult {
    for size in (12..=128).step_by(2) {
      let candidates = outlines(size, &mut Work::new(2_000_000))?;
      assert!(!candidates.is_empty(), "size {size}");
      for points in candidates {
        assert_eq!(points.len(), size);
        for i in 0..size {
          assert!((points[i].distance(points[(i + 1) % size]) - 1.).abs() < 1e-8);
          for j in i + 1..size {
            assert!(points[i].distance(points[j]) >= 1. - 1e-8);
          }
        }
      }
    }
    Ok(())
  }

  #[test]
  fn anchored_macrocycle_should_retain_the_exact_existing_point() -> TestResult {
    let graph = read_smiles("C1CCCCCCCCCCC1")?.into_graph();
    let ids: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
    let face: Vec<_> = (0..ids.len()).collect();
    let mut work = Work::new(2_000_000);
    let model = Macrocycle::new(&graph, &face, &ids, &mut work)?;
    let mut old = vec![None; ids.len()];
    let anchor = Point::new(42.125, -7.25);
    old[4] = Some(anchor);
    let points = model.embed(&old, &face, 0.7, &mut work, ids[0])?;
    assert_eq!(
      points.iter().find(|&&(row, _, _)| row == 4).map(|&(_, p, _)| p),
      Some(anchor)
    );
    Ok(())
  }

  #[test]
  fn incompatible_constraint_set_should_not_fall_back_to_wrong_geometry() -> TestResult {
    let graph = read_smiles("C1CCCCCCCCCCC1")?.into_graph();
    let ids: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
    let bond = graph.bonds().next().map(|(id, _)| id).ok_or("missing bond")?;
    let n = ids.len();
    let face: Vec<_> = (0..n).collect();
    let mut work = Work::new(2_000_000);
    let mut model = Macrocycle::new(&graph, &face, &ids, &mut work)?;
    // Asking every boundary edge to separate its adjacent ring references is
    // impossible for these outlines. A soft scoring penalty would hide that.
    model.requirements = (0..n)
      .map(|i| Requirement {
        bond,
        slots: [i, (i + 1) % n, (i + n - 1) % n, (i + 2) % n],
        expected: DoubleBondRelation::OppositeSides,
      })
      .collect();
    assert!(
      matches!(model.embed(&vec![None;n], &face, 0., &mut work, ids[0]), Err(Error::DoubleBondStereo(id)) if id == bond)
    );
    Ok(())
  }
}
