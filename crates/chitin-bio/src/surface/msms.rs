//! Analytical reduced-surface model for MSMS-style SAS/SES computation.
//!
//! MSMS represents a molecular surface with atom-contact spherical patches,
//! rolling-probe toroidal patches, and fixed-probe reentrant spherical patches.
//! Area measurement belongs to this analytical representation. A renderer may
//! independently tessellate the same patches into [`super::SurfaceMesh`].
//!
//! The reduced-surface construction and singularity trimming are intentionally
//! kept independent from the implicit rendering backend. This prevents grid
//! spacing, marching-tetrahedra topology, or mesh smoothing from changing
//! reported scientific areas.

pub mod arcs;
mod construction;
pub mod geometry;
pub mod neighbors;
pub mod patches;
pub mod tessellation;

pub use construction::{
  MsmsConstructionError, MsmsSurfaceGenerationError, build_accessible_probe_edges, build_accessible_probe_faces,
  build_msms_patch_geometry, build_msms_probe_faces, build_msms_probe_topology, generate_msms_surface,
};
pub use patches::{
  MsmsPatchConstructionError, build_contact_patch_geometry, build_reentrant_patches, build_toroidal_patch_geometry,
};
pub use tessellation::{
  MsmsTessellationError, MsmsTessellationParameters, tessellate_contact_patch, tessellate_msms_patch_domain,
  tessellate_reentrant_patch, tessellate_regular_toroidal_patch, tessellate_resolved_reentrant_patches,
  tessellate_toroidal_patch,
};

use std::f64::consts::PI;

use thiserror::Error;

use super::{SurfaceAtomScope, SurfacePartition};
use crate::structure::ChainId;

/// Validated physical parameters shared by MSMS measurement and tessellation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MsmsParameters {
  probe_radius: f64,
}

impl MsmsParameters {
  /// Creates analytical molecular-surface parameters.
  ///
  /// # Parameters
  ///
  /// * `probe_radius` is the rolling solvent-probe radius in ångströms.
  ///
  /// # Returns
  ///
  /// Validated parameters, or [`MsmsParameterError`] when the radius is not
  /// finite and strictly positive.
  ///
  /// # Examples
  ///
  /// ```
  /// use chitin_bio::surface::msms::MsmsParameters;
  ///
  /// let parameters = MsmsParameters::new(1.4)?;
  /// assert_eq!(parameters.probe_radius(), 1.4);
  /// # Ok::<(), chitin_bio::surface::msms::MsmsParameterError>(())
  /// ```
  pub fn new(probe_radius: f64) -> Result<Self, MsmsParameterError> {
    if !probe_radius.is_finite() || probe_radius <= 0.0 {
      return Err(MsmsParameterError::InvalidProbeRadius(probe_radius));
    }
    Ok(Self { probe_radius })
  }

  /// Returns the rolling solvent-probe radius in ångströms.
  pub const fn probe_radius(self) -> f64 {
    self.probe_radius
  }
}

impl Default for MsmsParameters {
  fn default() -> Self {
    Self { probe_radius: 1.4 }
  }
}

/// Request selecting atoms, domains, and probe geometry for MSMS construction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MsmsRequest {
  /// Atoms eligible to participate in analytical surface construction.
  pub atom_scope: SurfaceAtomScope,
  /// How selected atoms are split into independent analytical domains.
  pub partition: SurfacePartition,
  /// Validated rolling-probe parameters.
  pub parameters: MsmsParameters,
}

impl Default for MsmsRequest {
  fn default() -> Self {
    Self {
      atom_scope: SurfaceAtomScope::default(),
      // A molecular surface is defined by solvent accessibility against the
      // complete selected assembly. Per-chain domains remain an explicit
      // visualization/analysis option because they ignore inter-chain
      // occlusion and may overlap at biological interfaces.
      partition: SurfacePartition::Unified,
      parameters: MsmsParameters::default(),
    }
  }
}

/// Accessible probe faces discovered for one independently calculated domain.
#[derive(Clone, Debug, PartialEq)]
pub struct MsmsProbeFaceDomain {
  /// Chain identity for a per-chain domain, or `None` for a unified domain.
  pub chain_id: Option<ChainId>,
  /// Accessible, consistently oriented tangent-probe faces.
  pub faces: Vec<ReducedSurfaceFace>,
}

