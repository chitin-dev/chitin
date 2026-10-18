//! Bounded, deterministic branch search over complete molecular coordinates.
//!
//! Only single-bond bridges split movable branches. Rigid branch transforms
//! preserve angle magnitudes; terminal hydroxyl clearance has a separate bounded
//! saturated-carbon angle policy. Every transform preserves all bond lengths
//! and ring geometry. Linear triple/cumulated-double centers are protected.
//! This is drawing optimization, not a force field or chemical-energy model.

use super::{Layout2d, LayoutDiagnostic, LayoutError as Error, LayoutOptions};
use crate::geometry::{Bounds, Point};
use chitin_chem::{AtomId, BondId, BondOrder, MolecularGraph, perception::analyze_rings};
use std::{
  collections::{BTreeMap, BTreeSet},
  f64::consts::PI,
};

mod terminal;

/// One dense skeleton edge, retaining its original graph identity.
struct Edge {
  /// Source handle used by unresolved-crossing diagnostics.
  id: BondId,
  /// Dense endpoint rows in the same component model.
  ends: [usize; 2],
  /// A topological bridge is movable only when its supplied order is single.
  movable: bool,
}

/// Shared optimization budget; quality inspection itself is never truncated.
struct Budget(usize);

impl Budget {
  /// Charges work before doing it, returning false instead of overspending.
  fn charge(&mut self, units: usize) -> bool {
    if self.0 < units {
      self.0 = 0;
      false
    } else {
      self.0 -= units;
      true
    }
  }
}

/// Measured geometric penalties and final reportable contacts.
#[derive(Default)]
struct Quality {
  /// Dimensionless crowding, atom-edge contact, and crossing penalty.
  cost: f64,
  /// Dense atom pairs below the caller's diagnostic threshold.
  crowds: Vec<(usize, usize)>,
  /// Nonincident skeleton-edge pairs with a proper crossing.
  crossings: BTreeSet<(usize, usize)>,
}

/// Temporary component-local representation; no chemical graph ownership.
struct Model {
  /// Stable atom handles, independent of holes left by graph editing.
  ids: Vec<AtomId>,
  /// Every skeleton edge in stable bond order.
  edges: Vec<Edge>,
  /// Incident edge indices for iterative branch extraction.
  rows: Vec<Vec<usize>>,
  /// Whether branch rotation at each atom would violate a linear center.
  linear: Vec<bool>,
  /// Eligible saturated-carbon hydroxyls; ring oxygens and carbonyls are excluded.
  hydroxyls: Vec<terminal::Hydroxyl>,
  /// Dense axis/reference quadruples with required, coordinate-independent sides.
  double_stereo: Vec<([usize; 4], chitin_chem::DoubleBondRelation)>,
}

impl Model {
  /// Checks the angles at the only boundary where a rigid branch transform can change them.
  ///
  /// # Parameters
  /// - `moving`, `fixed`: endpoints of the cut single-bond bridge.
  /// - `before`, `after`: complete snapshots; the moving endpoint itself is fixed.
  ///
  /// # Returns
  /// Whether every outgoing neighbor retains its angle to the incoming cut bond.
  /// Other internal angles are already preserved by the rigid transform. Comparing
  /// normalized dot products permits reflection but rejects a 120° trial that
  /// collapses a trigonal center into a 180°/60° arrangement to reduce crowding.
  fn preserves_attachment_angles(&self, moving: usize, fixed: usize, before: &[Point], after: &[Point]) -> bool {
    let cosine = |points: &[Point], other: usize| {
      let center = points[moving];
      let a = points[fixed];
      let b = points[other];
      ((a.x - center.x) * (b.x - center.x) + (a.y - center.y) * (b.y - center.y))
        / (a.distance(center) * b.distance(center))
    };
    self.rows[moving].iter().all(|&edge| {
      let [a, b] = self.edges[edge].ends;
      let other = if a == moving { b } else { a };
      other == fixed || (cosine(before, other) - cosine(after, other)).abs() < 1e-8
    })
  }

