//! Rendering for the reusable single-line text input primitive.

use std::time::Instant;

use gpui::{
  App, Bounds, ClipboardItem, CursorStyle, Element, ElementId, Entity, GlobalElementId, InspectorElementId,
  InteractiveElement, IntoElement, KeyDownEvent, LayoutId, MouseButton, PaintQuad, ParentElement, Pixels, RenderOnce,
  ShapedLine, SharedString, Style, TextColor, Window, div, fill, point, prelude::*, px, relative, size,
};

use super::TextInputState;
use crate::themes::{UIThemes, builtins};

const DEFAULT_TEXT_INPUT_WIDTH: Pixels = px(240.0);
const DEFAULT_TEXT_INPUT_PADDING_X: Pixels = px(8.0);
const CARET_WIDTH: Pixels = px(2.0);

/// Built-in theme-based appearance variants for a [`TextInput`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextInputVariant {
  /// A prominent input surface for form controls.
  Primary,
  /// A neutral input surface for panels and toolbars.
  #[default]
  Secondary,
  /// A low-chrome input for composite controls that provide their own shell.
  Transparent,
}

/// Visual-only overrides for a [`TextInput`].
///
/// This style intentionally cannot alter editing behavior, focus ownership,
/// disabled behavior, keyboard handling, or emitted semantic events.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextInputStyle {
  background: Option<gpui::Rgba>,
  foreground: Option<gpui::Rgba>,
  placeholder_foreground: Option<gpui::Rgba>,
  selection_background: Option<gpui::Rgba>,
  caret: Option<gpui::Rgba>,
  border: Option<gpui::Rgba>,
  focus_border: Option<gpui::Rgba>,
  width: Option<Pixels>,
  height: Option<Pixels>,
  horizontal_padding: Option<Pixels>,
}

impl TextInputStyle {
  /// Creates a style with no overrides.
  ///
  /// # Parameters
  ///
  /// This function takes no Rust parameters.
  ///
  /// # Returns
  ///
  /// A visual style that preserves the selected [`TextInputVariant`] defaults.
  pub fn new() -> Self {
    Self::default()
  }

  /// Overrides the input background color.
  ///
  /// # Parameters
  ///
  /// `color` is the semantic theme token to use for the input surface.
  ///
  /// # Returns
  ///
  /// The updated visual-only style.
  pub fn background(mut self, color: gpui::Rgba) -> Self {
    self.background = Some(color);
    self
  }

  /// Overrides the input text color.
  ///
  /// # Parameters
  ///
  /// `color` is the semantic theme token to use for entered text.
  ///
  /// # Returns
  ///
  /// The updated visual-only style.
  pub fn foreground(mut self, color: gpui::Rgba) -> Self {
    self.foreground = Some(color);
    self
  }

  /// Overrides the placeholder text color.
  ///
  /// # Parameters
  ///
  /// `color` is the semantic theme token to use for placeholder text.
  ///
  /// # Returns
  ///
  /// The updated visual-only style.
  pub fn placeholder_foreground(mut self, color: gpui::Rgba) -> Self {
    self.placeholder_foreground = Some(color);
    self
  }

  /// Overrides the selected-range background color.
  ///
  /// # Parameters
  ///
  /// `color` is the semantic theme token to use behind selected text.
  ///
  /// # Returns
  ///
  /// The updated visual-only style.
  pub fn selection_background(mut self, color: gpui::Rgba) -> Self {
    self.selection_background = Some(color);
    self
  }

  /// Overrides the caret color.
  ///
  /// # Parameters
  ///
  /// `color` is the semantic theme token to use for the insertion caret.
  ///
  /// # Returns
  ///
  /// The updated visual-only style.
  pub fn caret(mut self, color: gpui::Rgba) -> Self {
    self.caret = Some(color);
    self
  }

  /// Overrides the default border color.
  ///
  /// # Parameters
  ///
  /// `color` is the semantic theme token to use for the idle border.
  ///
  /// # Returns
  ///
  /// The updated visual-only style.
  pub fn border(mut self, color: gpui::Rgba) -> Self {
    self.border = Some(color);
    self
  }

  /// Overrides the border color while the input is focused.
  ///
  /// # Parameters
  ///
  /// `color` is the semantic theme token to use for the focused border.
  ///
  /// # Returns
  ///
  /// The updated visual-only style.
  pub fn focus_border(mut self, color: gpui::Rgba) -> Self {
    self.focus_border = Some(color);
    self
  }

