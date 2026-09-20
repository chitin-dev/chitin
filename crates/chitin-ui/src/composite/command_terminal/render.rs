//! Rendering for structured command blocks and the live prompt.

use gpui::{App, Entity, IntoElement, ParentElement, RenderOnce, SharedString, Window, div, prelude::*};

use super::CommandTerminalState;
use crate::{
  primitive::{
    input::text::{TextInput, TextInputSize, TextInputStyle, TextInputVariant},
    terminal::{TerminalLine, TerminalSpan, TerminalViewport},
  },
  themes::{UIThemes, builtins},
};

/// A structured shell transcript whose editable prompt lives at its tail.
#[derive(IntoElement)]
pub struct CommandTerminal {
  state: Entity<CommandTerminalState>,
  theme: UIThemes,
  font_family: SharedString,
}

impl CommandTerminal {
  /// Creates a command terminal bound to persistent command-block state.
  pub fn new(state: Entity<CommandTerminalState>) -> Self {
    Self {
      state,
      theme: builtins::dark(),
      font_family: "monospace".into(),
    }
  }

  /// Sets the semantic UI theme.
  pub fn theme(mut self, theme: UIThemes) -> Self {
    self.theme = theme;
    self
  }

  /// Sets the monospace font family used by transcript and input.
  pub fn font_family(mut self, font_family: impl Into<SharedString>) -> Self {
    self.font_family = font_family.into();
    self
  }
}

impl RenderOnce for CommandTerminal {
  fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
    let state = self.state.read(cx);
    let input = state.input().clone();
    let viewport = state.viewport().clone();
    let prompt = state.prompt().clone();
    let busy = state.is_busy();
    let lines = state.blocks().iter().flat_map(block_lines).collect::<Vec<_>>();
    let theme = self.theme;
    let tail = (!busy).then(|| render_live_prompt(prompt, input, theme));

    TerminalViewport::new(viewport)
      .theme(theme)
      .font_family(self.font_family)
      .lines(lines)
      .when_some(tail, TerminalViewport::tail)
  }
}

/// Flattens one command block while preserving command-local output ordering.
fn block_lines(block: &super::CommandTerminalBlock) -> Vec<TerminalLine> {
  let mut command = block.prompt.clone();
  command.push(TerminalSpan::primary(block.input.clone()));
  let mut lines = Vec::with_capacity(block.output.len() + block.result.len() + 2);
  lines.push(command);
  lines.extend(block.output.iter().cloned());
  lines.extend(block.result.iter().cloned());
  lines.push(TerminalLine::default());
  lines
}

/// Renders the only editable line at the end of terminal scrollback.
fn render_live_prompt(
  prompt: TerminalLine,
  input: Entity<crate::primitive::input::text::TextInputState>,
  theme: UIThemes,
) -> impl IntoElement {
  div()
    .flex()
    .items_center()
    .min_h(gpui::px(26.0))
    .children(
      prompt
        .spans()
        .iter()
        .map(|span| div().text_color(span.tone.color(theme)).child(span.text.clone())),
    )
    .child(
      TextInput::new(input)
        .theme(theme)
        .variant(TextInputVariant::Transparent)
        .size(TextInputSize::Small)
        .style(
          TextInputStyle::new()
            .background(theme.background.primary)
            .border(theme.background.primary)
            .focus_border(theme.background.primary)
            .horizontal_padding(gpui::px(0.0)),
        )
        .full_width(true),
    )
}
