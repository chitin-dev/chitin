//! Analytical patch geometry derived from reduced-surface topology.

use std::collections::BTreeMap;

use glam::DVec3;
use thiserror::Error;

use super::{MsmsParameters, ReducedSurfaceFace, ReentrantPatch, geometry::MsmsAtom};

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
  use crate::surface::msms::geometry::tangent_probe_faces;

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
}
