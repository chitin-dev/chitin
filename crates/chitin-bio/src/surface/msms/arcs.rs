//! Analytical accessible arcs on atom-pair probe-center circles.

use std::f64::consts::TAU;

use glam::DVec3;
use thiserror::Error;

use super::{ReducedSurfaceEdge, geometry::MsmsAtom};

/// Relative tolerance used for circle and angular interval predicates.
const RELATIVE_ARC_TOLERANCE: f64 = 256.0 * f64::EPSILON;

/// Failure encountered while constructing one atom-pair probe circle.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum ProbeArcError {
  /// The pair must contain two distinct local atom indices.
  #[error("probe-circle support contains a duplicate atom index")]
  DuplicateAtomIndex,
  /// Atom indices, coordinates, radii, or probe radius are invalid.
  #[error("probe-circle geometry contains an invalid index, coordinate, or radius")]
  InvalidGeometry,
}

/// Returns every accessible arc on one atom-pair probe-center circle.
///
/// The two support atoms are expanded by the probe radius. Their sphere
/// intersection is parameterized as a circle, and every other expanded atom
/// contributes a closed occluded angular interval. The complement of the
/// merged interval union is returned as reduced-surface edges. An unobstructed
/// circle produces one edge with a $2\pi$ sweep; disjoint, contained, tangent,
/// or fully occluded pairs produce no edge.
///
/// # Parameters
///
/// * `atoms` contains every atom sphere in the calculation domain.
/// * `support` contains two distinct local indices into `atoms`.
/// * `probe_radius` is the rolling solvent-probe radius in ångströms.
///
/// # Returns
///
/// Accessible arcs in increasing angular order, or [`ProbeArcError`] for
/// invalid input geometry.
///
/// # Examples
///
/// ```
/// use chitin_bio::surface::msms::{arcs::accessible_probe_arcs, geometry::MsmsAtom};
/// use glam::DVec3;
///
/// let atoms = vec![
///   MsmsAtom { atom_index: 0, center: DVec3::ZERO, radius: 1.0 },
///   MsmsAtom { atom_index: 1, center: DVec3::new(2.0, 0.0, 0.0), radius: 1.0 },
/// ];
/// let arcs = accessible_probe_arcs(&atoms, [0, 1], 1.0)?;
/// assert_eq!(arcs.len(), 1);
/// assert!((arcs[0].sweep_angle - std::f64::consts::TAU).abs() < 1.0e-12);
/// # Ok::<(), chitin_bio::surface::msms::arcs::ProbeArcError>(())
/// ```
pub fn accessible_probe_arcs(
  atoms: &[MsmsAtom],
  support: [usize; 2],
  probe_radius: f64,
) -> Result<Vec<ReducedSurfaceEdge>, ProbeArcError> {
  validate_geometry(atoms, support, probe_radius)?;
  let first = &atoms[support[0]];
  let second = &atoms[support[1]];
  let Some(circle) = probe_circle(first, second, probe_radius) else {
    return Ok(Vec::new());
  };

  let mut occluded_intervals = Vec::new();
  for (atom_index, atom) in atoms.iter().enumerate() {
    if support.contains(&atom_index) {
      continue;
    }
    match occluded_interval(circle, atom, probe_radius) {
      CircleOcclusion::None => {}
      CircleOcclusion::Full => return Ok(Vec::new()),
      CircleOcclusion::Partial { center, half_width } => {
        append_wrapped_interval(&mut occluded_intervals, center - half_width, center + half_width);
      }
    }
  }

  let accessible_intervals = complement_intervals(occluded_intervals);
  let atom_indices = ordered_pair(first.atom_index, second.atom_index);
  Ok(
    accessible_intervals
      .into_iter()
      .map(|[start, end]| ReducedSurfaceEdge {
        atom_indices,
        probe_circle_center: circle.center.to_array(),
        probe_circle_axis: circle.axis.to_array(),
        probe_circle_basis: circle.basis.to_array(),
        probe_circle_radius: circle.radius,
        start_angle: start,
        sweep_angle: end - start,
      })
      .collect(),
  )
}

/// Orthonormal parameterization of one proper expanded-sphere intersection circle.
#[derive(Clone, Copy)]
struct ProbeCircle {
  center: DVec3,
  axis: DVec3,
  basis: DVec3,
  perpendicular_basis: DVec3,
  radius: f64,
}

/// Occlusion contributed by one non-support expanded atom.
enum CircleOcclusion {
  None,
  Full,
  Partial { center: f64, half_width: f64 },
}

