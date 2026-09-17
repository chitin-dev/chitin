//! Analytical patch geometry derived from reduced-surface topology.

use std::{
  collections::{BTreeMap, BTreeSet},
  f64::consts::{PI, TAU},
};

use glam::DVec3;
use thiserror::Error;

use super::{
  ContactBoundaryArc, ContactBoundaryArcUse, ContactBoundaryLoop, ContactPatchGeometry, ContactPatchTopology,
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
  /// An edge contact curve is inconsistent with its supporting atom sphere.
  #[error("reduced-surface edge {edge_index} has invalid contact geometry for atom {atom_index}")]
  InvalidContactGeometry {
    /// Index of the source reduced-surface edge.
    edge_index: usize,
    /// Source atom carrying the invalid contact curve.
    atom_index: usize,
  },
  /// An open contact arc lacks one of its incident probe faces.
  #[error("contact arc from edge {edge_index} does not have two incident faces")]
  MissingContactIncidence {
    /// Index of the source reduced-surface edge.
    edge_index: usize,
  },
  /// A face does not have exactly two contact arcs around one atom.
  #[error("atom {atom_index} has contact-arc degree {degree} at face {face_index}, expected 2")]
  InvalidContactFaceDegree {
    /// Source atom whose boundary is invalid.
    atom_index: usize,
    /// Reduced-surface face used as the loop vertex.
    face_index: usize,
    /// Number of incident contact arcs found at the face.
    degree: usize,
  },
  /// Contact arcs could not be traversed into a closed loop.
  #[error("atom {atom_index} has an open or branching contact boundary")]
  OpenContactBoundary {
    /// Source atom whose boundary did not close.
    atom_index: usize,
  },
  /// Oriented contact loops produced an invalid exposed solid angle.
  #[error("atom {atom_index} has an invalid contact-patch solid angle")]
  InvalidContactSolidAngle {
    /// Source atom whose area could not be integrated.
    atom_index: usize,
  },
}

/// Builds exact atom-sphere contact arcs and assembles them into closed loops.
///
/// Every reduced-surface edge induces one contact curve on each supporting
/// atom. Open curves meet at their incident tangent-probe faces; complete free
/// circles are already closed loops. Atoms without any curves become complete
/// spherical patches unless their expanded sphere is contained by another
/// atom in the domain.
///
/// # Parameters
///
/// * `atoms` contains the analytical atoms in the topology domain.
/// * `edges` contains accessible reduced-surface arcs with resolved face incidence.
/// * `parameters` supplies the rolling-probe radius.
///
/// # Returns
///
/// Deterministically ordered exposed atom patches, or a structured error when
/// contact curves do not form a two-manifold boundary.
///
/// # Examples
///
/// ```
/// use chitin_bio::surface::msms::{
///   ContactPatchTopology,
///   MsmsParameters,
///   build_contact_patch_geometry,
///   geometry::MsmsAtom,
/// };
/// use glam::DVec3;
///
/// let atoms = vec![MsmsAtom {
///   atom_index: 0,
///   center: DVec3::ZERO,
///   radius: 1.7,
/// }];
/// let patches = build_contact_patch_geometry(&atoms, &[], MsmsParameters::default())?;
/// assert_eq!(patches[0].topology, ContactPatchTopology::FullSphere);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn build_contact_patch_geometry(
  atoms: &[MsmsAtom],
  edges: &[ReducedSurfaceEdge],
  parameters: MsmsParameters,
) -> Result<Vec<ContactPatchGeometry>, MsmsPatchConstructionError> {
  let atom_lookup = atom_lookup(atoms)?;
  let mut patches = Vec::new();
  for atom in atoms {
    let mut boundary_arcs = Vec::new();
    for (edge_index, edge) in edges.iter().enumerate() {
      if edge.atom_indices.contains(&atom.atom_index) {
        boundary_arcs.push(contact_boundary_arc(
          edge_index,
          edge,
          atom,
          &atom_lookup,
          parameters.probe_radius(),
        )?);
      }
    }

    if boundary_arcs.is_empty() {
      if !expanded_atom_is_contained(atom, atoms, parameters.probe_radius())
        && !expanded_atom_surface_has_occluder(atom, atoms, parameters.probe_radius())
      {
        patches.push(ContactPatchGeometry {
          atom_index: atom.atom_index,
          atom_center: atom.center.to_array(),
          atom_radius: atom.radius,
          probe_radius: parameters.probe_radius(),
          boundary_arcs,
          boundary_loops: Vec::new(),
          topology: ContactPatchTopology::FullSphere,
          solid_angle: 4.0 * PI,
        });
      }
      continue;
    }

    let boundary_loops = assemble_contact_loops(atom.atom_index, &boundary_arcs)?;
    let solid_angle = contact_solid_angle(atom.atom_index, &boundary_arcs, &boundary_loops)?;
    patches.push(ContactPatchGeometry {
      atom_index: atom.atom_index,
      atom_center: atom.center.to_array(),
      atom_radius: atom.radius,
      probe_radius: parameters.probe_radius(),
      boundary_arcs,
      boundary_loops,
      topology: ContactPatchTopology::Bounded,
      solid_angle,
    });
  }
  Ok(patches)
}

