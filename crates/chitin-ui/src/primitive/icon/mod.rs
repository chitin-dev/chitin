//! Visual-only SVG icon elements.
//!
//! [`Icon`] deliberately has no interaction state, callbacks, or event stream.
//! Compose it inside interactive primitives such as [`crate::primitive::button::Button`].

use gpui::{IntoElement, Pixels, RenderOnce, Rgba, Styled, Window, svg};

use crate::themes::{UIThemes, builtins};

/// Default square size for a standalone icon.
pub const DEFAULT_ICON_SIZE: Pixels = gpui::px(16.0);

/// A visual-only SVG icon resolved through GPUI's asset source.
#[derive(IntoElement)]
pub struct Icon {
  path: gpui::SharedString,
  size: Pixels,
  color: Option<Rgba>,
  theme: UIThemes,
}

impl Icon {
  /// Creates an icon from an asset-relative SVG `path`.
  pub fn new(path: impl Into<gpui::SharedString>) -> Self {
    Self {
      path: path.into(),
      size: DEFAULT_ICON_SIZE,
      color: None,
      theme: builtins::dark(),
    }
  }

  /// Sets the square icon size.
  pub fn size(mut self, size: Pixels) -> Self {
    self.size = size;
    self
  }

  /// Sets a semantic color supplied by the caller.
  pub fn color(mut self, color: Rgba) -> Self {
    self.color = Some(color);
    self
  }

  /// Sets the theme used for the default secondary text color.
  pub fn theme(mut self, theme: UIThemes) -> Self {
    self.theme = theme;
    self
  }
}

impl RenderOnce for Icon {
  fn render(self, _: &mut Window, _: &mut gpui::App) -> impl IntoElement {
    svg()
      .path(self.path)
      .size(self.size)
      .text_color(self.color.unwrap_or(self.theme.text.secondary))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn default_icon_size_should_be_compact() {
    assert_eq!(DEFAULT_ICON_SIZE, gpui::px(16.0));
  }
}