/// Validates every input used by circle occlusion tests.
fn validate_geometry(atoms: &[MsmsAtom], support: [usize; 2], probe_radius: f64) -> Result<(), ProbeArcError> {
  if support[0] == support[1] {
    return Err(ProbeArcError::DuplicateAtomIndex);
  }
  if !probe_radius.is_finite() || probe_radius <= 0.0 || support.into_iter().any(|index| index >= atoms.len()) {
    return Err(ProbeArcError::InvalidGeometry);
  }
  if atoms
    .iter()
    .any(|atom| !atom.center.is_finite() || !atom.radius.is_finite() || atom.radius <= 0.0)
  {
    return Err(ProbeArcError::InvalidGeometry);
  }
  Ok(())
}

/// Constructs a proper intersection circle for two expanded atom spheres.
fn probe_circle(first: &MsmsAtom, second: &MsmsAtom, probe_radius: f64) -> Option<ProbeCircle> {
  let displacement = second.center - first.center;
  let distance = displacement.length();
  let first_radius = first.radius + probe_radius;
  let second_radius = second.radius + probe_radius;
  let scale = distance.max(first_radius).max(second_radius).max(1.0);
  let linear_tolerance = RELATIVE_ARC_TOLERANCE * scale;
  if distance <= linear_tolerance
    || distance >= first_radius + second_radius - linear_tolerance
    || distance <= (first_radius - second_radius).abs() + linear_tolerance
  {
    return None;
  }

  let axis = displacement / distance;
  let axial_distance = (first_radius.powi(2) - second_radius.powi(2) + distance.powi(2)) / (2.0 * distance);
  let radius_squared = first_radius.powi(2) - axial_distance.powi(2);
  if radius_squared <= linear_tolerance.powi(2) {
    return None;
  }
  let center = first.center + axial_distance * axis;
  let reference = if axis.x.abs() < 0.8 { DVec3::X } else { DVec3::Y };
  let basis = axis.cross(reference).normalize();
  let perpendicular_basis = axis.cross(basis);
  Some(ProbeCircle {
    center,
    axis,
    basis,
    perpendicular_basis,
    radius: radius_squared.sqrt(),
  })
}

/// Computes the angular interval hidden by one expanded non-support atom.
fn occluded_interval(circle: ProbeCircle, atom: &MsmsAtom, probe_radius: f64) -> CircleOcclusion {
  let displacement = atom.center - circle.center;
  let axial = displacement.dot(circle.axis);
  let planar = displacement - axial * circle.axis;
  let planar_distance = planar.length();
  let expanded_radius = atom.radius + probe_radius;
  let scale = circle.radius.max(planar_distance).max(expanded_radius).max(1.0);
  let linear_tolerance = RELATIVE_ARC_TOLERANCE * scale;
  let distance_squared_on_circle = circle.radius.powi(2) + displacement.length_squared();

  if planar_distance <= linear_tolerance {
    return if distance_squared_on_circle < expanded_radius.powi(2) {
      CircleOcclusion::Full
    } else {
      CircleOcclusion::None
    };
  }

  let cosine_boundary =
    (distance_squared_on_circle - expanded_radius.powi(2)) / (2.0 * circle.radius * planar_distance);
  if cosine_boundary <= -1.0 {
    return CircleOcclusion::Full;
  }
  if cosine_boundary >= 1.0 {
    return CircleOcclusion::None;
  }
  let center = planar.dot(circle.perpendicular_basis).atan2(planar.dot(circle.basis));
  CircleOcclusion::Partial {
    center,
    half_width: cosine_boundary.clamp(-1.0, 1.0).acos(),
  }
}

/// Splits a possibly wrapped angular interval into the canonical $[0,2\pi]$ range.
fn append_wrapped_interval(intervals: &mut Vec<[f64; 2]>, start: f64, end: f64) {
  let start = start.rem_euclid(TAU);
  let end = end.rem_euclid(TAU);
  if start <= end {
    intervals.push([start, end]);
  } else {
    intervals.push([0.0, end]);
    intervals.push([start, TAU]);
  }
}

