//! Independent rule-based placement, not a port of a reference toolkit's runtime.
//!
//! Leaf peeling isolates the one allowed cycle. A spatial grid limits local
//! crowding queries; only a bounded set of candidate rotations is evaluated.

use super::{Layout2d, LayoutDiagnostic, LayoutError as Error, LayoutOptions};
use crate::geometry::{Bounds, Point};
use chitin_chem::{AtomId, BondOrder, GraphError, MolecularGraph};
use std::{
  collections::{BTreeMap, VecDeque},
  f64::consts::{PI, TAU},
};

/// Temporary adjacency view; dense indices never replace domain handles.
struct Topology {
  /// Original graph handles, in stable iteration order.
  ids: Vec<AtomId>,
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

  /// Removes degree-zero/one vertices iteratively; retained vertices form cycle cores.
  ///
  /// # Returns
  /// One flag per dense vertex. This neither selects a cycle basis nor assigns
  /// aromaticity. Component cycle-rank validation happens before embedding.
  fn core(&self) -> Vec<bool> {
    let mut degree: Vec<_> = self.neighbors.iter().map(Vec::len).collect();
    let mut retained = vec![true; self.ids.len()];
    let mut queue: VecDeque<_> = degree
      .iter()
      .enumerate()
      .filter(|&(_, d)| *d < 2)
      .map(|(i, _)| i)
      .collect();
    while let Some(node) = queue.pop_front() {
      if !retained[node] {
        continue;
      }
      retained[node] = false;
      for &(other, _) in &self.neighbors[node] {
        if retained[other] {
          degree[other] -= 1;
          if degree[other] == 1 {
            queue.push_back(other);
          }
        }
      }
    }
    retained
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

/// Inserts one center and retains any crowding that bounded search could not avoid.
///
/// # Parameters
/// - `node`, `point`: previously unplaced vertex and candidate unit-space position.
/// - `topology`, `positions`, `grid`: same dense namespace; occupied grid entries
///   must already have stored positions.
/// - `diagnostics`, `threshold`: append-only close-center reports and distance rule.
///
/// # Returns
/// Success after storing the center and adding its index to the grid. Existing
/// positions are not moved; unresolved close pairs are retained as diagnostics.
///
/// # Errors
/// Rejects unrepresentable grid keys or a violated occupied-position invariant.
fn place(
  node: usize,
  point: Point,
  topology: &Topology,
  positions: &mut [Option<Point>],
  grid: &mut Grid,
  diagnostics: &mut Vec<LayoutDiagnostic>,
  threshold: f64,
) -> Result<(), Error> {
  for other in grid.nearby(point)? {
    if point.distance(position(positions, topology, other)?) < threshold {
      diagnostics.push(LayoutDiagnostic::CrowdedAtoms {
        a: topology.ids[other],
        b: topology.ids[node],
      });
    }
  }
  positions[node] = Some(point);
  grid.0.entry(Grid::key(point)?).or_default().push(node);
  Ok(())
}

/// Orders a unicyclic core by connectivity, not by ID-sorted polygon vertices.
///
/// # Returns
/// Empty for trees; otherwise cycle order starting at the smallest retained ID.
/// The caller must establish component cycle rank <= 1 before invoking this helper.
///
/// # Errors
/// Reports broken core adjacency or a traversal that fails to close in finite steps.
fn cycle(topology: &Topology, component: &[usize], core: &[bool]) -> Result<Vec<usize>, Error> {
  let Some(&start) = component.iter().find(|&&node| core[node]) else {
    return Ok(Vec::new());
  };
  let mut ordered = vec![start];
  let mut previous = start;
  let mut current = topology.neighbors[start]
    .iter()
    .find(|&&(node, _)| core[node])
    .map(|&(node, _)| node)
    .ok_or(Error::Invariant(topology.ids[start]))?;
  while current != start {
    if ordered.len() >= component.len() {
      return Err(Error::Invariant(topology.ids[start]));
    }
    ordered.push(current);
    let next = topology.neighbors[current]
      .iter()
      .find(|&&(node, _)| core[node] && node != previous)
      .map(|&(node, _)| node)
      .ok_or(Error::Invariant(topology.ids[current]))?;
    previous = current;
    current = next;
  }
  Ok(ordered)
}

/// Picks a bounded rotation of a sibling group, penalizing local center overlap.
///
/// Linear degree-two geometry is constrained rather than rotated to avoid crowds.
/// Other candidates preserve unit bond lengths; residual quality is diagnostic.
///
/// # Parameters
/// - `children`: unplaced neighbors, in deterministic adjacency order.
/// - `grid`: already fixed centers in this component only.
///
/// # Returns
/// The lowest-cost candidate among at most six rotations, with ties retaining
/// earlier candidates. This local heuristic does not optimize label envelopes
/// or certify that bond crossings are absent.
///
/// # Errors
/// Propagates invalid occupied positions or spatial-index coordinate failures.
fn candidates(
  topology: &Topology,
  positions: &[Option<Point>],
  grid: &Grid,
  branch: &Branch,
  children: &[(usize, BondOrder)],
) -> Result<Vec<(usize, Point, f64)>, Error> {
  let origin = position(positions, topology, branch.node)?;
  let row = &topology.neighbors[branch.node];
  let linear = row.len() == 2
    && (row.iter().any(|&(_, order)| order == BondOrder::Triple)
      || row.iter().all(|&(_, order)| order == BondOrder::Double));
  let offsets: Vec<_> = (0..children.len())
    .map(|i| {
      if linear {
        0.
      } else if branch.radial {
        (i as f64 - (children.len() - 1) as f64 / 2.) * PI / 3.
      } else if children.len() == 1 {
        if branch.depth.is_multiple_of(2) {
          PI / 3.
        } else {
          -PI / 3.
        }
      } else {
        (i as f64 - (children.len() - 1) as f64 / 2.) * TAU / (children.len() + 1) as f64
      }
    })
    .collect();
  let rotations = [0., PI / 3., -PI / 3., 2. * PI / 3., -2. * PI / 3., PI];
  let mut best = Vec::new();
  let mut best_cost = f64::INFINITY;
  for (attempt, rotation) in rotations.into_iter().take(if linear { 1 } else { 6 }).enumerate() {
    let mut group = Vec::new();
    let mut cost = attempt as f64 * 0.0001;
    for ((node, _), offset) in children.iter().zip(&offsets) {
      let angle = branch.heading + offset + rotation;
      let point = origin.translated(angle.cos(), angle.sin());
      for other in grid.nearby(point)? {
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
  Ok(best)
}

/// Embeds supported components, expands their trees, then packs center bounds.
///
/// # Parameters
/// - `graph`: source graph, never mutated or sanitized during placement.
/// - `options`: already validated by the public layout entry point.
///
/// # Returns
/// A complete coordinate snapshot plus unresolved center-crowding diagnostics.
/// Packed translations preserve every component's internal bond lengths.
///
/// # Errors
/// Returns typed unsupported-topology, resource, or coordinate failures. No input
/// facts are mutated and partially placed components are never returned.
pub(super) fn generate(graph: &MolecularGraph, options: &LayoutOptions) -> Result<Layout2d, Error> {
  let topology = Topology::new(graph)?;
  let core = topology.core();
  let mut positions = vec![None; topology.ids.len()];
  let mut diagnostics = Vec::new();
  let mut cursor = 0.;
  for component in &topology.components {
    let root = *component.first().ok_or(Error::CoordinateRange)?;
    let edges = component
      .iter()
      .map(|&node| topology.neighbors[node].len())
      .sum::<usize>()
      / 2;
    let rank = edges + 1 - component.len();
    if rank > 1 {
      return Err(Error::RingSystem {
        atom: topology.ids[root],
        cycles: rank,
      });
    }
    let ring = cycle(&topology, component, &core)?;
    let mut grid = Grid::default();
    let mut pending = VecDeque::new();
    if ring.is_empty() {
      let start = *component
        .iter()
        .find(|&&node| topology.neighbors[node].len() <= 1)
        .ok_or(Error::Invariant(topology.ids[root]))?;
      place(
        start,
        Point::new(0., 0.),
        &topology,
        &mut positions,
        &mut grid,
        &mut diagnostics,
        options.minimum_spacing,
      )?;
      pending.push_back(Branch {
        node: start,
        heading: -PI / 6.,
        depth: 0,
        radial: false,
      });
    } else {
      let radius = 1. / (2. * (PI / ring.len() as f64).sin());
      for (i, &node) in ring.iter().enumerate() {
        let angle = TAU * i as f64 / ring.len() as f64;
        place(
          node,
          Point::new(radius * angle.cos(), radius * angle.sin()),
          &topology,
          &mut positions,
          &mut grid,
          &mut diagnostics,
          options.minimum_spacing,
        )?;
        pending.push_back(Branch {
          node,
          heading: angle,
          depth: 0,
          radial: true,
        });
      }
    }
    while let Some(branch) = pending.pop_front() {
      let children: Vec<_> = topology.neighbors[branch.node]
        .iter()
        .filter(|&&(node, _)| positions[node].is_none())
        .copied()
        .collect();
      if children.is_empty() {
        continue;
      }
      for (node, point, heading) in candidates(&topology, &positions, &grid, &branch, &children)? {
        place(
          node,
          point,
          &topology,
          &mut positions,
          &mut grid,
          &mut diagnostics,
          options.minimum_spacing,
        )?;
        pending.push_back(Branch {
          node,
          heading,
          depth: branch.depth + 1,
          radial: false,
        });
      }
    }
    let points = component
      .iter()
      .map(|&node| position(&positions, &topology, node))
      .collect::<Result<Vec<_>, _>>()?;
    let bounds = Bounds::from_points(points)?.ok_or(Error::Invariant(topology.ids[root]))?;
    let dy = -(bounds.min().y + bounds.height() / 2.);
    for &node in component {
      positions[node] = Some(position(&positions, &topology, node)?.translated(cursor - bounds.min().x, dy));
    }
    cursor += bounds.width() + options.component_gap;
  }
  let positions = topology
    .ids
    .iter()
    .enumerate()
    .map(|(i, &id)| position(&positions, &topology, i).map(|p| (id, p)))
    .collect::<Result<BTreeMap<_, _>, _>>()?;
  let mut result = Layout2d::from_positions(positions)?;
  result.diagnostics = diagnostics;
  Ok(result)
}
