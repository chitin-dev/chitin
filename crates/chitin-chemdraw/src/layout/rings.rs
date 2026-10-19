//! Cyclic blocks dispatched to polygons, macrocycles, bridge paths or cage cores.
//!
//! Domain blocks own topological boundaries. Each cyclic block is embedded as a
//! whole before external branches expand, so local traversal order cannot seed a
//! second fused face as an unrelated spiro ring. All shared anchors remain fixed.

use super::{BondCrossing, LayoutError as Error, placement::Topology};
use crate::geometry::Point;
use chitin_chem::{AtomId, MolecularGraph, perception::analyze_rings};
use std::{
  cmp::Reverse,
  collections::BTreeMap,
  f64::consts::{PI, TAU},
};

mod bridged;
mod cages;
mod constraints;
mod fused_macrocycles;
mod geometry;
mod macrocycles;
use bridged::Bridged;
use constraints::Constraints;

/// Shared work allowance for ring discovery, assembly, and geometric trials.
pub(super) struct Work(usize);

impl Work {
  /// Creates one allowance for all ring systems in an automatic layout.
  pub(super) const fn new(limit: usize) -> Self {
    Self(limit)
  }

  /// Charges before work starts; exhaustion never permits partial geometry.
  fn charge(&mut self, units: usize) -> Result<(), Error> {
    self.0 = self.0.checked_sub(units).ok_or(Error::RingSearchBudgetExceeded)?;
    Ok(())
  }
}

/// Geometry selected from topology, rather than forcing every block into polygons.
enum Model {
  /// Exact saturated cage core with a planar, unequal-length projection.
  Cage(cages::Cage),
  /// Edge-fused regular polygons and their incident faces at each vertex.
  Polygons {
    /// Selected face cycles and their edge-sharing graph.
    constraints: Constraints,
    /// Faces at each atom for exterior sector allocation.
    incidence: BTreeMap<usize, Vec<usize>>,
  },
  /// Three internally disjoint paths between two bridgeheads.
  Bridged(Bridged),
  /// A simple macrocycle with nonregular, closed unit-edge candidates.
  Macrocycle(macrocycles::Macrocycle),
  /// One macrocycle and one small face sharing exactly one skeleton edge.
  FusedMacrocycle(fused_macrocycles::FusedMacrocycle),
}

/// Complete block coordinates and explicitly authored perspective presentation.
pub(super) struct Placement {
  /// Every original block row, final position and outward attachment heading.
  pub(super) atoms: Vec<(usize, Point, f64)>,
  /// Proper crossings that require a rear-bond gap, not a diagnostic suppression rule.
  pub(super) crossings: Vec<BondCrossing>,
}

impl Placement {
  /// Wraps a nonperspective block without inventing crossing order.
  fn flat(atoms: Vec<(usize, Point, f64)>) -> Self {
    Self {
      atoms,
      crossings: Vec::new(),
    }
  }
}

/// One biconnected cyclic block, embedded atomically before branch expansion.
struct System {
  /// Separate models keep shared-path geometry out of fused-face assembly.
  model: Model,
  /// Global dense atoms in stable handle order.
  atoms: Vec<usize>,
  /// Stable source handle locating an unsupported constraint system.
  source: AtomId,
  /// Topological cycle rank, not a chemical ring count.
  rank: usize,
}

/// Complete coordinates and center for one regular polygon candidate.
struct Pose {
  /// All polygon vertices, including already fixed shared anchors.
  points: Vec<(usize, Point)>,
  /// Interior center used to enforce opposite sides of a fused edge.
  center: Point,
}

/// Private assembly snapshot, published only after all polygon closures agree.
struct Embedding {
  /// Complete component-sized coordinate scratch space, including fixed anchors.
  points: Vec<Option<Point>>,
  /// Interior reference for every face in this system.
  centers: Vec<Point>,
}

/// Block-level constraints plus atom-to-system incidence.
pub(super) struct Rings {
  /// Systems sorted by largest face first, then stable input handles.
  systems: Vec<System>,
  /// Block indices sharing each atom, including spiro articulations.
  pub(super) membership: Vec<Vec<usize>>,
}

