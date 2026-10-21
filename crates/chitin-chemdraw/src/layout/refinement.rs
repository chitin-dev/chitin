//! Bounded, deterministic branch search over complete molecular coordinates.
//!
//! Single-bond bridges split movable branches; two-anchor separators additionally
//! allow complete ring fragments to reflect about their shared attachment axis.
//! Rigid branch transforms
//! preserve angle magnitudes; flexible ring candidates and terminal hydroxyl
//! clearance allow separately bounded saturated-carbon host-angle adjustments.
//! Every transform preserves all bond lengths
//! and internal fragment geometry. Linear triple/cumulated-double centers are protected.
//! This is drawing optimization, not a force field or chemical-energy model.

use super::quality::{self, Budget, Quality, segment_distance};
use super::{Layout2d, LayoutDiagnostic, LayoutError as Error, LayoutOptions};
use crate::geometry::{Bounds, Point};
use chitin_chem::{AtomId, BondId, BondOrder, MolecularGraph, perception::analyze_rings};
use std::{
  collections::{BTreeMap, BTreeSet},
  f64::consts::PI,
};

mod backbone;
mod combinations;
mod hinges;
mod ports;
mod terminal;
mod trials;

/// Additional host-angle freedom for whole-block ring candidate search only.
#[derive(Clone, Copy)]
pub(super) enum AttachmentPolicy {
  /// Preserve the established rigid branch policy for ordinary layouts.
  Rigid,
  /// Permit bounded saturated-carbon host-angle trials around flexible blocks.
  Saturated,
}

/// One dense skeleton edge, retaining its original graph identity.
struct Edge {
  /// Source handle used by unresolved-crossing diagnostics.
  id: BondId,
  /// Dense endpoint rows in the same component model.
  ends: [usize; 2],
  /// A topological bridge is movable only when its supplied order is single.
  movable: bool,
}

/// Temporary component-local representation; no chemical graph ownership.
struct Model {
  /// Stable atom handles, independent of holes left by graph editing.
  ids: Vec<AtomId>,
  /// Every skeleton edge in stable bond order.
  edges: Vec<Edge>,
  /// Parallel geometry-only edges consumed by the shared score evaluator.
  segments: Vec<[usize; 2]>,
  /// Incident edge indices for iterative branch extraction.
  rows: Vec<Vec<usize>>,
  /// Whether branch rotation at each atom would violate a linear center.
  linear: Vec<bool>,
  /// Two-anchor flips cannot change multiple-bond or supplied stereo centers.
  hinge_hosts: Vec<bool>,
  /// Neutral saturated carbon attachment hosts permit bounded drawing-angle flex.
  flexible_hosts: Vec<bool>,
  /// Neutral saturated ring carbons eligible for bounded external-port repair.
  ring_ports: Vec<bool>,
  /// Eligible saturated-carbon hydroxyls; ring oxygens and carbonyls are excluded.
  hydroxyls: Vec<terminal::Hydroxyl>,
  /// Dense axis/reference quadruples with required, coordinate-independent sides.
  double_stereo: Vec<([usize; 4], chitin_chem::DoubleBondRelation)>,
  /// Ordered foreground/background edge rows authored by a perspective template.
  perspective: Vec<(usize, usize)>,
  /// Host-relative label envelopes in normalized y-up units; no font dependency.
  labels: Vec<(usize, [Point; 2])>,
  /// Final label-feedback proposals cannot worsen the scored acyclic chain angles.
  preserve_chain_angles: bool,
}

