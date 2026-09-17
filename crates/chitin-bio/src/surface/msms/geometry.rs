//! Analytical probe-placement geometry used by reduced-surface construction.

use glam::DVec3;
use thiserror::Error;

use super::ReducedSurfaceFace;

/// Relative tolerance used only to absorb floating-point roundoff.
const RELATIVE_GEOMETRY_TOLERANCE: f64 = 128.0 * f64::EPSILON;

/// One atom sphere prepared for analytical MSMS construction.
#[derive(Clone, Debug, PartialEq)]
pub struct MsmsAtom {
  /// Stable index of the source atom in the structure scene.
  pub atom_index: usize,
  /// Cartesian atom center in ångströms.
  pub center: DVec3,
  /// Van der Waals radius in ångströms.
  pub radius: f64,
}

/// Geometric failure encountered while placing a probe tangent to three atoms.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum TangentProbeError {
  /// A support triplet must contain three different atom indices.
  #[error("tangent-probe support contains a duplicate atom index")]
  DuplicateAtomIndex,
  /// Atom centers do not define a stable plane.
  #[error("tangent-probe support atom centers are coincident or collinear")]
  DegenerateSupport,
  /// The three expanded atom spheres have no common point.
  #[error("expanded support spheres have no common tangent-probe center")]
  NoIntersection,
  /// Atom or probe geometry contains a non-finite or non-positive value.
  #[error("tangent-probe geometry contains an invalid coordinate or radius")]
  InvalidGeometry,
}

/// Probe centers tangent to a support triplet before accessibility filtering.
#[derive(Clone, Debug, PartialEq)]
struct TangentProbeCenters {
  /// First intersection on the positive side of the support plane.
  first: DVec3,
  /// Mirrored intersection, absent when the spheres meet in one point.
  second: Option<DVec3>,
}

/// Constructs oriented reduced-surface faces for one atom triplet.
///
/// The calculation intersects spheres centered on the atoms with radii
/// $r_i+r_p$. Each resulting probe center is discarded when it lies inside an
/// expanded non-support atom. Surviving faces are oriented so the support
/// triangle normal points toward the probe center.
///
/// # Parameters
///
/// * `atoms` contains every candidate atom sphere in the calculation domain.
/// * `support` contains three distinct indices into `atoms`.
/// * `probe_radius` is the rolling solvent-probe radius in ångströms.
///
/// # Returns
///
/// Zero, one, or two accessible reduced-surface faces, or a geometric error
/// when the support triplet cannot define tangent probe centers.
///
/// # Examples
///
/// Three equal atoms at the vertices of an equilateral triangle produce two
/// mirrored probe centers when no fourth atom occludes either side.
///
/// ```
/// use chitin_bio::surface::msms::geometry::{MsmsAtom, tangent_probe_faces};
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
/// let faces = tangent_probe_faces(&atoms, [0, 1, 2], 1.0)?;
/// assert_eq!(faces.len(), 2);
/// # Ok::<(), chitin_bio::surface::msms::geometry::TangentProbeError>(())
/// ```
pub fn tangent_probe_faces(
  atoms: &[MsmsAtom],
  support: [usize; 3],
  probe_radius: f64,
) -> Result<Vec<ReducedSurfaceFace>, TangentProbeError> {
  validate_support(atoms, support, probe_radius)?;
  let support_atoms = [&atoms[support[0]], &atoms[support[1]], &atoms[support[2]]];
  let centers = tangent_probe_centers(support_atoms, probe_radius)?;
  let mut faces = Vec::with_capacity(2);
  for center in [Some(centers.first), centers.second].into_iter().flatten() {
    if probe_center_is_accessible(center, atoms, support, probe_radius) {
      faces.push(oriented_face(support_atoms, center));
    }
  }
  Ok(faces)
}

/// Validates indices and finite positive geometry before analytical operations.
fn validate_support(atoms: &[MsmsAtom], support: [usize; 3], probe_radius: f64) -> Result<(), TangentProbeError> {
  if support[0] == support[1] || support[0] == support[2] || support[1] == support[2] {
    return Err(TangentProbeError::DuplicateAtomIndex);
  }
  if !probe_radius.is_finite() || probe_radius <= 0.0 {
    return Err(TangentProbeError::InvalidGeometry);
  }
  if support.into_iter().any(|index| index >= atoms.len()) {
    return Err(TangentProbeError::InvalidGeometry);
  }
  for atom in atoms {
    if !atom.center.is_finite() || !atom.radius.is_finite() || atom.radius <= 0.0 {
      return Err(TangentProbeError::InvalidGeometry);
    }
  }
  Ok(())
}

