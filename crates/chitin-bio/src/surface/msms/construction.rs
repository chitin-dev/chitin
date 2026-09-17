//! Reduced-surface construction stages shared by measurement and tessellation.

use std::{
  collections::{BTreeMap, BTreeSet},
  f64::consts::TAU,
};

use glam::DVec3;
use thiserror::Error;

use crate::structure::StructureScene;
use crate::surface::atoms::{SurfaceAtom, surface_atom_groups};

use super::{
  MsmsParameters, MsmsPatchConstructionError, MsmsProbeFaceDomain, MsmsProbeTopologyComponent, MsmsProbeTopologyDomain,
  MsmsRequest, MsmsRollingPatchDomain, ProbeArcEndpoint, ReducedSurfaceEdge, ReducedSurfaceFace,
  arcs::{ProbeArcError, accessible_probe_arcs},
  geometry::{MsmsAtom, TangentProbeError, tangent_probe_faces},
  neighbors::{ExpandedSphereNeighborGraph, NeighborGraphError},
  patches::{build_reentrant_patches, build_toroidal_patch_geometry},
};

/// Builds rolling toroidal and reentrant patch geometry for every selected domain.
///
/// This structure-level entry point shares atom preparation and one neighbor
/// graph with reduced-surface construction, then derives renderable analytical
/// geometry from its edges and faces. It deliberately excludes contact patches
/// and does not trim singular tori, so callers must not present the result as a
/// complete MSMS molecular surface.
///
/// # Parameters
///
/// * `scene` supplies source atom identities, coordinates, and classifications.
/// * `request` selects atoms, domain partitioning, and probe radius.
///
/// # Returns
///
/// Deterministically ordered rolling-patch domains, or
/// [`MsmsConstructionError`] when topology or patch geometry is invalid.
///
/// # Examples
///
/// ```
/// use chitin_bio::{
///   structure::{PdbParser, StructureScene},
///   surface::msms::{MsmsRequest, build_msms_rolling_patch_geometry},
/// };
///
/// let parsed = PdbParser::new().parse_bytes(
///   b"ATOM      1  C   GLY A   1       0.000   0.000   0.000  1.00 10.00           C  \n\
/// ATOM      2  CA  GLY A   1       2.000   0.000   0.000  1.00 10.00           C  \n\
/// END\n",
/// )?;
/// let scene = StructureScene::from_first_model(&parsed.structure)?;
/// let domains = build_msms_rolling_patch_geometry(&scene, MsmsRequest::default())?;
/// assert_eq!(domains.len(), 1);
/// assert_eq!(domains[0].toroidal_patches.len(), 1);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn build_msms_rolling_patch_geometry(
  scene: &StructureScene,
  request: MsmsRequest,
) -> Result<Vec<MsmsRollingPatchDomain>, MsmsConstructionError> {
  surface_atom_groups(scene, request.atom_scope, request.partition)
    .into_iter()
    .map(|(chain_id, atoms)| {
      let analytical_atoms = atoms.iter().map(msms_atom).collect::<Vec<_>>();
      let topology = build_probe_topology(&analytical_atoms, request.parameters)?;
      let toroidal_patches = build_toroidal_patch_geometry(&analytical_atoms, &topology.edges, request.parameters)?;
      let reentrant_patches = build_reentrant_patches(&analytical_atoms, &topology.faces, request.parameters)?;
      Ok(MsmsRollingPatchDomain {
        topology: MsmsProbeTopologyDomain {
          chain_id,
          edges: topology.edges,
          faces: topology.faces,
          components: topology.components,
        },
        toroidal_patches,
        reentrant_patches,
      })
    })
    .collect()
}

/// Builds accessible probe arcs and faces for every selected structure domain.
///
/// Atom preparation is shared with the implicit backend. Within each domain,
/// one conservative neighbor graph supplies both pair and triplet candidates,
/// avoiding duplicate spatial-index construction. This remains a probe
/// topology result rather than a complete [`super::ReducedSurface`]: exposed
/// isolated vertices and analytical patches are not yet assembled.
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
      let topology = build_probe_topology(&analytical_atoms, request.parameters)?;
      Ok(MsmsProbeTopologyDomain {
        chain_id,
        edges: topology.edges,
        faces: topology.faces,
        components: topology.components,
      })
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

/// Intermediate topology assembled for one analytical calculation domain.
struct ProbeTopology {
  edges: Vec<ReducedSurfaceEdge>,
  faces: Vec<ReducedSurfaceFace>,
  components: Vec<MsmsProbeTopologyComponent>,
}

