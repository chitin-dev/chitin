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

pub mod geometry;

use thiserror::Error;

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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReducedSurfaceEdge {
  /// Source atom indices ordered deterministically.
  pub atom_indices: [usize; 2],
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
  Reentrant(SphericalPatch),
  /// Saddle patch swept while a probe rolls around an atom pair.
  Toroidal(ToroidalPatch),
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

/// A trimmed spherical patch represented by its signed solid angle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SphericalPatch {
  /// Radius of the supporting atom or probe sphere in ångströms.
  pub radius: f64,
  /// Non-negative solid angle retained after analytical trimming, in steradians.
  pub solid_angle: f64,
}

impl SphericalPatch {
  /// Returns the analytical spherical-patch area in square ångströms.
  pub fn area(self) -> f64 {
    self.radius * self.radius * self.solid_angle
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
  fn full_spherical_patch_should_have_sphere_area() {
    let patch = SphericalPatch {
      radius: 2.0,
      solid_angle: 4.0 * PI,
    };

    assert!((patch.area() - 16.0 * PI).abs() < 1.0e-12);
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
}