impl Rings {
  /// Builds shared-edge constraints without assigning aromaticity or ring names.
  ///
  /// # Parameters
  /// - `graph`, `topology`: identical immutable input and dense row mapping.
  /// - `work`: shared discovery, cycle-reduction, and constraint budget.
  ///
  /// # Returns
  /// One rigid system per cyclic block. Independent and spiro blocks retain
  /// distinct identities, irrespective of their connected component's rank.
  ///
  /// # Errors
  /// Rejects unsupported cyclic topology, invalid references, or exhausted work.
  pub(super) fn new(graph: &MolecularGraph, topology: &Topology, work: &mut Work) -> Result<Self, Error> {
    work.charge(graph.atom_count() + graph.bond_count())?;
    let report = analyze_rings(graph)?;
    let indices: BTreeMap<_, _> = topology.ids.iter().enumerate().map(|(i, &id)| (id, i)).collect();
    let mut systems = Vec::new();
    for block in report.blocks().iter().filter(|block| block.cycle_rank() > 0) {
      let model = if let Some(cage) = cages::Cage::new(graph, block, &indices, work)? {
        Model::Cage(cage)
      } else if let Some(bridge) = Bridged::new(graph, block, &indices, work)? {
        Model::Bridged(bridge)
      } else {
        let constraints = Constraints::new(graph, block, &indices, work)?;
        if constraints.faces.len() == 1 && constraints.faces[0].len() >= 12 {
          Model::Macrocycle(macrocycles::Macrocycle::new(
            graph,
            &constraints.faces[0],
            &topology.ids,
            work,
          )?)
        } else if let Some(model) = fused_macrocycles::FusedMacrocycle::new(graph, &constraints, &topology.ids, work)? {
          Model::FusedMacrocycle(model)
        } else {
          let mut incidence = BTreeMap::<usize, Vec<usize>>::new();
          for (i, face) in constraints.faces.iter().enumerate() {
            work.charge(face.len())?;
            for &node in face {
              incidence.entry(node).or_default().push(i);
            }
          }
          Model::Polygons { constraints, incidence }
        }
      };
      systems.push(System {
        model,
        atoms: block.atoms().iter().map(|id| indices[id]).collect(),
        source: block.atoms()[0],
        rank: block.cycle_rank(),
      });
    }
    systems.sort_by(|a, b| {
      let largest = |system: &System| match &system.model {
        Model::Cage(model) => model.len(),
        Model::Polygons { constraints, .. } => constraints.faces.iter().map(Vec::len).max().unwrap_or(0),
        Model::Bridged(bridge) => bridge.perimeter_len(),
        Model::Macrocycle(model) => model.len(),
        Model::FusedMacrocycle(model) => model.len(),
      };
      largest(b).cmp(&largest(a)).then(a.atoms.cmp(&b.atoms))
    });
    let mut membership = vec![Vec::new(); topology.ids.len()];
    for (index, system) in systems.iter().enumerate() {
      for &node in &system.atoms {
        membership[node].push(index);
      }
    }
    Ok(Self { systems, membership })
  }

  /// Returns the highest-priority cyclic block in a component, if present.
  pub(super) fn seed(&self, component: &[usize]) -> Option<usize> {
    component
      .iter()
      .flat_map(|&node| self.membership[node].iter())
      .copied()
      .min()
  }

  /// Returns the number of rigid cyclic systems.
  pub(super) fn len(&self) -> usize {
    self.systems.len()
  }

  /// Tests whether adjacent vertices share a rigid cyclic block.
  ///
  /// For an existing graph edge, common block membership distinguishes a ring
  /// neighbor from an external bridge attachment. A fused junction can have
  /// three ring neighbors without any parent occupying its exterior sector.
  pub(super) fn shares_system(&self, a: usize, b: usize) -> bool {
    self.membership[a]
      .iter()
      .any(|system| self.membership[b].contains(system))
  }

