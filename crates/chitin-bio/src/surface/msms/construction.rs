//! Reduced-surface construction stages shared by measurement and tessellation.

use thiserror::Error;

use super::{
  MsmsParameters, ReducedSurfaceFace,
  geometry::{MsmsAtom, TangentProbeError, tangent_probe_faces},
  neighbors::{ExpandedSphereNeighborGraph, NeighborGraphError},
};

/// Failure produced while constructing analytical reduced-surface topology.
#[derive(Clone, Copy, Debug, Error, PartialEq)]
pub enum MsmsConstructionError {
  /// Input geometry could not form a conservative neighbor graph.
  #[error(transparent)]
  NeighborGraph(#[from] NeighborGraphError),
  /// A graph candidate failed for a reason other than geometric rejection.
  #[error("failed to solve tangent probe for support {support:?}: {source}")]
  TangentProbe {
    /// Local atom indices of the candidate support triplet.
    support: [usize; 3],
    /// Underlying analytical geometry failure.
    source: TangentProbeError,
  },
}

/// Builds all accessible tangent-probe faces from conservative spatial candidates.
///
/// The function constructs an expanded-sphere neighbor graph, enumerates only
/// graph triangles, solves the three-sphere intersection analytically, and
/// rejects probe centers occluded by any non-support atom. `NoIntersection`
/// and `DegenerateSupport` are expected candidate rejections rather than fatal
/// construction errors.
///
/// # Parameters
///
/// * `atoms` contains source atom indices, Cartesian centers, and van der Waals radii.
/// * `parameters` supplies the validated rolling-probe radius.
///
/// # Returns
///
/// Deterministically ordered, consistently oriented reduced-surface faces, or
/// [`MsmsConstructionError`] when input geometry is invalid.
///
/// # Examples
///
/// ```
/// use chitin_bio::surface::msms::{
///   MsmsParameters,
///   build_accessible_probe_faces,
///   geometry::MsmsAtom,
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
/// let faces = build_accessible_probe_faces(&atoms, MsmsParameters::new(1.0)?)?;
/// assert_eq!(faces.len(), 2);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn build_accessible_probe_faces(
  atoms: &[MsmsAtom],
  parameters: MsmsParameters,
) -> Result<Vec<ReducedSurfaceFace>, MsmsConstructionError> {
  let probe_radius = parameters.probe_radius();
  let graph = ExpandedSphereNeighborGraph::new(atoms, probe_radius)?;
  let mut faces = Vec::new();
  for support in graph.candidate_triplets() {
    match tangent_probe_faces(atoms, support, probe_radius) {
      Ok(mut support_faces) => faces.append(&mut support_faces),
      Err(TangentProbeError::NoIntersection | TangentProbeError::DegenerateSupport) => {}
      Err(source) => return Err(MsmsConstructionError::TangentProbe { support, source }),
    }
  }
  Ok(faces)
}

#[cfg(test)]
mod tests {
  use glam::DVec3;

  use super::*;

  fn equilateral_atoms() -> Vec<MsmsAtom> {
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
        center: DVec3::new(1.0, 3.0_f64.sqrt(), 0.0),
        radius: 1.0,
      },
    ]
  }

  fn unit_probe_parameters() -> MsmsParameters {
    MsmsParameters::new(1.0).unwrap_or_else(|error| panic!("unit probe radius should be valid: {error}"))
  }

  #[test]
  fn equilateral_atoms_should_build_two_accessible_faces() {
    let faces = build_accessible_probe_faces(&equilateral_atoms(), unit_probe_parameters())
      .unwrap_or_else(|error| panic!("equilateral atoms should construct probe faces: {error}"));

    assert_eq!(faces.len(), 2);
  }

  #[test]
  fn neighbor_graph_false_positive_should_not_become_a_face() {
    let atoms = vec![
      MsmsAtom {
        atom_index: 0,
        center: DVec3::ZERO,
        radius: 1.0,
      },
      MsmsAtom {
        atom_index: 1,
        center: DVec3::new(3.9, 0.0, 0.0),
        radius: 1.0,
      },
      MsmsAtom {
        atom_index: 2,
        center: DVec3::new(1.95, 3.3, 0.0),
        radius: 1.0,
      },
    ];
    let faces = build_accessible_probe_faces(&atoms, unit_probe_parameters())
      .unwrap_or_else(|error| panic!("pairwise candidates should be rejected without failing: {error}"));

    assert!(faces.is_empty());
  }

  #[test]
  fn invalid_atom_should_preserve_neighbor_graph_error() {
    let atoms = vec![MsmsAtom {
      atom_index: 0,
      center: DVec3::ZERO,
      radius: f64::NAN,
    }];

    assert_eq!(
      build_accessible_probe_faces(&atoms, MsmsParameters::default()),
      Err(MsmsConstructionError::NeighborGraph(NeighborGraphError::InvalidAtom {
        atom_index: 0
      }))
    );
  }
}
