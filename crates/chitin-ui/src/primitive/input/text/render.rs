//! Rendering for the reusable single-line text input primitive.

use std::time::Instant;

use gpui::{
  App, Bounds, ClipboardItem, ContentMask, CursorStyle, DispatchPhase, Element, ElementId, ElementInputHandler, Entity,
  GlobalElementId, Hitbox, HitboxBehavior, InspectorElementId, InteractiveElement, IntoElement, KeyDownEvent, LayoutId,
  MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, ParentElement, Pixels, RenderOnce, ShapedLine,
  SharedString, Style, TextColor, TextRun, UnderlineStyle, Window, div, fill, point, prelude::*, px, relative, size,
};

use super::TextInputState;
use crate::{
  primitive::input::appearance::InputAppearanceStyle,
  themes::{UIThemes, builtins},
};

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
  appearance: InputAppearanceStyle,
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
    self.appearance.background = Some(color);
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
    self.appearance.foreground = Some(color);
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
    self.appearance.placeholder_foreground = Some(color);
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
    self.appearance.selection_background = Some(color);
    self
  }

  /// Overrides the selected-range text color.
  ///
  /// # Parameters
  ///
  /// `color` is the semantic theme token to use for selected glyphs.
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
  /// `color` is the semantic theme token to use for the insertion caret.
  ///
  /// # Returns
  ///
  /// The updated visual-only style.
  pub fn caret(mut self, color: gpui::Rgba) -> Self {
    self.appearance.caret = Some(color);
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
    self.appearance.border = Some(color);
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
    self.appearance.focus_border = Some(color);
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
    self.appearance.width = Some(width);
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
    self.appearance.height = Some(height);
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
    self.appearance.horizontal_padding = Some(padding);
    self
  }

  /// Creates a text-input style from shared input appearance fields.
  ///
  /// # Parameters
  ///
  /// `appearance` supplies crate-private input visual overrides.
  ///
  /// # Returns
  ///
  /// A text-input style with the supplied visual overrides.
  pub(crate) fn from_appearance(appearance: InputAppearanceStyle) -> Self {
    Self { appearance }
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
    let (text, selection, marked_range, disabled, readonly, caret_visible) = {
      let state = self.state.read(cx);
      (
        state.text().to_owned(),
        state.selection(),
        state.marked_range(),
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
    let state_for_keys = state.clone();

    div()
      .flex()
      .items_center()
      .w(self.style.appearance.width.unwrap_or(DEFAULT_TEXT_INPUT_WIDTH))
      .when(self.full_width, |style| style.w_full())
      .h(self.style.appearance.height.unwrap_or(metrics.height))
      .px(
        self
          .style
          .appearance
          .horizontal_padding
          .unwrap_or(DEFAULT_TEXT_INPUT_PADDING_X),
      )
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
      // Because [`TextInputState`] should stay platform/ui-service independent.
      // Ctrl/Cmd copy, cut, and paste need platform clipboard access. They are local to the
      // component/entity model
      .on_key_down(move |event, window, cx| {
        let handled = if is_clipboard_shortcut(event, "c") {
          copy_selection_to_clipboard(&state_for_keys, cx)
        } else if is_clipboard_shortcut(event, "v") {
          paste_clipboard_text(&state_for_keys, cx)
        } else if is_clipboard_shortcut(event, "x") {
          cut_selection_to_clipboard(&state_for_keys, cx)
        } else {
          state_for_keys.update(cx, |state, cx| state.handle_key_down(event, cx))
        };
        if handled {
          window.invalidate_character_coordinates();
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
            state,
            text: text.into(),
            placeholder: self.placeholder.unwrap_or_default(),
            selection,
            marked_range,
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

/// Copies the editable selection to the clipboard, then removes it from the input.
///
/// # Parameters
///
/// `state` supplies the text input state entity.
///
/// `cx` provides platform clipboard access and state mutation.
///
/// # Returns
///
/// `true` when the selected text was copied and removed.
fn cut_selection_to_clipboard(state: &Entity<TextInputState>, cx: &mut App) -> bool {
  if state.read(cx).is_readonly() || !copy_selection_to_clipboard(state, cx) {
    return false;
  }

  state.update(cx, |state, cx| state.insert_text("", cx))
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
  state: Entity<TextInputState>,
  text: SharedString,
  placeholder: SharedString,
  selection: super::TextSelection,
  marked_range: Option<std::ops::Range<usize>>,
  colors: TextInputColors,
  show_caret: bool,
  caret_visible: bool,
  metrics: TextInputMetrics,
}

/// Precomputed paint data: the shaped line plus optional selection and caret quads.
struct TextInputContentPrepaint {
  line: ShapedLine,
  selection_line: Option<ShapedLine>,
  hitbox: Hitbox,
  text_is_empty: bool,
  selection: Option<PaintQuad>,
  selection_bounds: Option<Bounds<Pixels>>,
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

    let range = self.selection.range();
    // Shape the whole logical line once; x_for_index maps byte offsets to pixels.
    let runs = text_runs(run.clone(), (!empty).then_some(self.marked_range.clone()).flatten());
    let line = window.text_system().shape_line(display_text, font_size, &runs, None);

    // Selection highlight spans the shaped range and is vertically centered.
    let selection_bounds = (!empty && !range.is_empty()).then(|| {
      let start = line.x_for_index(range.start);
      let end = line.x_for_index(range.end);
      let selection_height = line.ascent + line.descent;
      let selection_top = bounds.top() + (bounds.size.height - selection_height) / 2.0;
      Bounds::new(
        point(bounds.left() + start, selection_top),
        size(end - start, selection_height),
      )
    });
    let selection = selection_bounds.map(|bounds| fill(bounds, self.colors.selection_background));
    let selection_line = selection_bounds.map(|_| {
      let selection_run = TextRun {
        color: TextColor::from(self.colors.selection_foreground),
        ..run
      };
      window
        .text_system()
        .shape_line(line.text.clone(), font_size, &[selection_run], None)
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
    TextInputContentPrepaint {
      line,
      selection_line,
      hitbox: window.insert_hitbox(bounds, HitboxBehavior::Normal),
      text_is_empty: empty,
      selection,
      selection_bounds,
      caret,
    }
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
    let focus_handle = self.state.read(cx).focus_handle().clone();
    self
      .state
      .update(cx, |state, _| state.update_layout_cache(prepaint.line.clone(), bounds));
    window.handle_input(&focus_handle, ElementInputHandler::new(bounds, self.state.clone()), cx);

    register_pointer_selection_handlers(
      self.state.clone(),
      prepaint.hitbox.clone(),
      prepaint.line.clone(),
      prepaint.text_is_empty,
      window,
    );

    // Selection sits below the glyphs.
    if let Some(selection) = prepaint.selection.take() {
      window.paint_quad(selection);
    }
    // Text glyphs in the middle.
    let _ = prepaint.line.paint(bounds.origin, bounds.size.height, window, cx);
    // Repaint the complete shaped line through the selected bounds. This changes glyph color
    // without splitting the base layout at selection boundaries.
    if let (Some(selection_line), Some(selection_bounds)) = (&prepaint.selection_line, prepaint.selection_bounds) {
      window.with_content_mask(
        Some(ContentMask {
          bounds: selection_bounds,
        }),
        |window| {
          let _ = selection_line.paint(bounds.origin, bounds.size.height, window, cx);
        },
      );
    }
    // Caret on top, gated by the blink phase computed during render.
    if let Some(caret) = prepaint.caret.take()
      && self.caret_visible
    {
      window.paint_quad(caret);
    }
  }
}

/// Builds visual text runs for one complete input line.
///
/// # Parameters
///
/// `run` supplies the base text styling for the complete line.
///
/// `marked_range` optionally identifies the active IME composition range.
///
/// # Returns
///
/// One run for ordinary text, or three contiguous runs with an underlined marked range.
fn text_runs(mut run: TextRun, marked_range: Option<std::ops::Range<usize>>) -> Vec<TextRun> {
  let Some(marked_range) = marked_range.filter(|range| range.start < range.end && range.end <= run.len) else {
    return vec![run];
  };

  let before = marked_range.start;
  let marked = marked_range.end - marked_range.start;
  let after = run.len - marked_range.end;
  let underline_color = run.color.to_hsla();
  let marked_run = TextRun {
    len: marked,
    underline: Some(UnderlineStyle {
      color: Some(underline_color),
      thickness: px(1.0),
      wavy: false,
    }),
    ..run.clone()
  };
  run.len = before;

  [
    (before > 0).then_some(run.clone()),
    Some(marked_run),
    (after > 0).then_some(TextRun { len: after, ..run }),
  ]
  .into_iter()
  .flatten()
  .collect()
}

/// Registers pointer handlers that place and extend the input selection.
///
/// # Parameters
///
/// `state` owns the selection and active pointer anchor.
///
/// `hitbox` identifies the interactive text viewport.
///
/// `line` maps pointer x coordinates to shaped-text offsets.
///
/// `text_is_empty` prevents placeholder glyphs from becoming input offsets.
///
/// `window` registers handlers for the current rendered frame.
///
/// # Returns
///
/// This function returns `()` after registering pointer handlers.
fn register_pointer_selection_handlers(
  state: Entity<TextInputState>,
  hitbox: Hitbox,
  line: ShapedLine,
  text_is_empty: bool,
  window: &mut Window,
) {
  window.on_mouse_event({
    let state = state.clone();
    let hitbox = hitbox.clone();
    let line = line.clone();
    move |event: &MouseDownEvent, phase, window, cx| {
      if phase != DispatchPhase::Bubble || event.button != MouseButton::Left || !hitbox.is_hovered(window) {
        return;
      }

      if state.read(cx).is_disabled() {
        return;
      }

      let offset = pointer_offset_for_position(event.position, hitbox.bounds, &line, text_is_empty);
      let focus_handle = state.read(cx).focus_handle().clone();
      window.focus(&focus_handle, cx);
      state.update(cx, |state, cx| {
        state.begin_pointer_selection(offset, event.modifiers.shift, cx)
      });
      window.invalidate_character_coordinates();
      cx.stop_propagation();
    }
  });

  window.on_mouse_event({
    let state = state.clone();
    let line = line.clone();
    let bounds = hitbox.bounds;
    move |event: &MouseMoveEvent, phase, window, cx| {
      if phase != DispatchPhase::Bubble || !event.dragging() {
        return;
      }

      let offset = pointer_offset_for_position(event.position, bounds, &line, text_is_empty);
      if state.update(cx, |state, cx| state.update_pointer_selection(offset, cx)) {
        window.invalidate_character_coordinates();
        cx.stop_propagation();
      }
    }
  });

  window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
    if phase == DispatchPhase::Capture && event.button == MouseButton::Left {
      state.update(cx, |state, _| state.end_pointer_selection());
    }
  });
}

/// Maps a window pointer position to one UTF-8 boundary in the shaped input line.
///
/// # Parameters
///
/// `position` supplies the pointer location in window coordinates.
///
/// `bounds` supplies the text viewport's window bounds.
///
/// `line` supplies the complete shaped input line.
///
/// `text_is_empty` identifies placeholder-only layouts.
///
/// # Returns
///
/// The nearest UTF-8 byte offset in the current input text.
fn pointer_offset_for_position(
  position: gpui::Point<Pixels>,
  bounds: Bounds<Pixels>,
  line: &ShapedLine,
  text_is_empty: bool,
) -> usize {
  if text_is_empty {
    return 0;
  }

  line.closest_index_for_x((position.x - bounds.left()).max(px(0.0)))
}

/// Resolved visual colors for one rendered text-input state.
#[derive(Clone, Copy)]
pub(crate) struct TextInputColors {
  pub(crate) background: gpui::Rgba,
  foreground: gpui::Rgba,
  placeholder_foreground: gpui::Rgba,
  selection_background: gpui::Rgba,
  selection_foreground: gpui::Rgba,
  caret: gpui::Rgba,
  pub(crate) border: gpui::Rgba,
  pub(crate) focus_border: gpui::Rgba,
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
  pub(crate) fn new(theme: UIThemes, variant: TextInputVariant, style: TextInputStyle, disabled: bool) -> Self {
    if disabled {
      return Self {
        background: theme.background.secondary,
        foreground: theme.text.disabled,
        placeholder_foreground: theme.text.disabled,
        selection_background: theme.background.selection,
        selection_foreground: theme.accent.foreground,
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
        selection_foreground: theme.accent.foreground,
        caret: theme.accent.primary,
        border: theme.border.primary,
        focus_border: theme.border.focus,
      },
      TextInputVariant::Secondary => Self {
        background: theme.background.tertiary,
        foreground: theme.text.primary,
        placeholder_foreground: theme.text.disabled,
        selection_background: theme.background.selection,
        selection_foreground: theme.accent.foreground,
        caret: theme.accent.primary,
        border: theme.border.primary,
        focus_border: theme.border.focus,
      },
      TextInputVariant::Transparent => Self {
        background: builtins::TRANSPARENT,
        foreground: theme.text.primary,
        placeholder_foreground: theme.text.disabled,
        selection_background: theme.background.selection,
        selection_foreground: theme.accent.foreground,
        caret: theme.accent.primary,
        border: builtins::TRANSPARENT,
        focus_border: builtins::TRANSPARENT,
      },
    };

    Self {
      background: style.appearance.background.unwrap_or(colors.background),
      foreground: style.appearance.foreground.unwrap_or(colors.foreground),
      placeholder_foreground: style
        .appearance
        .placeholder_foreground
        .unwrap_or(colors.placeholder_foreground),
      selection_background: style
        .appearance
        .selection_background
        .unwrap_or(colors.selection_background),
      selection_foreground: style
        .appearance
        .selection_foreground
        .unwrap_or(colors.selection_foreground),
      caret: style.appearance.caret.unwrap_or(colors.caret),
      border: style.appearance.border.unwrap_or(colors.border),
      focus_border: style.appearance.focus_border.unwrap_or(colors.focus_border),
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

  /// Returns the text size used by this input-size variant.
  ///
  /// # Parameters
  ///
  /// This method reads the selected text-input size variant.
  ///
  /// # Returns
  ///
  /// The font size used for text and placeholder layout.
  pub(crate) fn font_size(self) -> Pixels {
    self.metrics().font_size
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
  fn text_input_style_should_override_selection_foreground_color() {
    let theme = builtins::dark();
    let colors = TextInputColors::new(
      theme,
      TextInputVariant::Secondary,
      TextInputStyle::new().selection_foreground(theme.accent.foreground),
      false,
    );

    assert_eq!(colors.selection_foreground, theme.accent.foreground);
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

    assert_eq!(TextInputStyle::new().width(width).appearance.width, Some(width));
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
  fn is_clipboard_shortcut_should_accept_cut_key() {
    assert!(is_clipboard_shortcut(&key_down("x", true, false), "x"));
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
