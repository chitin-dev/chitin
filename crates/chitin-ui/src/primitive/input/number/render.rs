//! Rendering for the reusable numeric input primitive.

use gpui::{
  App, CursorStyle, Entity, InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels, RenderOnce,
  SharedString, Window, div, prelude::*, px,
};

use super::NumberInputState;
use crate::{
  primitive::icon::Icon,
  primitive::input::{
    appearance::InputAppearanceStyle,
    text::{TextInput, TextInputColors, TextInputSize, TextInputStyle, TextInputVariant},
  },
  themes::{UIThemes, builtins},
};

const DEFAULT_NUMBER_INPUT_WIDTH: Pixels = px(160.0);
const STEPPER_WIDTH: Pixels = px(20.0);

/// Built-in theme-based appearance variants for a [`NumberInput`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NumberInputVariant {
  /// A prominent numeric input surface for form controls.
  Primary,
  /// A neutral numeric input surface for panels and toolbars.
  #[default]
  Secondary,
  /// A low-chrome numeric input for composite controls that provide their own shell.
  Transparent,
}

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

/// Visual-only overrides for a [`NumberInput`].
///
/// This style intentionally cannot alter numeric parsing, validation, focus
/// ownership, keyboard stepping, disabled behavior, or emitted semantic events.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NumberInputStyle {
  appearance: InputAppearanceStyle,
  invalid_border: Option<gpui::Rgba>,
  prefix_foreground: Option<gpui::Rgba>,
  suffix_foreground: Option<gpui::Rgba>,
  stepper_border: Option<gpui::Rgba>,
  stepper_hover_background: Option<gpui::Rgba>,
  stepper_foreground: Option<gpui::Rgba>,
}

impl NumberInputStyle {
  /// Creates a style with no overrides.
  ///
  /// # Parameters
  ///
  /// This function takes no Rust parameters.
  ///
  /// # Returns
  ///
  /// A visual style that preserves the selected [`NumberInputVariant`] defaults.
  pub fn new() -> Self {
    Self::default()
  }

  /// Overrides the numeric-input background color.
  ///
  /// # Parameters
  ///
  /// * `color` is the semantic theme token to use for the input surface.
  ///
  /// # Returns
  ///
  /// The updated visual-only style.
  pub fn background(mut self, color: gpui::Rgba) -> Self {
    self.appearance.background = Some(color);
    self
  }

  /// Overrides the numeric-input text color.
  ///
  /// # Parameters
  ///
  /// * `color` is the semantic theme token to use for entered text.
  ///
  /// # Returns
  ///
  /// The updated visual-only style.
  pub fn foreground(mut self, color: gpui::Rgba) -> Self {
    self.appearance.foreground = Some(color);
    self
  }

  /// Overrides the numeric-input placeholder text color.
  ///
  /// # Parameters
  ///
  /// * `color` is the semantic theme token to use for placeholder text.
  ///
  /// # Returns
  ///
  /// The updated visual-only style.
  pub fn placeholder_foreground(mut self, color: gpui::Rgba) -> Self {
    self.appearance.placeholder_foreground = Some(color);
    self
  }

  /// Overrides the selected-range background color.
  ///
  /// # Parameters
  ///
  /// * `color` is the semantic theme token to use behind selected text.
  ///
  /// # Returns
  ///
  /// The updated visual-only style.
  pub fn selection_background(mut self, color: gpui::Rgba) -> Self {
    self.appearance.selection_background = Some(color);
    self
  }

  /// Overrides the selected-range text color.
  ///
  /// # Parameters
  ///
  /// * `color` is the semantic theme token to use for selected glyphs.
  ///
  /// # Returns
  ///
  /// The updated visual-only style.
  pub fn selection_foreground(mut self, color: gpui::Rgba) -> Self {
    self.appearance.selection_foreground = Some(color);
    self
  }

  /// Overrides the caret color.
  ///
  /// # Parameters
  ///
  /// * `color` is the semantic theme token to use for the insertion caret.
  ///
  /// # Returns
  ///
  /// The updated visual-only style.
  pub fn caret(mut self, color: gpui::Rgba) -> Self {
    self.appearance.caret = Some(color);
    self
  }