/// Projects one probe-center arc onto a supporting atom sphere.
fn contact_boundary_arc(
  edge_index: usize,
  edge: &ReducedSurfaceEdge,
  atom: &MsmsAtom,
  atom_lookup: &BTreeMap<usize, &MsmsAtom>,
  probe_radius: f64,
) -> Result<ContactBoundaryArc, MsmsPatchConstructionError> {
  validate_toroidal_frame(edge_index, edge)?;
  let center = DVec3::from_array(edge.probe_circle_center);
  let axis = DVec3::from_array(edge.probe_circle_axis);
  let center_offset = center - atom.center;
  let axial_offset = center_offset.dot(axis);
  let off_axis = center_offset - axial_offset * axis;
  let expanded_radius = atom.radius + probe_radius;
  let direction_axis_offset = axial_offset / expanded_radius;
  let direction_circle_radius = edge.probe_circle_radius / expanded_radius;
  let scale = atom.center.abs().max(center.abs()).max(DVec3::ONE).max_element();
  let linear_tolerance = 4096.0 * f64::EPSILON * scale;
  let unit_tolerance = 4096.0 * f64::EPSILON;
  if !atom.center.is_finite()
    || !atom.radius.is_finite()
    || atom.radius <= 0.0
    || off_axis.length() > linear_tolerance
    || (direction_axis_offset.mul_add(direction_axis_offset, direction_circle_radius.powi(2)) - 1.0).abs()
      > unit_tolerance
  {
    return Err(MsmsPatchConstructionError::InvalidContactGeometry {
      edge_index,
      atom_index: atom.atom_index,
    });
  }
  let other_atom_index = edge
    .atom_indices
    .iter()
    .find(|atom_index| **atom_index != atom.atom_index)
    .copied()
    .ok_or(MsmsPatchConstructionError::InvalidContactGeometry {
      edge_index,
      atom_index: atom.atom_index,
    })?;
  let other_atom = atom_lookup
    .get(&other_atom_index)
    .ok_or(MsmsPatchConstructionError::MissingEdgeAtom {
      edge_index,
      atom_index: other_atom_index,
    })?;
  let occluded_direction = (other_atom.center - atom.center).normalize_or_zero();
  if occluded_direction.length_squared() <= f64::EPSILON {
    return Err(MsmsPatchConstructionError::InvalidContactGeometry {
      edge_index,
      atom_index: atom.atom_index,
    });
  }
  let midpoint_angle = edge.start_angle + 0.5 * edge.sweep_angle;
  let basis = DVec3::from_array(edge.probe_circle_basis);
  let perpendicular_basis = axis.cross(basis);
  let midpoint_radial = midpoint_angle.cos() * basis + midpoint_angle.sin() * perpendicular_basis;
  let midpoint_tangent = -midpoint_angle.sin() * basis + midpoint_angle.cos() * perpendicular_basis;
  let midpoint_direction = direction_axis_offset * axis + direction_circle_radius * midpoint_radial;
  let probe_center = atom.center + expanded_radius * midpoint_direction;
  let exposed_gradient = probe_center - other_atom.center;
  let exposed_side = midpoint_direction.cross(midpoint_tangent).dot(exposed_gradient);
  if exposed_side.abs() <= linear_tolerance {
    return Err(MsmsPatchConstructionError::InvalidContactGeometry {
      edge_index,
      atom_index: atom.atom_index,
    });
  }
  Ok(ContactBoundaryArc {
    edge_index,
    atom_index: atom.atom_index,
    atom_center: atom.center.to_array(),
    atom_radius: atom.radius,
    axis: edge.probe_circle_axis,
    basis: edge.probe_circle_basis,
    direction_axis_offset,
    direction_circle_radius,
    start_angle: edge.start_angle,
    sweep_angle: edge.sweep_angle,
    face_indices: edge.face_indices,
    occluded_direction: occluded_direction.to_array(),
    exposed_on_left_when_forward: exposed_side > 0.0,
  })
}

