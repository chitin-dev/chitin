//! Reduced-surface construction stages shared by measurement and tessellation.

use thiserror::Error;

use crate::structure::StructureScene;
use crate::surface::atoms::{SurfaceAtom, surface_atom_groups};

use super::{
  MsmsParameters, MsmsProbeFaceDomain, MsmsProbeTopologyDomain, MsmsRequest, ReducedSurfaceEdge, ReducedSurfaceFace,
  arcs::{ProbeArcError, accessible_probe_arcs},
  geometry::{MsmsAtom, TangentProbeError, tangent_probe_faces},
  neighbors::{ExpandedSphereNeighborGraph, NeighborGraphError},
};

/// Builds accessible probe arcs and faces for every selected structure domain.
///
/// Atom preparation is shared with the implicit backend. Within each domain,
/// one conservative neighbor graph supplies both pair and triplet candidates,
/// avoiding duplicate spatial-index construction. This remains a probe
/// topology result rather than a complete [`super::ReducedSurface`]: exposed
/// vertices, edge-to-face incidence, and connected components are not yet
/// assembled.
///
/// # Parameters
///
/// * `scene` supplies source atom identities, coordinates, and classifications.
/// * `request` selects atoms, domain partitioning, and probe radius.
///
/// # Returns
///
/// Deterministically ordered topology domains, or [`MsmsConstructionError`]
/// when selected geometry is invalid.
///
/// # Examples
///
/// ```
/// use chitin_bio::{
///   structure::{PdbParser, StructureScene},
///   surface::msms::{MsmsRequest, build_msms_probe_topology},
/// };
///
/// let parsed = PdbParser::new().parse_bytes(
///   b"ATOM      1  C   GLY A   1       0.000   0.000   0.000  1.00 10.00           C  \n\
/// ATOM      2  CA  GLY A   1       2.000   0.000   0.000  1.00 10.00           C  \n\
/// END\n",
/// )?;
/// let scene = StructureScene::from_first_model(&parsed.structure)?;
/// let domains = build_msms_probe_topology(&scene, MsmsRequest::default())?;
/// assert_eq!(domains.len(), 1);
/// assert_eq!(domains[0].edges.len(), 1);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn build_msms_probe_topology(
  scene: &StructureScene,
  request: MsmsRequest,
) -> Result<Vec<MsmsProbeTopologyDomain>, MsmsConstructionError> {
  surface_atom_groups(scene, request.atom_scope, request.partition)
    .into_iter()
    .map(|(chain_id, atoms)| {
      let analytical_atoms = atoms.iter().map(msms_atom).collect::<Vec<_>>();
      let (edges, faces) = build_probe_topology(&analytical_atoms, request.parameters)?;
      Ok(MsmsProbeTopologyDomain { chain_id, edges, faces })
    })
    .collect()
}

/// Discovers accessible tangent-probe faces from a renderer-neutral structure scene.
///
/// Atom selection, solvent removal, van der Waals radii, and chain partitioning
/// are shared with the implicit surface backend. Each resolved domain is then
/// converted to double-precision analytical spheres and processed independently.
/// This compatibility function intentionally returns only probe faces; use
/// [`build_msms_probe_topology`] when pair arcs are also required. Isolated
/// vertices, connected components, and analytical patches remain later stages.
///
/// # Parameters
///
/// * `scene` supplies source atom identities, coordinates, and classifications.
/// * `request` selects atoms, domain partitioning, and probe radius.
///
/// # Returns
///
/// Deterministically ordered face domains, or [`MsmsConstructionError`] when
/// selected geometry is invalid.
///
/// # Examples
///
/// ```
/// use chitin_bio::{
///   structure::{PdbParser, StructureScene},
///   surface::msms::{MsmsRequest, build_msms_probe_faces},
/// };
///
/// let pdb = b"\
/// ATOM      1  C   GLY A   1       0.000   0.000   0.000  1.00 10.00           C  \n\
/// ATOM      2  CA  GLY A   1       2.000   0.000   0.000  1.00 10.00           C  \n\
/// ATOM      3  CB  GLY A   1       1.000   1.732   0.000  1.00 10.00           C  \n\
/// END\n";
/// let parsed = PdbParser::new().parse_bytes(pdb)?;
/// let scene = StructureScene::from_first_model(&parsed.structure)?;
/// let domains = build_msms_probe_faces(&scene, MsmsRequest::default())?;
/// assert_eq!(domains.len(), 1);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn build_msms_probe_faces(
  scene: &StructureScene,
  request: MsmsRequest,
) -> Result<Vec<MsmsProbeFaceDomain>, MsmsConstructionError> {
  surface_atom_groups(scene, request.atom_scope, request.partition)
    .into_iter()
    .map(|(chain_id, atoms)| {
      let analytical_atoms = atoms.iter().map(msms_atom).collect::<Vec<_>>();
      build_accessible_probe_faces(&analytical_atoms, request.parameters)
        .map(|faces| MsmsProbeFaceDomain { chain_id, faces })
    })
    .collect()
}