  /// Overrides the default numeric-input border color.
  ///
  /// # Parameters
  ///
  /// * `color` is the semantic theme token to use for the idle border.
  ///
  /// # Returns
  ///
  /// The updated visual-only style.
  pub fn border(mut self, color: gpui::Rgba) -> Self {
    self.appearance.border = Some(color);
    self
  }

  /// Overrides the numeric-input border color while focused.
  ///
  /// # Parameters
  ///
  /// * `color` is the semantic theme token to use for the focused border.
  ///
  /// # Returns
  ///
  /// The updated visual-only style.
  pub fn focus_border(mut self, color: gpui::Rgba) -> Self {
    self.appearance.focus_border = Some(color);
    self
  }

  /// Overrides the numeric-input border color for an invalid draft.
  ///
  /// # Parameters
  ///
  /// * `color` is the semantic theme token to use for validation errors.
  ///
  /// # Returns
  ///
  /// The updated visual-only style.
  pub fn invalid_border(mut self, color: gpui::Rgba) -> Self {
    self.invalid_border = Some(color);
    self
  }

  /// Overrides the non-editable prefix text color.
  ///
  /// # Parameters
  ///
  /// * `color` is the semantic theme token to use for prefix text.
  ///
  /// # Returns
  ///
  /// The updated visual-only style.
  pub fn prefix_foreground(mut self, color: gpui::Rgba) -> Self {
    self.prefix_foreground = Some(color);
    self
  }

  /// Overrides the non-editable suffix text color.
  ///
  /// # Parameters
  ///
  /// * `color` is the semantic theme token to use for suffix text.
  ///
  /// # Returns
  ///
  /// The updated visual-only style.
  pub fn suffix_foreground(mut self, color: gpui::Rgba) -> Self {
    self.suffix_foreground = Some(color);
    self
  }

  /// Sets an explicit visual width.
  ///
  /// # Parameters
  ///
  /// * `width` is the fixed rendered width before `full_width` expansion.
  ///
  /// # Returns
  ///
  /// The updated visual-only style.
  pub fn width(mut self, width: Pixels) -> Self {
    self.appearance.width = Some(width);
    self
  }

  /// Sets an explicit visual height.
  ///
  /// # Parameters
  ///
  /// * `height` is the fixed rendered height for the input shell.
  ///
  /// # Returns
  ///
  /// The updated visual-only style.
  pub fn height(mut self, height: Pixels) -> Self {
    self.appearance.height = Some(height);
    self
  }

  /// Sets horizontal padding around the numeric text viewport.
  ///
  /// # Parameters
  ///
  /// * `padding` is the inline spacing around the text viewport.
  ///
  /// # Returns
  ///
  /// The updated visual-only style.
  pub fn horizontal_padding(mut self, padding: Pixels) -> Self {
    self.appearance.horizontal_padding = Some(padding);
    self
  }

  /// Overrides the border separating the stepper controls from the editor.
  ///
  /// # Parameters
  ///
  /// * `color` is the semantic theme token to use for the stepper separator.
  ///
  /// # Returns
  ///
  /// The updated visual-only style.
  pub fn stepper_border(mut self, color: gpui::Rgba) -> Self {
    self.stepper_border = Some(color);
    self
  }

  /// Overrides the stepper-button background while hovered.
  ///
  /// # Parameters
  ///
  /// * `color` is the semantic theme token to use for an enabled hover surface.
  ///
  /// # Returns
  ///
  /// The updated visual-only style.
  pub fn stepper_hover_background(mut self, color: gpui::Rgba) -> Self {
    self.stepper_hover_background = Some(color);
    self
  }

  /// Overrides the enabled stepper icon color.
  ///
  /// # Parameters
  ///
  /// * `color` is the semantic theme token to use for the stepper icons.
  ///
  /// # Returns
  ///
  /// The updated visual-only style.
  pub fn stepper_foreground(mut self, color: gpui::Rgba) -> Self {
    self.stepper_foreground = Some(color);
    self
  }
}

