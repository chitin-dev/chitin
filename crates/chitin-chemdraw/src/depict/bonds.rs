//! Contextual bond geometry with shared label clearance and neighbor-ray joins.
//!
//! Ring doubles use the axis plus a short inward stroke; acyclic doubles choose
//! centered/offset strokes from adjacency. Triple side strokes use spacing/1.5.
//! Geometry and group label clearance are resolved before generic scene export.

use super::{DepictError as Error, DepictOptions, LabelBounds, clip, color::BondPaint};
use crate::{
  geometry::Point,
  scene::{Line, LineCap, Primitive, Source, positive},
};
use chitin_chem::{AtomId, BondId, BondOrder, GraphError, MolecularGraph};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Dense adjacency used to identify bridges once instead of searching every tree bond.
struct Context {
  /// Stable handles retained for coordinate lookup, never replaced in the graph.
  ids: Vec<AtomId>,
  /// Temporary handle-to-row lookup, scoped to this depiction pass.
  indices: BTreeMap<AtomId, usize>,
  /// Neighbor rows with edge identities, in stable neighbor-handle order.
  rows: Vec<Vec<(usize, BondId)>>,
  /// Edges outside all cycles; their doubles need no per-edge ring search.
  bridges: BTreeSet<BondId>,
}

/// A selected cycle's interior reference and endpoint-adjacent vertices.
struct Ring {
  /// Mean vertex position, exact for the regular single-cycle layout profile.
  center: Point,
  /// Other incident cycle vertex at the first edge endpoint.
  start_neighbor: Point,
  /// Other incident cycle vertex at the second endpoint.
  end_neighbor: Point,
}

impl Context {
  /// Builds adjacency and an iterative low-link bridge classification.
  ///
  /// # Parameters
  /// - `graph`: one immutable graph namespace, including disconnected fragments.
  ///
  /// # Returns
  /// O(V + E) temporary topology with O(V + E) DFS work after ordered lookups.
  /// No recursive traversal or chemistry/ring-cache mutation is performed.
  ///
  /// # Errors
  /// Propagates inconsistent endpoints rather than inventing connectivity.
  fn new(graph: &MolecularGraph) -> Result<Self, Error> {
    let ids: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
    let indices: BTreeMap<_, _> = ids.iter().enumerate().map(|(i, &id)| (id, i)).collect();
    let mut rows = vec![Vec::new(); ids.len()];
    for (edge, bond) in graph.bonds() {
      let [a, b] = bond.atoms();
      let a = *indices.get(&a).ok_or(GraphError::MissingAtom(a))?;
      let b = *indices.get(&b).ok_or(GraphError::MissingAtom(b))?;
      rows[a].push((b, edge));
      rows[b].push((a, edge));
    }
    for row in &mut rows {
      row.sort_unstable_by_key(|&(node, _)| node);
    }
    let bridges = chitin_chem::perception::analyze_rings(graph)?.bridges().clone();
    Ok(Self {
      ids,
      indices,
      rows,
      bridges,
    })
  }

  /// Finds the shortest alternate path only for an already classified cyclic edge.
  ///
  /// # Parameters
  /// - `edge`, `a`, `b`: one edge and its normalized endpoints in this context.
  /// - `points`: transformed positions for every live input atom.
  ///
  /// # Returns
  /// A deterministic local cycle reference, not an SSSR/aromaticity result.
  /// Single-cycle components have an unambiguous interior side. Multi-cycle
  /// manual coordinates use a shortest cycle, not a largest/aromatic-common-ring
  /// catalogue; that broader ring-choice model is not implemented here yet.
  ///
  /// # Errors
  /// Reports a violated bridge/path invariant as an unsupported bond geometry.
  fn ring(&self, edge: BondId, a: AtomId, b: AtomId, points: &BTreeMap<AtomId, Point>) -> Result<Option<Ring>, Error> {
    if self.bridges.contains(&edge) {
      return Ok(None);
    }
    let start = self.indices[&a];
    let end = self.indices[&b];
    let mut previous = vec![None; self.ids.len()];
    previous[start] = Some(start);
    let mut queue = VecDeque::from([start]);
    while let Some(node) = queue.pop_front() {
      if node == end {
        break;
      }
      for &(other, candidate) in &self.rows[node] {
        if candidate != edge && previous[other].is_none() {
          previous[other] = Some(node);
          queue.push_back(other);
        }
      }
    }
    let mut path = vec![end];
    let mut node = end;
    while node != start {
      node = previous[node].ok_or(Error::Bond(edge))?;
      path.push(node);
    }
    if path.len() < 3 {
      return Err(Error::Bond(edge));
    }
    let center = Point::new(
      path.iter().map(|&i| points[&self.ids[i]].x / path.len() as f64).sum(),
      path.iter().map(|&i| points[&self.ids[i]].y / path.len() as f64).sum(),
    );
    Ok(Some(Ring {
      center,
      start_neighbor: points[&self.ids[path[path.len() - 2]]],
      end_neighbor: points[&self.ids[path[1]]],
    }))
  }
}

