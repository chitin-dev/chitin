//! Rendering for the reusable numeric input primitive.

use gpui::{
  App, CursorStyle, Entity, InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels, RenderOnce,
  SharedString, Window, div, prelude::*, px,
};

use super::NumberInputState;
use crate::{
  primitive::icon::Icon,
  primitive::input::text::{TextInput, TextInputSize},
  themes::{UIThemes, builtins},
};

const DEFAULT_NUMBER_INPUT_WIDTH: Pixels = px(160.0);
const STEPPER_WIDTH: Pixels = px(20.0);

/// Visual size for a [`NumberInput`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NumberInputSize {
  /// Compact input for dense toolbars.
  Small,
  /// Default input for panels and forms.
  #[default]
  Medium,
  /// Taller input for prominent forms.
  Large,
}

/// Built-in numeric interaction affordances rendered by [`NumberInput`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NumberInputControls {
  /// Render no pointer controls and rely on keyboard stepping.
  None,
  /// Render increment and decrement controls that preserve editor focus.
  #[default]
  Stepper,
}

/// A numeric editing control bound to its own [`NumberInputState`] entity.
#[derive(IntoElement)]
pub struct NumberInput {
  state: Entity<NumberInputState>,
  prefix: Option<SharedString>,
  suffix: Option<SharedString>,
  placeholder: Option<SharedString>,
  theme: UIThemes,
  size: NumberInputSize,
  controls: NumberInputControls,
  full_width: bool,
}

impl NumberInput {
  /// Creates a numeric input that renders and updates `state`.
  ///
  /// # Parameters
  ///
  /// `state` owns numeric editing, focus, and semantic events.
  ///
  /// # Returns
  ///
  /// A numeric input with default semantic theme and size variants.
  pub fn new(state: Entity<NumberInputState>) -> Self {
    Self {
      state,
      prefix: None,
      suffix: None,
      placeholder: None,
      theme: builtins::dark(),
      size: NumberInputSize::default(),
      controls: NumberInputControls::default(),
      full_width: false,
    }
  }

  /// Sets non-editable content rendered before the numeric value.
  ///
  /// # Parameters
  ///
  /// `prefix` supplies the visual prefix text.
  ///
  /// # Returns
  ///
  /// This input configured with the supplied prefix.
  pub fn prefix(mut self, prefix: impl Into<SharedString>) -> Self {
    self.prefix = Some(prefix.into());
    self
  }

  /// Sets non-editable content rendered after the numeric value.
  ///
  /// # Parameters
  ///
  /// `suffix` supplies the visual suffix text.
  ///
  /// # Returns
  ///
  /// This input configured with the supplied suffix.
  pub fn suffix(mut self, suffix: impl Into<SharedString>) -> Self {
    self.suffix = Some(suffix.into());
    self
  }

  /// Sets placeholder text shown while the editing draft is empty.
  ///
  /// # Parameters
  ///
  /// `placeholder` supplies the visual empty-state text.
  ///
  /// # Returns
  ///
  /// This input configured with the supplied placeholder.
  pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
    self.placeholder = Some(placeholder.into());
    self
  }

  /// Sets the semantic theme used for this numeric input.
  ///
  /// # Parameters
  ///
  /// `theme` supplies semantic colors for the control.
  ///
  /// # Returns
  ///
  /// This input configured with the supplied theme.
  pub fn theme(mut self, theme: UIThemes) -> Self {
    self.theme = theme;
    self
  }

  /// Sets the visual size.
  ///
  /// # Parameters
  ///
  /// `size` selects a predefined control size.
  ///
  /// # Returns
  ///
  /// This input configured with the supplied size.
  pub fn size(mut self, size: NumberInputSize) -> Self {
    self.size = size;
    self
  }

  /// Sets built-in numeric interaction affordances.
  ///
  /// # Parameters
  ///
  /// `controls` selects the pointer controls rendered by this input.
  ///
  /// # Returns
  ///
  /// This input configured with the supplied controls.
  pub fn controls(mut self, controls: NumberInputControls) -> Self {
    self.controls = controls;
    self
  }

  /// Makes the control fill its parent width when enabled.
  ///
  /// # Parameters
  ///
  /// `full_width` selects whether the control fills its parent width.
  ///
  /// # Returns
  ///
  /// This input configured with the supplied width behavior.
  pub fn full_width(mut self, full_width: bool) -> Self {
    self.full_width = full_width;
    self
  }
}

impl RenderOnce for NumberInput {
  /// Renders one numeric input using the text primitive for focused text editing.
  ///
  /// # Parameters
  ///
  /// `cx` reads the numeric state and creates the child text input.
  ///
  /// # Returns
  ///
  /// The rendered numeric input control.
  fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
    let (input, validation_error, interaction_disabled) = {
      let state = self.state.read(cx);
      (
        state.text_input(),
        state.draft_state().has_validation_error(),
        state.is_disabled() || state.is_readonly(),
      )
    };
    let focus_handle = input.read(cx).focus_handle().clone();
    let theme = self.theme;
    let border_color = if validation_error {
      theme.text.error
    } else {
      theme.border.primary
    };
    let focus_border = if validation_error {
      theme.text.error
    } else {
      theme.border.focus
    };
    let state_for_keys = self.state.clone();