impl Model {
  /// Reserves coupled-ring repair only for blocks not owned by simple-ring models.
  fn restrict_hinges(&mut self, rings: &chitin_chem::perception::RingTopology) {
    for (row, &id) in self.ids.iter().enumerate() {
      self.hinge_hosts[row] &= rings
        .blocks()
        .iter()
        .any(|block| block.cycle_rank() >= 3 && block.atoms().contains(&id));
    }
  }
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
    let flexible_hosts: Vec<bool> = ids
      .iter()
      .enumerate()
      .map(|(i, &id)| {
        orders[i].iter().all(|&order| order == BondOrder::Single)
          && graph.atom(id).is_some_and(|atom| {
            atom.element() == chitin_chem::Element::Carbon
              && atom.formal_charge() == Some(0)
              && !atom.is_aromatic()
              && atom.unpaired_electrons().is_none_or(|count| count == 0)
          })
      })
      .collect();
    let ring_ports = flexible_hosts
      .iter()
      .enumerate()
      .map(|(row, &eligible)| eligible && rows[row].iter().any(|&e| !edges[e].movable))
      .collect();
    let hinge_hosts = ids
      .iter()
      .enumerate()
      .map(|(row, &id)| graph.tetrahedral(id).is_none() && orders[row].iter().all(|&order| order == BondOrder::Single))
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
    let segments = edges.iter().map(|edge| edge.ends).collect();
    Self {
      ids,
      edges,
      segments,
      rows,
      linear,
      hinge_hosts,
      flexible_hosts,
      ring_ports,
      hydroxyls,
      double_stereo,
      perspective: Vec::new(),
      labels: Vec::new(),
      preserve_chain_angles: false,
    }
  }

  /// Bounds host angles against stage entry, never against accumulated proposals.
  ///
  /// Internal branch geometry remains rigid. Only saturated carbon hosts qualify;
  /// planar multiple-bond centers cannot trade their angles for less crowding.
  /// Preserving ray order also avoids passing through a degenerate stereo view.
  fn preserves_host_angles(&self, host: usize, initial: &[Point], candidate: &[Point]) -> bool {
    let neighbors: Vec<_> = self.rows[host]
      .iter()
      .map(|&edge| {
        let [a, b] = self.edges[edge].ends;
        if a == host { b } else { a }
      })
      .collect();
    for (i, &a) in neighbors.iter().enumerate() {
      for &b in &neighbors[i + 1..] {
        let angle = |points: &[Point]| {
          let center = points[host];
          let u = points[a];
          let v = points[b];
          let cross = (u.x - center.x) * (v.y - center.y) - (u.y - center.y) * (v.x - center.x);
          let cosine = ((u.x - center.x) * (v.x - center.x) + (u.y - center.y) * (v.y - center.y))
            / (u.distance(center) * v.distance(center));
          (cosine.clamp(-1., 1.).acos(), cross)
        };
        let (before, side) = angle(initial);
        let (after, actual) = angle(candidate);
        if (after - before).abs() > PI / 6. + 1e-8
          || after < before.min(PI / 3.) - 1e-8
          || (side.abs() > 1e-8 && side * actual <= 0.)
        {
          return false;
        }
      }
    }
    true
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

  /// Tests whether a rigid branch cut can change any currently reported contact.
  ///
  /// Distances and intersections wholly inside either rigid half are invariant.
  /// Atom-edge contacts include all three rows, not just the edge's endpoints;
  /// measured label contacts lack row provenance and conservatively keep trials.
  fn affects_contacts(&self, branch: &[usize], current: &Quality) -> bool {
    current.label_contacts > 0
      || current
        .crowds
        .iter()
        .any(|&(a, b)| branch.contains(&a) != branch.contains(&b))
      || current.atom_edges.iter().any(|&(atom, edge)| {
        let [a, b] = self.edges[edge].ends;
        let moving = branch.contains(&atom);
        moving != branch.contains(&a) || moving != branch.contains(&b)
      })
      || current.crossings.iter().any(|&(a, b)| {
        let [first, second] = self.edges[a].ends;
        [second, self.edges[b].ends[0], self.edges[b].ends[1]]
          .iter()
          .any(|row| branch.contains(row) != branch.contains(&first))
      })
  }

  /// Measures nearby geometry with unit-cell broad-phase pruning.
  ///
  /// # Parameters
  /// - `points`: complete normalized component coordinates, including rigid projections.
  /// - `threshold`: reportable atom-center distance, independent of cost weights.
  /// - `budget`: candidate work accounting; inspection uses an unlimited budget.
  ///
  /// # Returns
  /// All close atom pairs and proper edge crossings, plus an atom-edge clearance
  /// penalty. `None` abandons a candidate before budget overspending. Grid costs
  /// are local for ordinary drawings, but dense overlap can still be quadratic.
  fn measure(&self, points: &[Point], threshold: f64, budget: &mut Budget) -> Option<Quality> {
    let mut quality = quality::measure(points, &self.segments, threshold, budget)?;
    if !budget.charge(self.perspective.len()) {
      return None;
    }
    for &(front, back) in &self.perspective {
      let pair = (front.min(back), front.max(back));
      if quality.crossings.remove(&pair) {
        quality.cost = (quality.cost - 8.).max(0.);
      }
    }
    for (i, &(host, bounds)) in self.labels.iter().enumerate() {
      if !budget.charge(self.edges.len() + self.labels.len()) {
        return None;
      }
      let bounds = bounds.map(|p| p.translated(points[host].x, points[host].y));
      for edge in &self.edges {
        let [a, b] = edge.ends;
        if a != host && b != host && crate::geometry::segment_enters_box(points[a], points[b], bounds, 0.04) {
          quality.cost += 5.;
          quality.label_contacts += 1;
        }
      }
      for &(other, region) in &self.labels[i + 1..] {
        let [min, max] = region.map(|p| p.translated(points[other].x, points[other].y));
        if bounds[0].x < max.x && bounds[1].x > min.x && bounds[0].y < max.y && bounds[1].y > min.y {
          quality.cost += 5.;
          quality.label_contacts += 1;
        }
      }
    }
    Some(quality)
  }
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
  let hinge_reserve = if (current.crossings.is_empty() && current.label_contacts == 0)
    || !model.perspective.is_empty()
    || !model.hinge_hosts.iter().any(|&eligible| eligible)
  {
    0
  } else {
    budget.0 / 3
  };
  budget.0 -= hinge_reserve;
  budget.0 -= reserve;
  let exhausted = improve_branches(model, points, &mut current, options, budget);
  let mut hinges = Budget(hinge_reserve);
  let hinge_exhausted = hinges::improve(model, points, &mut current, options, &mut hinges);
  budget.0 += hinges.0;
  let mut local = Budget(reserve);
  let local_exhausted = terminal::improve(model, points, &mut current, options, &mut local);
  budget.0 += local.0;
  exhausted || hinge_exhausted || local_exhausted
}