/// Accessible atom-pair arcs and tangent-probe faces for one calculation domain.
#[derive(Clone, Debug, PartialEq)]
pub struct MsmsProbeTopologyDomain {
  /// Chain identity for a per-chain domain, or `None` for a unified domain.
  pub chain_id: Option<ChainId>,
  /// Accessible rolling-probe arcs, including complete free circles.
  pub edges: Vec<ReducedSurfaceEdge>,
  /// Accessible, consistently oriented tangent-probe faces.
  pub faces: Vec<ReducedSurfaceFace>,
  /// Connected components induced by shared reduced-surface atoms.
  pub components: Vec<MsmsProbeTopologyComponent>,
}

/// Analytical patch geometry for one independently calculated domain.
///
/// This intermediate contains exact contact boundaries plus untrimmed
/// toroidal and reentrant patches. Contact areas are exact, but this is not a
/// complete molecular surface until singular patches have been trimmed.
#[derive(Clone, Debug, PartialEq)]
pub struct MsmsPatchGeometryDomain {
  /// Accessible reduced-surface topology underlying the patch geometry.
  pub topology: MsmsProbeTopologyDomain,
  /// Atom-sphere contact patches and their closed analytical boundaries.
  pub contact_patches: Vec<ContactPatchGeometry>,
  /// Untrimmed toroidal geometry generated from topology edges.
  pub toroidal_patches: Vec<ToroidalPatchGeometry>,
  /// Reentrant spherical triangles generated from topology faces.
  pub reentrant_patches: Vec<ReentrantPatch>,
}

impl MsmsPatchGeometryDomain {
  /// Summarizes resolved areas and unresolved singular topology.
  pub fn area_summary(&self) -> MsmsPatchAreaSummary {
    let contact_sas = self.contact_patches.iter().map(ContactPatchGeometry::sas_area).sum();
    let contact_ses = self.contact_patches.iter().map(ContactPatchGeometry::ses_area).sum();
    let regular_toroidal_ses = self
      .toroidal_patches
      .iter()
      .filter_map(ToroidalPatchGeometry::regular_area)
      .sum();
    let singular_torus_count = self
      .toroidal_patches
      .iter()
      .filter(|patch| patch.topology == ToroidalPatchTopology::SelfIntersecting)
      .count();
    let untrimmed_reentrant_ses = self.reentrant_patches.iter().map(|patch| patch.area()).sum();
    MsmsPatchAreaSummary {
      contact_sas,
      contact_ses,
      regular_toroidal_ses,
      untrimmed_reentrant_ses,
      singular_torus_count,
    }
  }
}

/// Analytical area accounting before singular patches are trimmed.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MsmsPatchAreaSummary {
  /// Solvent-accessible area contributed by exposed expanded atom spheres.
  pub contact_sas: f64,
  /// Solvent-excluded area contributed by atom-contact patches.
  pub contact_ses: f64,
  /// Solvent-excluded area from regular toroidal patches only.
  pub regular_toroidal_ses: f64,
  /// Reentrant area before singular probe intersections are removed.
  pub untrimmed_reentrant_ses: f64,
  /// Number of toroidal patches still requiring singularity trimming.
  pub singular_torus_count: usize,
}

impl MsmsPatchAreaSummary {
  /// Returns complete SAS/SES totals when no singular patch remains.
  pub fn resolved_areas(self) -> Option<MsmsSurfaceAreas> {
    (self.singular_torus_count == 0).then_some(MsmsSurfaceAreas {
      sas: self.contact_sas,
      ses: self.contact_ses + self.regular_toroidal_ses + self.untrimmed_reentrant_ses,
    })
  }
}

/// One connected component of accessible probe arcs and tangent-probe faces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MsmsProbeTopologyComponent {
  /// Source atom indices participating in the component.
  pub atom_indices: Vec<usize>,
  /// Indices into the containing domain's `edges` collection.
  pub edge_indices: Vec<usize>,
  /// Indices into the containing domain's `faces` collection.
  pub face_indices: Vec<usize>,
}

