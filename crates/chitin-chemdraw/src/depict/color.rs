//! Atom-label palettes are drawing policy, never properties of chemical elements.

use chitin_chem::Element;

use super::{Color, DepictOptions};

/// Color policy for complete atom labels, independent of bond-stroke styling.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ColorScheme {
  /// Every label uses the foreground, ignoring element palette overrides.
  #[default]
  Monochrome,
  /// Use per-element overrides, then the builtin palette, then the foreground.
  /// Carbon and hydrogen have no builtin entry, retaining skeletal contrast.
  Element,
}

impl DepictOptions {
  /// Resolves a complete label's color without changing geometry or atom facts.
  ///
  /// In element mode, `element_colors` overrides the builtin opaque palette.
  /// Unlisted elements fall back to `foreground`, including its alpha channel.
  /// Attached H, isotope, charge, and subscript runs follow their host label;
  /// explicit H graph vertices resolve independently as hydrogen. Bond strokes
  /// and generated H wedges always retain `foreground`.
  pub fn label_color(&self, element: Element) -> Color {
    match self.color_scheme {
      ColorScheme::Monochrome => self.foreground,
      ColorScheme::Element => self
        .element_colors
        .get(&element)
        .copied()
        .or_else(|| builtin(element))
        .unwrap_or(self.foreground),
    }
  }
}

/// Small, stable white-page palette, not a universal physical element standard.
///
/// Halogens deliberately share green. Sulfur uses a dark yellow to keep thin
/// glyphs visible on white. Unknown entries use the caller's foreground rather
/// than guessing colors for the rest of the periodic table.
fn builtin(element: Element) -> Option<Color> {
  let (red, green, blue) = match element {
    Element::Nitrogen => (0x30, 0x58, 0xd8),
    Element::Oxygen => (0xe5, 0x39, 0x35),
    Element::Sulfur => (0xb5, 0x9b, 0x00),
    Element::Phosphorus => (0xd9, 0x77, 0x06),
    Element::Fluorine | Element::Chlorine | Element::Bromine | Element::Iodine => (0x21, 0x8c, 0x3c),
    Element::Boron => (0xb4, 0x5d, 0x8a),
    Element::Silicon => (0x9c, 0x6b, 0x46),
    _ => return None,
  };
  Some(Color {
    red,
    green,
    blue,
    alpha: 255,
  })
}