  /// Sets an explicit visual width.
  ///
  /// # Parameters
  ///
  /// `width` is the fixed rendered width before `full_width` expansion.
  ///
  /// # Returns
  ///
  /// The updated visual-only style.
  pub fn width(mut self, width: Pixels) -> Self {
    self.width = Some(width);
    self
  }

  /// Sets an explicit visual height.
  ///
  /// # Parameters
  ///
  /// `height` is the fixed rendered height for the input shell.
  ///
  /// # Returns
  ///
  /// The updated visual-only style.
  pub fn height(mut self, height: Pixels) -> Self {
    self.height = Some(height);
    self
  }

  /// Sets horizontal padding.
  ///
  /// # Parameters
  ///
  /// `padding` is the inline spacing around the text viewport.
  ///
  /// # Returns
  ///
  /// The updated visual-only style.
  pub fn horizontal_padding(mut self, padding: Pixels) -> Self {
    self.horizontal_padding = Some(padding);
    self
  }
}

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
  placeholder: Option<SharedString>,
  theme: UIThemes,
  variant: TextInputVariant,
  style: TextInputStyle,
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
      variant: TextInputVariant::default(),
      style: TextInputStyle::default(),
      size: TextInputSize::default(),
      full_width: false,
      appearance: true,
    }
  }

  /// Sets placeholder text shown while the value is empty.
  pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
    self.placeholder = Some(placeholder.into());
    self
  }

  /// Sets the semantic theme used for this input.
  pub fn theme(mut self, theme: UIThemes) -> Self {
    self.theme = theme;
    self
  }

  /// Sets one of the built-in theme-based appearance variants.
  ///
  /// # Parameters
  ///
  /// `variant` selects a semantic visual treatment.
  ///
  /// # Returns
  ///
  /// The updated text input builder.
  pub fn variant(mut self, variant: TextInputVariant) -> Self {
    self.variant = variant;
    self
  }

  /// Applies visual-only appearance overrides.
  ///
  /// # Parameters
  ///
  /// `style` supplies component-specific visual overrides.
  ///
  /// # Returns
  ///
  /// The updated text input builder.
  pub fn style(mut self, style: TextInputStyle) -> Self {
    self.style = style;
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
  /// Renders the input shell, then delegates line painting to [`TextInputContent`].
  fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
    // Resolve the visual metrics and focus handle before touching the entity.
    let metrics = self.size.metrics();
    let focus_handle = self.state.read(cx).focus_handle().clone();
    let focused = focus_handle.is_focused(window);
    let state = self.state.clone();

    // Mirror the observed GPUI focus state into the model so it can emit Focus/Blur events.
    self.state.update(cx, |state, cx| state.sync_focus(focused, cx));

    // Snapshot every value the content element needs so it never borrows the entity.
    let (text, selection, disabled, readonly, caret_visible) = {
      let state = self.state.read(cx);
      (
        state.text().to_owned(),
        state.selection(),
        state.is_disabled(),
        state.is_readonly(),
        focused && state.caret_visible(Instant::now()),
      )
    };

    // Keep the frame loop alive while focused so the caret continues to blink.
    if focused && !disabled {
      window.request_animation_frame();
    }

    let theme = self.theme;
    let colors = TextInputColors::new(theme, self.variant, self.style, disabled);
    let state_for_mouse = state.clone();
    let state_for_keys = state.clone();

    div()
      .flex()
      .items_center()
      .w(self.style.width.unwrap_or(DEFAULT_TEXT_INPUT_WIDTH))
      .when(self.full_width, |style| style.w_full())
      .h(self.style.height.unwrap_or(metrics.height))
      .px(self.style.horizontal_padding.unwrap_or(DEFAULT_TEXT_INPUT_PADDING_X))
      .rounded_sm()
      .when(self.appearance, |style| {
        style
          .border_1()
          .border_color(colors.border)
          .focus(move |style| style.border_color(colors.focus_border))
          .bg(colors.background)
      })
      .text_size(metrics.font_size)
      .text_color(colors.foreground)
      .cursor(if disabled {
        CursorStyle::Arrow
      } else {
        CursorStyle::IBeam
      })
      .on_mouse_down(MouseButton::Left, move |_, window, cx| {
        if state_for_mouse.read(cx).is_disabled() {
          return;
        }
        let focus_handle = state_for_mouse.read(cx).focus_handle().clone();
        window.focus(&focus_handle, cx);
        cx.stop_propagation();
      })
      // Because [`TextInputState`] should stay platform/ui-service independent.
      // Ctrl+C and Ctrl+V need platform clipboard access. They are local to the
      // component/entity model
      .on_key_down(move |event, _, cx| {
        let handled = if is_clipboard_shortcut(event, "c") {
          copy_selection_to_clipboard(&state_for_keys, cx)
        } else if is_clipboard_shortcut(event, "v") {
          paste_clipboard_text(&state_for_keys, cx)
        } else {
          state_for_keys.update(cx, |state, cx| state.handle_key_down(event, cx))
        };
        if handled {
          cx.stop_propagation();
        }
      })
      .track_focus(&focus_handle)
      // Clip the content box so long lines never paint outside the input.
      .child(
        div()
          .relative()
          .flex()
          .items_center()
          .min_w_0()
          .flex_1()
          .h_full()
          .overflow_hidden()
          .child(TextInputContent {
            text: text.into(),
            placeholder: self.placeholder.unwrap_or_default(),
            selection,
            colors,
            show_caret: focused && !disabled,
            caret_visible,
            metrics,
          }),
      )
      .when(readonly && !disabled, |style| style.cursor(CursorStyle::IBeam))
  }
}

