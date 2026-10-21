//! Independent skeleton geometry with scale-relative tolerances and exact domains.

use crate::quality::{self, Metrics, Observation, State};
use anyhow::{Result, ensure};
use chitin_chem::{BondOrder, DoubleBondRelation, Element, MolecularGraph, perception::analyze_rings};
use chitin_chemdraw::{geometry::Point, layout::Layout2d};
use std::collections::{BTreeMap, BTreeSet};

/// Evaluates geometry without modifying it or consulting the optimizer objective.
///
/// # Parameters
/// - `graph`, `coordinates`: one immutable source graph and its final layout.
///
/// # Returns
/// Versioned metrics with explicit sample domains. A missing length scale makes
/// distance-based measurements inapplicable, not zero. Perspective geometry has
/// no planar length/angle penalties; crossings still require explicit annotation.
///
/// # Errors
/// Rejects missing/foreign coordinates and nonfinite geometry or derived distances.
pub fn measure(graph: &MolecularGraph, coordinates: &Layout2d) -> Result<Metrics> {
  let points = coordinates.positions();
  ensure!(points.len() == graph.atom_count(), "coordinate coverage mismatch");
  for (id, _) in graph.atoms() {
    let point = points.get(&id).ok_or_else(|| anyhow::anyhow!("missing coordinate"))?;
    ensure!(point.x.is_finite() && point.y.is_finite(), "nonfinite coordinate");
  }
  let atoms: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
  let edges: Vec<_> = graph.bonds().map(|(id, bond)| (id, bond.atoms())).collect();
  let mut lengths: Vec<_> = edges.iter().map(|(_, [a, b])| points[a].distance(points[b])).collect();
  ensure!(lengths.iter().all(|value| value.is_finite()), "nonfinite bond distance");
  lengths.sort_by(f64::total_cmp);
  let positive: Vec<_> = lengths.iter().copied().filter(|length| *length > 0.).collect();
  let scale = median(&positive);
  let perspective = !coordinates.drawing_depths().is_empty() || !coordinates.bond_crossings().is_empty();
  let mut result = Metrics {
    version: quality::VERSION.into(),
    profile: if perspective { "perspective" } else { "planar" }.into(),
    features: quality::characterize(graph)?,
    measurements: BTreeMap::new(),
    readability_clear: None,
    ring_geometry: None,
  };
  let connected: BTreeSet<_> = edges.iter().map(|(_, [a, b])| ((*a).min(*b), (*a).max(*b))).collect();
  let mut atom_pairs = (0, 0);
  let mut atom_edges = (0, 0);
  for (i, a) in atoms.iter().enumerate() {
    for b in &atoms[i + 1..] {
      if connected.contains(&((*a).min(*b), (*a).max(*b))) {
        continue;
      }
      atom_pairs.1 += 1;
      atom_pairs.0 += usize::from(scale.is_some_and(|s| points[a].distance(points[b]) / s < quality::ATOM_DISTANCE));
    }
    for (_, [b, c]) in &edges {
      if a == b || a == c {
        continue;
      }
      atom_edges.1 += 1;
      atom_edges.0 += usize::from(
        scale.is_some_and(|s| point_segment(points[a], points[b], points[c]) / s < quality::ATOM_EDGE_DISTANCE),
      );
    }
  }
  for (name, (bad, count)) in [
    ("nonbonded_atom_proximity", atom_pairs),
    ("atom_edge_proximity", atom_edges),
  ] {
    result.measurements.insert(
      name.into(),
      if scale.is_some() {
        Observation::rate(bad, count)
      } else {
        Observation::unavailable(State::NotApplicable, count, "no positive skeleton bond length")
      },
    );
  }
  let intentional: BTreeSet<_> = coordinates
    .bond_crossings()
    .iter()
    .map(|crossing| (crossing.front.min(crossing.back), crossing.front.max(crossing.back)))
    .collect();
  let mut crossings = (0, 0);
  for (i, (id, [a, b])) in edges.iter().enumerate() {
    for (other, [c, d]) in &edges[i + 1..] {
      if [a, b].iter().any(|atom| [c, d].contains(atom))
        || intentional.contains(&((*id).min(*other), (*id).max(*other)))
      {
        continue;
      }
      crossings.1 += 1;
      crossings.0 += usize::from(segments_contact(
        points[a],
        points[b],
        points[c],
        points[d],
        scale.unwrap_or(1.) * 1e-9,
      ));
    }
  }
  result.measurements.insert(
    "unannotated_edge_contact".into(),
    Observation::rate(crossings.0, crossings.1),
  );
  result.measurements.insert(
    "zero_length_bonds".into(),
    Observation::rate(lengths.iter().filter(|length| **length == 0.).count(), lengths.len()),
  );
  let normalized_errors: Vec<_> = scale
    .map(|s| lengths.iter().map(|length| length / s - 1.).collect())
    .unwrap_or_default();
  result.measurements.insert(
    "relative_bond_length_rmse".into(),
    if perspective {
      Observation::unavailable(State::NotApplicable, 0, "perspective permits unequal projected lengths")
    } else {
      Observation::rmse(&normalized_errors, quality::LENGTH_DEVIATION)
    },
  );
  add_angles(graph, coordinates, perspective, &mut result)?;
  let mut stereo_bad = 0;
  for (_, stereo) in graph.double_bond_stereos() {
    let [a, b] = stereo.ends();
    let [ra, rb] = stereo.references();
    let axis = Point::new(points[&b].x - points[&a].x, points[&b].y - points[&a].y);
    let side = |origin: Point, reference: Point| {
      (axis.x * (reference.y - origin.y) - axis.y * (reference.x - origin.x))
        / (points[&a].distance(points[&b]) * origin.distance(reference))
    };
    let left = side(points[&a], points[&ra]);
    let right = side(points[&b], points[&rb]);
    let relation = if left.is_finite() && right.is_finite() && left.abs() > 1e-8 && right.abs() > 1e-8 {
      Some(if left.signum() == right.signum() {
        DoubleBondRelation::SameSide
      } else {
        DoubleBondRelation::OppositeSides
      })
    } else {
      None
    };
    stereo_bad += usize::from(relation != Some(stereo.relation()));
  }
  result.measurements.insert(
    "double_bond_relation".into(),
    Observation::rate(stereo_bad, graph.double_bond_stereos().len()),
  );
  result.measurements.insert(
    "tetrahedral_depiction_parity".into(),
    Observation::unavailable(
      if graph.tetrahedral_centers().len() == 0 {
        State::NotApplicable
      } else {
        State::NotImplemented
      },
      graph.tetrahedral_centers().len(),
      "independent wedge/hash decoding not implemented",
    ),
  );
  result.measurements.insert(
    "chemical_label_semantics".into(),
    Observation::unavailable(
      State::NotImplemented,
      graph.atom_count(),
      "element/isotope/charge/H text semantics are not independently decoded",
    ),
  );
  result.measurements.insert(
    "component_radius_gyration".into(),
    component_extent(graph, points, scale)?,
  );
  crate::ring_geometry::measure(graph, points, scale, perspective, &mut result)?;
  ensure!(
    result
      .measurements
      .values()
      .all(|metric| metric.value.is_none_or(f64::is_finite)),
    "nonfinite quality measurement"
  );
  Ok(result)
}