/// Tests whether any neighboring expanded sphere covers part of this surface.
fn expanded_atom_surface_has_occluder(atom: &MsmsAtom, atoms: &[MsmsAtom], probe_radius: f64) -> bool {
  let radius = atom.radius + probe_radius;
  atoms.iter().any(|other| {
    if other.atom_index == atom.atom_index {
      return false;
    }
    let other_radius = other.radius + probe_radius;
    let distance = atom.center.distance(other.center);
    let scale = distance.max(radius).max(other_radius).max(1.0);
    let tolerance = 4096.0 * f64::EPSILON * scale;
    // A sphere strictly inside this atom cannot reach its surface. Any proper
    // lens intersection hides a finite cap; when no accessible intersection
    // arc survives, that cap belongs to a collectively buried atom.
    distance + other_radius > radius + tolerance && distance < radius + other_radius - tolerance
  })
}

/// Tests whether another expanded atom completely hides one expanded sphere.
fn expanded_atom_is_contained(atom: &MsmsAtom, atoms: &[MsmsAtom], probe_radius: f64) -> bool {
  let radius = atom.radius + probe_radius;
  atoms.iter().any(|other| {
    if other.atom_index == atom.atom_index {
      return false;
    }
    let other_radius = other.radius + probe_radius;
    let distance = atom.center.distance(other.center);
    let scale = distance.max(radius).max(other_radius).max(1.0);
    let tolerance = 4096.0 * f64::EPSILON * scale;
    if distance + radius < other_radius - tolerance {
      return true;
    }
    distance <= tolerance && (radius - other_radius).abs() <= tolerance && atom.atom_index > other.atom_index
  })
}

