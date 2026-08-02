use gpui::{
  AnyElement, App, CursorStyle, Entity, InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels,
  RenderOnce, Window, div, prelude::*, px,
};

use super::ButtonState;
use crate::themes::{UIThemes, builtins};

const DEFAULT_BUTTON_HEIGHT: Pixels = px(30.0);
const DEFAULT_BUTTON_PADDING_X: Pixels = px(10.0);

/// Visual style for a [`Button`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonStyle {
  /// A neutral button for standard actions.
  #[default]
  Secondary,
  /// A high-emphasis button for the primary action in a local context.
  Primary,
  /// A low-chrome button for toolbars and inline actions.
  Transparent,
}

/// Visual size for a [`Button`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonSize {
  /// Compact button for dense toolbars.
  Small,
  /// Default button for panels and forms.
  #[default]
  Medium,
  /// Taller button for prominent forms.
  Large,
}

/// A clickable container that can render text, icons, or any combination of children.
#[derive(IntoElement)]
pub struct Button {
  state: Entity<ButtonState>,
  children: Vec<AnyElement>,
  theme: UIThemes,
  style: ButtonStyle,
  size: ButtonSize,
  full_width: bool,
}

impl Button {
  /// Creates a button that renders and updates `state`.
  pub fn new(state: Entity<ButtonState>) -> Self {
    Self {
      state,
      children: Vec::new(),
      theme: builtins::dark(),
      style: ButtonStyle::default(),
      size: ButtonSize::default(),
      full_width: false,
    }
  }

  /// Appends text, an icon, or another GPUI element to this button.
  pub fn child(mut self, child: impl IntoElement) -> Self {
    self.children.push(child.into_any_element());
    self
  }

  /// Sets the semantic theme used for this button.
  pub fn theme(mut self, theme: UIThemes) -> Self {
    self.theme = theme;
    self
  }

  /// Sets the visual emphasis style.
  pub fn style(mut self, style: ButtonStyle) -> Self {
    self.style = style;
    self
  }

  /// Sets the visual size.
  pub fn size(mut self, size: ButtonSize) -> Self {
    self.size = size;
    self
  }

  /// Makes the button fill its parent width when enabled.
  pub fn full_width(mut self, full_width: bool) -> Self {
    self.full_width = full_width;
    self
  }
}

impl RenderOnce for Button {
  fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
    let metrics = self.size.metrics();
    let focus_handle = self.state.read(cx).focus_handle().clone();
    let focused = focus_handle.is_focused(window);
    self.state.update(cx, |state, cx| state.sync_focus(focused, cx));

    let (disabled, pressed) = {
      let state = self.state.read(cx);
      (state.is_disabled(), state.is_pressed())
    };
    let colors = ButtonColors::new(self.theme, self.style, pressed, disabled);
    let state_for_mouse_down = self.state.clone();
    let state_for_mouse_up = self.state.clone();
    let state_for_keys = self.state.clone();

    div()
      .flex()
      .items_center()
      .justify_center()
      .gap_1()
      .h(metrics.height)
      .w_auto()
      .when(self.full_width, |style| style.w_full())
      .px(DEFAULT_BUTTON_PADDING_X)
      .rounded_sm()
      .bg(colors.background)
      .text_size(metrics.font_size)
      .text_color(colors.foreground)
      .border_1()
      .border_color(colors.border)
      .cursor(if disabled {
        CursorStyle::Arrow
      } else {
        CursorStyle::PointingHand
      })
      .when(!disabled, |style| {
        style
          .hover(move |style| style.bg(colors.hover_background).text_color(colors.hover_foreground))
          .focus(move |style| style.border_color(self.theme.border.focus))
      })
      .on_mouse_down(MouseButton::Left, move |_, window, cx| {
        if state_for_mouse_down.read(cx).is_disabled() {
          return;
        }
        let focus_handle = state_for_mouse_down.read(cx).focus_handle().clone();
        state_for_mouse_down.update(cx, |state, cx| state.press(cx));
        window.focus(&focus_handle, cx);
        cx.stop_propagation();
      })
      .on_mouse_up(MouseButton::Left, move |_, _, cx| {
        let clicked = state_for_mouse_up.update(cx, |state, cx| state.release_and_click(cx));
        if clicked {
          cx.stop_propagation();
        }
      })
      .on_key_down(move |event, _, cx| {
        if matches!(event.keystroke.key.as_str(), "enter" | "space") {
          state_for_keys.update(cx, |state, cx| state.activate(cx));
          cx.stop_propagation();
        }
      })
      .track_focus(&focus_handle)
      .children(self.children)
  }
}

#[derive(Clone, Copy)]
struct ButtonMetrics {
  height: Pixels,
  font_size: Pixels,
}

impl ButtonSize {
  fn metrics(self) -> ButtonMetrics {
    match self {
      Self::Small => ButtonMetrics {
        height: px(24.0),
        font_size: px(12.0),
      },
      Self::Medium => ButtonMetrics {
        height: DEFAULT_BUTTON_HEIGHT,
        font_size: px(13.0),
      },
      Self::Large => ButtonMetrics {
        height: px(36.0),
        font_size: px(14.0),
      },
    }
  }
}

#[derive(Clone, Copy)]
struct ButtonColors {
  background: gpui::Rgba,
  hover_background: gpui::Rgba,
  foreground: gpui::Rgba,
  hover_foreground: gpui::Rgba,
  border: gpui::Rgba,
}

impl ButtonColors {
  fn new(theme: UIThemes, style: ButtonStyle, pressed: bool, disabled: bool) -> Self {
    if disabled {
      return Self {
        background: theme.background.secondary,
        hover_background: theme.background.secondary,
        foreground: theme.text.disabled,
        hover_foreground: theme.text.disabled,
        border: theme.border.muted,
      };
    }

    match style {
      ButtonStyle::Primary => Self {
        background: if pressed {
          theme.background.active
        } else {
          theme.accent.primary
        },
        hover_background: theme.background.active,
        foreground: theme.accent.foreground,
        hover_foreground: theme.accent.foreground,
        border: theme.accent.primary,
      },
      ButtonStyle::Secondary => Self {
        background: if pressed {
          theme.background.active
        } else {
          theme.background.tertiary
        },
        hover_background: theme.background.hover,
        foreground: theme.text.primary,
        hover_foreground: theme.text.hover,
        border: theme.border.primary,
      },
      ButtonStyle::Transparent => Self {
        background: if pressed {
          theme.background.active
        } else {
          builtins::TRANSPARENT
        },
        hover_background: theme.background.hover,
        foreground: theme.text.secondary,
        hover_foreground: theme.text.primary,
        border: builtins::TRANSPARENT,
      },
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn button_size_should_use_medium_height_by_default() {
    assert_eq!(ButtonSize::default().metrics().height, DEFAULT_BUTTON_HEIGHT);
  }
}
