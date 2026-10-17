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

/// Chooses between angle-preserving mirrored assignments of a sibling group.
///
/// Linear degree-two geometry is constrained rather than rotated to avoid crowds.
/// Mirroring preserves the incoming/outgoing angle sectors, unlike rotating the
/// group freely around its parent. Residual crowding remains diagnostic.
///
/// # Parameters
/// - `children`: unplaced neighbors, in deterministic adjacency order.
/// - `grid`: already fixed centers in this component only.
///
/// # Returns
/// The lowest-cost candidate among at most two mirror assignments, with ties retaining
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
  let offsets: Vec<_> = if branch.radial {
    (0..children.len())
      .map(|i| (i as f64 - (children.len() - 1) as f64 / 2.) * PI / 3.)
      .collect()
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
  for (attempt, sign) in [1., -1.].into_iter().take(if linear { 1 } else { 2 }).enumerate() {
    let mut group = Vec::new();
    let mut cost = attempt as f64 * 0.0001;
    for ((node, _), offset) in children.iter().zip(&offsets) {
      let angle = branch.heading + sign * offset;
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

/// Embeds supported components, expands their trees, then refines and packs them.
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
  let rings = Rings::new(graph, &topology)?;
  let mut embedded = vec![false; rings.len()];
  let mut positions = vec![None; topology.ids.len()];
  for component in &topology.components {
    let root = *component.first().ok_or(Error::CoordinateRange)?;
    let mut grid = Grid::default();
    let mut pending = VecDeque::new();
    if let Some(seed) = rings.seed(component) {
      embedded[seed] = true;
      for (node, point, heading) in rings.embed(seed, &positions, component, 0.)? {
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
    while let Some(branch) = pending.pop_front() {
      for &index in &rings.membership[branch.node] {
        if embedded[index] {
          continue;
        }
        embedded[index] = true;
        for (node, point, heading) in rings.embed(index, &positions, component, branch.heading)? {
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
      for (node, point, heading) in candidates(&topology, &positions, &grid, &branch, &children)? {
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
  let positions = topology
    .ids
    .iter()
    .enumerate()
    .map(|(i, &id)| position(&positions, &topology, i).map(|p| (id, p)))
    .collect::<Result<BTreeMap<_, _>, _>>()?;
  let mut result = Layout2d::from_positions(positions)?;
  // Placement-time contacts can disappear after whole-branch refinement. Report
  // only the final geometry, not obsolete pairs collected during expansion.
  super::refinement::run(graph, &mut result, options)?;
  Ok(result)
}