/// Repairs automatic geometry using host-relative, font-independent envelopes.
///
/// # Parameters
/// - `graph`, `layout`: matching complete automatic geometry and source handles.
/// - `regions`: measured host-relative boxes in normalized y-up units.
/// - `options`, `allowance`: geometric constraints and independent label-repair work.
///
/// # Returns
/// Updates `layout` with the bounded combined-search proposal and returns the
/// preceding local-search proposal separately. Rendering must compare both to
/// its original champion: proxy improvement does not certify final text layout.
/// Actual font measurement belongs to that caller; this layer consumes only
/// normalized rectangles. Existing perspective metadata is never altered.
///
/// # Errors
/// Reports missing coordinates or nonfinite bounds, never a partial component.
pub(crate) fn clear_labels(
  graph: &MolecularGraph,
  layout: &mut Layout2d,
  regions: &BTreeMap<AtomId, [Point; 2]>,
  options: &LayoutOptions,
  allowance: usize,
) -> Result<Option<Layout2d>, Error> {
  if regions.is_empty()
    || !layout.crossings.is_empty()
    || !layout.depths.is_empty()
    || allowance == 0
    || options.refinement_passes == 0
  {
    return Ok(None);
  }
  let rings = analyze_rings(graph)?;
  let mut budget = Budget(allowance);
  for component in graph.connected_components() {
    let mut model = Model::new(graph, component, rings.bridges());
    model.restrict_hinges(&rings);
    model.flexible_hosts.fill(false);
    model.preserve_chain_angles = true;
    model.labels = model
      .ids
      .iter()
      .enumerate()
      .filter_map(|(row, id)| regions.get(id).map(|&bounds| (row, bounds)))
      .collect();
    let mut points: Vec<_> = model.ids.iter().map(|id| layout.positions[id]).collect();
    improve(&model, &mut points, options, &mut budget);
    let quality = model
      .measure(&points, options.minimum_spacing, &mut Budget(usize::MAX))
      .ok_or(Error::CoordinateRange)?;
    publish_labels(&model, &points, &quality, layout);
  }
  layout.bounds = Bounds::from_points(layout.positions.values().copied())?;
  // Complete every component's established repair before optional search can
  // consume its remaining work. A speculative first component must not starve
  // another component or change the independent local-search proposal.
  let local = layout.clone();
  for component in graph.connected_components() {
    if !budget.charge(component.len() + graph.bond_count()) {
      break;
    }
    let mut model = Model::new(graph, component, rings.bridges());
    model.labels = model
      .ids
      .iter()
      .enumerate()
      .filter_map(|(row, id)| regions.get(id).map(|&bounds| (row, bounds)))
      .collect();
    let mut points: Vec<_> = model.ids.iter().map(|id| layout.positions[id]).collect();
    let Some(mut quality) = model.measure(&points, options.minimum_spacing, &mut budget) else {
      break;
    };
    combinations::improve(&model, &mut points, &mut quality, options, &mut budget);
    publish_labels(&model, &points, &quality, layout);
  }
  layout.bounds = Bounds::from_points(layout.positions.values().copied())?;
  Ok(Some(local))
}

