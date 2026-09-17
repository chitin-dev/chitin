//! Analytical patch geometry derived from reduced-surface topology.

use std::{
  collections::BTreeMap,
  f64::consts::{PI, TAU},
};

use glam::DVec3;
use thiserror::Error;

use super::{
  MsmsParameters, ReducedSurfaceEdge, ReducedSurfaceFace, ReentrantPatch, ToroidalPatchGeometry, ToroidalPatchTopology,
  geometry::MsmsAtom,
};

/// Failure produced while converting reduced-surface topology into patches.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum MsmsPatchConstructionError {
  /// A face references a source atom absent from its analytical domain.
  #[error("reduced-surface face {face_index} references missing source atom {atom_index}")]
  MissingAtom {
    /// Index of the face being converted.
    face_index: usize,
    /// Missing source atom index.
    atom_index: usize,
  },
  /// More than one analytical atom uses the same source identity.
  #[error("analytical domain contains duplicate source atom {atom_index}")]
  DuplicateAtom {
    /// Repeated source atom index.
    atom_index: usize,
  },
  /// A face contains a non-finite center or degenerate contact direction.
  #[error("reduced-surface face {face_index} has invalid reentrant geometry")]
  InvalidReentrantGeometry {
    /// Index of the invalid face.
    face_index: usize,
  },
  /// An edge references a source atom absent from its analytical domain.
  #[error("reduced-surface edge {edge_index} references missing source atom {atom_index}")]
  MissingEdgeAtom {
    /// Index of the edge being converted.
    edge_index: usize,
    /// Missing source atom index.
    atom_index: usize,
  },
  /// An edge contains a non-finite or non-orthonormal probe-circle frame.
  #[error("reduced-surface edge {edge_index} has invalid toroidal geometry")]
  InvalidToroidalGeometry {
    /// Index of the invalid edge.
    edge_index: usize,
  },
}

/// Builds untrimmed toroidal geometry from accessible rolling-probe arcs.
///
/// Each edge supplies the probe-center circle and its retained azimuth range.
/// The two supporting atoms determine the shorter probe-sphere meridian between
/// their contact curves. Patches whose meridian crosses a zero Jacobian are
/// classified as self-intersecting and retained for a later trimming stage;
/// their area is deliberately unavailable before trimming.
///
/// # Parameters
///
/// * `atoms` contains the analytical atoms in the edge domain.
/// * `edges` contains accessible, incident reduced-surface arcs.
/// * `parameters` supplies the rolling-probe radius.
///
/// # Returns
///
/// One toroidal geometry record per edge, in edge order, or a structured error
/// when an edge cannot be resolved to valid atom and circle geometry.
///
/// # Examples
///
/// ```
/// use chitin_bio::surface::msms::{
///   MsmsParameters,
///   ToroidalPatchTopology,
///   build_accessible_probe_edges,
///   build_toroidal_patch_geometry,
///   geometry::MsmsAtom,
/// };
/// use glam::DVec3;
///
/// let atoms = vec![
///   MsmsAtom { atom_index: 0, center: DVec3::ZERO, radius: 1.0 },
///   MsmsAtom { atom_index: 1, center: DVec3::new(2.0, 0.0, 0.0), radius: 1.0 },
/// ];
/// let parameters = MsmsParameters::new(1.0)?;
/// let edges = build_accessible_probe_edges(&atoms, parameters)?;
/// let patches = build_toroidal_patch_geometry(&atoms, &edges, parameters)?;
/// assert_eq!(patches[0].topology, ToroidalPatchTopology::Regular);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn build_toroidal_patch_geometry(
  atoms: &[MsmsAtom],
  edges: &[ReducedSurfaceEdge],
  parameters: MsmsParameters,
) -> Result<Vec<ToroidalPatchGeometry>, MsmsPatchConstructionError> {
  let atom_lookup = atom_lookup(atoms)?;
  edges
    .iter()
    .enumerate()
    .map(|(edge_index, edge)| toroidal_patch_geometry(edge_index, edge, &atom_lookup, parameters.probe_radius()))
    .collect()
}