  /// Embeds a complete cyclic system before publishing any of its new coordinates.
  ///
  /// # Parameters
  /// - `index`: previously unprocessed cyclic block.
  /// - `positions`: existing centers, including at most one external articulation.
  /// - `component`: the only rows allowed to influence overlap scoring.
  /// - `heading`: exterior attachment direction when entering a positioned atom.
  /// - `work`: shared allowance, also covering scratch snapshots and candidate scores.
  ///
  /// # Returns
  /// All system atom centers and exterior headings, including unchanged entry
  /// anchors. Polygon faces lie on opposite sides of their shared edge; bridge
  /// paths close on common endpoints. Every previously known anchor is checked.
  ///
  /// # Errors
  /// Rejects geometrically inconsistent regular-polygon constraints or exhausted
  /// work, never silently dropping a closing edge or stretching an anchor.
  pub(super) fn embed(
    &self,
    index: usize,
    positions: &[Option<Point>],
    component: &[usize],
    heading: f64,
    work: &mut Work,
  ) -> Result<Placement, Error> {
    let system = &self.systems[index];
    let constraints = match &system.model {
      Model::Cage(model) => {
        return model.embed(&system.atoms, positions, component, heading, work, system.source);
      }
      Model::Polygons { constraints, .. } => constraints,
      Model::Bridged(bridge) => {
        return bridge
          .embed(&system.atoms, positions, component, heading, work, system.source)
          .map(Placement::flat);
      }
      Model::Macrocycle(model) => {
        return model
          .embed(positions, component, heading, work, system.source)
          .map(Placement::flat);
      }
      Model::FusedMacrocycle(model) => {
        return model
          .embed(&system.atoms, positions, component, heading, work, system.source)
          .map(Placement::flat);
      }
    };
    work.charge(system.atoms.len() + constraints.faces.iter().map(Vec::len).sum::<usize>())?;
    let root = constraints
      .faces
      .iter()
      .enumerate()
      .max_by_key(|(i, face)| {
        (
          face.iter().filter(|&&node| positions[node].is_some()).count(),
          face.len(),
          Reverse(*i),
        )
      })
      .map(|(i, _)| i)
      .ok_or(Error::Invariant(system.source))?;
    let mut best = None;
    let mut best_cost = f64::INFINITY;
    for pose in poses(&constraints.faces[root], positions, heading, work)? {
      let Some(embedding) = system.assemble(root, pose, positions, work)? else {
        continue;
      };
      let cost = score(&system.atoms, &embedding.points, positions, component, work)?;
      if cost + 1e-10 < best_cost {
        best_cost = cost;
        best = Some(embedding);
      }
    }
    let Embedding { points, centers } = best.ok_or_else(|| system.unsupported())?;
    let mut result = Vec::new();
    for &node in &system.atoms {
      result.push((
        node,
        points[node].ok_or(Error::Invariant(system.source))?,
        system.outward(node, &points, &centers, work)?,
      ));
    }
    Ok(Placement::flat(result))
  }
}

impl System {
  /// Returns a source-located failure of the current regular-polygon model.
  fn unsupported(&self) -> Error {
    Error::RingSystem {
      atom: self.source,
      cycles: self.rank,
    }
  }

  /// Assembles faces by known shared edges, validating multi-anchor closure.
  ///
  /// The scratch snapshot is private until all faces agree. Face order prioritizes
  /// more already known edges, then vertices; atom ID order cannot prematurely
  /// place a pending face from only one vertex. Incompatible closure returns
  /// `None`, while resource exhaustion propagates as an error.
  fn assemble(
    &self,
    root: usize,
    pose: Pose,
    positions: &[Option<Point>],
    work: &mut Work,
  ) -> Result<Option<Embedding>, Error> {
    let Model::Polygons { constraints, .. } = &self.model else {
      return Err(Error::Invariant(self.source));
    };
    work.charge(positions.len() + constraints.faces.len())?;
    let mut points = positions.to_vec();
    let mut centers = vec![None; constraints.faces.len()];
    install(pose, root, &mut points, &mut centers);
    for _ in 1..constraints.faces.len() {
      let mut next = None;
      for (i, face) in constraints.faces.iter().enumerate() {
        work.charge(1 + face.len())?;
        if centers[i].is_some() || !constraints.neighbors[i].iter().any(|&other| centers[other].is_some()) {
          continue;
        }
        let edges = face
          .iter()
          .zip(face.iter().cycle().skip(1))
          .filter(|&(&a, &b)| points[a].is_some() && points[b].is_some())
          .count();
        let vertices = face.iter().filter(|&&node| points[node].is_some()).count();
        let key = (edges, vertices, face.len(), Reverse(i));
        if next.is_none_or(|(_, old)| key > old) {
          next = Some((i, key));
        }
      }
      let Some((next, _)) = next else {
        return Ok(None);
      };
      let mut chosen = None;
      for candidate in poses(&constraints.faces[next], &points, 0., work)? {
        let mut valid = true;
        for &other in &constraints.neighbors[next] {
          let Some(center) = centers[other] else {
            continue;
          };
          work.charge(constraints.faces[next].len() * constraints.faces[other].len())?;
          let common: Vec<_> = constraints.faces[next]
            .iter()
            .copied()
            .filter(|node| constraints.faces[other].contains(node))
            .collect();
          if common.len() != 2 {
            return Err(Error::Invariant(self.source));
          }
          let a = points[common[0]].ok_or(Error::Invariant(self.source))?;
          let b = points[common[1]].ok_or(Error::Invariant(self.source))?;
          if side(a, b, center) * side(a, b, candidate.center) >= -1e-10 {
            valid = false;
            break;
          }
        }
        if valid {
          chosen = Some(candidate);
          break;
        }
      }
      let Some(pose) = chosen else {
        return Ok(None);
      };
      install(pose, next, &mut points, &mut centers);
    }
    let centers = centers
      .into_iter()
      .collect::<Option<Vec<_>>>()
      .ok_or(Error::Invariant(self.source))?;
    Ok(Some(Embedding { points, centers }))
  }