/// Supplies a bounded local neighborhood for final, exact rendered-label ranking.
///
/// # Parameters
/// - `graph`, `layout`: immutable chemistry and current complete champion.
/// - `labels`: source hosts participating in actual label/stroke contacts.
/// - `options`, `allowance`: unchanged spacing and a separate bounded work pool.
///
/// # Returns
/// At most eight complete layouts, differing only at eligible terminal hydroxyl
/// labels involved in measured contacts. Every component diagnostic is refreshed
/// before a proposal leaves this stage. No font or depiction type is imported.
///
/// # Errors
/// Propagates inconsistent topology or unrepresentable extents. Work exhaustion
/// returns only complete candidates already prepared, without mutating input.
pub(crate) fn terminal_label_trials(
  graph: &MolecularGraph,
  layout: &Layout2d,
  labels: &BTreeSet<AtomId>,
  options: &LayoutOptions,
  allowance: usize,
) -> Result<Vec<Layout2d>, Error> {
  if allowance == 0 || labels.is_empty() || !layout.crossings.is_empty() || !layout.depths.is_empty() {
    return Ok(Vec::new());
  }
  let mut budget = Budget(allowance);
  if !budget.charge(graph.atom_count() + graph.bond_count()) {
    return Ok(Vec::new());
  }
  let rings = analyze_rings(graph)?;
  let mut trials = Vec::new();
  for component in graph.connected_components() {
    if !component.iter().any(|id| labels.contains(id)) {
      continue;
    }
    if !budget.charge(component.len() + graph.bond_count()) {
      break;
    }
    let model = Model::new(graph, component, rings.bridges());
    let points: Vec<_> = model.ids.iter().map(|id| layout.positions[id]).collect();
    let leaves: Vec<_> = model
      .ids
      .iter()
      .enumerate()
      .filter_map(|(row, id)| labels.contains(id).then_some(row))
      .collect();
    for candidate in terminal::label_trials(&model, &points, &leaves, &mut budget) {
      let Some(quality) = model.measure(&candidate, options.minimum_spacing, &mut budget) else {
        return Ok(trials);
      };
      if !budget.charge(layout.positions.len()) {
        return Ok(trials);
      }
      let mut trial = layout.clone();
      publish_labels(&model, &candidate, &quality, &mut trial);
      trial.bounds = Bounds::from_points(trial.positions.values().copied())?;
      trials.push(trial);
      if trials.len() == 8 {
        return Ok(trials);
      }
    }
  }
  Ok(trials)
}

/// Publishes only a complete, fully measured component into one detached proposal.
fn publish_labels(model: &Model, points: &[Point], quality: &Quality, layout: &mut Layout2d) {
  layout.diagnostics.retain(|d| match d {
    LayoutDiagnostic::CrowdedAtoms { a, .. } => !model.ids.contains(a),
    LayoutDiagnostic::CrossingBonds { a, .. } => !model.edges.iter().any(|edge| edge.id == *a),
    _ => true,
  });
  layout
    .diagnostics
    .extend(quality.crowds.iter().map(|&(a, b)| LayoutDiagnostic::CrowdedAtoms {
      a: model.ids[a],
      b: model.ids[b],
    }));
  layout
    .diagnostics
    .extend(quality.crossings.iter().map(|&(a, b)| LayoutDiagnostic::CrossingBonds {
      a: model.edges[a].id,
      b: model.edges[b].id,
    }));
  for (&id, &point) in model.ids.iter().zip(points) {
    layout.positions.insert(id, point);
  }
}