/// Returns the complement of a union of circular occlusion intervals.
fn complement_intervals(mut intervals: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
  if intervals.is_empty() {
    return vec![[0.0, TAU]];
  }
  intervals.sort_by(|first, second| first[0].total_cmp(&second[0]));
  let angular_tolerance = RELATIVE_ARC_TOLERANCE * TAU;
  let mut merged = Vec::with_capacity(intervals.len());
  for interval in intervals {
    let Some(last): Option<&mut [f64; 2]> = merged.last_mut() else {
      merged.push(interval);
      continue;
    };
    if interval[0] <= last[1] + angular_tolerance {
      last[1] = last[1].max(interval[1]);
    } else {
      merged.push(interval);
    }
  }

  let mut accessible = Vec::new();
  let mut cursor = 0.0;
  for [start, end] in merged {
    if start > cursor + angular_tolerance {
      accessible.push([cursor, start]);
    }
    cursor = cursor.max(end);
  }
  if cursor < TAU - angular_tolerance {
    accessible.push([cursor, TAU]);
  }
  merge_parameter_seam(&mut accessible, angular_tolerance);
  accessible
}

/// Joins one physical arc split across the zero-angle parameter seam.
fn merge_parameter_seam(intervals: &mut Vec<[f64; 2]>, angular_tolerance: f64) {
  if intervals.len() < 2 {
    return;
  }
  let first_reaches_zero = intervals[0][0] <= angular_tolerance;
  let last_index = intervals.len() - 1;
  let last_reaches_tau = intervals[last_index][1] >= TAU - angular_tolerance;
  if first_reaches_zero && last_reaches_tau {
    let wrapped = [intervals[last_index][0], TAU + intervals[0][1]];
    intervals.pop();
    intervals.remove(0);
    intervals.push(wrapped);
  }
}

/// Orders a source atom pair independently of local support orientation.
fn ordered_pair(first: usize, second: usize) -> [usize; 2] {
  if first < second {
    [first, second]
  } else {
    [second, first]
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn atom(atom_index: usize, center: DVec3, radius: f64) -> MsmsAtom {
    MsmsAtom {
      atom_index,
      center,
      radius,
    }
  }

  #[test]
  fn unobstructed_pair_should_produce_one_free_circular_edge() {
    let atoms = vec![atom(10, DVec3::ZERO, 1.0), atom(20, 2.0 * DVec3::X, 1.0)];
    let arcs = accessible_probe_arcs(&atoms, [0, 1], 1.0)
      .unwrap_or_else(|error| panic!("intersecting pair should produce a free edge: {error}"));

    assert_eq!(arcs.len(), 1);
    assert!((arcs[0].sweep_angle - TAU).abs() < 1.0e-12);
  }

  #[test]
  fn one_occluder_should_leave_the_complementary_arc() {
    let atoms = vec![
      atom(0, DVec3::ZERO, 1.0),
      atom(1, 2.0 * DVec3::X, 1.0),
      atom(2, DVec3::new(1.0, 2.0, 0.0), 1.0),
    ];
    let arcs = accessible_probe_arcs(&atoms, [0, 1], 1.0)
      .unwrap_or_else(|error| panic!("partially occluded pair should produce an arc: {error}"));

    assert_eq!(arcs.len(), 1);
    assert!(arcs[0].sweep_angle > std::f64::consts::PI && arcs[0].sweep_angle < TAU);
  }

  #[test]
  fn axial_occluder_should_remove_the_complete_probe_circle() {
    let atoms = vec![
      atom(0, DVec3::ZERO, 1.0),
      atom(1, 2.0 * DVec3::X, 1.0),
      atom(2, DVec3::X, 3.0),
    ];
    let arcs = accessible_probe_arcs(&atoms, [0, 1], 1.0)
      .unwrap_or_else(|error| panic!("valid full occlusion should not fail: {error}"));

    assert!(arcs.is_empty());
  }

  #[test]
  fn disjoint_pair_should_not_produce_probe_arcs() {
    let atoms = vec![atom(0, DVec3::ZERO, 1.0), atom(1, 5.0 * DVec3::X, 1.0)];
    let arcs = accessible_probe_arcs(&atoms, [0, 1], 1.0)
      .unwrap_or_else(|error| panic!("valid disjoint pair should not fail: {error}"));

    assert!(arcs.is_empty());
  }

  #[test]
  fn two_disjoint_occlusions_should_leave_two_accessible_arcs() {
    let atoms = vec![
      atom(0, DVec3::ZERO, 1.0),
      atom(1, 2.0 * DVec3::X, 1.0),
      atom(2, DVec3::new(1.0, 2.5, 0.0), 0.5),
      atom(3, DVec3::new(1.0, -2.5, 0.0), 0.5),
    ];
    let arcs = accessible_probe_arcs(&atoms, [0, 1], 1.0)
      .unwrap_or_else(|error| panic!("two partial occluders should produce complementary arcs: {error}"));

    assert_eq!(arcs.len(), 2);
  }
}
