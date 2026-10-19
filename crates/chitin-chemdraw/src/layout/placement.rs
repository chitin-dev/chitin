//! Ring-aware placement with constrained non-ring angle allocation.
//!
//! Ring blocks are seeded before branch expansion. A spatial grid limits local
//! crowding queries; a separate refinement pass can then move whole branches.

use super::{Layout2d, LayoutError as Error, LayoutOptions, rings::Rings};
use crate::geometry::Point;
use chitin_chem::{AtomId, BondOrder, GraphError, MolecularGraph};
use std::{
  collections::{BTreeMap, VecDeque},
  f64::consts::PI,
};

/// Atom count above which wide terminal-ligand searches are attempted.
const LARGE_COMPONENT: usize = 150;

/// Temporary adjacency view; dense indices never replace domain handles.
pub(super) struct Topology {
  /// Original graph handles, in stable iteration order.
  pub(super) ids: Vec<AtomId>,
  /// Neighbor dense indices and supplied integer bond orders.
  neighbors: Vec<Vec<(usize, BondOrder)>>,
  /// Disjoint connected components, each in stable handle order.
  components: Vec<Vec<usize>>,
}

impl Topology {
  /// Builds deterministic adjacency and rejects unsupported edge labels.
  ///
  /// # Parameters
  /// - `graph`: borrowed input with symmetric simple-graph adjacency.
  ///
  /// # Returns
  /// A detached O(V + E) topology view; IDs remain scoped to the borrowed input.
  ///
  /// # Errors
  /// Propagates endpoint-reference errors and reports noninteger/aromatic orders.
  fn new(graph: &MolecularGraph) -> Result<Self, Error> {
    let ids: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
    let indices: BTreeMap<_, _> = ids.iter().enumerate().map(|(i, &id)| (id, i)).collect();
    let mut neighbors = vec![Vec::new(); ids.len()];
    for (id, bond) in graph.bonds() {
      if !matches!(bond.order(), BondOrder::Single | BondOrder::Double | BondOrder::Triple) {
        return Err(Error::Bond(id));
      }
      let [a, b] = bond.atoms();
      let a = *indices.get(&a).ok_or(GraphError::MissingAtom(a))?;
      let b = *indices.get(&b).ok_or(GraphError::MissingAtom(b))?;
      neighbors[a].push((b, bond.order()));
      neighbors[b].push((a, bond.order()));
    }
    for row in &mut neighbors {
      row.sort_unstable_by_key(|&(node, _)| node);
    }
    let components = graph
      .connected_components()
      .into_iter()
      .map(|component| {
        component
          .into_iter()
          .map(|id| indices.get(&id).copied().ok_or(GraphError::MissingAtom(id)))
          .collect::<Result<Vec<_>, _>>()
      })
      .collect::<Result<Vec<_>, _>>()?;
    Ok(Self {
      ids,
      neighbors,
      components,
    })
  }
}

/// Unit-cell index for already placed centers; each atom is inserted exactly once.
#[derive(Default)]
struct Grid(BTreeMap<(i64, i64), Vec<usize>>);

impl Grid {
  /// Converts finite normalized coordinates to cells with room for neighbor offsets.
  ///
  /// # Errors
  /// Rejects nonfinite or out-of-range coordinates before float-to-integer casts.
  fn key(point: Point) -> Result<(i64, i64), Error> {
    if !point.is_finite() || point.x.abs() > i64::MAX as f64 / 2. || point.y.abs() > i64::MAX as f64 / 2. {
      return Err(Error::CoordinateRange);
    }
    Ok((point.x.floor() as i64, point.y.floor() as i64))
  }

  /// Visits the nine cells capable of containing centers less than one unit away.
  ///
  /// # Errors
  /// Propagates the coordinate-range guard before constructing adjacent cell keys.
  fn nearby(&self, point: Point) -> Result<impl Iterator<Item = usize> + '_, Error> {
    let (x, y) = Self::key(point)?;
    Ok(
      (-1..=1)
        .flat_map(move |dx| (-1..=1).flat_map(move |dy| self.0.get(&(x + dx, y + dy)).into_iter().flatten().copied())),
    )
  }
}

