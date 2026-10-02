//! Frontend-only molecular presentation requests, independent of GPU/UI types.

/// Display style for the atom-and-bond layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum RenderAtomStyle {
  /// Disable the layer.
  None,
  /// Draw sticks.
  Stick,
  /// Draw balls and sticks.
  BallAndStick,
  /// Draw atom spheres.
  Sphere,
}

/// Display style for the polymer layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum RenderPolymerStyle {
  /// Disable the layer.
  None,
  /// Draw secondary-structure ribbons.
  Cartoon,
}

/// Display style for the surface layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum RenderSurfaceStyle {
  /// Disable the layer.
  None,
  /// Draw a filled surface.
  Solid,
}

/// Molecular-surface generation algorithm selected by a frontend.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum RenderSurfaceBackend {
  /// Sample an implicit scalar field.
  ImplicitScalarField,
  /// Use analytical MSMS geometry.
  Msms,
}

/// One presentation operation applied to an explicitly resolved rendering tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderCommand {
  /// Read the current presentation settings.
  Status,
  /// Change only the atom layer.
  AtomStyle(RenderAtomStyle),
  /// Change only the polymer layer.
  PolymerStyle(RenderPolymerStyle),
  /// Change only the surface layer.
  SurfaceStyle(RenderSurfaceStyle),
  /// Change the surface algorithm without enabling a disabled surface layer.
  SurfaceBackend(RenderSurfaceBackend),
}

impl RenderCommand {
  /// Returns the stable command identity.
  pub const fn id(self) -> super::CommandId {
    match self {
      Self::Status => super::CommandId::RenderStatus,
      Self::AtomStyle(_) => super::CommandId::RenderAtomStyle,
      Self::PolymerStyle(_) => super::CommandId::RenderPolymerStyle,
      Self::SurfaceStyle(_) => super::CommandId::RenderSurfaceStyle,
      Self::SurfaceBackend(_) => super::CommandId::RenderSurfaceBackend,
    }
  }
}