/// Returns whether a key event is a platform clipboard shortcut for `key`.
///
/// # Parameters
///
/// `event` supplies the normalized GPUI key event.
///
/// `key` supplies the lower-case clipboard key to match.
///
/// # Returns
///
/// `true` when Ctrl or the platform modifier is pressed with `key`.
fn is_clipboard_shortcut(event: &KeyDownEvent, key: &str) -> bool {
  let modifiers = event.keystroke.modifiers;
  event.keystroke.key == key && (modifiers.control || modifiers.platform)
}

/// Copies the current selected input text into the platform clipboard.
///
/// # Parameters
///
/// `state` supplies the text input state entity.
///
/// `cx` provides platform clipboard access.
///
/// # Returns
///
/// `true` when selected text was copied.
fn copy_selection_to_clipboard(state: &Entity<TextInputState>, cx: &mut App) -> bool {
  let Some(selected_text) = state.read(cx).selected_text() else {
    return false;
  };

  if selected_text.is_empty() {
    return false;
  }

  cx.write_to_clipboard(ClipboardItem::new_string(selected_text.to_string()));
  true
}

/// Pastes text from the platform clipboard into the input selection.
///
/// # Parameters
///
/// `state` supplies the text input state entity.
///
/// `cx` provides platform clipboard access and state mutation.
///
/// # Returns
///
/// `true` when clipboard text changed the input.
fn paste_clipboard_text(state: &Entity<TextInputState>, cx: &mut App) -> bool {
  let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
    return false;
  };

  state.update(cx, |state, cx| state.insert_text(&text, cx))
}

/// A single shaped input line with selection and caret paint overlays.
struct TextInputContent {
  text: SharedString,
  placeholder: SharedString,
  selection: super::TextSelection,
  colors: TextInputColors,
  show_caret: bool,
  caret_visible: bool,
  metrics: TextInputMetrics,
}

/// Precomputed paint data: the shaped line plus optional selection and caret quads.
struct TextInputContentPrepaint {
  line: ShapedLine,
  selection: Option<PaintQuad>,
  caret: Option<PaintQuad>,
}

impl IntoElement for TextInputContent {
  type Element = Self;

  fn into_element(self) -> Self::Element {
    self
  }
}

impl Element for TextInputContent {
  type RequestLayoutState = ();
  type PrepaintState = TextInputContentPrepaint;

  fn id(&self) -> Option<ElementId> {
    None
  }

  fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
    None
  }

  /// Requests a box that fills the parent so clipping matches the content area.
  ///
  /// - `window`: used to register the requested layout box.
  ///
  /// Returns the layout id and no custom layout state.
  fn request_layout(
    &mut self,
    _: Option<&GlobalElementId>,
    _: Option<&InspectorElementId>,
    window: &mut Window,
    cx: &mut App,
  ) -> (LayoutId, Self::RequestLayoutState) {
    // Fill 100% of the parent; the outer container already imposes the real size.
    let mut style = Style::default();
    style.size.width = relative(1.).into();
    style.size.height = relative(1.).into();
    (window.request_layout(style, [], cx), ())
  }

  /// Shapes the line and computes the selection and caret quads for painting.
  ///
  /// - `bounds`: the layout box returned by [`Self::request_layout`].
  /// - `window`: provides the base text style and the text system used to shape the line.
  ///
  /// Returns the prepared paint state consumed by [`Self::paint`].
  fn prepaint(
    &mut self,
    _: Option<&GlobalElementId>,
    _: Option<&InspectorElementId>,
    bounds: Bounds<Pixels>,
    _: &mut Self::RequestLayoutState,
    window: &mut Window,
    _: &mut App,
  ) -> Self::PrepaintState {
    // Fall back to the placeholder when there is no real text yet.
    let empty = self.text.is_empty();
    let display_text = if empty {
      self.placeholder.clone()
    } else {
      self.text.clone()
    };

    // Build a text run whose color communicates placeholder vs real content.
    let text_style = window.text_style();
    let font_size = text_style.font_size.to_pixels(window.rem_size());
    let mut run = text_style.to_run(display_text.len());
    run.color = TextColor::from(if empty {
      self.colors.placeholder_foreground
    } else {
      self.colors.foreground
    });

    // Shape the whole logical line once; x_for_index maps byte offsets to pixels.
    let line = window.text_system().shape_line(display_text, font_size, &[run], None);

    // Selection highlight spans the shaped range and is vertically centered.
    let range = self.selection.range();
    let selection = (!empty && !range.is_empty()).then(|| {
      let start = line.x_for_index(range.start);
      let end = line.x_for_index(range.end);
      let selection_height = line.ascent + line.descent;
      let selection_top = bounds.top() + (bounds.size.height - selection_height) / 2.0;
      fill(
        Bounds::new(
          point(bounds.left() + start, selection_top),
          size(end - start, selection_height),
        ),
        self.colors.selection_background,
      )
    });

    // Caret is a thin quad at the head, shown only when the selection is collapsed.
    let caret = (self.show_caret && range.is_empty()).then(|| {
      let x = line.x_for_index(self.selection.head());
      let caret_top = bounds.top() + (bounds.size.height - self.metrics.caret_height) / 2.0;
      fill(
        Bounds::new(
          point(bounds.left() + x, caret_top),
          size(CARET_WIDTH, self.metrics.caret_height),
        ),
        self.colors.caret,
      )
    });
    TextInputContentPrepaint { line, selection, caret }
  }

  /// Paints selection, text, then caret so overlays keep the correct stacking order.
  ///
  /// - `bounds`: the layout box used as the text origin.
  /// - `window`/`cx`: forwarded to the shaped line painter.
  fn paint(
    &mut self,
    _: Option<&GlobalElementId>,
    _: Option<&InspectorElementId>,
    bounds: Bounds<Pixels>,
    _: &mut Self::RequestLayoutState,
    prepaint: &mut Self::PrepaintState,
    window: &mut Window,
    cx: &mut App,
  ) {
    // Selection sits below the glyphs.
    if let Some(selection) = prepaint.selection.take() {
      window.paint_quad(selection);
    }
    // Text glyphs in the middle.
    let _ = prepaint.line.paint(bounds.origin, bounds.size.height, window, cx);
    // Caret on top, gated by the blink phase computed during render.
    if let Some(caret) = prepaint.caret.take()
      && self.caret_visible
    {
      window.paint_quad(caret);
    }
  }
}

/// Resolved visual colors for one rendered text-input state.
#[derive(Clone, Copy)]
struct TextInputColors {
  background: gpui::Rgba,
  foreground: gpui::Rgba,
  placeholder_foreground: gpui::Rgba,
  selection_background: gpui::Rgba,
  caret: gpui::Rgba,
  border: gpui::Rgba,
  focus_border: gpui::Rgba,
}