/// Suspended tree expansion; heading points away from the parent or ring center.
struct Branch {
  /// Current dense vertex, already inserted into the spatial grid.
  node: usize,
  /// Forward angle away from the incoming edge, or outward from a cycle center.
  heading: f64,
  /// Tree expansion depth used to alternate zigzag bends deterministically.
  depth: usize,
  /// Cycle seeds spread attachments around their exterior rather than a chain bend.
  radial: bool,
}

/// Returns a stored position, treating an absent internal placement as a bug.
fn position(positions: &[Option<Point>], topology: &Topology, node: usize) -> Result<Point, Error> {
  positions[node].ok_or(Error::Invariant(topology.ids[node]))
}

/// Inserts one generated center into the placement snapshot and spatial index.
///
/// # Parameters
/// - `node`, `point`: previously unplaced vertex and candidate unit-space position.
/// - `positions`, `grid`: the same dense namespace, with no entry for this node.
///
/// # Returns
/// Success after storing the center and adding its index to the grid. Quality
/// diagnostics belong to final inspection, not provisional placement contacts.
///
/// # Errors
/// Rejects unrepresentable grid keys before changing either collection.
fn place(node: usize, point: Point, positions: &mut [Option<Point>], grid: &mut Grid) -> Result<(), Error> {
  let key = Grid::key(point)?;
  positions[node] = Some(point);
  grid.0.entry(key).or_default().push(node);
  Ok(())
}

/// Rotates a crowded terminal ligand around its parent to restore clearance.
///
/// Only a degree-one vertex moves. Its bond length is preserved by construction
/// and the rotation is limited to forty degrees, so the parent keeps a
/// recognizable angle. The pass is deliberately last: it repairs residual
/// contacts of whole-branch placement without reshuffling any subtree.
fn relax_terminal_leaves(topology: &Topology, rings: &Rings, positions: &mut [Option<Point>]) -> Result<(), Error> {
  // The wide search is only justified for very large flexible inputs; smaller
  // drawings keep the established local angle policy and pose selection.
  let wide = positions.len() > LARGE_COMPONENT;
  for node in 0..positions.len() {
    let (Some(point), 1) = (positions[node], topology.neighbors[node].len()) else {
      continue;
    };
    let (parent, _) = topology.neighbors[node][0];
    // A multiple-bond center fixes its substituent sectors. The narrow policy
    // keeps its ring exception for aromatic hydroxyls; the wide search never
    // rotates a double-bond reference, because that can invert E/Z.
    if topology.neighbors[parent]
      .iter()
      .any(|&(_, order)| order != BondOrder::Single)
      && (wide || rings.membership[parent].is_empty())
    {
      continue;
    }
    let Some(origin) = positions[parent] else {
      continue;
    };
    let before = close_pairs(node, point, positions);
    if before == 0 {
      continue;
    }
    let base = (point.y - origin.y).atan2(point.x - origin.x);
    let mut best = (before, nearest_atom(node, point, positions), point);
    for step in 1..=if wide { 18 } else { 4 } {
      for sign in [1., -1.] {
        let angle = base + sign * step as f64 * PI / 18.;
        let mut blocked = false;
        for &(other, _) in &topology.neighbors[parent] {
          if other == node {
            continue;
          }
          if let Some(placed) = positions[other] {
            let existing = (placed.y - origin.y).atan2(placed.x - origin.x);
            let limit = if wide { PI / 3. } else { PI / 2.5 };
            if ((angle - existing + PI).rem_euclid(2. * PI) - PI).abs() < limit {
              blocked = true;
              break;
            }
          }
        }
        if blocked {
          continue;
        }
        let candidate = origin.translated(angle.cos(), angle.sin());
        if wide && leaf_crosses(node, origin, candidate, positions, topology) {
          continue;
        }
        let pairs = close_pairs(node, candidate, positions);
        // Wide repairs may graze a bond when that buys real atomic separation;
        // the narrow policy keeps its original hard rejection.
        if near_bond(node, candidate, positions, topology) && (!wide || pairs >= best.0) {
          continue;
        }
        let nearest = nearest_atom(node, candidate, positions);
        if pairs < best.0 || (pairs == best.0 && nearest > best.1 + 1e-9) {
          best = (pairs, nearest, candidate);
        }
      }
    }
    if best.0 < before || (best.0 == before && best.1 > nearest_atom(node, point, positions) + 1e-9) {
      positions[node] = Some(best.2);
    }
  }
  Ok(())
}