/// Invalid physical parameter supplied to analytical surface construction.
#[derive(Clone, Copy, Debug, Error, PartialEq)]
pub enum MsmsParameterError {
  /// Probe radii must be finite and strictly positive.
  #[error("MSMS probe radius must be finite and positive, got {0}")]
  InvalidProbeRadius(f64),
}

/// Analytical SAS and SES areas in square ångströms.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MsmsSurfaceAreas {
  /// Area traced by the rolling probe center.
  pub sas: f64,
  /// Area bounding the volume excluded from the rolling probe.
  pub ses: f64,
}

/// Connectivity from which analytical molecular-surface patches are derived.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReducedSurface {
  /// Atom vertices participating in the solvent-accessible boundary.
  pub vertices: Vec<ReducedSurfaceVertex>,
  /// Atom pairs over which a probe rolls to create toroidal patches.
  pub edges: Vec<ReducedSurfaceEdge>,
  /// Atom triples defining fixed probe positions and reentrant patches.
  pub faces: Vec<ReducedSurfaceFace>,
}

/// One exposed atom in a reduced surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReducedSurfaceVertex {
  /// Index of the source atom in the structure scene.
  pub atom_index: usize,
}

/// One probe-accessible atom pair in a reduced surface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReducedSurfaceEdge {
  /// Source atom indices ordered deterministically.
  pub atom_indices: [usize; 2],
  /// Center of the probe-center intersection circle in ångströms.
  pub probe_circle_center: [f64; 3],
  /// Unit atom-pair axis normal to the probe-center circle.
  pub probe_circle_axis: [f64; 3],
  /// Deterministic unit vector defining zero angle in the circle plane.
  pub probe_circle_basis: [f64; 3],
  /// Radius of the probe-center circle in ångströms.
  pub probe_circle_radius: f64,
  /// Counter-clockwise arc start angle in radians.
  pub start_angle: f64,
  /// Positive angular sweep in radians; a full free edge uses $2\pi$.
  pub sweep_angle: f64,
  /// Incident face at the start and end of the arc, respectively.
  ///
  /// A complete free circle has no endpoints and therefore stores
  /// `[None, None]`.
  pub face_indices: [Option<usize>; 2],
}

/// Endpoint of an open reduced-surface probe arc.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbeArcEndpoint {
  /// Endpoint at `start_angle`.
  Start,
  /// Endpoint at `start_angle + sweep_angle`.
  End,
}

/// One fixed rolling-probe position supported by three atoms.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReducedSurfaceFace {
  /// Source atom indices ordered according to the oriented reduced-surface face.
  pub atom_indices: [usize; 3],
  /// Analytical center of the tangent probe sphere in ångströms.
  pub probe_center: [f64; 3],
}

/// One analytical patch contributing to the solvent-excluded surface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MsmsPatch {
  /// Convex spherical patch lying on an atom.
  Contact(ContactPatch),
  /// Concave spherical patch lying on a fixed probe sphere.
  Reentrant(ReentrantPatch),
  /// Saddle patch swept while a probe rolls around an atom pair.
  Toroidal(ToroidalPatch),
}

/// Concave spherical triangle supported by one fixed rolling probe.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReentrantPatch {
  /// Index of the source reduced-surface face.
  pub face_index: usize,
  /// Fixed probe center in ångströms.
  pub probe_center: [f64; 3],
  /// Rolling-probe radius in ångströms.
  pub probe_radius: f64,
  /// Unit directions from the probe center toward the three contact points.
  ///
  /// Their order follows the oriented reduced-surface face.
  pub contact_directions: [[f64; 3]; 3],
  /// Non-negative solid angle of the spherical triangle in steradians.
  pub solid_angle: f64,
}