impl TextInputColors {
  /// Resolves semantic text-input colors for one appearance state.
  ///
  /// # Parameters
  ///
  /// `theme` supplies semantic UI color tokens.
  ///
  /// `variant` selects the built-in visual treatment.
  ///
  /// `style` supplies visual-only component overrides.
  ///
  /// `disabled` describes whether the input should use disabled visuals.
  ///
  /// # Returns
  ///
  /// Resolved colors for the rendered input.
  fn new(theme: UIThemes, variant: TextInputVariant, style: TextInputStyle, disabled: bool) -> Self {
    if disabled {
      return Self {
        background: theme.background.secondary,
        foreground: theme.text.disabled,
        placeholder_foreground: theme.text.disabled,
        selection_background: theme.background.selection,
        caret: theme.accent.primary,
        border: theme.border.muted,
        focus_border: theme.border.muted,
      };
    }

    let colors = match variant {
      TextInputVariant::Primary => Self {
        background: theme.background.secondary,
        foreground: theme.text.primary,
        placeholder_foreground: theme.text.secondary,
        selection_background: theme.background.selection,
        caret: theme.accent.primary,
        border: theme.border.primary,
        focus_border: theme.border.focus,
      },
      TextInputVariant::Secondary => Self {
        background: theme.background.tertiary,
        foreground: theme.text.primary,
        placeholder_foreground: theme.text.disabled,
        selection_background: theme.background.selection,
        caret: theme.accent.primary,
        border: theme.border.primary,
        focus_border: theme.border.focus,
      },
      TextInputVariant::Transparent => Self {
        background: builtins::TRANSPARENT,
        foreground: theme.text.primary,
        placeholder_foreground: theme.text.disabled,
        selection_background: theme.background.selection,
        caret: theme.accent.primary,
        border: builtins::TRANSPARENT,
        focus_border: builtins::TRANSPARENT,
      },
    };

    Self {
      background: style.background.unwrap_or(colors.background),
      foreground: style.foreground.unwrap_or(colors.foreground),
      placeholder_foreground: style.placeholder_foreground.unwrap_or(colors.placeholder_foreground),
      selection_background: style.selection_background.unwrap_or(colors.selection_background),
      caret: style.caret.unwrap_or(colors.caret),
      border: style.border.unwrap_or(colors.border),
      focus_border: style.focus_border.unwrap_or(colors.focus_border),
    }
  }
}

/// Pixel metrics derived from the selected [`TextInputSize`] variant.
#[derive(Clone, Copy)]
struct TextInputMetrics {
  height: Pixels,
  font_size: Pixels,
  caret_height: Pixels,
}

impl TextInputSize {
  /// Returns the height, font size, and caret height for this size variant.
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
  use gpui::{KeyDownEvent, Keystroke, Modifiers};

  #[test]
  fn text_input_size_should_use_medium_height_by_default() {
    assert_eq!(TextInputSize::default().metrics().height, px(30.0));
  }

  #[test]
  fn text_input_style_should_override_focus_border_color() {
    let theme = builtins::dark();
    let colors = TextInputColors::new(
      theme,
      TextInputVariant::Secondary,
      TextInputStyle::new().focus_border(builtins::TRANSPARENT),
      false,
    );

    assert_eq!(colors.focus_border, builtins::TRANSPARENT);
  }

  #[test]
  fn text_input_variant_should_make_transparent_background() {
    let theme = builtins::dark();
    let colors = TextInputColors::new(theme, TextInputVariant::Transparent, TextInputStyle::new(), false);

    assert_eq!(colors.background, builtins::TRANSPARENT);
  }

  #[test]
  fn text_input_style_should_override_width() {
    let width = px(320.0);

    assert_eq!(TextInputStyle::new().width(width).width, Some(width));
  }

  #[test]
  fn is_clipboard_shortcut_should_accept_control_key() {
    assert!(is_clipboard_shortcut(&key_down("c", true, false), "c"));
  }

  #[test]
  fn is_clipboard_shortcut_should_accept_platform_key() {
    assert!(is_clipboard_shortcut(&key_down("v", false, true), "v"));
  }

  #[test]
  fn is_clipboard_shortcut_should_reject_unmodified_key() {
    assert!(!is_clipboard_shortcut(&key_down("c", false, false), "c"));
  }

  fn key_down(key: &str, control: bool, platform: bool) -> KeyDownEvent {
    KeyDownEvent {
      keystroke: Keystroke {
        modifiers: Modifiers {
          control,
          platform,
          ..Default::default()
        },
        key: key.to_owned(),
        key_char: Some(key.to_owned()),
      },
      is_held: false,
      prefer_character_input: false,
    }
  }
}
