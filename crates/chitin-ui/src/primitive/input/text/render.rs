use std::time::Instant;

use gpui::{
  App, CursorStyle, Entity, InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels, RenderOnce, Window,
  div, prelude::*, px,
};

use super::TextInputState;
use crate::themes::{UIThemes, builtins};

const DEFAULT_TEXT_INPUT_WIDTH: Pixels = px(240.0);
const DEFAULT_TEXT_INPUT_PADDING_X: Pixels = px(8.0);
const CARET_WIDTH: Pixels = px(2.0);

/// Visual size for a [`TextInput`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextInputSize {
  /// Compact input for dense toolbars.
  Small,
  /// Default input for sidebars and panels.
  #[default]
  Medium,
  /// Taller input for prominent forms.
  Large,
}

/// A single-line text input bound to its own [`TextInputState`] entity.
#[derive(IntoElement)]
pub struct TextInput {
  state: Entity<TextInputState>,
  placeholder: Option<gpui::SharedString>,
  theme: UIThemes,
  size: TextInputSize,
  full_width: bool,
  appearance: bool,
}

impl TextInput {
  /// Creates a text input that renders and updates `state`.
  pub fn new(state: Entity<TextInputState>) -> Self {
    Self {
      state,
      placeholder: None,
      theme: builtins::dark(),
      size: TextInputSize::default(),
      full_width: false,
      appearance: true,
    }
  }

  /// Sets placeholder text shown while the value is empty.
  pub fn placeholder(mut self, placeholder: impl Into<gpui::SharedString>) -> Self {
    self.placeholder = Some(placeholder.into());
    self
  }

  /// Sets the semantic theme used for this input.
  pub fn theme(mut self, theme: UIThemes) -> Self {
    self.theme = theme;
    self
  }

  /// Sets the input size.
  pub fn size(mut self, size: TextInputSize) -> Self {
    self.size = size;
    self
  }

  /// Makes the input fill its parent width when enabled.
  pub fn full_width(mut self, full_width: bool) -> Self {
    self.full_width = full_width;
    self
  }

  /// Shows or hides the input background and border.
  pub fn appearance(mut self, appearance: bool) -> Self {
    self.appearance = appearance;
    self
  }
}

impl RenderOnce for TextInput {
  fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
    let metrics = self.size.metrics();
    let focus_handle = self.state.read(cx).focus_handle().clone();
    let focused = focus_handle.is_focused(window);
    let state = self.state.clone();

    self.state.update(cx, |state, cx| state.sync_focus(focused, cx));

    let (text, disabled, readonly, caret_visible) = {
      let state = self.state.read(cx);
      (
        state.text().to_owned(),
        state.is_disabled(),
        state.is_readonly(),
        focused && state.caret_visible(Instant::now()),
      )
    };

    if focused && !disabled {
      window.request_animation_frame();
    }

    let placeholder = self.placeholder.unwrap_or_default();
    let text_is_empty = text.is_empty();
    let theme = self.theme;
    let state_for_mouse = state.clone();
    let state_for_keys = state.clone();

    div()
      .flex()
      .items_center()
      .gap(px(2.0))
      .w(DEFAULT_TEXT_INPUT_WIDTH)
      .when(self.full_width, |style| style.w_full())
      .h(metrics.height)
      .px(DEFAULT_TEXT_INPUT_PADDING_X)
      .rounded_sm()
      .when(self.appearance, |style| {
        style
          .border_1()
          .border_color(theme.border.primary)
          .focus(move |style| style.border_color(theme.border.focus))
          .bg(theme.background.tertiary)
      })
      .text_size(metrics.font_size)
      .cursor(if disabled {
        CursorStyle::Arrow
      } else {
        CursorStyle::IBeam
      })
      .when(disabled, |style| {
        style.text_color(theme.text.disabled).bg(theme.background.secondary)
      })
      .on_mouse_down(MouseButton::Left, move |_, window, cx| {
        if state_for_mouse.read(cx).is_disabled() {
          return;
        }
        let focus_handle = state_for_mouse.read(cx).focus_handle().clone();
        window.focus(&focus_handle, cx);
        cx.stop_propagation();
      })
      .on_key_down(move |event, _, cx| {
        let handled = state_for_keys.update(cx, |state, cx| state.handle_key_down(event, cx));
        if handled {
          cx.stop_propagation();
        }
      })
      .track_focus(&focus_handle)
      .child(
        div()
          .flex()
          .items_center()
          .min_w_0()
          .overflow_hidden()
          .text_color(if text_is_empty {
            theme.text.disabled
          } else {
            theme.text.primary
          })
          .child(if text_is_empty { placeholder } else { text.into() })
          .when(focused && !disabled, |style| {
            style.child(
              div()
                .flex_none()
                .w(CARET_WIDTH)
                .h(metrics.caret_height)
                .bg(theme.accent.primary)
                .opacity(if caret_visible { 1.0 } else { 0.0 }),
            )
          }),
      )
      .when(readonly && !disabled, |style| style.cursor(CursorStyle::IBeam))
  }
}

#[derive(Clone, Copy)]
struct TextInputMetrics {
  height: Pixels,
  font_size: Pixels,
  caret_height: Pixels,
}

impl TextInputSize {
  fn metrics(self) -> TextInputMetrics {
    match self {
      Self::Small => TextInputMetrics {
        height: px(24.0),
        font_size: px(12.0),
        caret_height: px(14.0),
      },
      Self::Medium => TextInputMetrics {
        height: px(30.0),
        font_size: px(13.0),
        caret_height: px(15.0),
      },
      Self::Large => TextInputMetrics {
        height: px(36.0),
        font_size: px(14.0),
        caret_height: px(16.0),
      },
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn text_input_size_should_use_medium_height_by_default() {
    assert_eq!(TextInputSize::default().metrics().height, px(30.0));
  }
}