/// Builds the concave spherical patches supported by tangent-probe faces.
///
/// Each supporting atom determines one probe/atom contact direction. The
/// oriented directions form a spherical triangle on the rolling probe. Its
/// solid angle is evaluated analytically, while the center and boundary
/// directions are retained for later adaptive display tessellation.
///
/// # Parameters
///
/// * `atoms` contains the analytical atoms in the face domain.
/// * `faces` contains accessible, oriented tangent-probe faces.
/// * `parameters` supplies the rolling-probe radius.
///
/// # Returns
///
/// One reentrant patch per face, in face order, or a structured error when the
/// topology does not reference valid non-degenerate atom geometry.
///
/// # Examples
///
/// ```
/// use chitin_bio::surface::msms::{
///   MsmsParameters,
///   build_reentrant_patches,
///   geometry::{MsmsAtom, tangent_probe_faces},
/// };
/// use glam::DVec3;
///
/// let atoms = vec![
///   MsmsAtom { atom_index: 0, center: DVec3::ZERO, radius: 1.0 },
///   MsmsAtom { atom_index: 1, center: DVec3::new(2.0, 0.0, 0.0), radius: 1.0 },
///   MsmsAtom {
///     atom_index: 2,
///     center: DVec3::new(1.0, 3.0_f64.sqrt(), 0.0),
///     radius: 1.0,
///   },
/// ];
/// let parameters = MsmsParameters::new(1.0)?;
/// let faces = tangent_probe_faces(&atoms, [0, 1, 2], parameters.probe_radius())?;
/// let patches = build_reentrant_patches(&atoms, &faces, parameters)?;
/// assert_eq!(patches.len(), 2);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn build_reentrant_patches(
  atoms: &[MsmsAtom],
  faces: &[ReducedSurfaceFace],
  parameters: MsmsParameters,
) -> Result<Vec<ReentrantPatch>, MsmsPatchConstructionError> {
  let atom_lookup = atom_lookup(atoms)?;
  faces
    .iter()
    .enumerate()
    .map(|(face_index, face)| reentrant_patch(face_index, face, &atom_lookup, parameters.probe_radius()))
    .collect()
}

/// Creates a source-index lookup while enforcing identity uniqueness.
fn atom_lookup(atoms: &[MsmsAtom]) -> Result<BTreeMap<usize, &MsmsAtom>, MsmsPatchConstructionError> {
  let mut lookup = BTreeMap::new();
  for atom in atoms {
    if lookup.insert(atom.atom_index, atom).is_some() {
      return Err(MsmsPatchConstructionError::DuplicateAtom {
        atom_index: atom.atom_index,
      });
    }
  }
  Ok(lookup)
}

/// Converts one oriented reduced-surface face into a spherical triangle.
fn reentrant_patch(
  face_index: usize,
  face: &ReducedSurfaceFace,
  atom_lookup: &BTreeMap<usize, &MsmsAtom>,
  probe_radius: f64,
) -> Result<ReentrantPatch, MsmsPatchConstructionError> {
  let probe_center = DVec3::from_array(face.probe_center);
  if !probe_center.is_finite() {
    return Err(MsmsPatchConstructionError::InvalidReentrantGeometry { face_index });
  }

  let mut directions = [DVec3::ZERO; 3];
  for (slot, atom_index) in face.atom_indices.into_iter().enumerate() {
    let atom = atom_lookup
      .get(&atom_index)
      .ok_or(MsmsPatchConstructionError::MissingAtom { face_index, atom_index })?;
    let displacement = atom.center - probe_center;
    if !displacement.is_finite() || displacement.length_squared() <= f64::EPSILON {
      return Err(MsmsPatchConstructionError::InvalidReentrantGeometry { face_index });
    }
    directions[slot] = displacement.normalize();
  }

  let solid_angle = spherical_triangle_solid_angle(directions);
  if !solid_angle.is_finite() || solid_angle <= f64::EPSILON {
    return Err(MsmsPatchConstructionError::InvalidReentrantGeometry { face_index });
  }
  Ok(ReentrantPatch {
    face_index,
    probe_center: face.probe_center,
    probe_radius,
    contact_directions: directions.map(|direction| direction.to_array()),
    solid_angle,
  })
}