/// One parallel stroke before endpoint-label clipping, with axial shortening.
struct Stroke {
  /// Signed distance along the bond's unit normal.
  offset: f64,
  /// Signed axial insets; a negative value is a bounded centered-double miter.
  trim: [f64; 2],
  /// Centered doubles may extend a bounded amount to meet an actual neighbor ray.
  miter: bool,
}

/// Returns a two-dimensional determinant for orientation/intersection tests.
fn cross(a: Point, b: Point) -> f64 {
  a.x * b.y - a.y * b.x
}
/// Returns a vector difference without changing either point.
fn difference(a: Point, b: Point) -> Point {
  Point::new(a.x - b.x, a.y - b.y)
}
/// Returns the projection onto a unit direction.
fn dot(a: Point, b: Point) -> f64 {
  a.x * b.x + a.y * b.y
}

/// Intersects an offset stroke with an actual adjacent edge ray, not its extension.
///
/// # Returns
/// A signed axial meeting distance, or no intersection for collinear rays or
/// a meeting point behind the neighbor ray's origin. This ray test prevents
/// detached strokes caused by intersecting the wrong extension of a neighbor.
fn intersection(direction: Point, normal: Point, offset: f64, neighbor: Point) -> Option<f64> {
  let denominator = cross(direction, neighbor);
  // At the second endpoint the forward axis is reversed, but the stroke's
  // normal is not. Keep that orientation factor in the neighbor-ray parameter.
  if denominator.abs() < 1e-10 || offset * cross(direction, normal) / denominator < 0. {
    None
  } else {
    Some(-offset * cross(normal, neighbor) / denominator)
  }
}

/// Returns only inward shortening; ring/auxiliary strokes never extend past vertices.
fn corner(direction: Point, normal: Point, offset: f64, neighbor: Point) -> f64 {
  intersection(direction, normal, offset, neighbor).unwrap_or(0.).max(0.)
}

/// Chooses the populated half-plane for a non-ring auxiliary line.
///
/// Neighbors decide first; other graph points break ties. Axis orientation is
/// normalized spatially by the caller, so reversing edge endpoints is harmless.
fn side(
  graph: &MolecularGraph,
  points: &BTreeMap<AtomId, Point>,
  a: AtomId,
  b: AtomId,
  direction: Point,
) -> Result<f64, Error> {
  let mut count = [0_usize; 2];
  for (id, opposite) in [(a, b), (b, a)] {
    for (neighbor, _) in graph.neighbors(id)? {
      if neighbor == opposite {
        continue;
      }
      let signed = cross(direction, difference(points[&neighbor], points[&id]));
      if signed.abs() > 1e-10 {
        count[usize::from(signed < 0.)] += 1;
      }
    }
  }
  if count[0] == count[1] {
    for (&id, &point) in points {
      if id == a || id == b {
        continue;
      }
      let signed = cross(direction, difference(point, points[&a]));
      if signed.abs() > 1e-10 {
        count[usize::from(signed < 0.)] += 1;
      }
    }
  }
  Ok(if count[0] > count[1] { 1. } else { -1. })
}