    div()
      .flex()
      .items_center()
      .w(DEFAULT_NUMBER_INPUT_WIDTH)
      .when(self.full_width, |style| style.w_full())
      .rounded_sm()
      .border_1()
      .border_color(border_color)
      .bg(theme.background.tertiary)
      .focus(move |style| style.border_color(focus_border))
      .on_key_down(move |event, window, cx| {
        let stepped = state_for_keys.update(cx, |state, cx| state.handle_key_down(event, cx));
        if stepped {
          window.invalidate_character_coordinates();
          cx.stop_propagation();
        }
      })
      .track_focus(&focus_handle)
      .when_some(self.prefix, |style, prefix| {
        style.child(div().pl_2().text_color(theme.text.secondary).child(prefix))
      })
      .child(
        TextInput::new(input)
          .placeholder(self.placeholder.unwrap_or_default())
          .theme(theme)
          .size(self.size.text_input_size())
          .appearance(false)
          .full_width(true),
      )
      .when_some(self.suffix, |style, suffix| {
        style.child(div().pr_2().text_color(theme.text.secondary).child(suffix))
      })
      .when(self.controls == NumberInputControls::Stepper, |style| {
        style.child(render_stepper(self.state, focus_handle, theme, interaction_disabled))
      })
  }
}

/// Renders focus-preserving pointer controls for incrementing and decrementing a number.
///
/// # Parameters
///
/// `state` owns numeric stepping behavior.
/// `focus_handle` identifies the nested text editor that must retain focus.
/// `theme` supplies semantic visual tokens.
/// `disabled` prevents pointer-driven stepping when editing is unavailable.
///
/// # Returns
///
/// A compact vertical increment/decrement control group.
fn render_stepper(
  state: Entity<NumberInputState>,
  focus_handle: gpui::FocusHandle,
  theme: UIThemes,
  disabled: bool,
) -> gpui::Div {
  div()
    .w(STEPPER_WIDTH)
    .flex_none()
    .h_full()
    .flex()
    .flex_col()
    .border_l_1()
    .border_color(theme.border.primary)
    .child(render_stepper_button(
      state.clone(),
      focus_handle.clone(),
      theme,
      disabled,
      1,
      "icons/number-increment.svg",
    ))
    .child(render_stepper_button(
      state,
      focus_handle,
      theme,
      disabled,
      -1,
      "icons/number-decrement.svg",
    ))
}

/// Renders one focus-preserving numeric stepper button.
///
/// # Parameters
///
/// `state` owns numeric stepping behavior.
/// `focus_handle` identifies the nested text editor that must retain focus.
/// `theme` supplies semantic visual tokens.
/// `disabled` prevents pointer-driven stepping when editing is unavailable.
/// `count` supplies the signed step count.
/// `icon_path` identifies the visual-only step direction icon.
///
/// # Returns
///
/// A half-height pointer target for one numeric step direction.
fn render_stepper_button(
  state: Entity<NumberInputState>,
  focus_handle: gpui::FocusHandle,
  theme: UIThemes,
  disabled: bool,
  count: i32,
  icon_path: &'static str,
) -> gpui::Div {
  div()
    .flex_1()
    .flex()
    .items_center()
    .justify_center()
    .text_color(if disabled {
      theme.text.disabled
    } else {
      theme.text.secondary
    })
    .cursor(if disabled {
      CursorStyle::Arrow
    } else {
      CursorStyle::PointingHand
    })
    .when(!disabled, |style| {
      style.hover(move |style| style.bg(theme.background.hover))
    })
    .when(!disabled, |style| {
      style.on_mouse_down(MouseButton::Left, move |_, window, cx| {
        window.focus(&focus_handle, cx);
        let stepped = state.update(cx, |state, cx| state.step_by(count, cx));
        if stepped {
          window.invalidate_character_coordinates();
          cx.stop_propagation();
        }
      })
    })
    .child(Icon::new(icon_path).size(px(12.0)).color(if disabled {
      theme.text.disabled
    } else {
      theme.text.secondary
    }))
}

impl NumberInputSize {
  /// Maps one numeric-input size to the matching text-input size.
  ///
  /// # Parameters
  ///
  /// This method reads the selected numeric-input size.
  ///
  /// # Returns
  ///
  /// The matching text-input size variant.
  fn text_input_size(self) -> TextInputSize {
    match self {
      Self::Small => TextInputSize::Small,
      Self::Medium => TextInputSize::Medium,
      Self::Large => TextInputSize::Large,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  /// Verifies the default numeric size maps to the default text input size.
  #[test]
  fn number_input_size_should_use_medium_text_input_size_by_default() {
    assert_eq!(NumberInputSize::default().text_input_size(), TextInputSize::Medium);
  }

  #[test]
  fn number_input_controls_should_show_steppers_by_default() {
    assert_eq!(NumberInputControls::default(), NumberInputControls::Stepper);
  }
}