/// Converts one accessible pair arc into an untrimmed toroidal parameter patch.
fn toroidal_patch_geometry(
  edge_index: usize,
  edge: &ReducedSurfaceEdge,
  atom_lookup: &BTreeMap<usize, &MsmsAtom>,
  probe_radius: f64,
) -> Result<ToroidalPatchGeometry, MsmsPatchConstructionError> {
  validate_toroidal_frame(edge_index, edge)?;
  let atoms = edge
    .atom_indices
    .map(|atom_index| {
      atom_lookup
        .get(&atom_index)
        .copied()
        .ok_or(MsmsPatchConstructionError::MissingEdgeAtom { edge_index, atom_index })
    })
    .into_iter()
    .collect::<Result<Vec<_>, _>>()?;
  let center = DVec3::from_array(edge.probe_circle_center);
  let axis = DVec3::from_array(edge.probe_circle_axis);
  let polar_angles = [
    contact_polar_angle(
      edge_index,
      atoms[0],
      center,
      axis,
      edge.probe_circle_radius,
      probe_radius,
    )?,
    contact_polar_angle(
      edge_index,
      atoms[1],
      center,
      axis,
      edge.probe_circle_radius,
      probe_radius,
    )?,
  ];
  let (polar_start, polar_sweep) = shorter_positive_sweep(polar_angles[0], polar_angles[1]);
  if polar_sweep <= 1024.0 * f64::EPSILON || polar_sweep > PI + 1024.0 * f64::EPSILON {
    return Err(MsmsPatchConstructionError::InvalidToroidalGeometry { edge_index });
  }
  let topology = toroidal_topology(edge.probe_circle_radius, probe_radius, polar_start, polar_sweep);

  Ok(ToroidalPatchGeometry {
    edge_index,
    atom_indices: edge.atom_indices,
    probe_circle_center: edge.probe_circle_center,
    probe_circle_axis: edge.probe_circle_axis,
    probe_circle_basis: edge.probe_circle_basis,
    major_radius: edge.probe_circle_radius,
    probe_radius,
    azimuth_start: edge.start_angle,
    azimuth_sweep: edge.sweep_angle,
    polar_start,
    polar_sweep,
    topology,
  })
}

/// Validates the circle frame and parameter ranges required by a torus.
fn validate_toroidal_frame(edge_index: usize, edge: &ReducedSurfaceEdge) -> Result<(), MsmsPatchConstructionError> {
  let center = DVec3::from_array(edge.probe_circle_center);
  let axis = DVec3::from_array(edge.probe_circle_axis);
  let basis = DVec3::from_array(edge.probe_circle_basis);
  let finite_parameters = edge.probe_circle_radius.is_finite()
    && edge.start_angle.is_finite()
    && edge.sweep_angle.is_finite()
    && edge.probe_circle_radius > 0.0
    && edge.sweep_angle > 0.0
    && edge.sweep_angle <= TAU + 1024.0 * f64::EPSILON;
  let frame_is_orthonormal = (axis.length_squared() - 1.0).abs() <= 4096.0 * f64::EPSILON
    && (basis.length_squared() - 1.0).abs() <= 4096.0 * f64::EPSILON
    && axis.dot(basis).abs() <= 4096.0 * f64::EPSILON;
  if !center.is_finite() || !axis.is_finite() || !basis.is_finite() || !finite_parameters || !frame_is_orthonormal {
    return Err(MsmsPatchConstructionError::InvalidToroidalGeometry { edge_index });
  }
  Ok(())
}