/// Resolves joins on non-axis strokes using real neighbor rays.
///
/// # Parameters
/// - `graph`, `points`: immutable adjacency and matching drawing-space positions.
/// - `a`, `b`: spatially normalized endpoints, independent of stored edge order.
/// - `direction`: the unit axis, from which the fixed left normal is derived.
/// - `stroke`: signed offset and geometric permission for a bounded miter.
/// - `clearance`: perpendicular centerline separation after a neighbor-ray meeting.
///
/// # Returns
/// Offset auxiliary strokes only shorten. Centered double strokes can extend
/// by at most a quarter bond length to meet the neighboring skeleton instead of
/// leaving a detached pair. Their label clearance is still enforced afterwards.
/// Detached double auxiliaries retreat by the clearance divided by the meeting
/// angle's sine, keeping round caps visibly separate even at acute corners.
///
/// # Errors
/// Propagates invalid graph-local neighbor references.
fn join(
  graph: &MolecularGraph,
  points: &BTreeMap<AtomId, Point>,
  a: AtomId,
  b: AtomId,
  direction: Point,
  stroke: &mut Stroke,
  clearance: f64,
) -> Result<(), Error> {
  let normal = Point::new(-direction.y, direction.x);
  for (end, (id, opposite)) in [(a, b), (b, a)].into_iter().enumerate() {
    let forward = if end == 0 {
      direction
    } else {
      Point::new(-direction.x, -direction.y)
    };
    let mut meeting: Option<f64> = None;
    for (neighbor, _) in graph.neighbors(id)? {
      let ray = difference(points[&neighbor], points[&id]);
      if neighbor != opposite
        && let Some(value) = intersection(forward, normal, stroke.offset, ray)
      {
        // Both strokes have round caps. A centerline meeting alone would make
        // the auxiliary look like an extra skeleton edge rather than a double.
        let sine = cross(forward, ray).abs() / ray.x.hypot(ray.y);
        let value = value + clearance / sine;
        meeting = Some(meeting.map_or(value, |old| old.max(value)));
      }
    }
    if let Some(value) = meeting {
      stroke.trim[end] = if stroke.miter {
        value.max(-points[&a].distance(points[&b]) * 0.25)
      } else {
        stroke.trim[end].max(value)
      };
    }
  }
  Ok(())
}