/// Counts other placed atoms closer than the crowding threshold.
fn close_pairs(node: usize, point: Point, positions: &[Option<Point>]) -> usize {
  positions
    .iter()
    .enumerate()
    .filter(|&(other, candidate)| other != node && candidate.is_some_and(|other| point.distance(other) < 0.55))
    .count()
}

/// Returns the nearest other placed atom distance.
fn nearest_atom(node: usize, point: Point, positions: &[Option<Point>]) -> f64 {
  positions
    .iter()
    .enumerate()
    .filter(|&(other, _)| other != node)
    .filter_map(|(_, &candidate)| candidate.map(|other| point.distance(other)))
    .fold(f64::INFINITY, f64::min)
}

/// Reports whether the moved leaf bond would cross a nonincident bond.
fn leaf_crosses(node: usize, origin: Point, point: Point, positions: &[Option<Point>], topology: &Topology) -> bool {
  topology.neighbors.iter().enumerate().any(|(other, row)| {
    if other == node {
      return false;
    }
    let Some(first) = positions[other] else {
      return false;
    };
    row.iter().any(|&(next, _)| {
      next != node && positions[next].is_some_and(|second| super::quality::crosses(origin, point, first, second))
    })
  })
}

/// Reports whether a candidate leaf position grazes a nonincident bond.
fn near_bond(node: usize, point: Point, positions: &[Option<Point>], topology: &Topology) -> bool {
  topology.neighbors.iter().enumerate().any(|(other, row)| {
    if other == node {
      return false;
    }
    let Some(origin) = positions[other] else {
      return false;
    };
    row.iter().any(|&(next, _)| {
      next != node
        && positions[next].is_some_and(|beyond| super::quality::segment_distance(point, origin, beyond) < 0.15)
    })
  })
}