/// Connects atom-sphere contact arcs through their incident probe faces.
fn assemble_contact_loops(
  atom_index: usize,
  arcs: &[ContactBoundaryArc],
) -> Result<Vec<ContactBoundaryLoop>, MsmsPatchConstructionError> {
  let mut loops = Vec::new();
  let mut adjacency = BTreeMap::<usize, Vec<(usize, bool)>>::new();
  let mut unused = BTreeSet::new();
  for (arc_index, arc) in arcs.iter().enumerate() {
    match arc.face_indices {
      [None, None] => loops.push(ContactBoundaryLoop {
        arcs: vec![ContactBoundaryArcUse {
          arc_index,
          reversed: !arc.exposed_on_left_when_forward,
        }],
      }),
      [Some(start), Some(end)] => {
        adjacency.entry(start).or_default().push((arc_index, false));
        adjacency.entry(end).or_default().push((arc_index, true));
        unused.insert(arc_index);
      }
      _ => {
        return Err(MsmsPatchConstructionError::MissingContactIncidence {
          edge_index: arc.edge_index,
        });
      }
    }
  }
  for (&face_index, incident) in &adjacency {
    if incident.len() != 2 {
      return Err(MsmsPatchConstructionError::InvalidContactFaceDegree {
        atom_index,
        face_index,
        degree: incident.len(),
      });
    }
  }

  while let Some(first_arc_index) = unused.pop_first() {
    let first_arc = &arcs[first_arc_index];
    let [Some(start_face), Some(mut current_face)] = first_arc.face_indices else {
      return Err(MsmsPatchConstructionError::OpenContactBoundary { atom_index });
    };
    let mut directed_arcs = vec![ContactBoundaryArcUse {
      arc_index: first_arc_index,
      reversed: false,
    }];
    while current_face != start_face {
      let Some((next_arc_index, arrives_at_end)) = adjacency
        .get(&current_face)
        .and_then(|incident| incident.iter().find(|(arc_index, _)| unused.contains(arc_index)))
        .copied()
      else {
        return Err(MsmsPatchConstructionError::OpenContactBoundary { atom_index });
      };
      unused.remove(&next_arc_index);
      directed_arcs.push(ContactBoundaryArcUse {
        arc_index: next_arc_index,
        reversed: arrives_at_end,
      });
      let next_faces = arcs[next_arc_index].face_indices;
      current_face = if arrives_at_end { next_faces[0] } else { next_faces[1] }
        .ok_or(MsmsPatchConstructionError::OpenContactBoundary { atom_index })?;
    }
    let mut boundary_loop = ContactBoundaryLoop { arcs: directed_arcs };
    orient_contact_loop(atom_index, &mut boundary_loop, arcs)?;
    loops.push(boundary_loop);
  }
  Ok(loops)
}

/// Reverses a loop when its current traversal keeps the occluded side on the left.
fn orient_contact_loop(
  atom_index: usize,
  boundary_loop: &mut ContactBoundaryLoop,
  arcs: &[ContactBoundaryArc],
) -> Result<(), MsmsPatchConstructionError> {
  let first_use = boundary_loop
    .arcs
    .first()
    .ok_or(MsmsPatchConstructionError::OpenContactBoundary { atom_index })?;
  let first_arc = &arcs[first_use.arc_index];
  let exposed_on_left = first_arc.exposed_on_left_when_forward != first_use.reversed;
  if !exposed_on_left {
    boundary_loop.arcs.reverse();
    for arc_use in &mut boundary_loop.arcs {
      arc_use.reversed = !arc_use.reversed;
    }
  }
  Ok(())
}

/// Integrates exposed unit-sphere area from oriented small-circle loops.
fn contact_solid_angle(
  atom_index: usize,
  arcs: &[ContactBoundaryArc],
  loops: &[ContactBoundaryLoop],
) -> Result<f64, MsmsPatchConstructionError> {
  let mut solid_angle = 0.0;
  for boundary_loop in loops {
    solid_angle += contact_loop_solid_angle(atom_index, arcs, boundary_loop)?;
  }
  let tolerance = 8192.0 * f64::EPSILON * 4.0 * PI;
  if !solid_angle.is_finite() || solid_angle <= tolerance || solid_angle > 4.0 * PI + tolerance {
    return Err(MsmsPatchConstructionError::InvalidContactSolidAngle { atom_index });
  }
  Ok(solid_angle.min(4.0 * PI))
}