/// Generates contextual bond strokes, then applies one shared label-clearance interval.
///
/// # Parameters
/// - `graph`: source topology; temporary bridge/cycle information is local only.
/// - `points`, `labels`: validated-stage data in one y-down drawing namespace.
/// - `circle_edges`: fully accepted aromatic-circle edges drawn as single axes;
///   integer graph orders remain available for valence and all other bonds.
/// - `options`: validated proportions and colors, shared by every bond type.
/// - `output`: append-only generic scene commands; no SVG-specific geometry here.
///
/// # Errors
/// Returns graph-reference, unsupported-representation, or fully occluded stroke
/// failures. A caller-owned drawing is created only after all bonds succeed.
pub(super) fn draw(
  graph: &MolecularGraph,
  points: &BTreeMap<AtomId, Point>,
  labels: &BTreeMap<AtomId, LabelBounds>,
  expanded_hydrogens: &BTreeSet<AtomId>,
  circle_edges: &BTreeSet<BondId>,
  options: &DepictOptions,
  output: &mut Vec<Primitive>,
) -> Result<(), Error> {
  let context = Context::new(graph)?;
  let marks = super::stereo::plan(graph, points, &context.bridges, expanded_hydrogens)?;
  for (id, bond) in graph.bonds() {
    let atoms = bond.atoms();
    let paint = BondPaint::new(
      atoms.map(|atom| points[&atom]),
      [
        graph.atom(atoms[0]).ok_or(GraphError::MissingAtom(atoms[0]))?.element(),
        graph.atom(atoms[1]).ok_or(GraphError::MissingAtom(atoms[1]))?.element(),
      ],
      options,
    );
    if let Some(mark) = marks.get(&id) {
      super::stereo::draw(id, mark, points, labels, options, &paint, output)?;
      continue;
    }
    let [mut a, mut b] = bond.atoms();
    // Stable spatial orientation makes auxiliary-side tie breaks independent of
    // source edge orientation and handle allocation order.
    if (points[&a]
      .x
      .total_cmp(&points[&b].x)
      .then(points[&a].y.total_cmp(&points[&b].y)))
    .is_gt()
    {
      std::mem::swap(&mut a, &mut b);
    }
    let start = points[&a];
    let end = points[&b];
    let length = start.distance(end);
    if !positive(length) {
      return Err(Error::OccludedBond(id));
    }
    let direction = Point::new((end.x - start.x) / length, (end.y - start.y) / length);
    let normal = Point::new(-direction.y, direction.x);
    let short = length * (1. - options.short_bond_ratio) / 2.;
    let order = if circle_edges.contains(&id) {
      BondOrder::Single
    } else {
      bond.order()
    };
    let mut strokes = match order {
      BondOrder::Single => vec![Stroke {
        offset: 0.,
        trim: [0.; 2],
        miter: false,
      }],
      BondOrder::Triple => [-1., 0., 1.]
        .into_iter()
        .map(|side| Stroke {
          offset: side * options.bond_spacing / 1.5,
          trim: [0.; 2],
          miter: false,
        })
        .collect(),
      BondOrder::Double => {
        if let Some(ring) = context.ring(id, a, b, points)? {
          let sign = if dot(difference(ring.center, start), normal) >= 0. {
            1.
          } else {
            -1.
          };
          let offset = sign * options.bond_spacing;
          let trim_a = short.max(corner(
            direction,
            normal,
            offset,
            difference(ring.start_neighbor, start),
          ));
          let trim_b = short.max(corner(
            Point::new(-direction.x, -direction.y),
            normal,
            offset,
            difference(ring.end_neighbor, end),
          ));
          vec![
            Stroke {
              offset: 0.,
              trim: [0.; 2],
              miter: false,
            },
            Stroke {
              offset,
              trim: [trim_a, trim_b],
              miter: false,
            },
          ]
        } else {
          let extra_a = graph.neighbors(a)?.len() - 1;
          let extra_b = graph.neighbors(b)?.len() - 1;
          let cumulated = [a, b].into_iter().any(|atom| {
            graph.neighbors(atom).is_ok_and(|mut neighbors| {
              neighbors.any(|(_, edge)| edge != id && graph.bond(edge).is_some_and(|b| b.order() == BondOrder::Double))
            })
          });
          if cumulated
            || (extra_a == 0 && extra_b == 0)
            || (extra_a == 0 && extra_b > 1)
            || (extra_b == 0 && extra_a > 1)
          {
            [-0.5, 0.5]
              .into_iter()
              .map(|side| Stroke {
                offset: side * options.bond_spacing,
                trim: [0.; 2],
                miter: true,
              })
              .collect()
          } else {
            vec![
              Stroke {
                offset: 0.,
                trim: [0.; 2],
                miter: false,
              },
              Stroke {
                offset: side(graph, points, a, b, direction)? * options.bond_spacing,
                trim: [short; 2],
                miter: false,
              },
            ]
          }
        }
      }
      _ => return Err(Error::Bond(id)),
    };
    for stroke in &mut strokes {
      if stroke.offset != 0. {
        let clearance = if order == BondOrder::Double && !stroke.miter {
          // Ink wider than the parallel spacing already merges the two double
          // strokes. Bound that unsupported visual clearance rather than
          // erasing the auxiliary in otherwise exportable thick-ink styles.
          2. * options.stroke_width.min(options.bond_spacing / 2.)
        } else {
          0.
        };
        join(graph, points, a, b, direction, stroke, clearance)?;
      }
    }
    // Acute projected ring corners can consume the entire inward auxiliary.
    // Keep both double-bond strokes by using the centered representation;
    // never erase the second stroke or reverse its endpoints to force a join.
    if order == BondOrder::Double
      && strokes
        .iter()
        .any(|stroke| stroke.trim.iter().sum::<f64>() >= length * 0.8)
    {
      strokes = [-0.5, 0.5]
        .into_iter()
        .map(|side| Stroke {
          offset: side * options.bond_spacing,
          trim: [0.; 2],
          miter: false,
        })
        .collect();
    }
    let mut interval = [f64::NEG_INFINITY, f64::INFINITY];
    for stroke in &strokes {
      let p = start.translated(normal.x * stroke.offset, normal.y * stroke.offset);
      let q = end.translated(normal.x * stroke.offset, normal.y * stroke.offset);
      if let Some(bounds) = labels.get(&a) {
        let clipped = clip(p, q, Some(*bounds)).ok_or(Error::OccludedBond(id))?;
        interval[0] = interval[0].max(dot(difference(clipped, p), direction));
      }
      if let Some(bounds) = labels.get(&b) {
        let clipped = clip(q, p, Some(*bounds)).ok_or(Error::OccludedBond(id))?;
        interval[1] = interval[1].min(dot(difference(clipped, p), direction));
      }
    }
    for stroke in strokes {
      let from = interval[0].max(stroke.trim[0]);
      let to = interval[1].min(length - stroke.trim[1]);
      if to <= from {
        return Err(Error::OccludedBond(id));
      }
      paint.line(
        Line {
          start: start.translated(
            direction.x * from + normal.x * stroke.offset,
            direction.y * from + normal.y * stroke.offset,
          ),
          end: start.translated(
            direction.x * to + normal.x * stroke.offset,
            direction.y * to + normal.y * stroke.offset,
          ),
          width: options.stroke_width,
          color: options.foreground,
          caps: [LineCap::Round; 2],
          source: Some(Source::Bond(id)),
        },
        output,
      );
    }
  }
  Ok(())
}