/// Median of sorted positive lengths, averaging the middle pair for even sizes.
fn median(values: &[f64]) -> Option<f64> {
  let middle = values.len() / 2;
  values.get(middle).map(|right| {
    if values.len().is_multiple_of(2) {
      values[middle - 1] / 2. + right / 2.
    } else {
      *right
    }
  })
}

/// Eligible acyclic carbon centers and simple small cycle blocks only.
fn add_angles(graph: &MolecularGraph, layout: &Layout2d, perspective: bool, output: &mut Metrics) -> Result<()> {
  let topology = analyze_rings(graph)?;
  let points = layout.positions();
  let mut chain_errors = Vec::new();
  let mut ring_errors = Vec::new();
  if !perspective {
    for (center, atom) in graph.atoms() {
      if atom.element() != Element::Carbon {
        continue;
      }
      let neighbors: Vec<_> = graph.neighbors(center)?.collect();
      if neighbors.len() != 2 || neighbors.iter().any(|(_, edge)| !topology.bridges().contains(edge)) {
        continue;
      }
      let orders: Vec<_> = neighbors
        .iter()
        .filter_map(|(_, edge)| graph.bond(*edge).map(|bond| bond.order()))
        .collect();
      let target = if orders.contains(&BondOrder::Triple) || orders == [BondOrder::Double, BondOrder::Double] {
        180.
      } else if orders == [BondOrder::Single, BondOrder::Single] {
        120.
      } else {
        continue;
      };
      chain_errors.push(angle(points[&center], points[&neighbors[0].0], points[&neighbors[1].0]) - target);
    }
    for block in topology
      .blocks()
      .iter()
      .filter(|block| block.cycle_rank() == 1 && (3..=8).contains(&block.atoms().len()))
    {
      let edges: BTreeSet<_> = block.bonds().iter().copied().collect();
      let target = (block.atoms().len() - 2) as f64 * 180. / block.atoms().len() as f64;
      for center in block.atoms() {
        let neighbors: Vec<_> = graph
          .neighbors(*center)?
          .filter(|(_, edge)| edges.contains(edge))
          .collect();
        if let [(a, _), (b, _)] = neighbors.as_slice() {
          ring_errors.push(angle(points[center], points[a], points[b]) - target);
        }
      }
    }
  }
  for (key, errors) in [
    ("acyclic_carbon_angle_rmse_deg", chain_errors),
    ("simple_ring_angle_rmse_deg", ring_errors),
  ] {
    output.measurements.insert(
      key.into(),
      if perspective {
        Observation::unavailable(State::NotApplicable, 0, "planar angle policy excludes perspective")
      } else {
        Observation::rmse(&errors, quality::ANGLE_DEVIATION_DEG)
      },
    );
  }
  Ok(())
}