/// Intersects the three atom spheres expanded by the probe radius.
fn tangent_probe_centers(atoms: [&MsmsAtom; 3], probe_radius: f64) -> Result<TangentProbeCenters, TangentProbeError> {
  let p1 = atoms[0].center;
  let p2 = atoms[1].center;
  let p3 = atoms[2].center;
  let expanded = [
    atoms[0].radius + probe_radius,
    atoms[1].radius + probe_radius,
    atoms[2].radius + probe_radius,
  ];

  let p1_to_p2 = p2 - p1;
  let distance_12 = p1_to_p2.length();
  let geometry_scale = distance_12
    .max((p3 - p1).length())
    .max(expanded.into_iter().fold(0.0, f64::max))
    .max(1.0);
  let linear_tolerance = RELATIVE_GEOMETRY_TOLERANCE * geometry_scale;
  if distance_12 <= linear_tolerance {
    return Err(TangentProbeError::DegenerateSupport);
  }

  let axis_x = p1_to_p2 / distance_12;
  let p1_to_p3 = p3 - p1;
  let projection_x = axis_x.dot(p1_to_p3);
  let orthogonal = p1_to_p3 - projection_x * axis_x;
  let projection_y = orthogonal.length();
  if projection_y <= linear_tolerance {
    return Err(TangentProbeError::DegenerateSupport);
  }
  let axis_y = orthogonal / projection_y;
  let axis_z = axis_x.cross(axis_y);

  let x = (expanded[0].powi(2) - expanded[1].powi(2) + distance_12.powi(2)) / (2.0 * distance_12);
  let y = (expanded[0].powi(2) - expanded[2].powi(2) + projection_x.powi(2) + projection_y.powi(2)
    - 2.0 * projection_x * x)
    / (2.0 * projection_y);
  let height_squared = expanded[0].powi(2) - x.powi(2) - y.powi(2);
  let squared_scale = expanded[0].powi(2).max(x.powi(2) + y.powi(2)).max(1.0);
  let squared_tolerance = RELATIVE_GEOMETRY_TOLERANCE * squared_scale;
  if height_squared < -squared_tolerance {
    return Err(TangentProbeError::NoIntersection);
  }

  let base = p1 + x * axis_x + y * axis_y;
  let height = height_squared.max(0.0).sqrt();
  if height <= linear_tolerance {
    return Ok(TangentProbeCenters {
      first: base,
      second: None,
    });
  }
  Ok(TangentProbeCenters {
    first: base + height * axis_z,
    second: Some(base - height * axis_z),
  })
}

/// Tests whether a tangent probe avoids every non-support expanded atom sphere.
fn probe_center_is_accessible(center: DVec3, atoms: &[MsmsAtom], support: [usize; 3], probe_radius: f64) -> bool {
  atoms.iter().enumerate().all(|(index, atom)| {
    if support.contains(&index) {
      return true;
    }
    let expanded_radius = atom.radius + probe_radius;
    let scale = center.distance(atom.center).max(expanded_radius).max(1.0);
    center.distance(atom.center) + RELATIVE_GEOMETRY_TOLERANCE * scale >= expanded_radius
  })
}