  /// Builds one component's adjacency and chemically conservative movable edges.
  ///
  /// # Returns
  /// Dense local geometry with bridge membership from the shared domain analysis.
  /// Unknown chemistry is not inferred; layout already validated bond orders.
  fn new(graph: &MolecularGraph, ids: Vec<AtomId>, bridges: &BTreeSet<BondId>) -> Self {
    let indices: BTreeMap<_, _> = ids.iter().enumerate().map(|(i, &id)| (id, i)).collect();
    let mut rows = vec![Vec::new(); ids.len()];
    let mut edges = Vec::new();
    let mut orders = vec![Vec::new(); ids.len()];
    for (id, bond) in graph.bonds() {
      let [a, b] = bond.atoms();
      let (Some(&a), Some(&b)) = (indices.get(&a), indices.get(&b)) else {
        continue;
      };
      rows[a].push(edges.len());
      rows[b].push(edges.len());
      orders[a].push(bond.order());
      orders[b].push(bond.order());
      edges.push(Edge {
        id,
        ends: [a, b],
        movable: bond.order() == BondOrder::Single && bridges.contains(&id),
      });
    }
    let linear = orders
      .iter()
      .map(|row| {
        row.len() == 2 && (row.contains(&BondOrder::Triple) || row.iter().all(|&order| order == BondOrder::Double))
      })
      .collect();
    let hydroxyls = terminal::collect(graph, &ids, &indices);
    let double_stereo = graph
      .double_bond_stereos()
      .filter_map(|(_, stereo)| {
        let [a, b] = stereo.ends();
        let [ra, rb] = stereo.references();
        Some((
          [
            *indices.get(&a)?,
            *indices.get(&b)?,
            *indices.get(&ra)?,
            *indices.get(&rb)?,
          ],
          stereo.relation(),
        ))
      })
      .collect();
    Self {
      ids,
      edges,
      rows,
      linear,
      hydroxyls,
      double_stereo,
    }
  }

  /// Rejects optional moves that would invert or flatten a required alkene.
  fn preserves_double_stereo(&self, points: &[Point]) -> bool {
    self.double_stereo.iter().all(|(indices, expected)| {
      crate::stereo_geometry::relation(indices.map(|index| points[index])) == Some(*expected)
    })
  }

  /// Extracts the smaller side of a bridge, preserving its boundary endpoint.
  ///
  /// # Returns
  /// Dense movable rows and the fixed endpoint. Returns `None` when the work
  /// budget is depleted; cyclic edges never reach this function.
  fn branch(&self, edge: usize, budget: &mut Budget) -> Option<(Vec<usize>, usize, usize)> {
    let [a, b] = self.edges[edge].ends;
    // A terminal branch used to traverse the entire opposite skeleton only to
    // discover that rotating about the leaf itself cannot move any geometry.
    for (leaf, host) in [(a, b), (b, a)] {
      if self.rows[leaf].len() == 1 {
        return budget.charge(1).then_some((vec![leaf], leaf, host));
      }
    }
    let mut seen = vec![false; self.ids.len()];
    let mut stack = vec![a];
    seen[a] = true;
    let mut side = Vec::new();
    while let Some(node) = stack.pop() {
      if !budget.charge(1 + self.rows[node].len()) {
        return None;
      }
      side.push(node);
      for &next in &self.rows[node] {
        if next == edge {
          continue;
        }
        for &other in &self.edges[next].ends {
          if !seen[other] {
            seen[other] = true;
            stack.push(other);
          }
        }
      }
    }
    if side.len() <= self.ids.len() / 2 {
      Some((side, a, b))
    } else {
      Some(((0..self.ids.len()).filter(|&node| !seen[node]).collect(), b, a))
    }
  }