/// One atom-sphere contact curve induced by a reduced-surface edge.
#[derive(Clone, Debug, PartialEq)]
pub struct ContactBoundaryArc {
  /// Index of the source reduced-surface edge.
  pub edge_index: usize,
  /// Source atom carrying this contact curve.
  pub atom_index: usize,
  /// Atom center in ångströms.
  pub atom_center: [f64; 3],
  /// Van der Waals radius of the atom in ångströms.
  pub atom_radius: f64,
  /// Unit atom-pair axis shared with the probe-center circle.
  pub axis: [f64; 3],
  /// Unit vector defining zero azimuth around the axis.
  pub basis: [f64; 3],
  /// Constant axial coordinate on the atom's unit sphere.
  pub direction_axis_offset: f64,
  /// Radius of the contact circle on the atom's unit sphere.
  pub direction_circle_radius: f64,
  /// Starting azimuth inherited from the reduced-surface edge.
  pub start_angle: f64,
  /// Positive azimuth sweep inherited from the reduced-surface edge.
  pub sweep_angle: f64,
  /// Incident reduced-surface faces at the start and end of the arc.
  pub face_indices: [Option<usize>; 2],
  /// Unit direction from this atom toward the neighboring occluding atom.
  ///
  /// This point lies on the non-exposed side of the boundary and supplies a
  /// stable stereographic projection pole for display tessellation.
  pub occluded_direction: [f64; 3],
  /// Whether increasing azimuth keeps the exposed region on the left.
  pub exposed_on_left_when_forward: bool,
}

impl ContactBoundaryArc {
  /// Evaluates the outward atom-sphere direction at an azimuth offset.
  pub fn direction(&self, angle_offset: f64) -> [f64; 3] {
    let axis = glam::DVec3::from_array(self.axis);
    let basis = glam::DVec3::from_array(self.basis);
    let perpendicular_basis = axis.cross(basis);
    let angle = self.start_angle + angle_offset;
    let radial = angle.cos() * basis + angle.sin() * perpendicular_basis;
    (self.direction_axis_offset * axis + self.direction_circle_radius * radial).to_array()
  }

  /// Evaluates one atom-sphere contact position in ångströms.
  pub fn position(&self, angle_offset: f64) -> [f64; 3] {
    let center = glam::DVec3::from_array(self.atom_center);
    (center + self.atom_radius * glam::DVec3::from_array(self.direction(angle_offset))).to_array()
  }
}

/// Directed use of a contact arc inside one closed boundary loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContactBoundaryArcUse {
  /// Index into [`ContactPatchGeometry::boundary_arcs`].
  pub arc_index: usize,
  /// Whether the loop traverses the stored arc from end to start.
  pub reversed: bool,
}

/// One closed boundary loop on an atom sphere.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContactBoundaryLoop {
  /// Directed contact arcs in traversal order.
  pub arcs: Vec<ContactBoundaryArcUse>,
}

/// Topological form of one exposed atom-contact patch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContactPatchTopology {
  /// The complete atom sphere is exposed and has no boundary.
  FullSphere,
  /// One or more closed contact loops bound the exposed region.
  Bounded,
}

/// Analytical boundary geometry of one exposed atom-contact patch.
#[derive(Clone, Debug, PartialEq)]
pub struct ContactPatchGeometry {
  /// Source atom index in the structure scene.
  pub atom_index: usize,
  /// Atom center in ångströms.
  pub atom_center: [f64; 3],
  /// Van der Waals radius in ångströms.
  pub atom_radius: f64,
  /// Rolling solvent-probe radius in ångströms.
  pub probe_radius: f64,
  /// Contact curves owned by this atom patch.
  pub boundary_arcs: Vec<ContactBoundaryArc>,
  /// Closed loops assembled from [`Self::boundary_arcs`].
  pub boundary_loops: Vec<ContactBoundaryLoop>,
  /// Whether this is a complete sphere or a bounded spherical region.
  pub topology: ContactPatchTopology,
  /// Exact exposed solid angle in steradians.
  pub solid_angle: f64,
}

impl ContactPatchGeometry {
  /// Returns the exact atom-contact SES area in square ångströms.
  pub fn ses_area(&self) -> f64 {
    self.atom_radius * self.atom_radius * self.solid_angle
  }

  /// Returns the corresponding expanded-sphere SAS area in square ångströms.
  pub fn sas_area(&self) -> f64 {
    let accessible_radius = self.atom_radius + self.probe_radius;
    accessible_radius * accessible_radius * self.solid_angle
  }
}

