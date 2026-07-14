//! Color conversion helpers for theme authors.
//!
//! Chitin theme definitions should prefer perceptual color spaces for palette
//! design, then convert to GPUI's rendering color types at the edge. OKLCH is a
//! useful authoring space because changes to lightness and chroma tend to feel
//! more uniform than editing RGB channels directly.

use gpui::{Hsla, Rgba};

/// Converts an OKLCH color to GPUI [`Hsla`].
///
/// The input follows common CSS OKLCH conventions:
///
/// - `lightness`: perceptual lightness, usually in the range `0.0..=1.0`
/// - `chroma`: color intensity, usually `0.0` for neutral colors and higher for
///   more saturated colors
/// - `hue_degrees`: hue angle in degrees
/// - `alpha`: opacity in the range `0.0..=1.0`
///
/// Values are converted through OKLab to sRGB, clipped to the displayable sRGB
/// gamut, then converted to GPUI's HSLA representation.
///
/// # Examples
///
/// ```
/// use chitin_ui::themes::colors::oklch_to_hsla;
///
/// let white = oklch_to_hsla(1.0, 0.0, 0.0, 1.0);
/// assert!((white.l - 1.0).abs() < 0.000_1);
/// assert_eq!(white.a, 1.0);
/// ```
pub fn oklch_to_hsla(lightness: f32, chroma: f32, hue_degrees: f32, alpha: f32) -> Hsla {
  let hue_radians = hue_degrees.to_radians();
  let lab_a = chroma * hue_radians.cos();
  let lab_b = chroma * hue_radians.sin();

  let l_prime = lightness + 0.396_337_78 * lab_a + 0.215_803_76 * lab_b;
  let m_prime = lightness - 0.105_561_346 * lab_a - 0.063_854_17 * lab_b;
  let s_prime = lightness - 0.089_484_18 * lab_a - 1.291_485_5 * lab_b;

  let l = l_prime * l_prime * l_prime;
  let m = m_prime * m_prime * m_prime;
  let s = s_prime * s_prime * s_prime;

  let linear_red = 4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s;
  let linear_green = -1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s;
  let linear_blue = -0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s;

  Rgba {
    r: linear_srgb_to_srgb(linear_red),
    g: linear_srgb_to_srgb(linear_green),
    b: linear_srgb_to_srgb(linear_blue),
    a: alpha.clamp(0.0, 1.0),
  }
  .into()
}

fn linear_srgb_to_srgb(value: f32) -> f32 {
  let value = if value <= 0.003_130_8 {
    12.92 * value
  } else {
    1.055 * value.powf(1.0 / 2.4) - 0.055
  };

  value.clamp(0.0, 1.0)
}

/// Creates an OKLCH color with full opacity.
///
/// This is a convenience wrapper around [`oklcha`] with alpha set to `1.0`.
///
/// # Arguments
///
/// * `lightness` - Perceptual lightness, typically in range `0.0..=1.0`
/// * `chroma` - Color intensity, `0.0` for neutral grays, higher for saturation
/// * `hue_degrees` - Hue angle in degrees (0.0 = red, 120.0 = green, 240.0 = blue)
///
/// # Examples
///
/// ```
/// use chitin_ui::themes::colors::oklch;
///
/// // Pure white
/// let white = oklch(1.0, 0.0, 0.0);
///
/// // A vivid red
/// let red = oklch(0.628, 0.258, 29.234);
/// ```
pub fn oklch(lightness: f32, chroma: f32, hue_degrees: f32) -> gpui::Hsla {
  oklcha(lightness, chroma, hue_degrees, 1.0)
}

/// Creates an OKLCH color with explicit alpha channel.
///
/// This is a convenience wrapper around [`oklch_to_hsla`] that matches the naming
/// convention of CSS's `oklcha()` function, where the trailing 'a' indicates
/// alpha support.
///
/// # Arguments
///
/// * `lightness` - Perceptual lightness, typically in range `0.0..=1.0`
/// * `chroma` - Color intensity, `0.0` for neutral grays, higher for saturation
/// * `hue_degrees` - Hue angle in degrees (0.0 = red, 120.0 = green, 240.0 = blue)
/// * `alpha` - Opacity in range `0.0..=1.0` (0.0 = fully transparent, 1.0 = fully opaque)
///
/// # Examples
///
/// ```
/// use chitin_ui::themes::colors::oklcha;
///
/// // Semi-transparent white
/// let translucent_white = oklcha(1.0, 0.0, 0.0, 0.5);
///
/// // A muted green with some transparency
/// let muted_green = oklcha(0.6, 0.1, 140.0, 0.8);
/// ```
pub fn oklcha(lightness: f32, chroma: f32, hue_degrees: f32, alpha: f32) -> gpui::Hsla {
  oklch_to_hsla(lightness, chroma, hue_degrees, alpha)
}

