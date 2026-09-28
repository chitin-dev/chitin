//! GPUI state wrapper around the shared terminal transcript model.

use std::collections::VecDeque;

use chitin_terminal::{
  TerminalBlock, TerminalBlockId, TerminalBlockStatus, TerminalBuffer, TerminalLine, TerminalProfile,
};
use gpui::{AppContext, Context, Entity};

use crate::primitive::{input::text::TextInputState, terminal::TerminalViewportState};

pub use chitin_terminal::{
  TerminalBlock as CommandTerminalBlock, TerminalBlockId as CommandTerminalId,
  TerminalBlockStatus as CommandTerminalStatus,
};

/// Persistent command history, draft input, and viewport state.
pub struct CommandTerminalState {
  input: Entity<TextInputState>,
  viewport: Entity<TerminalViewportState>,
  buffer: TerminalBuffer,
}

impl CommandTerminalState {
  /// Creates an empty built-in-shell terminal with one live prompt.
  pub fn new(prompt: TerminalLine, cx: &mut Context<Self>) -> Self {
    Self::with_profile(TerminalProfile::BuiltinShell, prompt, cx)
  }

  /// Creates a terminal state for an explicit terminal profile.
  pub fn with_profile(profile: TerminalProfile, prompt: TerminalLine, cx: &mut Context<Self>) -> Self {
    Self {
      input: cx.new(TextInputState::new),
      viewport: cx.new(TerminalViewportState::new),
      buffer: TerminalBuffer::new(profile, prompt),
    }
  }

  /// Returns the profile represented by this terminal state.
  pub const fn profile(&self) -> TerminalProfile {
    self.buffer.profile()
  }

  /// Returns the editable live-prompt input.
  pub fn input(&self) -> &Entity<TextInputState> {
    &self.input
  }

  /// Returns the terminal viewport state.
  pub fn viewport(&self) -> &Entity<TerminalViewportState> {
    &self.viewport
  }

  /// Returns submitted command blocks in display order.
  pub fn blocks(&self) -> &VecDeque<TerminalBlock> {
    self.buffer.blocks()
  }

  /// Returns the prompt used for the next submission.
  pub fn prompt(&self) -> &TerminalLine {
    self.buffer.prompt()
  }

  /// Returns whether a command currently owns the foreground slot.
  pub const fn is_busy(&self) -> bool {
    self.buffer.is_busy()
  }

  /// Freezes the live prompt into a running command block.
  pub fn begin_submission(&mut self, input: String, cx: &mut Context<Self>) -> TerminalBlockId {
    let id = self.buffer.begin_submission(input);
    self.input.update(cx, |input, cx| {
      input.clear(cx);
      input.set_disabled(true, cx);
    });
    self.reveal_tail(cx);
    id
  }

  /// Appends a finished block for output that no command produced.
  pub fn push_notice(&mut self, input: String, lines: Vec<TerminalLine>, cx: &mut Context<Self>) {
    self.buffer.push_notice(input, lines);
    self.reveal_tail(cx);
  }

  /// Replaces transient output for one command without growing scrollback.
  pub fn set_output(&mut self, id: TerminalBlockId, output: Vec<TerminalLine>, cx: &mut Context<Self>) {
    if self
      .buffer
      .blocks()
      .iter()
      .find(|block| block.id == id)
      .is_some_and(|block| block.output == output)
    {
      return;
    }
    self.buffer.set_output(id, output);
    self.reveal_tail(cx);
  }

  /// Finishes one block and restores the editable live prompt.
  pub fn finish(
    &mut self,
    id: TerminalBlockId,
    status: TerminalBlockStatus,
    result: Vec<TerminalLine>,
    cx: &mut Context<Self>,
  ) {
    let was_active = self.buffer.is_active(id);
    self.buffer.finish(id, status, result);
    if was_active {
      self.input.update(cx, |input, cx| input.set_disabled(false, cx));
    }
    self.reveal_tail(cx);
  }

  /// Rejects one frozen input line and displays its error below the command.
  pub fn reject(&mut self, id: TerminalBlockId, message: String, cx: &mut Context<Self>) {
    self.finish(
      id,
      TerminalBlockStatus::Rejected,
      vec![TerminalLine::new([chitin_terminal::TerminalSpan::error(format!(
        "error: {message}"
      ))])],
      cx,
    );
  }

  /// Clears submitted blocks and restores the editable live prompt.
  pub fn clear(&mut self, cx: &mut Context<Self>) {
    self.buffer.clear();
    self.input.update(cx, |input, cx| {
      input.clear(cx);
      input.set_disabled(false, cx);
    });
    self.reveal_tail(cx);
  }

  /// Replaces the prompt used by subsequent commands.
  pub fn set_prompt(&mut self, prompt: TerminalLine) {
    self.buffer.set_prompt(prompt);
  }

  /// Requests that the newest command and live prompt remain visible.
  fn reveal_tail(&self, cx: &mut Context<Self>) {
    self.viewport.update(cx, |viewport, _| viewport.reveal_tail());
    cx.notify();
  }
}
