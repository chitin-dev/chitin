//! Viewport clear-color policy shared by desktop and export paths.

/// Background policy for a molecular viewport.
///
/// [`Auto`](Self::Auto) follows the host theme: dark themes clear to black and
/// light themes clear to white. [`Custom`](Self::Custom) keeps an explicit sRGB
/// color independently of the theme.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ViewportBackground {
  /// Follow the host theme.
  #[default]
  Auto,
  /// Use an explicit sRGB color.
  Custom([u8; 3]),
}

impl ViewportBackground {
  /// Resolves the linear RGB clear color consumed by the renderer.
  ///
  /// `dark` selects the automatic branch when this policy is [`Self::Auto`].
  /// Custom colors are converted from sRGB so the drawn result matches the
  /// color the user selected rather than appearing washed out.
  pub fn resolve(self, dark: bool) -> [f64; 3] {
    match self {
      Self::Auto if dark => [0.0, 0.0, 0.0],
      Self::Auto => [1.0, 1.0, 1.0],
      Self::Custom(color) => color.map(srgb_to_linear),
    }
  }
}

/// Converts one sRGB channel to linear RGB.
fn srgb_to_linear(value: u8) -> f64 {
  let value = f64::from(value) / 255.0;
  if value <= 0.040_45 {
    value / 12.92
  } else {
    ((value + 0.055) / 1.055).powf(2.4)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn automatic_background_should_follow_the_theme() {
    assert_eq!(ViewportBackground::Auto.resolve(true), [0.0, 0.0, 0.0]);
    assert_eq!(ViewportBackground::Auto.resolve(false), [1.0, 1.0, 1.0]);
  }

  #[test]
  fn custom_background_should_ignore_the_theme_and_convert_srgb() {
    let black = ViewportBackground::Custom([0, 0, 0]);
    let white = ViewportBackground::Custom([255, 255, 255]);
    let mid = ViewportBackground::Custom([128, 128, 128]);
    assert_eq!(black.resolve(false), [0.0, 0.0, 0.0]);
    assert_eq!(white.resolve(true), [1.0, 1.0, 1.0]);
    assert_eq!(mid.resolve(true), mid.resolve(false));
    assert!((mid.resolve(true)[0] - 0.215_86).abs() < 1.0e-4);
  }
}