/// Converts a CSS-style CIE Lab color to GPUI [`Hsla`].
///
/// The input follows CSS `lab()` conventions:
///
/// - `lightness_percent`: CIE lightness in percent, usually `0.0..=100.0`
/// - `a`: green/red opponent axis
/// - `b`: blue/yellow opponent axis
/// - `alpha`: opacity in the range `0.0..=1.0`
///
/// CSS Lab is D50-referenced. This function converts Lab to XYZ D50, adapts
/// D50 to D65 with the Bradford matrix, converts to sRGB, clips to the sRGB
/// gamut, and finally returns GPUI's HSLA representation.
///
/// # Examples
///
/// ```
/// use chitin_ui::themes::colors::lab_to_hsla;
///
/// let white = lab_to_hsla(100.0, 0.0, 0.0, 1.0);
/// assert!((white.l - 1.0).abs() < 0.000_1);
/// ```
pub fn lab_to_hsla(lightness_percent: f32, a: f32, b: f32, alpha: f32) -> Hsla {
  let fy = (lightness_percent + 16.0) / 116.0;
  let fx = fy + a / 500.0;
  let fz = fy - b / 200.0;

  let x_d50 = 0.964_22 * lab_inverse_transfer(fx);
  let y_d50 = lab_inverse_transfer(fy);
  let z_d50 = 0.825_21 * lab_inverse_transfer(fz);

  let x_d65 = 0.955_576_6 * x_d50 - 0.023_039_3 * y_d50 + 0.063_163_6 * z_d50;
  let y_d65 = -0.028_289_5 * x_d50 + 1.009_941_6 * y_d50 + 0.021_007_7 * z_d50;
  let z_d65 = 0.012_298_2 * x_d50 - 0.020_483 * y_d50 + 1.329_909_8 * z_d50;

  let linear_red = 3.240_454_2 * x_d65 - 1.537_138_5 * y_d65 - 0.498_531_4 * z_d65;
  let linear_green = -0.969_266 * x_d65 + 1.876_010_8 * y_d65 + 0.041_556 * z_d65;
  let linear_blue = 0.055_643_4 * x_d65 - 0.204_025_9 * y_d65 + 1.057_225_2 * z_d65;

  Rgba {
    r: linear_srgb_to_srgb(linear_red),
    g: linear_srgb_to_srgb(linear_green),
    b: linear_srgb_to_srgb(linear_blue),
    a: alpha.clamp(0.0, 1.0),
  }
  .into()
}

/// Creates a CSS Lab color with full opacity.
pub fn lab(lightness_percent: f32, a: f32, b: f32) -> Hsla {
  laba(lightness_percent, a, b, 1.0)
}

/// Creates a CSS Lab color with explicit alpha.
pub fn laba(lightness_percent: f32, a: f32, b: f32, alpha: f32) -> Hsla {
  lab_to_hsla(lightness_percent, a, b, alpha)
}

fn lab_inverse_transfer(value: f32) -> f32 {
  const EPSILON: f32 = 216.0 / 24_389.0;
  const KAPPA: f32 = 24_389.0 / 27.0;

  let cubed = value * value * value;

  if cubed > EPSILON {
    cubed
  } else {
    (116.0 * value - 16.0) / KAPPA
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn oklch_to_hsla_should_convert_neutral_black() {
    let color = oklch_to_hsla(0.0, 0.0, 0.0, 1.0);

    assert_close(color.l, 0.0);
    assert_close(color.s, 0.0);
    assert_close(color.a, 1.0);
  }

  #[test]
  fn oklch_to_hsla_should_convert_neutral_white() {
    let color = oklch_to_hsla(1.0, 0.0, 0.0, 0.5);

    assert_close(color.l, 1.0);
    assert_close(color.s, 0.0);
    assert_close(color.a, 0.5);
  }

  #[test]
  fn oklch_to_hsla_should_convert_srgb_red_approximation() {
    let color = oklch_to_hsla(0.627_955_4, 0.257_683_3, 29.233_9, 1.0);

    assert!(color.h < 0.01 || color.h > 0.99);
    assert!((color.s - 1.0).abs() < 0.01);
    assert!((color.l - 0.5).abs() < 0.01);
  }

  #[test]
  fn lab_to_hsla_should_convert_neutral_white() {
    let color = lab_to_hsla(100.0, 0.0, 0.0, 1.0);

    assert_close(color.l, 1.0);
    assert_close(color.s, 0.0);
    assert_close(color.a, 1.0);
  }

  #[test]
  fn lab_to_hsla_should_convert_neutral_black() {
    let color = lab_to_hsla(0.0, 0.0, 0.0, 0.5);

    assert_close(color.l, 0.0);
    assert_close(color.s, 0.0);
    assert_close(color.a, 0.5);
  }

  fn assert_close(actual: f32, expected: f32) {
    assert!(
      (actual - expected).abs() < 0.000_1,
      "expected {actual} to be close to {expected}",
    );
  }
}