/// Applies Gauss-Bonnet to one exposed-side-oriented contact loop.
fn contact_loop_solid_angle(
  atom_index: usize,
  arcs: &[ContactBoundaryArc],
  boundary_loop: &ContactBoundaryLoop,
) -> Result<f64, MsmsPatchConstructionError> {
  if boundary_loop.arcs.is_empty() {
    return Err(MsmsPatchConstructionError::OpenContactBoundary { atom_index });
  }
  let geodesic_curvature = boundary_loop
    .arcs
    .iter()
    .map(|arc_use| {
      let arc = &arcs[arc_use.arc_index];
      let signed_sweep = if arc_use.reversed {
        -arc.sweep_angle
      } else {
        arc.sweep_angle
      };
      arc.direction_axis_offset * signed_sweep
    })
    .sum::<f64>();
  let mut turning_angle = 0.0;
  for index in 0..boundary_loop.arcs.len() {
    let incoming = boundary_loop.arcs[index];
    let outgoing = boundary_loop.arcs[(index + 1) % boundary_loop.arcs.len()];
    let (incoming_direction, incoming_tangent) = contact_arc_endpoint(&arcs[incoming.arc_index], incoming, false);
    let (outgoing_direction, outgoing_tangent) = contact_arc_endpoint(&arcs[outgoing.arc_index], outgoing, true);
    if incoming_direction.distance(outgoing_direction) > 1.0e-9 {
      return Err(MsmsPatchConstructionError::OpenContactBoundary { atom_index });
    }
    let vertex_direction = (incoming_direction + outgoing_direction).normalize_or_zero();
    if vertex_direction.length_squared() <= f64::EPSILON {
      return Err(MsmsPatchConstructionError::InvalidContactSolidAngle { atom_index });
    }
    turning_angle += vertex_direction
      .dot(incoming_tangent.cross(outgoing_tangent))
      .atan2(incoming_tangent.dot(outgoing_tangent));
  }
  let area = (TAU - turning_angle - geodesic_curvature).rem_euclid(4.0 * PI);
  if !area.is_finite() {
    return Err(MsmsPatchConstructionError::InvalidContactSolidAngle { atom_index });
  }
  Ok(area)
}