/// Finds one atom-contact meridian angle in the stored torus frame.
fn contact_polar_angle(
  edge_index: usize,
  atom: &MsmsAtom,
  circle_center: DVec3,
  axis: DVec3,
  major_radius: f64,
  probe_radius: f64,
) -> Result<f64, MsmsPatchConstructionError> {
  let center_offset = atom.center - circle_center;
  let axial_offset = center_offset.dot(axis);
  let off_axis = center_offset - axial_offset * axis;
  let expanded_radius = atom.radius + probe_radius;
  let geometry_scale = atom.center.abs().max(circle_center.abs()).max(DVec3::ONE).max_element();
  let tolerance = 4096.0 * f64::EPSILON * geometry_scale;
  let tangent_distance = major_radius.hypot(axial_offset);
  if !atom.center.is_finite()
    || !atom.radius.is_finite()
    || atom.radius <= 0.0
    || off_axis.length() > tolerance
    || (tangent_distance - expanded_radius).abs() > tolerance
  {
    return Err(MsmsPatchConstructionError::InvalidToroidalGeometry { edge_index });
  }
  Ok(axial_offset.atan2(-major_radius).rem_euclid(TAU))
}

/// Selects the minor positive circular interval between two parameters.
fn shorter_positive_sweep(first: f64, second: f64) -> (f64, f64) {
  let forward = (second - first).rem_euclid(TAU);
  if forward <= PI {
    (first, forward)
  } else {
    (second, TAU - forward)
  }
}

/// Classifies whether the torus Jacobian vanishes inside a retained meridian.
fn toroidal_topology(
  major_radius: f64,
  probe_radius: f64,
  polar_start: f64,
  polar_sweep: f64,
) -> ToroidalPatchTopology {
  if major_radius > probe_radius {
    return ToroidalPatchTopology::Regular;
  }
  let first_root = (-major_radius / probe_radius).clamp(-1.0, 1.0).acos();
  let second_root = TAU - first_root;
  if circular_interval_contains(polar_start, polar_sweep, first_root)
    || circular_interval_contains(polar_start, polar_sweep, second_root)
  {
    ToroidalPatchTopology::SelfIntersecting
  } else {
    ToroidalPatchTopology::Regular
  }
}

/// Tests membership in one positive, at-most-half-circle angular interval.
fn circular_interval_contains(start: f64, sweep: f64, angle: f64) -> bool {
  let angular_tolerance = 1024.0 * f64::EPSILON * TAU;
  (angle - start).rem_euclid(TAU) <= sweep + angular_tolerance
}

