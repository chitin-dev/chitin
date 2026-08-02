use gpui::{
  App, ClipboardItem, CursorStyle, Entity, InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels,
  RenderOnce, Window, div, prelude::*, px,
};

use super::TextState;
use crate::themes::{UIThemes, builtins};

/// Visual size for a [`Text`] element.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextSize {
  /// Compact text for dense controls.
  Small,
  /// Default text for panels and forms.
  #[default]
  Medium,
  /// Larger text for local headings and status values.
  Large,
}

/// A text element that can optionally copy its complete value with Ctrl/Cmd+C.
#[derive(IntoElement)]
pub struct Text {
  state: Entity<TextState>,
  theme: UIThemes,
  size: TextSize,
}

impl Text {
  /// Creates a text element that renders and observes `state`.
  pub fn new(state: Entity<TextState>) -> Self {
    Self {
      state,
      theme: builtins::dark(),
      size: TextSize::default(),
    }
  }

  /// Sets the semantic theme used for this text.
  pub fn theme(mut self, theme: UIThemes) -> Self {
    self.theme = theme;
    self
  }

  /// Sets the text size.
  pub fn size(mut self, size: TextSize) -> Self {
    self.size = size;
    self
  }
}

impl RenderOnce for Text {
  fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
    let focus_handle = self.state.read(cx).focus_handle().clone();
    let focused = focus_handle.is_focused(window);
    self.state.update(cx, |state, cx| state.sync_focus(focused, cx));

    let (text, copyable) = {
      let state = self.state.read(cx);
      (state.text().to_owned(), state.is_copyable())
    };
    let state_for_mouse = self.state.clone();
    let state_for_copy = self.state.clone();
    let theme = self.theme;

    div()
      .text_size(self.size.font_size())
      .text_color(theme.text.primary)
      .cursor(if copyable {
        CursorStyle::IBeam
      } else {
        CursorStyle::Arrow
      })
      .when(copyable, |style| {
        style
          .focus(move |style| style.text_color(theme.text.selection))
          .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            let focus_handle = state_for_mouse.read(cx).focus_handle().clone();
            window.focus(&focus_handle, cx);
            cx.stop_propagation();
          })
          .on_key_down(move |event, _, cx| {
            let modifiers = event.keystroke.modifiers;
            if event.keystroke.key != "c" || !(modifiers.platform || modifiers.control) {
              return;
            }

            let copied = state_for_copy.update(cx, |state, cx| state.copy(cx));
            if let Some(value) = copied {
              cx.write_to_clipboard(ClipboardItem::new_string(value.to_string()));
              cx.stop_propagation();
            }
          })
          .track_focus(&focus_handle)
      })
      .child(text)
  }
}

impl TextSize {
  fn font_size(self) -> Pixels {
    match self {
      Self::Small => px(12.0),
      Self::Medium => px(13.0),
      Self::Large => px(16.0),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn text_size_should_use_medium_font_size_by_default() {
    assert_eq!(TextSize::default().font_size(), px(13.0));
  }
}