  /// Finds an exterior angular gap rather than using the last embedded face's radius.
  ///
  /// Multiple fused faces can occupy different sectors at one atom. Candidate gap
  /// bisectors inside any incident polygon are excluded, so substituents attach
  /// outside the system. Overcoordinated drafts with no exterior sector retain
  /// the largest gap and can still produce final crowding diagnostics.
  fn outward(&self, node: usize, points: &[Option<Point>], centers: &[Point], work: &mut Work) -> Result<f64, Error> {
    let Model::Polygons { constraints, incidence } = &self.model else {
      return Err(Error::Invariant(self.source));
    };
    let origin = points[node].ok_or(Error::Invariant(self.source))?;
    let mut neighbors = BTreeMap::new();
    for &i in &incidence[&node] {
      let face = &constraints.faces[i];
      work.charge(face.len())?;
      let index = face
        .iter()
        .position(|&vertex| vertex == node)
        .ok_or(Error::Invariant(self.source))?;
      for other in [
        face[(index + 1) % face.len()],
        face[(index + face.len() - 1) % face.len()],
      ] {
        let point = points[other].ok_or(Error::Invariant(self.source))?;
        neighbors.insert(other, (point.y - origin.y).atan2(point.x - origin.x));
      }
    }
    let mut angles: Vec<_> = neighbors.into_values().collect();
    angles.sort_by(f64::total_cmp);
    let mut best = None;
    let mut fallback = (0., 0.);
    for i in 0..angles.len() {
      let gap = (angles[(i + 1) % angles.len()] - angles[i]).rem_euclid(TAU);
      let heading = angles[i] + gap / 2.;
      if gap > fallback.0 {
        fallback = (gap, heading);
      }
      let mut interior = false;
      for &face in &incidence[&node] {
        work.charge(1)?;
        let center = centers[face];
        let toward = (center.y - origin.y).atan2(center.x - origin.x);
        let delta = (heading - toward + PI).rem_euclid(TAU) - PI;
        let half = PI / 2. - PI / constraints.faces[face].len() as f64;
        if delta.abs() < half - 1e-8 {
          interior = true;
          break;
        }
      }
      if !interior && best.is_none_or(|(old, _)| gap > old + 1e-10) {
        best = Some((gap, heading));
      }
    }
    Ok(best.unwrap_or(fallback).1)
  }
}

/// Installs only new centers, retaining existing anchor values exactly.
fn install(pose: Pose, face: usize, points: &mut [Option<Point>], centers: &mut [Option<Point>]) {
  for (node, point) in pose.points {
    if points[node].is_none() {
      points[node] = Some(point);
    }
  }
  centers[face] = Some(pose.center);
}