impl ReentrantPatch {
  /// Returns the analytical reentrant area in square ångströms.
  pub fn area(self) -> f64 {
    self.probe_radius * self.probe_radius * self.solid_angle
  }
}

/// Singularity classification of one untrimmed toroidal patch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToroidalPatchTopology {
  /// The torus parameterization is regular throughout the retained interval.
  Regular,
  /// The retained interval crosses a self-intersection circle.
  SelfIntersecting,
}

/// Untrimmed toroidal geometry swept by one accessible probe arc.
///
/// The geometry retains its complete local frame for analytical trimming and
/// display tessellation. [`ToroidalPatchTopology::SelfIntersecting`] records a
/// valid rolling-probe configuration that must be split before its area can be
/// included in [`MsmsSurfaceAreas`].
#[derive(Clone, Debug, PartialEq)]
pub struct ToroidalPatchGeometry {
  /// Index of the source reduced-surface edge.
  pub edge_index: usize,
  /// Source atoms touched by the rolling probe.
  pub atom_indices: [usize; 2],
  /// Center of the probe-center circle in ångströms.
  pub probe_circle_center: [f64; 3],
  /// Unit normal of the probe-center circle.
  pub probe_circle_axis: [f64; 3],
  /// Unit vector defining zero azimuth in the circle plane.
  pub probe_circle_basis: [f64; 3],
  /// Distance from the torus axis to the rolling-probe center.
  pub major_radius: f64,
  /// Rolling-probe radius in ångströms.
  pub probe_radius: f64,
  /// Starting azimuth inherited from the reduced-surface edge.
  pub azimuth_start: f64,
  /// Positive azimuth swept by the rolling probe.
  pub azimuth_sweep: f64,
  /// Starting meridian angle at one atom-contact curve.
  pub polar_start: f64,
  /// Positive minor meridian sweep toward the other contact curve.
  pub polar_sweep: f64,
  /// Whether the retained parameter rectangle crosses a torus singularity.
  pub topology: ToroidalPatchTopology,
}

impl ToroidalPatchGeometry {
  /// Evaluates a point at offsets from the patch's stored parameter origins.
  pub fn position(&self, azimuth_offset: f64, polar_offset: f64) -> [f64; 3] {
    let axis = glam::DVec3::from_array(self.probe_circle_axis);
    let basis = glam::DVec3::from_array(self.probe_circle_basis);
    let perpendicular_basis = axis.cross(basis);
    let azimuth = self.azimuth_start + azimuth_offset;
    let polar = self.polar_start + polar_offset;
    let radial = azimuth.cos() * basis + azimuth.sin() * perpendicular_basis;
    let center = glam::DVec3::from_array(self.probe_circle_center);
    let position =
      center + (self.major_radius + self.probe_radius * polar.cos()) * radial + self.probe_radius * polar.sin() * axis;
    position.to_array()
  }

  /// Evaluates the SES outward unit normal at one patch parameter.
  pub fn outward_normal(&self, azimuth_offset: f64, polar_offset: f64) -> [f64; 3] {
    let axis = glam::DVec3::from_array(self.probe_circle_axis);
    let basis = glam::DVec3::from_array(self.probe_circle_basis);
    let perpendicular_basis = axis.cross(basis);
    let azimuth = self.azimuth_start + azimuth_offset;
    let polar = self.polar_start + polar_offset;
    let radial = azimuth.cos() * basis + azimuth.sin() * perpendicular_basis;
    // Solvent occupies the rolling probe, so the SES normal points from the
    // probe surface back toward its center.
    (-(polar.cos() * radial + polar.sin() * axis)).to_array()
  }

  /// Returns the analytical area when the untrimmed patch is non-singular.
  pub fn regular_area(&self) -> Option<f64> {
    if self.topology != ToroidalPatchTopology::Regular {
      return None;
    }
    let polar_end = self.polar_start + self.polar_sweep;
    let meridian_integral =
      self.major_radius * self.polar_sweep + self.probe_radius * (polar_end.sin() - self.polar_start.sin());
    Some((self.probe_radius * self.azimuth_sweep * meridian_integral).abs())
  }