/// Chooses between angle-preserving mirrored assignments of a sibling group.
///
/// Linear degree-two geometry is constrained rather than rotated to avoid crowds.
/// Mirroring preserves the incoming/outgoing angle sectors, unlike rotating the
/// group freely around its parent. Residual crowding remains diagnostic.
///
/// # Parameters
/// - `children`: unplaced neighbors, in deterministic adjacency order.
/// - `grid`: already fixed centers in this component only.
/// - `rings`: cyclic membership distinguishing fused neighbors from an external
///   parent; neighbor count alone cannot establish exterior-sector occupancy.
///
/// # Returns
/// The lowest-cost candidate among mirrored assignments and, for rotatable ring
/// attachments, fine whole-group rotations. Ties retain earlier candidates.
/// This local heuristic does not optimize label envelopes or certify that bond
/// crossings are absent.
///
/// # Errors
/// Propagates invalid occupied positions or spatial-index coordinate failures.
fn candidates(
  topology: &Topology,
  positions: &[Option<Point>],
  grid: &Grid,
  branch: &Branch,
  children: &[(usize, BondOrder)],
  rings: &Rings,
) -> Result<Vec<(usize, Point, f64)>, Error> {
  let origin = position(positions, topology, branch.node)?;
  let row = &topology.neighbors[branch.node];
  let linear = row.len() == 2
    && (row.iter().any(|&(_, order)| order == BondOrder::Triple)
      || row.iter().all(|&(_, order)| order == BondOrder::Double));
  let offsets: Vec<_> = if branch.radial {
    let external_parent = row
      .iter()
      .any(|&(node, _)| positions[node].is_some() && !rings.shares_system(branch.node, node));
    if children.len() == 1 && row.len() - children.len() >= 3 && external_parent {
      // An entered ring already has an external parent in its exterior sector.
      // Leave that occupied direction fixed and try either adjacent outer slot.
      vec![PI / 3.]
    } else {
      (0..children.len())
        .map(|i| (i as f64 - (children.len() - 1) as f64 / 2.) * PI / 3.)
        .collect()
    }
  } else {
    let bend = if branch.depth.is_multiple_of(2) {
      super::angles::CHAIN_BEND
    } else {
      -super::angles::CHAIN_BEND
    };
    super::angles::offsets(children.len(), row.len() > children.len(), bend, linear)
  };
  let mut best = Vec::new();
  let mut best_cost = f64::INFINITY;
  // The canonical assignment is evaluated first. Ring attachments additionally
  // try fine whole-group rotations, which pay a bend penalty and therefore only
  // win when they remove a real contact.
  let mut rotations: Vec<f64> = vec![0.];
  let mut index = 0;
  while index < rotations.len() {
    let rotation = rotations[index];
    for (attempt, sign) in [1., -1.].into_iter().take(if linear { 1 } else { 2 }).enumerate() {
      let mut group = Vec::new();
      let mut cost = attempt as f64 * 0.0001 + 0.6 * rotation * rotation;
      for ((node, _), offset) in children.iter().zip(&offsets) {
        let angle = branch.heading + rotation + sign * offset;
        let point = origin.translated(angle.cos(), angle.sin());
        if rotation.abs() > 1e-9 {
          for &(other, _) in row {
            if other == *node || positions[other].is_none() {
              continue;
            }
            let placed = position(positions, topology, other)?;
            let existing = (placed.y - origin.y).atan2(placed.x - origin.x);
            let delta = (angle - existing + PI).rem_euclid(2. * PI) - PI;
            if delta.abs() < PI / 3. {
              cost += 4. * (PI / 3. - delta.abs()).powi(2);
            }
          }
        }
        for other in grid.nearby(point)? {
          if other == branch.node {
            continue;
          }
          let distance = point.distance(position(positions, topology, other)?);
          cost += (0.95 - distance).max(0.).powi(2);
        }
        for &(_, sibling, _) in &group {
          cost += (0.95 - point.distance(sibling)).max(0.).powi(2);
        }
        group.push((*node, point, angle));
      }
      if cost < best_cost {
        best = group;
        best_cost = cost;
      }
    }
    // Ring attachments may rotate even when aromatic Kekule bonds make the
    // parent look unsaturated; an acyclic multiple-bond center may not, because
    // its 120-degree sectors are a fixed chemical projection.
    let rotatable =
      row.iter().all(|&(_, order)| order == BondOrder::Single) || !rings.membership[branch.node].is_empty();
    if index == 0 && !branch.radial && rotatable && !linear {
      for step in 1..=6 {
        let angle = step as f64 * PI / 12.;
        rotations.push(angle);
        rotations.push(-angle);
      }
    }
    index += 1;
  }
  Ok(best)
}