/// Generates regular polygon candidates and verifies every known anchor.
///
/// Two or more known vertices require an adjacent anchored edge; extra anchors
/// become closure constraints, not permission to warp the polygon. No feasible
/// candidate returns an empty list, leaving the caller to report the block.
fn poses(face: &[usize], positions: &[Option<Point>], heading: f64, work: &mut Work) -> Result<Vec<Pose>, Error> {
  work.charge(face.len())?;
  let known: Vec<_> = face
    .iter()
    .enumerate()
    .filter(|&(_, &node)| positions[node].is_some())
    .map(|(i, _)| i)
    .collect();
  let radius = 1. / (2. * (PI / face.len() as f64).sin());
  let mut frames = Vec::new();
  match known.as_slice() {
    [] => frames.push((Point::new(0., 0.), 0., 1.)),
    &[anchor] => {
      let point = positions[face[anchor]].ok_or(Error::CoordinateRange)?;
      for rotation in [0., PI / 3., -PI / 3., 2. * PI / 3., -2. * PI / 3., PI] {
        let direction = heading + rotation;
        frames.push((
          point.translated(radius * direction.cos(), radius * direction.sin()),
          direction + PI - TAU * anchor as f64 / face.len() as f64,
          1.,
        ));
      }
    }
    _ => {
      let Some(a) =
        (0..face.len()).find(|&i| positions[face[i]].is_some() && positions[face[(i + 1) % face.len()]].is_some())
      else {
        return Ok(Vec::new());
      };
      let b = (a + 1) % face.len();
      let start = positions[face[a]].ok_or(Error::CoordinateRange)?;
      let end = positions[face[b]].ok_or(Error::CoordinateRange)?;
      if (start.distance(end) - 1.).abs() > 1e-8 {
        return Ok(Vec::new());
      }
      let dx = end.x - start.x;
      let dy = end.y - start.y;
      let apothem = 0.5 / (PI / face.len() as f64).tan();
      for sign in [-1., 1.] {
        let center = Point::new(
          (start.x + end.x) / 2. - sign * dy * apothem,
          (start.y + end.y) / 2. + sign * dx * apothem,
        );
        let angle = (start.y - center.y).atan2(start.x - center.x);
        frames.push((center, angle - sign * TAU * a as f64 / face.len() as f64, sign));
      }
    }
  }
  let mut candidates = Vec::new();
  for (center, angle, sign) in frames {
    work.charge(face.len())?;
    let points: Vec<_> = face
      .iter()
      .enumerate()
      .map(|(i, &node)| {
        let angle = angle + sign * TAU * i as f64 / face.len() as f64;
        (node, center.translated(radius * angle.cos(), radius * angle.sin()))
      })
      .collect();
    if points
      .iter()
      .all(|&(node, point)| positions[node].is_none_or(|old| old.distance(point) < 1e-8))
    {
      candidates.push(Pose { points, center });
    }
  }
  Ok(candidates)
}

/// Scores new centers against their component only, with unit-cell pruning.
///
/// Cells contain existing anchors first, then each new system atom once. A
/// threshold below one avoids numerical tie-breaking from unit bonded neighbors.
/// Dense overlap remains budgeted; fragments never influence each other's scores.
fn score(
  atoms: &[usize],
  points: &[Option<Point>],
  old: &[Option<Point>],
  component: &[usize],
  work: &mut Work,
) -> Result<f64, Error> {
  let mut grid = BTreeMap::<(i64, i64), Vec<Point>>::new();
  work.charge(component.len())?;
  for &node in component {
    if let Some(point) = old[node] {
      grid.entry(cell(point)?).or_default().push(point);
    }
  }
  let mut cost = 0.;
  for &node in atoms {
    if old[node].is_some() {
      continue;
    }
    let point = points[node].ok_or(Error::CoordinateRange)?;
    let (x, y) = cell(point)?;
    work.charge(9)?;
    for dx in -1..=1 {
      for dy in -1..=1 {
        if let Some(others) = grid.get(&(x + dx, y + dy)) {
          work.charge(others.len())?;
          for &other in others {
            cost += (0.95 - point.distance(other)).max(0.).powi(2);
          }
        }
      }
    }
    grid.entry((x, y)).or_default().push(point);
  }
  Ok(cost)
}

/// Returns a guarded spatial cell with room for neighboring-cell offsets.
fn cell(point: Point) -> Result<(i64, i64), Error> {
  if !point.is_finite() || point.x.abs() > i64::MAX as f64 / 2. || point.y.abs() > i64::MAX as f64 / 2. {
    return Err(Error::CoordinateRange);
  }
  Ok((point.x.floor() as i64, point.y.floor() as i64))
}

/// Returns the signed cross product locating a point relative to an edge.
fn side(a: Point, b: Point, p: Point) -> f64 {
  (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x)
}