/// Rechecks complete geometry for final rendered-candidate acceptance.
///
/// # Returns
/// Atom-center, atom-edge and proper-crossing counts in normalized layout units.
/// Authored perspective crossings are excluded only by their exact source IDs.
/// This is not the independent benchmark's tolerance-based ink/edge evaluator.
///
/// # Errors
/// Rejects missing source coordinates and unrepresentable geometry. Depiction
/// validates source coverage before this read-only assessment is requested.
pub(crate) fn contact_counts(graph: &MolecularGraph, layout: &Layout2d, spacing: f64) -> Result<[usize; 3], Error> {
  let ids: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
  let mut model = Model::new(graph, ids, &BTreeSet::new());
  for crossing in &layout.crossings {
    if let (Some(front), Some(back)) = (
      model.edges.iter().position(|edge| edge.id == crossing.front),
      model.edges.iter().position(|edge| edge.id == crossing.back),
    ) {
      model.perspective.push((front, back));
    }
  }
  let points = model
    .ids
    .iter()
    .map(|id| layout.positions.get(id).copied().ok_or(Error::Invariant(*id)))
    .collect::<Result<Vec<_>, _>>()?;
  let quality = model
    .measure(&points, spacing, &mut Budget(usize::MAX))
    .ok_or(Error::CoordinateRange)?;
  Ok([quality.crowds.len(), quality.atom_edges.len(), quality.crossings.len()])
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
  let initial = points.clone();
  for _ in 0..options.refinement_passes {
    if current.cost < 1e-12 || current.clear() {
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
      // Perspective templates have a preferred upright view. A complete cage
      // must not be mirrored/rotated merely to remove an authored crossing.
      if !budget.charge(branch.len() * model.perspective.len() * 4) {
        return true;
      }
      if model.perspective.iter().any(|&(front, back)| {
        model.edges[front]
          .ends
          .iter()
          .chain(&model.edges[back].ends)
          .any(|node| branch.contains(node))
      }) {
        continue;
      }
      let flexible =
        model.flexible_hosts[fixed] && !model.hydroxyls.iter().any(|h| branch.len() == 1 && h.leaf == moving);
      if branch.len() == 1 && !flexible {
        continue;
      }
      // Rigidly moving both atoms of a crowded pair cannot separate them.
      // Focus scarce trials on cuts that actually change a reported contact.
      if !current.crowds.is_empty()
        && current.crossings.is_empty()
        && !current
          .crowds
          .iter()
          .any(|&(a, b)| branch.contains(&a) != branch.contains(&b))
      {
        continue;
      }
      let origin = points[moving];
      let vector = Point::new(origin.x - points[fixed].x, origin.y - points[fixed].y);
      let angle = vector.y.atan2(vector.x);
      let mut best = None;
      let mut best_cost = current.cost;
      let mut exhausted = false;
      // Singleton and two-point branches frequently yield identity/duplicate
      // poses. Larger bent fragments rarely do; avoid caching whole subtrees.
      let mut tried = (branch.len() <= 2).then(trials::Trials::default);
      for transform in 0..if flexible {
        7
      } else if model.linear[moving] {
        1
      } else {
        3
      } {
        if !budget.charge(points.len() + branch.len()) {
          exhausted = true;
          break;
        }
        let mut candidate = points.clone();
        let pivot = if transform >= 3 { points[fixed] } else { origin };
        for &node in &branch {
          let dx = points[node].x - pivot.x;
          let dy = points[node].y - pivot.y;
          let (x, y) = if transform == 0 {
            let (sin, cos) = (2. * angle).sin_cos();
            (cos * dx + sin * dy, sin * dx - cos * dy)
          } else if transform < 3 {
            let (sin, cos) = (if transform == 1 { 2. * PI / 3. } else { -2. * PI / 3. }).sin_cos();
            (cos * dx - sin * dy, sin * dx + cos * dy)
          } else {
            let angle = [PI / 12., -PI / 12., PI / 6., -PI / 6.][transform - 3];
            let (sin, cos) = angle.sin_cos();
            (cos * dx - sin * dy, sin * dx + cos * dy)
          };
          candidate[node] = pivot.translated(x, y);
        }
        if !budget.charge(model.rows[moving].len() + model.double_stereo.len()) {
          exhausted = true;
          break;
        }
        if (transform < 3 && !model.preserves_attachment_angles(moving, fixed, points, &candidate))
          || (transform >= 3 && !model.preserves_host_angles(fixed, &initial, &candidate))
          || !model.preserves_double_stereo(&candidate)
        {
          continue;
        }
        if let Some(tried) = &mut tried {
          match tried.insert(&branch, points, &candidate, budget) {
            Some(true) => {}
            Some(false) => continue,
            None => {
              exhausted = true;
              break;
            }
          }
        }
        let Some(quality) = model.measure(&candidate, options.minimum_spacing, budget) else {
          exhausted = true;
          break;
        };
        if quality.cost + 1e-10 < best_cost {
          best_cost = quality.cost;
          let clear = quality.clear();
          best = Some((candidate, quality));
          if clear {
            break;
          }
        }
      }
      if let Some((candidate, quality)) = best {
        *points = candidate;
        *current = quality;
        changed = true;
        if current.clear() {
          return false;
        }
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
pub(super) fn run(
  graph: &MolecularGraph,
  layout: &mut Layout2d,
  options: &LayoutOptions,
  budget: &mut Budget,
  projected_budget: &mut Budget,
  backbone_budget: &mut Budget,
  attachments: AttachmentPolicy,
) -> Result<(), Error> {
  let rings = analyze_rings(graph)?;
  let mut projected_exhausted = false;
  let mut exhausted = false;
  let mut cursor = 0.;
  layout.diagnostics.clear();
  for component in graph.connected_components() {
    let mut model = Model::new(graph, component, rings.bridges());
    model.restrict_hinges(&rings);
    if matches!(attachments, AttachmentPolicy::Rigid) {
      model.flexible_hosts.fill(false);
    }
    for crossing in &layout.crossings {
      let front = model.edges.iter().position(|edge| edge.id == crossing.front);
      let back = model.edges.iter().position(|edge| edge.id == crossing.back);
      match (front, back) {
        (Some(front), Some(back)) => model.perspective.push((front, back)),
        (None, None) => {}
        _ => return Err(Error::Invariant(model.ids[0])),
      }
    }
    let mut points: Vec<_> = model
      .ids
      .iter()
      .map(|id| layout.positions.get(id).copied().ok_or(Error::Invariant(*id)))
      .collect::<Result<_, _>>()?;
    for &(front, back) in &model.perspective {
      let [a, b] = model.edges[front].ends;
      let [c, d] = model.edges[back].ends;
      if !quality::crosses(points[a], points[b], points[c], points[d]) {
        return Err(Error::Invariant(model.ids[a]));
      }
    }
    if !exhausted && options.refinement_passes > 0 {
      exhausted = improve(&model, &mut points, options, budget);
    }
    if options.refinement_passes > 0 && backbone_budget.0 > 0 {
      backbone::route(&model, &mut points, options, backbone_budget);
    }
    if !projected_exhausted
      && options.refinement_passes > 0
      && options.optimization.iterations > 0
      && options.optimization.work_budget > 0
    {
      projected_exhausted = super::optimization::component(
        super::optimization::Scope {
          graph,
          anchors: &BTreeSet::new(),
          bridges: rings.bridges(),
        },
        &model.ids,
        &mut points,
        &options.optimization,
        projected_budget,
        true,
      )?
      .exhausted;
    }
    if model.perspective.is_empty() {
      orient(&mut points);
    }
    let mut quality = model
      .measure(&points, options.minimum_spacing, &mut Budget(usize::MAX))
      .ok_or(Error::CoordinateRange)?;
    if options.refinement_passes > 0 && model.perspective.is_empty() {
      exhausted |= ports::improve(&model, &mut points, &mut quality, options, budget);
    }
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
  if projected_exhausted {
    layout.diagnostics.push(LayoutDiagnostic::OptimizationBudgetExhausted);
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
  fn authored_crossing_should_not_suppress_other_accidental_intersections() -> TestResult {
    let graph = read_smiles("CC.CC.CC.CC")?.into_graph();
    let ids = graph.atoms().map(|(id, _)| id).collect();
    let rings = analyze_rings(&graph)?;
    let mut model = Model::new(&graph, ids, rings.bridges());
    model.perspective.push((0, 1));
    let points = [
      Point::new(-1., -1.),
      Point::new(1., 1.),
      Point::new(-1., 1.),
      Point::new(1., -1.),
      Point::new(3., -1.),
      Point::new(5., 1.),
      Point::new(3., 1.),
      Point::new(5., -1.),
    ];
    let quality = model
      .measure(&points, 0.55, &mut Budget(usize::MAX))
      .ok_or("measurement failed")?;
    assert_eq!(quality.crossings, BTreeSet::from([(2, 3)]));
    Ok(())
  }

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
