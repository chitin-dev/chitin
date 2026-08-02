use gpui::{
  AnyElement, App, CursorStyle, Entity, InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels,
  RenderOnce, Window, div, prelude::*, px,
};

use super::ButtonState;
use crate::themes::{UIThemes, builtins};

const DEFAULT_BUTTON_HEIGHT: Pixels = px(30.0);
const DEFAULT_BUTTON_PADDING_X: Pixels = px(10.0);

/// Built-in theme-based appearance variants for a [`Button`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonVariant {
  /// A neutral button for standard actions.
  #[default]
  Secondary,
  /// A high-emphasis button for the primary action in a local context.
  Primary,
  /// A low-chrome button for toolbars and inline actions.
  Transparent,
}

/// Visual-only overrides for a [`Button`].
///
/// This style intentionally cannot alter button interaction, focus ownership,
/// keyboard activation, disabled behavior, or emitted events.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ButtonStyle {
  background: Option<gpui::Rgba>,
  hover_background: Option<gpui::Rgba>,
  pressed_background: Option<gpui::Rgba>,
  foreground: Option<gpui::Rgba>,
  hover_foreground: Option<gpui::Rgba>,
  border: Option<gpui::Rgba>,
  focus_border: Option<gpui::Rgba>,
  width: Option<Pixels>,
  horizontal_padding: Option<Pixels>,
}

impl ButtonStyle {
  /// Creates a style with no overrides.
  pub fn new() -> Self {
    Self::default()
  }

  /// Overrides the default background color.
  pub fn background(mut self, color: gpui::Rgba) -> Self {
    self.background = Some(color);
    self
  }

  /// Overrides the hover background color.
  pub fn hover_background(mut self, color: gpui::Rgba) -> Self {
    self.hover_background = Some(color);
    self
  }

  /// Overrides the pressed background color.
  pub fn pressed_background(mut self, color: gpui::Rgba) -> Self {
    self.pressed_background = Some(color);
    self
  }

  /// Overrides the default foreground color.
  pub fn foreground(mut self, color: gpui::Rgba) -> Self {
    self.foreground = Some(color);
    self
  }

  /// Overrides the hover foreground color.
  pub fn hover_foreground(mut self, color: gpui::Rgba) -> Self {
    self.hover_foreground = Some(color);
    self
  }

  /// Overrides the default border color.
  pub fn border(mut self, color: gpui::Rgba) -> Self {
    self.border = Some(color);
    self
  }

  /// Overrides the border color while the button is focused.
  pub fn focus_border(mut self, color: gpui::Rgba) -> Self {
    self.focus_border = Some(color);
    self
  }

  /// Sets an explicit visual width.
  pub fn width(mut self, width: Pixels) -> Self {
    self.width = Some(width);
    self
  }

  /// Sets horizontal padding.
  pub fn horizontal_padding(mut self, padding: Pixels) -> Self {
    self.horizontal_padding = Some(padding);
    self
  }
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
  variant: ButtonVariant,
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
      variant: ButtonVariant::default(),
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

  /// Sets one of the built-in theme-based appearance variants.
  pub fn variant(mut self, variant: ButtonVariant) -> Self {
    self.variant = variant;
    self
  }

  /// Applies visual-only appearance overrides.
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
    let colors = ButtonColors::new(self.theme, self.variant, self.style, pressed, disabled);
    let state_for_mouse_down = self.state.clone();
    let state_for_mouse_up = self.state.clone();
    let state_for_keys = self.state.clone();

    div()
      .flex()
      .items_center()
      .justify_center()
      .gap_1()
      .h(metrics.height)
      .when_some(self.style.width, |style, width| style.w(width))
      .when(self.style.width.is_none(), |style| style.w_auto())
      .when(self.full_width, |style| style.w_full())
      .px(self.style.horizontal_padding.unwrap_or(DEFAULT_BUTTON_PADDING_X))
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
        style.hover(move |style| style.bg(colors.hover_background).text_color(colors.hover_foreground))
      })
      .when(!disabled, |style| {
        style.focus(move |style| style.border_color(colors.focus_border))
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
  focus_border: gpui::Rgba,
}

impl ButtonColors {
  fn new(theme: UIThemes, variant: ButtonVariant, style: ButtonStyle, pressed: bool, disabled: bool) -> Self {
    if disabled {
      return Self {
        background: theme.background.secondary,
        hover_background: theme.background.secondary,
        foreground: theme.text.disabled,
        hover_foreground: theme.text.disabled,
        border: theme.border.muted,
        focus_border: theme.border.muted,
      };
    }

    let colors = match variant {
      ButtonVariant::Primary => Self {
        background: if pressed {
          theme.background.active
        } else {
          theme.accent.primary
        },
        hover_background: theme.background.active,
        foreground: theme.accent.foreground,
        hover_foreground: theme.accent.foreground,
        border: theme.accent.primary,
        focus_border: theme.border.focus,
      },
      ButtonVariant::Secondary => Self {
        background: if pressed {
          theme.background.active
        } else {
          theme.background.tertiary
        },
        hover_background: theme.background.hover,
        foreground: theme.text.primary,
        hover_foreground: theme.text.hover,
        border: theme.border.primary,
        focus_border: theme.border.focus,
      },
      ButtonVariant::Transparent => Self {
        background: if pressed {
          theme.background.active
        } else {
          builtins::TRANSPARENT
        },
        hover_background: theme.background.hover,
        foreground: theme.text.secondary,
        hover_foreground: theme.text.primary,
        border: builtins::TRANSPARENT,
        focus_border: theme.border.focus,
      },
    };

    Self {
      background: if pressed {
        style
          .pressed_background
          .or(style.background)
          .unwrap_or(colors.background)
      } else {
        style.background.unwrap_or(colors.background)
      },
      hover_background: style.hover_background.unwrap_or(colors.hover_background),
      foreground: style.foreground.unwrap_or(colors.foreground),
      hover_foreground: style.hover_foreground.unwrap_or(colors.hover_foreground),
      border: style.border.unwrap_or(colors.border),
      focus_border: style.focus_border.unwrap_or(theme.border.focus),
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

  #[test]
  fn button_style_should_override_focus_border_color() {
    let theme = builtins::dark();
    let colors = ButtonColors::new(
      theme,
      ButtonVariant::Transparent,
      ButtonStyle::new().focus_border(builtins::TRANSPARENT),
      false,
      false,
    );

    assert_eq!(colors.focus_border, builtins::TRANSPARENT);
  }

  #[test]
  fn button_style_should_override_pressed_background_color() {
    let theme = builtins::dark();
    let colors = ButtonColors::new(
      theme,
      ButtonVariant::Transparent,
      ButtonStyle::new().pressed_background(theme.background.warning),
      true,
      false,
    );

    assert_eq!(colors.background, theme.background.warning);
  }
}
