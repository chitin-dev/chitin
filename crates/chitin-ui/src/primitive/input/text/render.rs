use std::time::Instant;

use gpui::{
  App, Bounds, CursorStyle, Element, ElementId, Entity, GlobalElementId, InspectorElementId, InteractiveElement,
  IntoElement, LayoutId, MouseButton, PaintQuad, ParentElement, Pixels, RenderOnce, ShapedLine, SharedString, Style,
  TextColor, Window, div, fill, point, prelude::*, px, relative, size,
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
  placeholder: Option<SharedString>,
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
  pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
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
    let state_for_mouse = state.clone();
    let state_for_keys = state.clone();

    div()
      .flex()
      .items_center()
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
            theme,
            show_caret: focused && !disabled,
            caret_visible,
            metrics,
          }),
      )
      .when(readonly && !disabled, |style| style.cursor(CursorStyle::IBeam))
  }
}

/// A single shaped input line with selection and caret paint overlays.
struct TextInputContent {
  text: SharedString,
  placeholder: SharedString,
  selection: super::TextSelection,
  theme: UIThemes,
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
      self.theme.text.disabled
    } else {
      self.theme.text.primary
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
        self.theme.background.selection,
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
        self.theme.accent.primary,
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

  #[test]
  fn text_input_size_should_use_medium_height_by_default() {
    assert_eq!(TextInputSize::default().metrics().height, px(30.0));
  }
}