  /// Measures nearby geometry with unit-cell broad-phase pruning.
  ///
  /// # Parameters
  /// - `points`: complete component coordinates with unit skeleton edges.
  /// - `threshold`: reportable atom-center distance, independent of cost weights.
  /// - `budget`: candidate work accounting; inspection uses an unlimited budget.
  ///
  /// # Returns
  /// All close atom pairs and proper edge crossings, plus an atom-edge clearance
  /// penalty. `None` abandons a candidate before budget overspending. Grid costs
  /// are local for ordinary drawings, but dense overlap can still be quadratic.
  fn measure(&self, points: &[Point], threshold: f64, budget: &mut Budget) -> Option<Quality> {
    if !budget.charge(points.len() + self.edges.len()) {
      return None;
    }
    let mut atoms = BTreeMap::<(i64, i64), Vec<usize>>::new();
    let mut edges = BTreeMap::<(i64, i64), Vec<usize>>::new();
    for (i, &point) in points.iter().enumerate() {
      atoms.entry(cell(point)).or_default().push(i);
    }
    for (i, edge) in self.edges.iter().enumerate() {
      let [a, b] = edge.ends.map(|node| cell(points[node]));
      for x in a.0.min(b.0)..=a.0.max(b.0) {
        for y in a.1.min(b.1)..=a.1.max(b.1) {
          edges.entry((x, y)).or_default().push(i);
        }
      }
    }
    let mut quality = Quality::default();
    for (i, &point) in points.iter().enumerate() {
      let (x, y) = cell(point);
      let mut close_edges = BTreeSet::<usize>::new();
      for dx in -1..=1 {
        for dy in -1..=1 {
          if let Some(others) = atoms.get(&(x + dx, y + dy)) {
            for &j in others.iter().filter(|&&j| j > i) {
              if !budget.charge(1) {
                return None;
              }
              let distance = point.distance(points[j]);
              quality.cost += (0.95 - distance).max(0.).powi(2);
              if distance < threshold {
                quality.crowds.push((i, j));
              }
            }
          }
          if let Some(candidates) = edges.get(&(x + dx, y + dy)) {
            close_edges.extend(candidates.iter().copied());
          }
        }
      }
      for &edge in &close_edges {
        if !budget.charge(1) {
          return None;
        }
        let [a, b] = self.edges[edge].ends;
        if a != i && b != i {
          quality.cost += 4. * (0.25 - segment_distance(point, points[a], points[b])).max(0.).powi(2);
        }
      }
    }
    // A crossing can occur far from either endpoint's cell in future non-unit
    // layouts. Edge bounding cells, rather than atom hits, own this broad phase.
    let mut tested = BTreeSet::new();
    for bucket in edges.values() {
      for (offset, &i) in bucket.iter().enumerate() {
        for &j in &bucket[offset + 1..] {
          if !budget.charge(1) {
            return None;
          }
          if !tested.insert((i, j)) {
            continue;
          }
          let [a, b] = self.edges[i].ends;
          let [c, d] = self.edges[j].ends;
          if a != c && a != d && b != c && b != d && crosses(points[a], points[b], points[c], points[d]) {
            quality.cost += 8.;
            quality.crossings.insert((i, j));
          }
        }
      }
    }
    Some(quality)
  }
}

/// Returns a generated finite point's cell; automatic embedding bounds casts.
fn cell(point: Point) -> (i64, i64) {
  (point.x.floor() as i64, point.y.floor() as i64)
}

/// Returns the distance to a finite unit-length skeleton segment.
fn segment_distance(point: Point, a: Point, b: Point) -> f64 {
  let dx = b.x - a.x;
  let dy = b.y - a.y;
  let t = (((point.x - a.x) * dx + (point.y - a.y) * dy) / (dx * dx + dy * dy)).clamp(0., 1.);
  point.distance(a.translated(t * dx, t * dy))
}

