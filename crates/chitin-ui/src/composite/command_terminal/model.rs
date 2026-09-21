//! Persistent command-block and live-prompt state.

use std::collections::VecDeque;

use gpui::{AppContext, Context, Entity};

use crate::primitive::{
  input::text::TextInputState,
  terminal::{TerminalLine, TerminalViewportState},
};

const MAX_COMMAND_BLOCKS: usize = 512;

/// Stable identity for one submitted terminal command block.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CommandTerminalId(u64);

impl CommandTerminalId {
  /// Returns the terminal-local numeric identity.
  pub const fn get(self) -> u64 {
    self.0
  }
}

/// Presentation lifecycle of one terminal command block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandTerminalStatus {
  /// The command is still executing and owns the foreground prompt.
  Running,
  /// The command completed successfully.
  Succeeded,
  /// The command execution failed.
  Failed,
  /// The command stopped through cooperative cancellation.
  Cancelled,
  /// The submitted line was rejected before execution.
  Rejected,
}

impl CommandTerminalStatus {
  /// Returns whether a new live prompt may be rendered.
  pub const fn is_terminal(self) -> bool {
    !matches!(self, Self::Running)
  }
}

/// One immutable submitted command and its ordered output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandTerminalBlock {
  /// Terminal-local block identity.
  pub id: CommandTerminalId,
  /// Prompt captured when this command was submitted.
  pub prompt: TerminalLine,
  /// Exact command text submitted by the user.
  pub input: String,
  /// Replaceable executor output such as progress and artifacts.
  pub output: Vec<TerminalLine>,
  /// Final structured result lines appended after executor output.
  pub result: Vec<TerminalLine>,
  /// Current presentation lifecycle.
  pub status: CommandTerminalStatus,
}

/// Persistent command history, draft input, and viewport state.
pub struct CommandTerminalState {
  input: Entity<TextInputState>,
  viewport: Entity<TerminalViewportState>,
  prompt: TerminalLine,
  blocks: VecDeque<CommandTerminalBlock>,
  active: Option<CommandTerminalId>,
  next_id: u64,
}

