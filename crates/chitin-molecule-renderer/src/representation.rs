//! Composable molecular representation-layer configuration.

/// Visual style used by the atom-and-bond representation layer.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum AtomStyle {
  /// Draw atoms with the bond radius and show bonds.
  #[default]
  Stick,
  /// Draw reduced element-colored atoms together with bonds.
  BallAndStick,
  /// Draw element-colored atoms without bonds.
  Sphere,
}

impl AtomStyle {
  /// Parses a frontend or command-line atom style name.
  pub fn from_name(name: &str) -> Option<Self> {
    match name.trim().to_ascii_lowercase().as_str() {
      "stick" => Some(Self::Stick),
      "ball-and-stick" | "ball_and_stick" | "ballandstick" => Some(Self::BallAndStick),
      "sphere" | "spheres" => Some(Self::Sphere),
      _ => None,
    }
  }
}

/// Visual style used by the polymer representation layer.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum PolymerStyle {
  /// Draw protein backbones as secondary-structure-aware ribbons.
  #[default]
  Cartoon,
}

/// Visual style used by the molecular-surface representation layer.
///
/// Surface state is modeled alongside the implemented layers so frontends do
/// not need another representation-state migration when surface tessellation
/// is added. The current renderer reports this layer as unsupported instead of
/// silently pretending that a surface was drawn.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceStyle {
  /// Draw a filled molecular surface.
  #[default]
  Solid,
}

/// One semantic molecule layer paired with its visual style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepresentationLayer {
  /// Atom and bond geometry.
  Atom(AtomStyle),
  /// Polymer backbone and secondary-structure geometry.
  Polymer(PolymerStyle),
  /// Molecular-surface geometry.
  Surface(SurfaceStyle),
}

/// Independently configurable molecular representation layers.
///
/// Atom, polymer, and surface layers are optional and may be enabled together.
/// The renderer currently implements atom and polymer layers; surface state is
/// reserved for the surface mesh pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepresentationLayers {
  atom: Option<AtomStyle>,
  polymer: Option<PolymerStyle>,
  surface: Option<SurfaceStyle>,
}

impl RepresentationLayers {
  /// Creates a representation with every layer disabled.
  pub const fn empty() -> Self {
    Self {
      atom: None,
      polymer: None,
      surface: None,
    }
  }

  /// Creates an atom-only representation using `style`.
  pub const fn atom(style: AtomStyle) -> Self {
    Self::empty().with_atom(style)
  }

  /// Returns the enabled atom style.
  pub const fn atom_style(self) -> Option<AtomStyle> {
    self.atom
  }

  /// Returns the enabled polymer style.
  pub const fn polymer_style(self) -> Option<PolymerStyle> {
    self.polymer
  }

  /// Returns the enabled surface style.
  pub const fn surface_style(self) -> Option<SurfaceStyle> {
    self.surface
  }

  /// Enables the atom layer with `style`.
  pub const fn with_atom(mut self, style: AtomStyle) -> Self {
    self.atom = Some(style);
    self
  }

  /// Disables the atom layer.
  pub const fn without_atom(mut self) -> Self {
    self.atom = None;
    self
  }

  /// Enables the polymer layer with `style`.
  pub const fn with_polymer(mut self, style: PolymerStyle) -> Self {
    self.polymer = Some(style);
    self
  }

  /// Disables the polymer layer.
  pub const fn without_polymer(mut self) -> Self {
    self.polymer = None;
    self
  }

  /// Enables the surface layer with `style`.
  pub const fn with_surface(mut self, style: SurfaceStyle) -> Self {
    self.surface = Some(style);
    self
  }

  /// Disables the surface layer.
  pub const fn without_surface(mut self) -> Self {
    self.surface = None;
    self
  }

  /// Enables or replaces the semantic layer described by `layer`.
  pub const fn with_layer(self, layer: RepresentationLayer) -> Self {
    match layer {
      RepresentationLayer::Atom(style) => self.with_atom(style),
      RepresentationLayer::Polymer(style) => self.with_polymer(style),
      RepresentationLayer::Surface(style) => self.with_surface(style),
    }
  }
}

impl Default for RepresentationLayers {
  fn default() -> Self {
    Self::atom(AtomStyle::default())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn layers_should_allow_atom_polymer_and_surface_styles_together() {
    let layers = RepresentationLayers::empty()
      .with_layer(RepresentationLayer::Atom(AtomStyle::Stick))
      .with_layer(RepresentationLayer::Polymer(PolymerStyle::Cartoon))
      .with_layer(RepresentationLayer::Surface(SurfaceStyle::Solid));

    assert_eq!(
      (layers.atom_style(), layers.polymer_style(), layers.surface_style()),
      (
        Some(AtomStyle::Stick),
        Some(PolymerStyle::Cartoon),
        Some(SurfaceStyle::Solid),
      ),
    );
  }

  #[test]
  fn disabling_polymer_should_preserve_the_atom_layer() {
    let layers = RepresentationLayers::atom(AtomStyle::Stick)
      .with_polymer(PolymerStyle::Cartoon)
      .without_polymer();

    assert_eq!(layers, RepresentationLayers::atom(AtomStyle::Stick));
  }
}