/// Evaluates the unsigned solid angle of a unit-vector spherical triangle.
fn spherical_triangle_solid_angle(directions: [DVec3; 3]) -> f64 {
  let numerator = directions[0].dot(directions[1].cross(directions[2])).abs();
  let denominator =
    1.0 + directions[0].dot(directions[1]) + directions[1].dot(directions[2]) + directions[2].dot(directions[0]);
  2.0 * numerator.atan2(denominator)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::surface::msms::{build_accessible_probe_edges, geometry::tangent_probe_faces};

  fn equilateral_atoms() -> Vec<MsmsAtom> {
    vec![
      MsmsAtom {
        atom_index: 10,
        center: DVec3::ZERO,
        radius: 1.0,
      },
      MsmsAtom {
        atom_index: 20,
        center: 2.0 * DVec3::X,
        radius: 1.0,
      },
      MsmsAtom {
        atom_index: 30,
        center: DVec3::new(1.0, 3.0_f64.sqrt(), 0.0),
        radius: 1.0,
      },
    ]
  }

  fn equal_pair(distance: f64) -> Vec<MsmsAtom> {
    vec![
      MsmsAtom {
        atom_index: 10,
        center: DVec3::ZERO,
        radius: 1.0,
      },
      MsmsAtom {
        atom_index: 20,
        center: distance * DVec3::X,
        radius: 1.0,
      },
    ]
  }

  fn toroidal_geometry_for_pair(distance: f64) -> ToroidalPatchGeometry {
    let atoms = equal_pair(distance);
    let parameters =
      MsmsParameters::new(1.0).unwrap_or_else(|error| panic!("unit probe radius should be valid: {error}"));
    let edges = build_accessible_probe_edges(&atoms, parameters)
      .unwrap_or_else(|error| panic!("intersecting pair should produce a probe edge: {error}"));
    build_toroidal_patch_geometry(&atoms, &edges, parameters)
      .unwrap_or_else(|error| panic!("probe edge should produce toroidal geometry: {error}"))
      .into_iter()
      .next()
      .unwrap_or_else(|| panic!("intersecting pair should produce one toroidal patch"))
  }

  #[test]
  fn mirrored_probe_faces_should_produce_equal_reentrant_areas() {
    let atoms = equilateral_atoms();
    let parameters =
      MsmsParameters::new(1.0).unwrap_or_else(|error| panic!("unit probe radius should be valid: {error}"));
    let faces = tangent_probe_faces(&atoms, [0, 1, 2], parameters.probe_radius())
      .unwrap_or_else(|error| panic!("equilateral atoms should accept tangent probes: {error}"));
    let patches = build_reentrant_patches(&atoms, &faces, parameters)
      .unwrap_or_else(|error| panic!("probe faces should produce reentrant patches: {error}"));

    assert_eq!(patches.len(), 2);
    assert!((patches[0].area() - patches[1].area()).abs() < 1.0e-12);
    assert!(patches[0].area() > 0.0);
    assert!(
      patches
        .iter()
        .flat_map(|patch| patch.contact_directions)
        .all(|direction| { (DVec3::from_array(direction).length() - 1.0).abs() < 1.0e-12 })
    );
  }

  #[test]
  fn missing_support_atom_should_return_structured_error() {
    let face = ReducedSurfaceFace {
      atom_indices: [10, 20, 30],
      probe_center: [1.0, 1.0, 1.0],
    };
    let atoms = equilateral_atoms();

    assert_eq!(
      build_reentrant_patches(&atoms[..2], &[face], MsmsParameters::default()),
      Err(MsmsPatchConstructionError::MissingAtom {
        face_index: 0,
        atom_index: 30,
      })
    );
  }

  #[test]
  fn separated_support_atoms_should_produce_a_regular_torus() {
    let patch = toroidal_geometry_for_pair(2.0);

    assert_eq!(patch.topology, ToroidalPatchTopology::Regular);
    assert!(patch.regular_area().is_some_and(|area| area > 0.0));
  }

  #[test]
  fn near_tangent_support_atoms_should_produce_a_singular_torus() {
    let patch = toroidal_geometry_for_pair(3.8);

    assert_eq!(patch.topology, ToroidalPatchTopology::SelfIntersecting);
    assert_eq!(patch.regular_area(), None);
  }

  #[test]
  fn torus_meridian_boundaries_should_lie_on_atom_contact_spheres() {
    let atoms = equal_pair(2.0);
    let patch = toroidal_geometry_for_pair(2.0);
    let points = [patch.position(0.0, 0.0), patch.position(0.0, patch.polar_sweep)];

    assert!(points.into_iter().all(|point| {
      let point = DVec3::from_array(point);
      atoms
        .iter()
        .any(|atom| (point.distance(atom.center) - atom.radius).abs() < 1.0e-12)
    }));
  }

  #[test]
  fn torus_outward_normal_should_be_unit_length() {
    let patch = toroidal_geometry_for_pair(2.0);
    let normal = DVec3::from_array(patch.outward_normal(0.37, 0.41 * patch.polar_sweep));

    assert!((normal.length() - 1.0).abs() < 1.0e-12);
  }
}