/// A numeric editing control bound to its own [`NumberInputState`] entity.
#[derive(IntoElement)]
pub struct NumberInput {
  state: Entity<NumberInputState>,
  prefix: Option<SharedString>,
  suffix: Option<SharedString>,
  placeholder: Option<SharedString>,
  theme: UIThemes,
  variant: NumberInputVariant,
  size: NumberInputSize,
  controls: NumberInputControls,
  style: NumberInputStyle,
  full_width: bool,
}

impl NumberInput {
  /// Creates a numeric input that renders and updates `state`.
  ///
  /// # Parameters
  ///
  /// * `state` owns numeric editing, focus, and semantic events.
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
      variant: NumberInputVariant::default(),
      size: NumberInputSize::default(),
      controls: NumberInputControls::default(),
      style: NumberInputStyle::default(),
      full_width: false,
    }
  }

  /// Sets non-editable content rendered before the numeric value.
  ///
  /// # Parameters
  ///
  /// * `prefix` supplies the visual prefix text.
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
  /// * `suffix` supplies the visual suffix text.
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
  /// * `placeholder` supplies the visual empty-state text.
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
  /// * `theme` supplies semantic colors for the control.
  ///
  /// # Returns
  ///
  /// This input configured with the supplied theme.
  pub fn theme(mut self, theme: UIThemes) -> Self {
    self.theme = theme;
    self
  }

  /// Sets one of the built-in theme-based appearance variants.
  ///
  /// # Parameters
  ///
  /// * `variant` selects a semantic visual treatment.
  ///
  /// # Returns
  ///
  /// The updated numeric input builder.
  pub fn variant(mut self, variant: NumberInputVariant) -> Self {
    self.variant = variant;
    self
  }

  /// Sets the visual size.
  ///
  /// # Parameters
  ///
  /// * `size` selects a predefined control size.
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
  /// * `controls` selects the pointer controls rendered by this input.
  ///
  /// # Returns
  ///
  /// This input configured with the supplied controls.
  pub fn controls(mut self, controls: NumberInputControls) -> Self {
    self.controls = controls;
    self
  }

  /// Applies visual-only appearance overrides.
  ///
  /// # Parameters
  ///
  /// * `style` supplies component-specific visual overrides.
  ///
  /// # Returns
  ///
  /// The updated numeric input builder.
  pub fn style(mut self, style: NumberInputStyle) -> Self {
    self.style = style;
    self
  }

  /// Makes the control fill its parent width when enabled.
  ///
  /// # Parameters
  ///
  /// * `full_width` selects whether the control fills its parent width.
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
  /// * `cx` reads the numeric state and creates the child text input.
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
    let colors = NumberInputColors::new(theme, self.variant, self.style, interaction_disabled);
    let border_color = if validation_error {
      self.style.invalid_border.unwrap_or(colors.invalid_border)
    } else {
      colors.border
    };
    let focus_border = if validation_error {
      self.style.invalid_border.unwrap_or(colors.invalid_border)
    } else {
      colors.focus_border
    };
    let state_for_keys = self.state.clone();
    let affix_font_size = self.size.font_size();

    div()
      .flex()
      .items_center()
      .w(self.style.appearance.width.unwrap_or(DEFAULT_NUMBER_INPUT_WIDTH))
      .when(self.full_width, |style| style.w_full())
      .rounded_sm()
      .border_1()
      .border_color(border_color)
      .bg(colors.background)
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
        style.child(
          div()
            .pl_2()
            .text_size(affix_font_size)
            .text_color(colors.prefix_foreground)
            .child(prefix),
        )
      })
      .child(
        TextInput::new(input)
          .placeholder(self.placeholder.unwrap_or_default())
          .theme(theme)
          .variant(self.variant.text_input_variant())
          .style(TextInputStyle::from_appearance(self.style.appearance))
          .size(self.size.text_input_size())
          .appearance(false)
          .full_width(true),
      )
      .when_some(self.suffix, |style, suffix| {
        style.child(
          div()
            .pr_2()
            .text_color(colors.suffix_foreground)
            .text_size(affix_font_size)
            .child(suffix),
        )
      })
      .when(self.controls == NumberInputControls::Stepper, |style| {
        style.child(render_stepper(self.state, focus_handle, colors, interaction_disabled))
      })
  }
}