/// Orients a reduced-surface face toward its tangent probe center.
fn oriented_face(atoms: [&MsmsAtom; 3], probe_center: DVec3) -> ReducedSurfaceFace {
  let normal = (atoms[1].center - atoms[0].center).cross(atoms[2].center - atoms[0].center);
  let mut atom_indices = [atoms[0].atom_index, atoms[1].atom_index, atoms[2].atom_index];
  if normal.dot(probe_center - atoms[0].center) < 0.0 {
    atom_indices.swap(1, 2);
  }
  ReducedSurfaceFace {
    atom_indices,
    probe_center: probe_center.to_array(),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn equilateral_atoms() -> Vec<MsmsAtom> {
    let sqrt_3 = 3.0_f64.sqrt();
    vec![
      MsmsAtom {
        atom_index: 10,
        center: DVec3::ZERO,
        radius: 1.0,
      },
      MsmsAtom {
        atom_index: 20,
        center: DVec3::new(2.0, 0.0, 0.0),
        radius: 1.0,
      },
      MsmsAtom {
        atom_index: 30,
        center: DVec3::new(1.0, sqrt_3, 0.0),
        radius: 1.0,
      },
    ]
  }

  #[test]
  fn equilateral_support_should_produce_two_mirrored_probe_centers() {
    let faces = tangent_probe_faces(&equilateral_atoms(), [0, 1, 2], 1.0)
      .unwrap_or_else(|error| panic!("equilateral support should accept a tangent probe: {error}"));

    assert_eq!(faces.len(), 2);
    assert!((faces[0].probe_center[2] + faces[1].probe_center[2]).abs() < 1.0e-12);
    assert!((faces[0].probe_center[2].abs() - (8.0_f64 / 3.0).sqrt()).abs() < 1.0e-12);
  }

  #[test]
  fn fourth_atom_should_occlude_one_probe_center() {
    let mut atoms = equilateral_atoms();
    let sqrt_3 = 3.0_f64.sqrt();
    atoms.push(MsmsAtom {
      atom_index: 40,
      center: DVec3::new(1.0, sqrt_3 / 3.0, 2.0),
      radius: 1.0,
    });

    let faces = tangent_probe_faces(&atoms, [0, 1, 2], 1.0)
      .unwrap_or_else(|error| panic!("support should retain its unobstructed probe center: {error}"));

    assert_eq!(faces.len(), 1);
    assert!(faces[0].probe_center[2] < 0.0);
  }

  #[test]
  fn inaccessible_triplet_should_return_no_faces() {
    let mut atoms = equilateral_atoms();
    let sqrt_3 = 3.0_f64.sqrt();
    for z in [-2.0, 2.0] {
      atoms.push(MsmsAtom {
        atom_index: atoms.len() + 40,
        center: DVec3::new(1.0, sqrt_3 / 3.0, z),
        radius: 1.0,
      });
    }

    let faces = tangent_probe_faces(&atoms, [0, 1, 2], 1.0)
      .unwrap_or_else(|error| panic!("valid support should be filtered without a geometric error: {error}"));

    assert!(faces.is_empty());
  }

  #[test]
  fn collinear_support_should_return_degenerate_error() {
    let atoms = vec![
      MsmsAtom {
        atom_index: 0,
        center: DVec3::ZERO,
        radius: 1.0,
      },
      MsmsAtom {
        atom_index: 1,
        center: DVec3::X,
        radius: 1.0,
      },
      MsmsAtom {
        atom_index: 2,
        center: 2.0 * DVec3::X,
        radius: 1.0,
      },
    ];

    assert_eq!(
      tangent_probe_faces(&atoms, [0, 1, 2], 1.0),
      Err(TangentProbeError::DegenerateSupport)
    );
  }

  #[test]
  fn disjoint_expanded_spheres_should_return_no_intersection_error() {
    let mut atoms = equilateral_atoms();
    atoms[1].center = DVec3::new(10.0, 0.0, 0.0);

    assert_eq!(
      tangent_probe_faces(&atoms, [0, 1, 2], 1.0),
      Err(TangentProbeError::NoIntersection)
    );
  }

  #[test]
  fn mirrored_faces_should_point_toward_their_probe_centers() {
    let atoms = equilateral_atoms();
    let faces = tangent_probe_faces(&atoms, [0, 1, 2], 1.0)
      .unwrap_or_else(|error| panic!("equilateral support should accept a tangent probe: {error}"));

    assert_eq!(faces[0].atom_indices, [10, 20, 30]);
    assert_eq!(faces[1].atom_indices, [10, 30, 20]);
  }

  #[test]
  fn tangent_probe_solution_should_be_translation_invariant() {
    let atoms = equilateral_atoms();
    let translation = DVec3::new(1.0e9, -2.0e9, 3.0e9);
    let translated = atoms
      .iter()
      .map(|atom| MsmsAtom {
        atom_index: atom.atom_index,
        center: atom.center + translation,
        radius: atom.radius,
      })
      .collect::<Vec<_>>();
    let original_faces = tangent_probe_faces(&atoms, [0, 1, 2], 1.0)
      .unwrap_or_else(|error| panic!("original support should accept a tangent probe: {error}"));
    let translated_faces = tangent_probe_faces(&translated, [0, 1, 2], 1.0)
      .unwrap_or_else(|error| panic!("translated support should accept a tangent probe: {error}"));

    for (original, translated) in original_faces.iter().zip(&translated_faces) {
      let restored = DVec3::from_array(translated.probe_center) - translation;
      assert!(restored.distance(DVec3::from_array(original.probe_center)) < 1.0e-6);
    }
  }
}