/// Tests proper intersections, excluding collinearity and endpoint tangencies.
fn crosses(a: Point, b: Point, c: Point, d: Point) -> bool {
  let side = |p: Point, q: Point, r: Point| (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
  side(a, b, c) * side(a, b, d) < -1e-12 && side(c, d, a) * side(c, d, b) < -1e-12
}

/// Makes a component's widest sampled direction horizontal in 30-degree steps.
///
/// # Parameters
/// - `points`: complete component geometry; internal distances are immutable.
///
/// # Returns
/// Rotated centers with the widest x extent among six orientations, retaining
/// the initial orientation on ties. Six linear projections instead of an
/// all-pairs longest-distance scan bound work to O(V) without introducing
/// arbitrary bond angles. Isotropic/single-atom components need no rotation.
fn orient(points: &mut [Point]) {
  let mut best = 0.;
  let mut width = f64::NEG_INFINITY;
  for angle in [0., PI / 6., -PI / 6., PI / 3., -PI / 3., PI / 2.] {
    let (sin, cos) = angle.sin_cos();
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for point in points.iter() {
      let x = cos * point.x - sin * point.y;
      min = min.min(x);
      max = max.max(x);
    }
    if max - min > width + 1e-10 {
      best = angle;
      width = max - min;
    }
  }
  if best == 0. {
    return;
  }
  let (sin, cos) = best.sin_cos();
  for point in points {
    *point = Point::new(cos * point.x - sin * point.y, sin * point.x + cos * point.y);
  }
}

/// Optimizes a component while reserving work for final terminal-group clearance.
///
/// Whole-branch transforms can bring distant hydroxyls together, so local work
/// must run after them. A quarter of the remaining shared budget is reserved;
/// otherwise a large skeleton can exhaust all work before any leaf is repaired.
/// Unspent reserved work is returned for later disconnected components.
fn improve(model: &Model, points: &mut Vec<Point>, options: &LayoutOptions, budget: &mut Budget) -> bool {
  let Some(mut current) = model.measure(points, options.minimum_spacing, &mut Budget(usize::MAX)) else {
    return false;
  };
  if current.cost < 1e-12 {
    return false;
  }
  let reserve = if model.hydroxyls.is_empty() { 0 } else { budget.0 / 4 };
  budget.0 -= reserve;
  let exhausted = improve_branches(model, points, &mut current, options, budget);
  let mut local = Budget(reserve);
  let local_exhausted = terminal::improve(model, points, &mut current, options, &mut local);
  budget.0 += local.0;
  exhausted || local_exhausted
}

/// Optimizes one component by best-improving rigid branch transforms.
///
/// Reflection is always eligible; ±120-degree rotations around the movable
/// endpoint are excluded at linear centers. Equal-cost candidates are rejected
/// to prevent drift, and every trial starts from the exact original snapshot.
/// All candidates must preserve attachment angles before cost can be considered.
/// Returns whether the shared work budget was exhausted, never partial geometry.
fn improve_branches(
  model: &Model,
  points: &mut Vec<Point>,
  current: &mut Quality,
  options: &LayoutOptions,
  budget: &mut Budget,
) -> bool {
  for _ in 0..options.refinement_passes {
    if current.cost < 1e-12 {
      return false;
    }
    let mut changed = false;
    for edge in 0..model.edges.len() {
      if !model.edges[edge].movable {
        continue;
      }
      if !budget.charge(points.len()) {
        return true;
      }
      let Some((branch, moving, fixed)) = model.branch(edge, budget) else {
        return true;
      };
      if branch.len() == 1 {
        continue;
      }
      let origin = points[moving];
      let vector = Point::new(origin.x - points[fixed].x, origin.y - points[fixed].y);
      let angle = vector.y.atan2(vector.x);
      let mut best = None;
      let mut best_cost = current.cost;
      let mut exhausted = false;
      for transform in 0..if model.linear[moving] { 1 } else { 3 } {
        if !budget.charge(points.len() + branch.len()) {
          exhausted = true;
          break;
        }
        let mut candidate = points.clone();
        for &node in &branch {
          let dx = points[node].x - origin.x;
          let dy = points[node].y - origin.y;
          let (x, y) = if transform == 0 {
            let (sin, cos) = (2. * angle).sin_cos();
            (cos * dx + sin * dy, sin * dx - cos * dy)
          } else {
            let (sin, cos) = (if transform == 1 { 2. * PI / 3. } else { -2. * PI / 3. }).sin_cos();
            (cos * dx - sin * dy, sin * dx + cos * dy)
          };
          candidate[node] = origin.translated(x, y);
        }
        if !budget.charge(model.rows[moving].len() + model.double_stereo.len()) {
          exhausted = true;
          break;
        }
        if !model.preserves_attachment_angles(moving, fixed, points, &candidate)
          || !model.preserves_double_stereo(&candidate)
        {
          continue;
        }
        let Some(quality) = model.measure(&candidate, options.minimum_spacing, budget) else {
          exhausted = true;
          break;
        };
        if quality.cost + 1e-10 < best_cost {
          best_cost = quality.cost;
          best = Some((candidate, quality));
        }
      }
      if let Some((candidate, quality)) = best {
        *points = candidate;
        *current = quality;
        changed = true;
      }
      if exhausted {
        // Retain the best fully scored candidate even if a later trial could
        // not finish. A half-scored candidate never replaces complete geometry.
        return true;
      }
    }
    if !changed {
      break;
    }
  }
  false
}

/// Refines, inspects, and repacks complete automatic coordinates per component.
///
/// # Parameters
/// - `graph`, `layout`: identical live-ID coverage, never a partial embedding.
/// - `options`: bounded search policy and final diagnostic threshold.
///
/// # Returns
/// The best complete geometry in place, with fresh crowd/crossing diagnostics.
/// Fragments share a work budget but never influence one another's scoring.
/// Disabling search still performs final inspection; labels are not modelled.
///
/// # Errors
/// Reports graph/coordinate invariants; no graph facts are changed.
pub(super) fn run(graph: &MolecularGraph, layout: &mut Layout2d, options: &LayoutOptions) -> Result<(), Error> {
  let rings = analyze_rings(graph)?;
  let mut budget = Budget(options.refinement_budget);
  let mut exhausted = false;
  let mut cursor = 0.;
  layout.diagnostics.clear();
  for component in graph.connected_components() {
    let model = Model::new(graph, component, rings.bridges());
    let mut points: Vec<_> = model
      .ids
      .iter()
      .map(|id| layout.positions.get(id).copied().ok_or(Error::Invariant(*id)))
      .collect::<Result<_, _>>()?;
    if !exhausted && options.refinement_passes > 0 {
      exhausted = improve(&model, &mut points, options, &mut budget);
    }
    orient(&mut points);
    let quality = model
      .measure(&points, options.minimum_spacing, &mut Budget(usize::MAX))
      .ok_or(Error::CoordinateRange)?;
    for (a, b) in quality.crowds {
      layout.diagnostics.push(LayoutDiagnostic::CrowdedAtoms {
        a: model.ids[a],
        b: model.ids[b],
      });
    }
    for (a, b) in quality.crossings {
      layout.diagnostics.push(LayoutDiagnostic::CrossingBonds {
        a: model.edges[a].id,
        b: model.edges[b].id,
      });
    }
    let bounds = Bounds::from_points(points.iter().copied())?.ok_or(Error::CoordinateRange)?;
    for (&id, point) in model.ids.iter().zip(&points) {
      layout.positions.insert(
        id,
        point.translated(cursor - bounds.min().x, -(bounds.min().y + bounds.height() / 2.)),
      );
    }
    cursor += bounds.width() + options.component_gap;
  }
  if exhausted {
    layout.diagnostics.push(LayoutDiagnostic::RefinementBudgetExhausted);
  }
  layout.bounds = Bounds::from_points(layout.positions.values().copied())?;
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use chitin_chem::io::smiles::read_smiles;

  type TestResult = Result<(), Box<dyn std::error::Error>>;

  #[test]
  fn folded_chain_should_improve_without_stretching_bonds() -> TestResult {
    let graph = read_smiles("CCCCCCC")?.into_graph();
    let ids: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
    // A chain wound around a hexagon closes geometrically, not topologically.
    // Reflection can unwind it while preserving every pre-existing bond angle.
    let points: Vec<_> = (0..7)
      .map(|i| {
        let angle = i as f64 * PI / 3.;
        Point::new(angle.cos(), angle.sin())
      })
      .collect();
    let topology = analyze_rings(&graph)?;
    let model = Model::new(&graph, ids.clone(), topology.bridges());
    let before = model
      .measure(&points, 0.55, &mut Budget(usize::MAX))
      .ok_or("measurement failed")?
      .cost;
    let mut after = points.clone();
    assert!(!improve(
      &model,
      &mut after,
      &LayoutOptions::default(),
      &mut Budget(1_000_000)
    ));
    let cost = model
      .measure(&after, 0.55, &mut Budget(usize::MAX))
      .ok_or("measurement failed")?
      .cost;
    assert!(cost + 1e-8 < before, "{before} -> {cost}");
    for edge in &model.edges {
      let [a, b] = edge.ends;
      assert!((after[a].distance(after[b]) - points[a].distance(points[b])).abs() < 1e-9);
    }
    Ok(())
  }

  #[test]
  fn crossing_detection_should_ignore_shared_endpoints_but_find_interior_crossings() -> TestResult {
    let graph = read_smiles("CCCC")?.into_graph();
    let ids = graph.atoms().map(|(id, _)| id).collect();
    let model = Model::new(&graph, ids, analyze_rings(&graph)?.bridges());
    let d = 0.5_f64.sqrt();
    let points = [
      Point::new(-d / 2., -d / 2.),
      Point::new(d / 2., d / 2.),
      Point::new(-d / 2., d / 2.),
      Point::new(d / 2., -d / 2.),
    ];
    let quality = model
      .measure(&points, 0.55, &mut Budget(usize::MAX))
      .ok_or("measurement failed")?;
    assert_eq!(quality.crossings, BTreeSet::from([(0, 2)]));
    Ok(())
  }

  #[test]
  fn trigonal_attachment_should_allow_reflection_but_reject_angle_collapsing_rotation() -> TestResult {
    let graph = read_smiles("CC(=O)C")?.into_graph();
    let ids = graph.atoms().map(|(id, _)| id).collect();
    let model = Model::new(&graph, ids, analyze_rings(&graph)?.bridges());
    let y = 3_f64.sqrt() / 2.;
    let before = [
      Point::new(-1., 0.),
      Point::new(0., 0.),
      Point::new(0.5, y),
      Point::new(0.5, -y),
    ];
    let reflected = [before[0], before[1], before[3], before[2]];
    assert!(model.preserves_attachment_angles(1, 0, &before, &reflected));
    let mut rotated = before;
    let (sin, cos) = (2. * PI / 3.).sin_cos();
    for node in [2, 3] {
      let p = before[node];
      rotated[node] = Point::new(cos * p.x - sin * p.y, sin * p.x + cos * p.y);
    }
    assert!(!model.preserves_attachment_angles(1, 0, &before, &rotated));
    Ok(())
  }

  #[test]
  fn budget_exhaustion_should_never_commit_a_worse_or_partially_transformed_candidate() -> TestResult {
    let graph = read_smiles("CCCCCCC")?.into_graph();
    let ids = graph.atoms().map(|(id, _)| id).collect();
    let model = Model::new(&graph, ids, analyze_rings(&graph)?.bridges());
    let points: Vec<_> = (0..7)
      .map(|i| {
        let angle = i as f64 * PI / 3.;
        Point::new(angle.cos(), angle.sin())
      })
      .collect();
    let before = model
      .measure(&points, 0.55, &mut Budget(usize::MAX))
      .ok_or("measurement failed")?
      .cost;
    for budget in [1, 32, 128, 512, 2048] {
      let mut candidate = points.clone();
      improve(&model, &mut candidate, &LayoutOptions::default(), &mut Budget(budget));
      let after = model
        .measure(&candidate, 0.55, &mut Budget(usize::MAX))
        .ok_or("measurement failed")?
        .cost;
      assert!(after <= before + 1e-10, "budget {budget}: {before} -> {after}");
      for edge in &model.edges {
        let [a, b] = edge.ends;
        assert!((candidate[a].distance(candidate[b]) - 1.).abs() < 1e-9);
      }
    }
    Ok(())
  }
}
