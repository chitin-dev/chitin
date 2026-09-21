//! Rendering for terminal lines and their scroll viewport.

use gpui::{
  AnyElement, App, Entity, InteractiveElement, IntoElement, MouseButton, ParentElement, RenderOnce, SharedString,
  Window, div, prelude::*,
};

use super::{TerminalLine, TerminalViewportState};
use crate::themes::{UIThemes, builtins};

/// Scrollable monospace output surface with an optional interactive tail.
#[derive(IntoElement)]
pub struct TerminalViewport {
  state: Entity<TerminalViewportState>,
  lines: Vec<TerminalLine>,
  tail: Option<AnyElement>,
  theme: UIThemes,
  font_family: SharedString,
}

impl TerminalViewport {
  /// Creates an empty terminal viewport bound to persistent scroll state.
  pub fn new(state: Entity<TerminalViewportState>) -> Self {
    Self {
      state,
      lines: Vec::new(),
      tail: None,
      theme: builtins::dark(),
      font_family: "monospace".into(),
    }
  }

  /// Replaces the ordered terminal lines.
  pub fn lines(mut self, lines: impl IntoIterator<Item = TerminalLine>) -> Self {
    self.lines = lines.into_iter().flat_map(TerminalLine::into_rows).collect();
    self
  }

  /// Sets an interactive element rendered after all immutable lines.
  pub fn tail(mut self, tail: impl IntoElement) -> Self {
    self.tail = Some(tail.into_any_element());
    self
  }

  /// Sets the semantic UI theme.
  pub fn theme(mut self, theme: UIThemes) -> Self {
    self.theme = theme;
    self
  }

  /// Sets the monospace font family used by all viewport content.
  pub fn font_family(mut self, font_family: impl Into<SharedString>) -> Self {
    self.font_family = font_family.into();
    self
  }
}

impl RenderOnce for TerminalViewport {
  fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
    let scroll_handle = self.state.read(cx).scroll_handle().clone();
    let focus_handle = self.state.read(cx).focus_handle().clone();
    let state_for_mouse = self.state.clone();
    let theme = self.theme;

    div()
      .flex()
      .flex_col()
      .flex_1()
      .min_h_0()
      .overflow_y_scroll()
      .track_scroll(&scroll_handle)
      .track_focus(&focus_handle)
      .on_mouse_down(MouseButton::Left, move |_, window, cx| {
        let focus = state_for_mouse.read(cx).focus_handle().clone();
        window.focus(&focus, cx);
      })
      .px_3()
      .py_2()
      .gap_1()
      .font_family(self.font_family)
      .text_sm()
      .children(self.lines.into_iter().map(|line| render_line(line, theme)))
      .when_some(self.tail, |viewport, tail| viewport.child(tail))
  }
}

/// Renders one soft-wrapping terminal line.
fn render_line(line: TerminalLine, theme: UIThemes) -> impl IntoElement {
  div().flex().flex_wrap().min_h(gpui::px(18.0)).children(
    line
      .spans()
      .iter()
      .map(|span| div().text_color(span.tone.color(theme)).child(span.text.clone())),
  )
}