/// Evaluates the direction and traversal tangent at one directed arc endpoint.
fn contact_arc_endpoint(arc: &ContactBoundaryArc, arc_use: ContactBoundaryArcUse, at_start: bool) -> (DVec3, DVec3) {
  let stored_start = at_start != arc_use.reversed;
  let angle_offset = if stored_start { 0.0 } else { arc.sweep_angle };
  let axis = DVec3::from_array(arc.axis);
  let basis = DVec3::from_array(arc.basis);
  let perpendicular_basis = axis.cross(basis);
  let angle = arc.start_angle + angle_offset;
  let radial = angle.cos() * basis + angle.sin() * perpendicular_basis;
  let forward_tangent = -angle.sin() * basis + angle.cos() * perpendicular_basis;
  let tangent = if arc_use.reversed {
    -forward_tangent
  } else {
    forward_tangent
  };
  (
    arc.direction_axis_offset * axis + arc.direction_circle_radius * radial,
    tangent,
  )
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

  fn synthetic_contact_arc(edge_index: usize, faces: [usize; 2]) -> ContactBoundaryArc {
    ContactBoundaryArc {
      edge_index,
      atom_index: 10,
      atom_center: [0.0; 3],
      atom_radius: 1.0,
      axis: [1.0, 0.0, 0.0],
      basis: [0.0, 1.0, 0.0],
      direction_axis_offset: 0.0,
      direction_circle_radius: 1.0,
      start_angle: 0.0,
      sweep_angle: PI,
      face_indices: [Some(faces[0]), Some(faces[1])],
      occluded_direction: [1.0, 0.0, 0.0],
      exposed_on_left_when_forward: true,
    }
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

  #[test]
  fn isolated_atom_should_produce_a_full_contact_sphere() {
    let atoms = vec![MsmsAtom {
      atom_index: 10,
      center: DVec3::ZERO,
      radius: 1.7,
    }];
    let patches = build_contact_patch_geometry(&atoms, &[], MsmsParameters::default())
      .unwrap_or_else(|error| panic!("isolated atom should produce a contact patch: {error}"));

    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].topology, ContactPatchTopology::FullSphere);
    assert!(patches[0].boundary_arcs.is_empty());
    assert!((patches[0].solid_angle - 4.0 * PI).abs() < 1.0e-12);
  }

  #[test]
  fn contained_atom_should_not_produce_a_contact_patch() {
    let atoms = vec![
      MsmsAtom {
        atom_index: 10,
        center: DVec3::ZERO,
        radius: 2.0,
      },
      MsmsAtom {
        atom_index: 20,
        center: DVec3::ZERO,
        radius: 1.0,
      },
    ];
    let patches = build_contact_patch_geometry(&atoms, &[], MsmsParameters::default())
      .unwrap_or_else(|error| panic!("contained atom filtering should succeed: {error}"));

    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].atom_index, 10);
  }

  #[test]
  fn proper_expanded_sphere_intersection_should_occlude_surface_area() {
    let atoms = equal_pair(2.0);

    assert!(expanded_atom_surface_has_occluder(&atoms[0], &atoms, 1.0));
    assert!(expanded_atom_surface_has_occluder(&atoms[1], &atoms, 1.0));
  }

  #[test]
  fn nested_smaller_sphere_should_not_occlude_outer_surface() {
    let atoms = vec![
      MsmsAtom {
        atom_index: 10,
        center: DVec3::ZERO,
        radius: 2.0,
      },
      MsmsAtom {
        atom_index: 20,
        center: DVec3::ZERO,
        radius: 1.0,
      },
    ];

    assert!(!expanded_atom_surface_has_occluder(&atoms[0], &atoms, 1.0));
  }

  #[test]
  fn free_probe_circle_should_bound_both_atom_contact_patches() {
    let atoms = equal_pair(2.0);
    let parameters =
      MsmsParameters::new(1.0).unwrap_or_else(|error| panic!("unit probe radius should be valid: {error}"));
    let edges = build_accessible_probe_edges(&atoms, parameters)
      .unwrap_or_else(|error| panic!("intersecting pair should produce a free edge: {error}"));
    let patches = build_contact_patch_geometry(&atoms, &edges, parameters)
      .unwrap_or_else(|error| panic!("free edge should produce atom contact boundaries: {error}"));

    assert_eq!(patches.len(), 2);
    assert!(patches.iter().all(|patch| {
      patch.topology == ContactPatchTopology::Bounded
        && patch.boundary_arcs.len() == 1
        && patch.boundary_loops.len() == 1
        && patch.boundary_loops[0].arcs.len() == 1
    }));
    assert!(patches.iter().flat_map(|patch| &patch.boundary_arcs).all(|arc| {
      let direction = DVec3::from_array(arc.direction(0.37));
      let position = DVec3::from_array(arc.position(0.37));
      (direction.length() - 1.0).abs() < 1.0e-12
        && (position.distance(DVec3::from_array(arc.atom_center)) - arc.atom_radius).abs() < 1.0e-12
    }));
    assert!(
      patches
        .iter()
        .all(|patch| (patch.solid_angle - 3.0 * PI).abs() < 1.0e-12)
    );
  }

  #[test]
  fn incident_contact_arcs_should_form_a_closed_loop() {
    let arcs = [synthetic_contact_arc(0, [1, 2]), synthetic_contact_arc(1, [2, 1])];
    let loops =
      assemble_contact_loops(10, &arcs).unwrap_or_else(|error| panic!("degree-two contact arcs should close: {error}"));

    assert_eq!(loops.len(), 1);
    assert_eq!(loops[0].arcs.len(), 2);
    assert!(!loops[0].arcs[0].reversed);
    assert!(!loops[0].arcs[1].reversed);
  }
}