/// Returns a 0..180 degree angle; coincident directions are maximally distorted.
fn angle(center: Point, a: Point, b: Point) -> f64 {
  let divisor = center.distance(a) * center.distance(b);
  if divisor == 0. {
    return 0.;
  }
  (((a.x - center.x) * (b.x - center.x) + (a.y - center.y) * (b.y - center.y)) / divisor)
    .clamp(-1., 1.)
    .acos()
    .to_degrees()
}

/// Per-component centroid spread, weighted by atom count and normalized by L².
fn component_extent(
  graph: &MolecularGraph,
  points: &BTreeMap<chitin_chem::AtomId, Point>,
  scale: Option<f64>,
) -> Result<Observation> {
  let Some(scale) = scale else {
    return Ok(Observation::unavailable(State::NotApplicable, 0, "no bond scale"));
  };
  let mut remaining: BTreeSet<_> = graph.atoms().map(|(id, _)| id).collect();
  let mut total = 0.;
  while let Some(first) = remaining.pop_first() {
    let mut stack = vec![first];
    let mut component = Vec::new();
    while let Some(id) = stack.pop() {
      component.push(points[&id]);
      for (neighbor, _) in graph.neighbors(id)? {
        if remaining.remove(&neighbor) {
          stack.push(neighbor);
        }
      }
    }
    let count = component.len() as f64;
    let centroid = Point::new(
      component.iter().map(|p| p.x / count).sum(),
      component.iter().map(|p| p.y / count).sum(),
    );
    total += component
      .iter()
      .map(|p| (p.distance(centroid) / scale).powi(2))
      .sum::<f64>();
  }
  let value = total / graph.atom_count() as f64;
  ensure!(value.is_finite(), "nonfinite normalized extent");
  Ok(Observation::measured(value, graph.atom_count(), None))
}

/// Shortest distance to a closed segment, including coincident endpoints.
pub(crate) fn point_segment(p: Point, a: Point, b: Point) -> f64 {
  let dx = b.x - a.x;
  let dy = b.y - a.y;
  let denominator = dx * dx + dy * dy;
  let t = if denominator > 0. {
    ((p.x - a.x) * dx + (p.y - a.y) * dy) / denominator
  } else {
    0.
  };
  p.distance(Point::new(a.x + t.clamp(0., 1.) * dx, a.y + t.clamp(0., 1.) * dy))
}

/// Proper crossing or a scale-relative endpoint/collinear contact.
pub(crate) fn segments_contact(a: Point, b: Point, c: Point, d: Point, epsilon: f64) -> bool {
  let side = |a: Point, b: Point, p: Point| (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
  side(a, b, c) * side(a, b, d) < 0. && side(c, d, a) * side(c, d, b) < 0.
    || [(a, c, d), (b, c, d), (c, a, b), (d, a, b)]
      .iter()
      .any(|&(p, a, b)| point_segment(p, a, b) <= epsilon)
}

#[cfg(test)]
mod tests;