/// Builds pair arcs and triple faces from one shared candidate graph.
fn build_probe_topology(
  atoms: &[MsmsAtom],
  parameters: MsmsParameters,
) -> Result<ProbeTopology, MsmsConstructionError> {
  let probe_radius = parameters.probe_radius();
  let graph = ExpandedSphereNeighborGraph::new(atoms, probe_radius)?;
  let mut edges = probe_edges_from_graph(atoms, probe_radius, &graph)?;
  let faces = probe_faces_from_graph(atoms, probe_radius, &graph)?;
  attach_incident_faces(&mut edges, &faces)?;
  let components = probe_topology_components(&edges, &faces);
  Ok(ProbeTopology {
    edges,
    faces,
    components,
  })
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
  /// Reduced-surface topology could not be converted into analytical patches.
  #[error(transparent)]
  Patch(#[from] MsmsPatchConstructionError),
  /// An open probe arc endpoint has no matching tangent-probe face.
  #[error("probe arc {edge_index} {endpoint:?} endpoint has no incident face")]
  MissingIncidentFace {
    /// Index of the edge in its topology domain.
    edge_index: usize,
    /// Arc endpoint that could not be resolved.
    endpoint: ProbeArcEndpoint,
  },
  /// An open probe arc endpoint matches more than one tangent-probe face.
  #[error("probe arc {edge_index} {endpoint:?} endpoint has {match_count} incident-face matches")]
  AmbiguousIncidentFace {
    /// Index of the edge in its topology domain.
    edge_index: usize,
    /// Arc endpoint with ambiguous incidence.
    endpoint: ProbeArcEndpoint,
    /// Number of geometrically coincident face candidates.
    match_count: usize,
  },
}

/// Resolves each finite probe arc endpoint to its tangent-probe face.
///
/// Candidate faces are first restricted to triples containing both supporting
/// atoms. Their analytical probe centers are then compared with the endpoint
/// evaluated from the edge's circle parameterization. Full free circles have
/// no endpoints and retain `[None, None]` incidence.
///
/// # Parameters
///
/// * `edges` contains accessible arcs whose incidence is updated in place.
/// * `faces` contains accessible tangent-probe positions in the same domain.
///
/// # Returns
///
/// `Ok(())` when every open endpoint has one face, or a structured topology
/// error when an endpoint is missing or geometrically ambiguous.
fn attach_incident_faces(
  edges: &mut [ReducedSurfaceEdge],
  faces: &[ReducedSurfaceFace],
) -> Result<(), MsmsConstructionError> {
  for (edge_index, edge) in edges.iter_mut().enumerate() {
    if edge_is_free_circle(edge) {
      edge.face_indices = [None, None];
      continue;
    }

    let endpoints = [
      (ProbeArcEndpoint::Start, edge.start_angle),
      (ProbeArcEndpoint::End, edge.start_angle + edge.sweep_angle),
    ];
    for (slot, (endpoint, angle)) in endpoints.into_iter().enumerate() {
      let position = probe_arc_position(edge, angle);
      let matching_faces = faces
        .iter()
        .enumerate()
        .filter(|(_, face)| {
          edge
            .atom_indices
            .iter()
            .all(|atom_index| face.atom_indices.contains(atom_index))
        })
        .filter(|(_, face)| endpoint_matches_face(position, face))
        .map(|(face_index, _)| face_index)
        .collect::<Vec<_>>();
      edge.face_indices[slot] = match matching_faces.as_slice() {
        [face_index] => Some(*face_index),
        [] => {
          return Err(MsmsConstructionError::MissingIncidentFace { edge_index, endpoint });
        }
        matches => {
          return Err(MsmsConstructionError::AmbiguousIncidentFace {
            edge_index,
            endpoint,
            match_count: matches.len(),
          });
        }
      };
    }
  }
  Ok(())
}

/// Returns whether an edge represents a closed probe-center circle.
fn edge_is_free_circle(edge: &ReducedSurfaceEdge) -> bool {
  let angular_tolerance = 512.0 * f64::EPSILON * TAU;
  (edge.sweep_angle - TAU).abs() <= angular_tolerance
}

/// Evaluates one probe-center position on an edge circle.
fn probe_arc_position(edge: &ReducedSurfaceEdge, angle: f64) -> DVec3 {
  let center = DVec3::from_array(edge.probe_circle_center);
  let axis = DVec3::from_array(edge.probe_circle_axis);
  let basis = DVec3::from_array(edge.probe_circle_basis);
  let perpendicular_basis = axis.cross(basis);
  center + edge.probe_circle_radius * (angle.cos() * basis + angle.sin() * perpendicular_basis)
}

/// Tests endpoint/face coincidence with a scale-relative roundoff tolerance.
fn endpoint_matches_face(endpoint: DVec3, face: &ReducedSurfaceFace) -> bool {
  let face_center = DVec3::from_array(face.probe_center);
  let scale = endpoint.abs().max(face_center.abs()).max(DVec3::ONE).max_element();
  endpoint.distance(face_center) <= 4096.0 * f64::EPSILON * scale
}

/// Groups probe topology by connectivity through shared source atoms.
///
/// # Parameters
///
/// * `edges` contains the domain's accessible pair arcs.
/// * `faces` contains the domain's accessible tangent-probe faces.
///
/// # Returns
///
/// Deterministically ordered components. Atoms without an accessible edge or
/// face are intentionally absent because isolated-vertex discovery is a later
/// reduced-surface construction stage.
fn probe_topology_components(
  edges: &[ReducedSurfaceEdge],
  faces: &[ReducedSurfaceFace],
) -> Vec<MsmsProbeTopologyComponent> {
  let mut adjacency = BTreeMap::<usize, BTreeSet<usize>>::new();
  for edge in edges {
    connect_support_atoms(&mut adjacency, &edge.atom_indices);
  }
  for face in faces {
    connect_support_atoms(&mut adjacency, &face.atom_indices);
  }

  let mut unvisited = adjacency.keys().copied().collect::<BTreeSet<_>>();
  let mut components = Vec::new();
  while let Some(start) = unvisited.pop_first() {
    let mut pending = vec![start];
    let mut atom_indices = BTreeSet::new();
    while let Some(atom_index) = pending.pop() {
      if !atom_indices.insert(atom_index) {
        continue;
      }
      unvisited.remove(&atom_index);
      if let Some(neighbors) = adjacency.get(&atom_index) {
        pending.extend(
          neighbors
            .iter()
            .copied()
            .filter(|neighbor| !atom_indices.contains(neighbor)),
        );
      }
    }

    let edge_indices = edges
      .iter()
      .enumerate()
      .filter(|(_, edge)| {
        edge
          .atom_indices
          .iter()
          .any(|atom_index| atom_indices.contains(atom_index))
      })
      .map(|(index, _)| index)
      .collect();
    let face_indices = faces
      .iter()
      .enumerate()
      .filter(|(_, face)| {
        face
          .atom_indices
          .iter()
          .any(|atom_index| atom_indices.contains(atom_index))
      })
      .map(|(index, _)| index)
      .collect();
    components.push(MsmsProbeTopologyComponent {
      atom_indices: atom_indices.into_iter().collect(),
      edge_indices,
      face_indices,
    });
  }
  components
}

/// Adds a complete support clique to an atom-connectivity graph.
fn connect_support_atoms<const N: usize>(adjacency: &mut BTreeMap<usize, BTreeSet<usize>>, support: &[usize; N]) {
  for &atom_index in support {
    let neighbors = adjacency.entry(atom_index).or_default();
    neighbors.extend(support.iter().copied().filter(|neighbor| *neighbor != atom_index));
  }
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
    assert_eq!(edges[0].face_indices, [None, None]);
  }

  #[test]
  fn equilateral_topology_should_attach_each_arc_to_both_probe_faces() {
    let topology = build_probe_topology(&equilateral_atoms(), unit_probe_parameters())
      .unwrap_or_else(|error| panic!("equilateral topology should be complete: {error}"));

    assert_eq!(topology.edges.len(), 3);
    assert_eq!(topology.faces.len(), 2);
    assert!(topology.edges.iter().all(|edge| {
      edge.face_indices[0].is_some() && edge.face_indices[1].is_some() && edge.face_indices[0] != edge.face_indices[1]
    }));
    assert_eq!(topology.components.len(), 1);
    assert_eq!(topology.components[0].atom_indices, [10, 20, 30]);
    assert_eq!(topology.components[0].edge_indices, [0, 1, 2]);
    assert_eq!(topology.components[0].face_indices, [0, 1]);
  }

  #[test]
  fn disconnected_free_edges_should_form_separate_components() {
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
      MsmsAtom {
        atom_index: 30,
        center: 10.0 * DVec3::X,
        radius: 1.0,
      },
      MsmsAtom {
        atom_index: 40,
        center: 12.0 * DVec3::X,
        radius: 1.0,
      },
    ];
    let topology = build_probe_topology(&atoms, unit_probe_parameters())
      .unwrap_or_else(|error| panic!("disconnected pairs should construct topology: {error}"));

    assert_eq!(topology.edges.len(), 2);
    assert!(topology.faces.is_empty());
    assert_eq!(topology.components.len(), 2);
    assert_eq!(topology.components[0].atom_indices, [10, 20]);
    assert_eq!(topology.components[1].atom_indices, [30, 40]);
  }
}