impl CommandTerminalState {
  /// Creates an empty command terminal with one live prompt.
  pub fn new(prompt: TerminalLine, cx: &mut Context<Self>) -> Self {
    Self {
      input: cx.new(TextInputState::new),
      viewport: cx.new(TerminalViewportState::new),
      prompt,
      blocks: VecDeque::new(),
      active: None,
      next_id: 1,
    }
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
  pub fn blocks(&self) -> &VecDeque<CommandTerminalBlock> {
    &self.blocks
  }

  /// Returns the prompt used for the next submission.
  pub fn prompt(&self) -> &TerminalLine {
    &self.prompt
  }

  /// Returns whether a command currently owns the foreground slot.
  pub const fn is_busy(&self) -> bool {
    self.active.is_some()
  }

  /// Freezes the live prompt into a running command block.
  ///
  /// # Parameters
  ///
  /// * `input` is the exact submitted command line.
  /// * `cx` updates the input primitive and schedules viewport repainting.
  ///
  /// # Returns
  ///
  /// A terminal-local identity used to attach subsequent output and results.
  pub fn begin_submission(&mut self, input: String, cx: &mut Context<Self>) -> CommandTerminalId {
    let id = self.reserve_block();
    self.blocks.push_back(CommandTerminalBlock {
      id,
      prompt: self.prompt.clone(),
      input,
      output: Vec::new(),
      result: Vec::new(),
      status: CommandTerminalStatus::Running,
    });
    self.active = Some(id);
    self.input.update(cx, |input, cx| {
      input.clear(cx);
      input.set_disabled(true, cx);
    });
    self.reveal_tail(cx);
    id
  }

  /// Appends a finished block for output that no command produced.
  ///
  /// The live prompt keeps its text and stays editable, so completion
  /// candidates can be listed above the line still being edited.
  ///
  /// # Parameters
  ///
  /// * `input` is the unfinished line the output was produced for.
  /// * `lines` are the output rows appended below the echoed line.
  /// * `cx` reveals the new tail and schedules viewport repainting.
  ///
  /// # Returns
  ///
  /// This function returns `()` and never claims the foreground slot.
  pub fn push_notice(&mut self, input: String, lines: Vec<TerminalLine>, cx: &mut Context<Self>) {
    let id = self.reserve_block();
    self.blocks.push_back(CommandTerminalBlock {
      id,
      prompt: self.prompt.clone(),
      input,
      output: Vec::new(),
      result: lines,
      status: CommandTerminalStatus::Succeeded,
    });
    self.reveal_tail(cx);
  }

  /// Replaces transient output for one command without growing scrollback.
  ///
  /// # Parameters
  ///
  /// * `id` identifies the command block receiving executor output.
  /// * `output` contains the current progress, messages, and artifacts.
  /// * `cx` reveals changed output and schedules repainting.
  ///
  /// # Returns
  ///
  /// This function returns `()` and leaves unknown command identities unchanged.
  pub fn set_output(&mut self, id: CommandTerminalId, output: Vec<TerminalLine>, cx: &mut Context<Self>) {
    if let Some(block) = self.blocks.iter_mut().find(|block| block.id == id) {
      if block.output == output {
        return;
      }
      block.output = output;
      self.reveal_tail(cx);
    }
  }

  /// Finishes one block and restores the editable live prompt.
  ///
  /// # Parameters
  ///
  /// * `id` identifies the running command block.
  /// * `status` is its terminal presentation state.
  /// * `result` contains final frontend-formatted result lines.
  /// * `cx` re-enables input and schedules tail repainting.
  ///
  /// # Returns
  ///
  /// This function returns `()` and leaves unknown command identities unchanged.
  pub fn finish(
    &mut self,
    id: CommandTerminalId,
    status: CommandTerminalStatus,
    result: Vec<TerminalLine>,
    cx: &mut Context<Self>,
  ) {
    let Some(block) = self.blocks.iter_mut().find(|block| block.id == id) else {
      return;
    };
    block.status = status;
    block.result = result;
    if self.active == Some(id) {
      self.active = None;
      self.input.update(cx, |input, cx| input.set_disabled(false, cx));
    }
    self.reveal_tail(cx);
  }

  /// Rejects one frozen input line and displays its error below the command.
  ///
  /// # Parameters
  ///
  /// * `id` identifies the command block created for the attempted input.
  /// * `message` describes why parsing or routing rejected the command.
  /// * `cx` restores input and schedules terminal repainting.
  ///
  /// # Returns
  ///
  /// This function returns `()` after converting the message into an error row.
  pub fn reject(&mut self, id: CommandTerminalId, message: String, cx: &mut Context<Self>) {
    self.finish(
      id,
      CommandTerminalStatus::Rejected,
      vec![TerminalLine::new([crate::primitive::terminal::TerminalSpan::error(
        format!("error: {message}"),
      )])],
      cx,
    );
  }

  /// Clears submitted blocks and restores the editable live prompt.
  pub fn clear(&mut self, cx: &mut Context<Self>) {
    self.blocks.clear();
    self.active = None;
    self.input.update(cx, |input, cx| {
      input.clear(cx);
      input.set_disabled(false, cx);
    });
    self.reveal_tail(cx);
  }

  /// Replaces the prompt used by subsequent commands.
  pub fn set_prompt(&mut self, prompt: TerminalLine) {
    self.prompt = prompt;
  }

  /// Reserves the next block identity, evicting the oldest block at capacity.
  fn reserve_block(&mut self) -> CommandTerminalId {
    let id = CommandTerminalId(self.next_id);
    self.next_id = self.next_id.saturating_add(1);
    if self.blocks.len() == MAX_COMMAND_BLOCKS {
      self.blocks.pop_front();
    }
    id
  }

  /// Requests that the newest command and live prompt remain visible.
  fn reveal_tail(&self, cx: &mut Context<Self>) {
    self.viewport.update(cx, |viewport, _| viewport.reveal_tail());
    cx.notify();
  }
}