  /// Splits a radial singularity into regular end patches for display.
  ///
  /// A spindle torus is singular where
  /// `major_radius + probe_radius * cos(polar) = 0`. The physically retained
  /// toric face consists of parameter intervals on which this factor is
  /// non-negative. Every root collapses an entire azimuth row to one singular
  /// point, so the resulting grids represent the triangular toric faces used
  /// by the reduced-surface construction.
  pub fn split_radial_singularity(&self) -> Vec<Self> {
    if self.topology == ToroidalPatchTopology::Regular {
      return vec![self.clone()];
    }
    let ratio = (-self.major_radius / self.probe_radius).clamp(-1.0, 1.0);
    let principal_root = ratio.acos();
    let interval_start = self.polar_start;
    let interval_end = self.polar_start + self.polar_sweep;
    let mut boundaries = vec![interval_start, interval_end];
    let first_period = ((interval_start - principal_root) / (2.0 * PI)).floor() as i64 - 1;
    let last_period = ((interval_end + principal_root) / (2.0 * PI)).ceil() as i64 + 1;
    for period in first_period..=last_period {
      let offset = period as f64 * 2.0 * PI;
      for root in [principal_root + offset, -principal_root + offset] {
        if root > interval_start + 1.0e-12 && root < interval_end - 1.0e-12 {
          boundaries.push(root);
        }
      }
    }
    boundaries.sort_by(f64::total_cmp);
    boundaries.dedup_by(|left, right| (*left - *right).abs() <= 1.0e-12);
    boundaries
      .windows(2)
      .filter_map(|interval| {
        let start = interval[0];
        let end = interval[1];
        let midpoint = 0.5 * (start + end);
        let radial_factor = self.major_radius + self.probe_radius * midpoint.cos();
        (radial_factor >= -1.0e-12).then(|| {
          let mut patch = self.clone();
          patch.polar_start = start;
          patch.polar_sweep = end - start;
          patch.topology = ToroidalPatchTopology::Regular;
          patch
        })
      })
      .collect()
  }
}

impl MsmsPatch {
  /// Returns the exact SES area of this already-trimmed analytical patch.
  pub fn ses_area(self) -> f64 {
    match self {
      Self::Contact(patch) => patch.ses_area(),
      Self::Reentrant(patch) => patch.area(),
      Self::Toroidal(patch) => patch.area(),
    }
  }

  /// Returns this patch's contribution to the solvent-accessible area.
  ///
  /// Only contact patches have a corresponding SAS patch. Reentrant and
  /// toroidal patches are introduced by the inward offset that constructs the
  /// solvent-excluded surface and therefore contribute zero.
  pub fn sas_area(self) -> f64 {
    match self {
      Self::Contact(patch) => patch.sas_area(),
      Self::Reentrant(_) | Self::Toroidal(_) => 0.0,
    }
  }
}

/// A trimmed contact patch shared by the SES atom sphere and expanded SAS sphere.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContactPatch {
  /// Van der Waals radius of the supporting atom in ångströms.
  pub atom_radius: f64,
  /// Rolling solvent-probe radius in ångströms.
  pub probe_radius: f64,
  /// Non-negative exposed solid angle retained after trimming, in steradians.
  pub solid_angle: f64,
}

impl ContactPatch {
  /// Returns the contact contribution to SES area in square ångströms.
  pub fn ses_area(self) -> f64 {
    self.atom_radius * self.atom_radius * self.solid_angle
  }

  /// Returns the corresponding expanded-sphere SAS area in square ångströms.
  pub fn sas_area(self) -> f64 {
    let accessible_radius = self.atom_radius + self.probe_radius;
    accessible_radius * accessible_radius * self.solid_angle
  }
}

/// A non-singular rectangular patch in standard torus parameters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToroidalPatch {
  /// Distance from the torus axis to the rolling-probe center in ångströms.
  pub major_radius: f64,
  /// Rolling-probe radius in ångströms.
  pub probe_radius: f64,
  /// Signed sweep about the atom-pair axis in radians.
  pub azimuth_sweep: f64,
  /// Starting probe-circle parameter in radians.
  pub polar_start: f64,
  /// Ending probe-circle parameter in radians.
  pub polar_end: f64,
}