/// Builds pair arcs and triple faces from one shared candidate graph.
fn build_probe_topology(
  atoms: &[MsmsAtom],
  parameters: MsmsParameters,
) -> Result<(Vec<ReducedSurfaceEdge>, Vec<ReducedSurfaceFace>), MsmsConstructionError> {
  let probe_radius = parameters.probe_radius();
  let graph = ExpandedSphereNeighborGraph::new(atoms, probe_radius)?;
  let edges = probe_edges_from_graph(atoms, probe_radius, &graph)?;
  let faces = probe_faces_from_graph(atoms, probe_radius, &graph)?;
  Ok((edges, faces))
}

/// Converts one shared surface atom to double-precision MSMS geometry.
fn msms_atom(atom: &SurfaceAtom) -> MsmsAtom {
  MsmsAtom {
    atom_index: atom.atom_index,
    center: atom.position,
    radius: atom.radius,
  }
}

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
  /// An atom-pair candidate could not be converted into accessible probe arcs.
  #[error("failed to construct probe arcs for support {support:?}: {source}")]
  ProbeArc {
    /// Local atom indices of the candidate support pair.
    support: [usize; 2],
    /// Underlying probe-circle geometry failure.
    source: ProbeArcError,
  },
}

/// Builds all accessible atom-pair probe arcs from conservative spatial candidates.
///
/// Each candidate pair is converted to its expanded-sphere intersection circle.
/// Occlusion intervals contributed by all non-support atoms are merged, and
/// their circular complement becomes one or more reduced-surface edges. A full
/// unobstructed circle is retained as a free edge with a $2\pi$ sweep.
///
/// # Parameters
///
/// * `atoms` contains source atom indices, Cartesian centers, and van der Waals radii.
/// * `parameters` supplies the validated rolling-probe radius.
///
/// # Returns
///
/// Deterministically ordered accessible probe arcs, or
/// [`MsmsConstructionError`] when input geometry is invalid.
///
/// # Examples
///
/// ```
/// use chitin_bio::surface::msms::{
///   MsmsParameters,
///   build_accessible_probe_edges,
///   geometry::MsmsAtom,
/// };
/// use glam::DVec3;
///
/// let atoms = vec![
///   MsmsAtom { atom_index: 0, center: DVec3::ZERO, radius: 1.0 },
///   MsmsAtom { atom_index: 1, center: DVec3::new(2.0, 0.0, 0.0), radius: 1.0 },
/// ];
/// let edges = build_accessible_probe_edges(&atoms, MsmsParameters::new(1.0)?)?;
/// assert_eq!(edges.len(), 1);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn build_accessible_probe_edges(
  atoms: &[MsmsAtom],
  parameters: MsmsParameters,
) -> Result<Vec<ReducedSurfaceEdge>, MsmsConstructionError> {
  let probe_radius = parameters.probe_radius();
  let graph = ExpandedSphereNeighborGraph::new(atoms, probe_radius)?;
  probe_edges_from_graph(atoms, probe_radius, &graph)
}

/// Resolves candidate pairs from an already validated neighbor graph.
fn probe_edges_from_graph(
  atoms: &[MsmsAtom],
  probe_radius: f64,
  graph: &ExpandedSphereNeighborGraph,
) -> Result<Vec<ReducedSurfaceEdge>, MsmsConstructionError> {
  let mut edges = Vec::new();
  for support in graph.candidate_pairs() {
    match accessible_probe_arcs(atoms, support, probe_radius) {
      Ok(mut support_edges) => edges.append(&mut support_edges),
      Err(source) => return Err(MsmsConstructionError::ProbeArc { support, source }),
    }
  }
  Ok(edges)
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
  probe_faces_from_graph(atoms, probe_radius, &graph)
}

/// Resolves candidate triplets from an already validated neighbor graph.
fn probe_faces_from_graph(
  atoms: &[MsmsAtom],
  probe_radius: f64,
  graph: &ExpandedSphereNeighborGraph,
) -> Result<Vec<ReducedSurfaceFace>, MsmsConstructionError> {
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

  #[test]
  fn two_intersecting_atoms_should_build_one_free_edge() {
    let atoms = vec![
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
    ];
    let edges = build_accessible_probe_edges(&atoms, unit_probe_parameters())
      .unwrap_or_else(|error| panic!("intersecting atoms should construct a free edge: {error}"));

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].atom_indices, [10, 20]);
  }
}
