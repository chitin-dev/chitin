//! Shared visual fields for single-line input primitives.

use gpui::{Pixels, Rgba};

/// Crate-private visual overrides shared by text-like input controls.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct InputAppearanceStyle {
  pub(crate) background: Option<Rgba>,
  pub(crate) foreground: Option<Rgba>,
  // placeholder should have same background with the text input
  pub(crate) placeholder_foreground: Option<Rgba>,
  pub(crate) selection_background: Option<Rgba>,
  // the foreground of selected texts can be different from normal foreground
  pub(crate) selection_foreground: Option<Rgba>,
  pub(crate) caret: Option<Rgba>,
  pub(crate) border: Option<Rgba>,
  pub(crate) focus_border: Option<Rgba>,
  pub(crate) width: Option<Pixels>,
  pub(crate) height: Option<Pixels>,
  pub(crate) horizontal_padding: Option<Pixels>,
}