/// Embeds supported components, expands their trees, then refines and packs them.
///
/// # Parameters
/// - `graph`: detached integer-bond graph; handles match the source exactly.
/// - `source`: original chemical facts, including supplied aromatic markers.
/// - `options`: already validated by the public layout entry point.
///
/// # Returns
/// A complete coordinate snapshot plus unresolved center-crowding diagnostics.
/// Packed translations preserve every component's internal bond lengths.
///
/// # Errors
/// Returns typed unsupported-topology, resource, or coordinate failures. No input
/// facts are mutated and partially placed components are never returned.
pub(super) fn generate(
  graph: &MolecularGraph,
  source: &MolecularGraph,
  options: &LayoutOptions,
) -> Result<Layout2d, Error> {
  let topology = Topology::new(graph)?;
  let mut ring_work = super::rings::Work::new(options.ring_search_budget);
  let rings = Rings::new(graph, source, &topology, &mut ring_work)?;
  let mut refinement_work = super::quality::Budget(options.refinement_budget);
  let mut optimization_work = super::quality::Budget(options.optimization.work_budget);
  let attachment_policy = if rings.variants() > 1 {
    super::refinement::AttachmentPolicy::Saturated
  } else {
    super::refinement::AttachmentPolicy::Rigid
  };
  // A compact folded macrocycle is a deliberate drawing choice, but only when
  // the complete molecule is conflict-free. Those poses are tried first and
  // cost no more than a bounded share of the shared refinement allowance.
  if options.refinement_passes > 0 {
    for variant in 0..rings.folded().min(3) {
      if refinement_work.0 < 200_000 {
        break;
      }
      let mut candidate = embed(graph, &topology, &rings, &mut ring_work, options, variant)?;
      super::refinement::run(
        graph,
        &mut candidate,
        options,
        &mut refinement_work,
        &mut optimization_work,
        attachment_policy,
      )?;
      let clean = !candidate.diagnostics().iter().any(|d| {
        matches!(
          d,
          super::LayoutDiagnostic::CrowdedAtoms { .. } | super::LayoutDiagnostic::CrossingBonds { .. }
        )
      });
      if clean {
        return Ok(candidate);
      }
    }
  }
  let mut result = embed(graph, &topology, &rings, &mut ring_work, options, 0)?;
  // Rank complete molecules before spending the shared refinement allowance.
  // Ring-only scores cannot see collisions with the expanded substituents.
  let mut alternatives = Vec::new();
  if options.refinement_passes > 0 && rings.variants() > 1 {
    let (cost, conflicts) = complete_cost(graph, &result, &mut ring_work)?;
    // A clean initial pose needs no alternative enumeration. In particular,
    // long unsubstituted macrocycles must not pay for twelve identical scans.
    if conflicts {
      alternatives.push((cost, result));
      for variant in 1..rings.variants() {
        let candidate = embed(graph, &topology, &rings, &mut ring_work, options, variant)?;
        alternatives.push((complete_cost(graph, &candidate, &mut ring_work)?.0, candidate));
      }
      alternatives.sort_by(|a, b| b.0.total_cmp(&a.0));
      result = alternatives.pop().ok_or(Error::CoordinateRange)?.1;
    }
  }
  super::refinement::run(
    graph,
    &mut result,
    options,
    &mut refinement_work,
    &mut optimization_work,
    attachment_policy,
  )?;
  if options.refinement_passes > 0
    && rings.variants() > 1
    && result.diagnostics().iter().any(|d| {
      matches!(
        d,
        super::LayoutDiagnostic::CrowdedAtoms { .. } | super::LayoutDiagnostic::CrossingBonds { .. }
      )
    })
  {
    let mut cost = complete_cost(graph, &result, &mut ring_work)?.0;
    while let Some((_, mut candidate)) = alternatives.pop() {
      super::refinement::run(
        graph,
        &mut candidate,
        options,
        &mut refinement_work,
        &mut optimization_work,
        attachment_policy,
      )?;
      let candidate_cost = complete_cost(graph, &candidate, &mut ring_work)?.0;
      if candidate_cost + 1e-10 < cost {
        result = candidate;
        cost = candidate_cost;
      }
      if !result.diagnostics().iter().any(|d| {
        matches!(
          d,
          super::LayoutDiagnostic::CrowdedAtoms { .. } | super::LayoutDiagnostic::CrossingBonds { .. }
        )
      }) {
        break;
      }
    }
  }
  Ok(result)
}

/// Scores complete fragments, so external groups influence ring-pose selection.
///
/// Trials share the ring work allowance; authored perspective crossings are
/// removed only by their exact source identities, never by geometry alone.
///
/// # Parameters
/// - `graph`, `layout`: matching source topology and complete coordinates.
/// - `work`: shared ring allowance, including final quality inspection.
///
/// # Returns
/// Drawing cost and whether unresolved skeleton conflicts require alternatives.
/// Text-label collisions are outside this atom-center and bond evaluator.
///
/// # Errors
/// Propagates work exhaustion before accepting an unscored candidate.
fn complete_cost(
  graph: &MolecularGraph,
  layout: &Layout2d,
  work: &mut super::rings::Work,
) -> Result<(f64, bool), Error> {
  let mut cost = 0.;
  let mut conflicts = false;
  for component in graph.connected_components() {
    let indices: BTreeMap<_, _> = component.iter().enumerate().map(|(i, &id)| (id, i)).collect();
    let points: Vec<_> = component.iter().map(|id| layout.positions[id]).collect();
    let bonds: Vec<_> = graph
      .bonds()
      .filter_map(|(id, b)| {
        let [a, b] = b.atoms();
        Some((id, [*indices.get(&a)?, *indices.get(&b)?]))
      })
      .collect();
    let edges: Vec<_> = bonds.iter().map(|(_, ends)| *ends).collect();
    let quality = work.measure(&points, &edges)?;
    let mut crossings = quality.crossings.len();
    cost += quality.cost + 10. * quality.crowds.len() as f64 + 100. * quality.crossings.len() as f64;
    for crossing in &layout.crossings {
      let a = bonds.iter().position(|(id, _)| *id == crossing.front);
      let b = bonds.iter().position(|(id, _)| *id == crossing.back);
      if let (Some(a), Some(b)) = (a, b)
        && quality.crossings.contains(&(a.min(b), a.max(b)))
      {
        cost -= 108.;
        crossings -= 1;
      }
    }
    conflicts |= !quality.crowds.is_empty() || crossings > 0;
  }
  Ok((cost.max(0.), conflicts))
}

