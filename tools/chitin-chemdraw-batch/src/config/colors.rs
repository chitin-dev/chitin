//! YAML-only color transport; serialization stays outside domain/drawing crates.

use std::collections::BTreeMap;

use serde::Deserialize;

use chitin_chem::Element;
use chitin_chemdraw::{Color, depict::ColorScheme};

/// Strict YAML spelling for the drawing library's color policy.
#[derive(Debug, Default, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Scheme {
  /// Retain the existing single-foreground output.
  #[default]
  Monochrome,
  /// Color labels by element, leaving all bonds monochrome.
  Element,
}

impl From<Scheme> for ColorScheme {
  fn from(scheme: Scheme) -> Self {
    match scheme {
      Scheme::Monochrome => Self::Monochrome,
      Scheme::Element => Self::Element,
    }
  }
}

/// Validated overrides; malformed keys/colors fail while parsing configuration.
#[derive(Debug, Default, Deserialize)]
#[serde(try_from = "BTreeMap<String, String>")]
pub(super) struct Palette(pub BTreeMap<Element, Color>);

impl TryFrom<BTreeMap<String, String>> for Palette {
  type Error = String;

  fn try_from(values: BTreeMap<String, String>) -> Result<Self, Self::Error> {
    let mut palette = BTreeMap::new();
    for (symbol, source) in values {
      let element = Element::from_symbol(&symbol).map_err(|error| error.to_string())?;
      let color = hex(&source).map_err(|error| format!("element_colors.{symbol}: {error}"))?;
      palette.insert(element, color);
    }
    Ok(Self(palette))
  }
}

/// Accepts exactly #RRGGBB or #RRGGBBAA; YAML values must therefore be quoted.
///
/// # Errors
/// Rejects shorthand, CSS names, missing '#', non-ASCII hex, and extra channels.
/// Validation happens before byte slicing, preventing UTF-8 boundary panics.
fn hex(source: &str) -> Result<Color, &'static str> {
  let invalid = "expected '#RRGGBB' or '#RRGGBBAA'";
  let code = source.strip_prefix('#').ok_or(invalid)?;
  if !matches!(code.len(), 6 | 8) || !code.bytes().all(|byte| byte.is_ascii_hexdigit()) {
    return Err(invalid);
  }
  let channel = |offset| u8::from_str_radix(&code[offset..offset + 2], 16).map_err(|_| invalid);
  Ok(Color {
    red: channel(0)?,
    green: channel(2)?,
    blue: channel(4)?,
    alpha: if code.len() == 8 { channel(6)? } else { 255 },
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn hex_should_preserve_rgb_and_optional_alpha() -> Result<(), &'static str> {
    assert_eq!(
      hex("#1234Ab")?,
      Color {
        red: 0x12,
        green: 0x34,
        blue: 0xab,
        alpha: 255
      }
    );
    assert_eq!(
      hex("#1234AB80")?,
      Color {
        red: 0x12,
        green: 0x34,
        blue: 0xab,
        alpha: 128
      }
    );
    Ok(())
  }

  #[test]
  fn invalid_hex_should_fail_without_panicking() {
    for value in ["#fff", "red", "123456", "#1234567", "#000000000", "#é1234", "#12345z"] {
      assert!(hex(value).is_err(), "{value}");
    }
  }
}