/// Renders focus-preserving pointer controls for incrementing and decrementing a number.
///
/// # Parameters
///
/// * `state` owns numeric stepping behavior.
/// * `focus_handle` identifies the nested text editor that must retain focus.
/// * `colors` supplies resolved semantic visual tokens.
/// * `disabled` prevents pointer-driven stepping when editing is unavailable.
///
/// # Returns
///
/// A compact vertical increment/decrement control group.
fn render_stepper(
  state: Entity<NumberInputState>,
  focus_handle: gpui::FocusHandle,
  colors: NumberInputColors,
  disabled: bool,
) -> gpui::Div {
  div()
    .w(STEPPER_WIDTH)
    .flex_none()
    .h_full()
    .flex()
    .flex_col()
    .border_l_1()
    .border_color(colors.stepper_border)
    .child(render_stepper_button(
      state.clone(),
      focus_handle.clone(),
      colors,
      disabled,
      1,
      "icons/number-increment.svg",
    ))
    .child(render_stepper_button(
      state,
      focus_handle,
      colors,
      disabled,
      -1,
      "icons/number-decrement.svg",
    ))
}

/// Renders one focus-preserving numeric stepper button.
///
/// # Parameters
///
/// * `state` owns numeric stepping behavior.
/// * `focus_handle` identifies the nested text editor that must retain focus.
/// * `colors` supplies resolved semantic visual tokens.
/// * `disabled` prevents pointer-driven stepping when editing is unavailable.
/// * `count` supplies the signed step count.
/// * `icon_path` identifies the visual-only step direction icon.
///
/// # Returns
///
/// A half-height pointer target for one numeric step direction.
///
/// # Notice
///
/// This buttons is not similar with primitive button, number inputs is not a
/// composite components formed by [`TextInput`] and [`Button`], it's input core
/// is derived from [`TextInput`] but it should be in same primitive level with
/// them.
fn render_stepper_button(
  state: Entity<NumberInputState>,
  focus_handle: gpui::FocusHandle,
  colors: NumberInputColors,
  disabled: bool,
  count: i32,
  icon_path: &'static str,
) -> gpui::Div {
  div()
    .flex_1()
    .flex()
    .items_center()
    .justify_center()
    .text_color(colors.stepper_foreground)
    .cursor(if disabled {
      CursorStyle::Arrow
    } else {
      CursorStyle::PointingHand
    })
    .when(!disabled, |style| {
      style.hover(move |style| style.bg(colors.stepper_hover_background))
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
    .child(Icon::new(icon_path).size(px(12.0)).color(colors.stepper_foreground))
}

/// Resolved colors for one numeric-input appearance state.
#[derive(Clone, Copy)]
struct NumberInputColors {
  background: gpui::Rgba,
  border: gpui::Rgba,
  focus_border: gpui::Rgba,
  invalid_border: gpui::Rgba,
  prefix_foreground: gpui::Rgba,
  suffix_foreground: gpui::Rgba,
  stepper_border: gpui::Rgba,
  stepper_hover_background: gpui::Rgba,
  stepper_foreground: gpui::Rgba,
}

impl NumberInputColors {
  /// Resolves semantic numeric-input colors for one appearance state.
  ///
  /// # Parameters
  ///
  /// * `theme` supplies semantic UI color tokens.
  /// * `variant` selects the built-in visual treatment.
  /// * `style` supplies visual-only component overrides.
  /// * `disabled` describes whether the input should use disabled visuals.
  ///
  /// # Returns
  ///
  /// Resolved colors for the rendered numeric input.
  fn new(theme: UIThemes, variant: NumberInputVariant, style: NumberInputStyle, disabled: bool) -> Self {
    let text_colors = TextInputColors::new(
      theme,
      variant.text_input_variant(),
      TextInputStyle::from_appearance(style.appearance),
      disabled,
    );

    if disabled {
      return Self {
        background: text_colors.background,
        border: text_colors.border,
        focus_border: text_colors.focus_border,
        invalid_border: text_colors.border,
        prefix_foreground: theme.text.disabled,
        suffix_foreground: theme.text.disabled,
        stepper_border: text_colors.border,
        stepper_hover_background: theme.background.secondary,
        stepper_foreground: theme.text.disabled,
      };
    }

    Self {
      background: text_colors.background,
      border: text_colors.border,
      focus_border: text_colors.focus_border,
      invalid_border: theme.text.error,
      prefix_foreground: style.prefix_foreground.unwrap_or(theme.text.secondary),
      suffix_foreground: style.suffix_foreground.unwrap_or(theme.text.secondary),
      stepper_border: style.stepper_border.unwrap_or(text_colors.border),
      stepper_hover_background: style.stepper_hover_background.unwrap_or(theme.background.hover),
      stepper_foreground: style.stepper_foreground.unwrap_or(theme.text.secondary),
    }
  }
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

  /// Returns the font size shared by numeric text, placeholders, and affixes.
  ///
  /// # Parameters
  ///
  /// This method reads the selected numeric-input size.
  ///
  /// # Returns
  ///
  /// The font size used by the embedded text input.
  fn font_size(self) -> Pixels {
    self.text_input_size().font_size()
  }
}

impl NumberInputVariant {
  /// Maps one numeric-input appearance variant to its embedded text-input variant.
  ///
  /// # Parameters
  ///
  /// This method reads the selected numeric-input appearance variant.
  ///
  /// # Returns
  ///
  /// The matching text-input appearance variant.
  fn text_input_variant(self) -> TextInputVariant {
    match self {
      Self::Primary => TextInputVariant::Primary,
      Self::Secondary => TextInputVariant::Secondary,
      Self::Transparent => TextInputVariant::Transparent,
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
  fn number_input_size_should_use_the_embedded_text_font_size_for_affixes() {
    assert_eq!(NumberInputSize::Small.font_size(), px(12.0));
  }

  #[test]
  fn number_input_controls_should_show_steppers_by_default() {
    assert_eq!(NumberInputControls::default(), NumberInputControls::Stepper);
  }

  #[test]
  fn number_input_variant_should_make_transparent_background() {
    let colors = NumberInputColors::new(
      builtins::dark(),
      NumberInputVariant::Transparent,
      NumberInputStyle::new(),
      false,
    );

    assert_eq!(colors.background, builtins::TRANSPARENT);
  }

  #[test]
  fn number_input_style_should_store_flat_visual_overrides() {
    let theme = builtins::dark();
    let style = NumberInputStyle::new()
      .background(theme.background.primary)
      .foreground(theme.text.primary)
      .placeholder_foreground(theme.text.secondary)
      .selection_background(theme.background.selection)
      .selection_foreground(theme.accent.foreground)
      .caret(theme.accent.primary)
      .border(theme.border.primary)
      .focus_border(theme.border.focus)
      .invalid_border(theme.text.error)
      .prefix_foreground(theme.text.secondary)
      .width(px(240.0))
      .height(px(32.0))
      .horizontal_padding(px(12.0))
      .suffix_foreground(theme.text.primary)
      .stepper_border(theme.border.muted)
      .stepper_hover_background(theme.background.hover)
      .stepper_foreground(theme.text.secondary);

    assert_eq!(style.appearance.background, Some(theme.background.primary));
    assert_eq!(style.appearance.foreground, Some(theme.text.primary));
    assert_eq!(style.appearance.placeholder_foreground, Some(theme.text.secondary));
    assert_eq!(style.appearance.selection_background, Some(theme.background.selection));
    assert_eq!(style.appearance.selection_foreground, Some(theme.accent.foreground));
    assert_eq!(style.appearance.caret, Some(theme.accent.primary));
    assert_eq!(style.appearance.border, Some(theme.border.primary));
    assert_eq!(style.appearance.focus_border, Some(theme.border.focus));
    assert_eq!(style.invalid_border, Some(theme.text.error));
    assert_eq!(style.prefix_foreground, Some(theme.text.secondary));
    assert_eq!(style.appearance.width, Some(px(240.0)));
    assert_eq!(style.appearance.height, Some(px(32.0)));
    assert_eq!(style.appearance.horizontal_padding, Some(px(12.0)));
    assert_eq!(style.suffix_foreground, Some(theme.text.primary));
    assert_eq!(style.stepper_border, Some(theme.border.muted));
    assert_eq!(style.stepper_hover_background, Some(theme.background.hover));
    assert_eq!(style.stepper_foreground, Some(theme.text.secondary));
  }
}