/// Expands every branch of one finite ring-pose combination before scoring it.
fn embed(
  graph: &MolecularGraph,
  topology: &Topology,
  rings: &Rings,
  ring_work: &mut super::rings::Work,
  options: &LayoutOptions,
  variant: usize,
) -> Result<Layout2d, Error> {
  let mut embedded = vec![false; rings.len()];
  let mut positions = vec![None; topology.ids.len()];
  let mut crossings = Vec::new();
  for component in &topology.components {
    let root = *component.first().ok_or(Error::CoordinateRange)?;
    let mut grid = Grid::default();
    let mut pending = VecDeque::new();
    if let Some(seed) = rings.seed(component) {
      embedded[seed] = true;
      let placed = rings.embed(seed, &positions, component, 0., ring_work, variant)?;
      crossings.extend(placed.crossings);
      for (node, point, heading) in placed.atoms {
        place(node, point, &mut positions, &mut grid)?;
        pending.push_back(Branch {
          node,
          heading,
          depth: 0,
          radial: true,
        });
      }
    } else {
      let start = *component
        .iter()
        .find(|&&node| topology.neighbors[node].len() <= 1)
        .ok_or(Error::Invariant(topology.ids[root]))?;
      place(start, Point::new(0., 0.), &mut positions, &mut grid)?;
      pending.push_back(Branch {
        node: start,
        heading: -PI / 6.,
        depth: 0,
        radial: false,
      });
    }
    while let Some(mut branch) = pending.pop_front() {
      for &index in &rings.membership[branch.node] {
        if embedded[index] {
          continue;
        }
        embedded[index] = true;
        let placed = rings.embed(index, &positions, component, branch.heading, ring_work, variant)?;
        crossings.extend(placed.crossings);
        for (node, point, heading) in placed.atoms {
          if positions[node].is_some() {
            if node == branch.node {
              branch.heading = heading;
              branch.radial = true;
            }
            continue;
          }
          place(node, point, &mut positions, &mut grid)?;
          pending.push_front(Branch {
            node,
            heading,
            depth: branch.depth,
            radial: true,
          });
        }
      }
      let children: Vec<_> = topology.neighbors[branch.node]
        .iter()
        .filter(|&&(node, _)| positions[node].is_none())
        .copied()
        .collect();
      if children.is_empty() {
        continue;
      }
      for (node, point, heading) in candidates(topology, &positions, &grid, &branch, &children, rings)? {
        place(node, point, &mut positions, &mut grid)?;
        pending.push_back(Branch {
          node,
          heading,
          depth: branch.depth + 1,
          radial: false,
        });
      }
    }
  }
  relax_terminal_leaves(topology, rings, &mut positions)?;
  let positions = topology
    .ids
    .iter()
    .enumerate()
    .map(|(i, &id)| position(&positions, topology, i).map(|p| (id, p)))
    .collect::<Result<BTreeMap<_, _>, _>>()?;
  let mut result = Layout2d::from_positions(positions)?;
  result.crossings = crossings;
  super::alkenes::enforce(graph, &mut result, options.stereo_budget)?;
  // E/Z repair can reflect a large side onto the rest of a long chain. Repair
  // surviving contacts by rotating rigid acyclic halves around bridge bonds.
  // Placement-time contacts can disappear after whole-branch refinement. Report
  // only the final geometry, not obsolete pairs collected during expansion.
  Ok(result)
}