impl ToroidalPatch {
  /// Integrates the standard torus area element over the trimmed parameter range.
  pub fn area(self) -> f64 {
    let polar_sweep = self.polar_end - self.polar_start;
    let meridian_integral =
      self.major_radius * polar_sweep + self.probe_radius * (self.polar_end.sin() - self.polar_start.sin());
    (self.probe_radius * self.azimuth_sweep * meridian_integral).abs()
  }
}

/// Complete analytical MSMS result before optional display tessellation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MsmsSurface {
  /// Reduced-surface connectivity used to generate the patches.
  pub reduced_surface: ReducedSurface,
  /// Trimmed analytical patches comprising the solvent-excluded surface.
  pub patches: Vec<MsmsPatch>,
}

impl MsmsSurface {
  /// Integrates analytical SAS and SES areas from the trimmed patch set. The
  /// SES value includes contact, toroidal, and reentrant patches; the SAS value
  /// contains the expanded counterpart of each contact patch.
  pub fn areas(&self) -> MsmsSurfaceAreas {
    self
      .patches
      .iter()
      .fold(MsmsSurfaceAreas::default(), |mut areas, patch| {
        areas.sas += patch.sas_area();
        areas.ses += patch.ses_area();
        areas
      })
  }
}

#[cfg(test)]
mod tests {
  use std::f64::consts::{FRAC_PI_2, PI, TAU};

  use super::*;

  #[test]
  fn default_request_should_compute_one_assembly_surface() {
    assert_eq!(MsmsRequest::default().partition, SurfacePartition::Unified);
  }

  #[test]
  fn isolated_atom_contact_patch_should_integrate_exact_sas_and_ses() {
    let atom_radius = 2.0;
    let probe_radius = 1.4;
    let surface = MsmsSurface {
      reduced_surface: ReducedSurface::default(),
      patches: vec![MsmsPatch::Contact(ContactPatch {
        atom_radius,
        probe_radius,
        solid_angle: 4.0 * PI,
      })],
    };

    let areas = surface.areas();

    assert!((areas.ses - 4.0 * PI * atom_radius * atom_radius).abs() < 1.0e-12);
    assert!((areas.sas - 4.0 * PI * (atom_radius + probe_radius).powi(2)).abs() < 1.0e-12);
  }

  #[test]
  fn complete_torus_should_have_analytical_area() {
    let patch = ToroidalPatch {
      major_radius: 3.0,
      probe_radius: 1.5,
      azimuth_sweep: TAU,
      polar_start: 0.0,
      polar_end: TAU,
    };

    assert!((patch.area() - 4.0 * PI * PI * 3.0 * 1.5).abs() < 1.0e-12);
  }

  #[test]
  fn quarter_torus_should_integrate_trimmed_parameter_range() {
    let patch = ToroidalPatch {
      major_radius: 3.0,
      probe_radius: 1.0,
      azimuth_sweep: FRAC_PI_2,
      polar_start: 0.0,
      polar_end: FRAC_PI_2,
    };
    let expected = FRAC_PI_2 * (3.0 * FRAC_PI_2 + 1.0);

    assert!((patch.area() - expected).abs() < 1.0e-12);
  }

  #[test]
  fn unresolved_singular_patch_should_withhold_complete_area_totals() {
    let summary = MsmsPatchAreaSummary {
      contact_sas: 10.0,
      contact_ses: 5.0,
      regular_toroidal_ses: 2.0,
      untrimmed_reentrant_ses: 3.0,
      singular_torus_count: 1,
    };

    assert_eq!(summary.resolved_areas(), None);
  }

  #[test]
  fn regular_patch_summary_should_resolve_complete_area_totals() {
    let summary = MsmsPatchAreaSummary {
      contact_sas: 10.0,
      contact_ses: 5.0,
      regular_toroidal_ses: 2.0,
      untrimmed_reentrant_ses: 3.0,
      singular_torus_count: 0,
    };

    assert_eq!(
      summary.resolved_areas(),
      Some(MsmsSurfaceAreas { sas: 10.0, ses: 10.0 })
    );
  }
}
