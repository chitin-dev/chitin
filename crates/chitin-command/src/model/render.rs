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
  /// Select a layer's coloring policy; uniform optionally supplies an sRGB color.
  Color {
    layer: RenderLayer,
    scheme: RenderColorScheme,
    value: Option<RenderRgb>,
  },
  /// Change a layer's opacity without disabling it or regenerating geometry.
  Opacity { layer: RenderLayer, value: RenderOpacity },
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
      Self::Color { .. } => super::CommandId::RenderColor,
      Self::Opacity { .. } => super::CommandId::RenderOpacity,
    }
  }
}

/// Independently styled molecular layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum RenderLayer {
  Atom,
  Polymer,
  Surface,
}

/// Coloring policies available in the first appearance implementation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum RenderColorScheme {
  Uniform,
  Element,
  Chain,
  ChainElement,
}

/// Validated sRGB color, excluding alpha (opacity is a separate setting).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderRgb(pub [u8; 3]);

impl std::str::FromStr for RenderRgb {
  type Err = String;
  fn from_str(value: &str) -> Result<Self, Self::Err> {
    let hex = value.strip_prefix('#').unwrap_or(value);
    if hex.len() != 6 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
      return Err("expected an sRGB color in #RRGGBB format".into());
    }
    let number = u32::from_str_radix(hex, 16).map_err(|error| error.to_string())?;
    Ok(Self([(number >> 16) as u8, (number >> 8) as u8, number as u8]))
  }
}

/// Finite normalized opacity, represented in basis points for stable equality.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderOpacity(u16);

impl RenderOpacity {
  /// Validates opacity before a typed request can be constructed.
  pub fn new(value: f32) -> Result<Self, String> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
      return Err("opacity must be a finite value between 0 and 1".into());
    }
    Ok(Self((value * 10_000.0).round() as u16))
  }
  /// Returns normalized opacity.
  pub fn value(self) -> f32 {
    self.0 as f32 / 10_000.0
  }
}

impl std::str::FromStr for RenderOpacity {
  type Err = String;
  fn from_str(value: &str) -> Result<Self, Self::Err> {
    Self::new(value.parse::<f32>().map_err(|error| error.to_string())?)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  #[test]
  fn opacity_should_reject_non_finite_and_out_of_range_values() {
    for value in ["NaN", "inf", "-0.1", "1.1"] {
      assert!(value.parse::<RenderOpacity>().is_err(), "{value}");
    }
    assert_eq!("0.35".parse::<RenderOpacity>().map(RenderOpacity::value), Ok(0.35));
  }
  #[test]
  fn rgb_should_accept_only_complete_hex_colors() {
    assert_eq!("#B8B8B8".parse::<RenderRgb>(), Ok(RenderRgb([184; 3])));
    for value in ["#FFF", "#GG0000", "#12345678", "红色"] {
      assert!(value.parse::<RenderRgb>().is_err());
    }
  }
}
