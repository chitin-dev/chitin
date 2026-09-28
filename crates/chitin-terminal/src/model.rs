//! Profile-aware, frontend-independent terminal transcript state.

use std::collections::VecDeque;

/// Execution profile hosted by a terminal surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TerminalProfile {
  /// Chitin's structured command language and application command executor.
  BuiltinShell,
  /// The operating system's interactive shell attached to a native PTY.
  SystemShell,
}

impl TerminalProfile {
  /// Returns the user-facing profile name.
  pub const fn label(self) -> &'static str {
    match self {
      Self::BuiltinShell => "Built-in shell",
      Self::SystemShell => "System shell",
    }
  }
}

/// Semantic foreground tone shared by terminal renderers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TerminalTone {
  /// Primary terminal content.
  #[default]
  Primary,
  /// De-emphasized paths, metadata, and hints.
  Secondary,
  /// Prompt and other interactive accents.
  Accent,
  /// Successful result content.
  Success,
  /// Recoverable warning content.
  Warning,
  /// Failed or rejected command content.
  Error,
  /// Informational and progress content.
  Info,
}

/// One styled text span in a terminal line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalSpan {
  /// Text rendered by this span.
  pub text: String,
  /// Semantic foreground tone.
  pub tone: TerminalTone,
}

impl TerminalSpan {
  /// Creates a span with a semantic foreground tone.
  pub fn new(text: impl Into<String>, tone: TerminalTone) -> Self {
    Self {
      text: text.into(),
      tone,
    }
  }

  /// Creates primary terminal content.
  pub fn primary(text: impl Into<String>) -> Self {
    Self::new(text, TerminalTone::Primary)
  }

  /// Creates de-emphasized terminal content.
  pub fn secondary(text: impl Into<String>) -> Self {
    Self::new(text, TerminalTone::Secondary)
  }

  /// Creates prompt or interactive accent content.
  pub fn accent(text: impl Into<String>) -> Self {
    Self::new(text, TerminalTone::Accent)
  }

  /// Creates successful terminal content.
  pub fn success(text: impl Into<String>) -> Self {
    Self::new(text, TerminalTone::Success)
  }

  /// Creates warning terminal content.
  pub fn warning(text: impl Into<String>) -> Self {
    Self::new(text, TerminalTone::Warning)
  }

  /// Creates failed or rejected terminal content.
  pub fn error(text: impl Into<String>) -> Self {
    Self::new(text, TerminalTone::Error)
  }

  /// Creates informational and progress content.
  pub fn info(text: impl Into<String>) -> Self {
    Self::new(text, TerminalTone::Info)
  }
}

/// One soft-wrapping row of terminal spans.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TerminalLine {
  spans: Vec<TerminalSpan>,
}

impl TerminalLine {
  /// Creates a line from ordered styled spans.
  pub fn new(spans: impl IntoIterator<Item = TerminalSpan>) -> Self {
    Self {
      spans: spans.into_iter().collect(),
    }
  }

  /// Creates a line containing primary text.
  pub fn plain(text: impl Into<String>) -> Self {
    Self::new([TerminalSpan::primary(text)])
  }

  /// Returns the ordered styled spans.
  pub fn spans(&self) -> &[TerminalSpan] {
    &self.spans
  }

  /// Appends one styled span.
  pub fn push(&mut self, span: TerminalSpan) {
    self.spans.push(span);
  }

  /// Returns whether the line contains no text spans.
  pub fn is_empty(&self) -> bool {
    self.spans.is_empty()
  }

  /// Splits embedded newline characters into independently laid-out rows.
  pub fn into_rows(self) -> Vec<Self> {
    if self.spans.iter().all(|span| !span.text.contains('\n')) {
      return vec![self];
    }

    let mut rows = Vec::new();
    let mut current = Vec::new();
    for span in self.spans {
      for (index, text) in span.text.split('\n').enumerate() {
        if index > 0 {
          rows.push(Self {
            spans: std::mem::take(&mut current),
          });
        }
        let text = text.strip_suffix('\r').unwrap_or(text);
        if !text.is_empty() {
          current.push(TerminalSpan::new(text.to_owned(), span.tone));
        }
      }
    }
    rows.push(Self { spans: current });
    rows
  }
}

/// Stable identity for one submitted command block.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TerminalBlockId(u64);

impl TerminalBlockId {
  /// Returns the terminal-local numeric identity.
  pub const fn get(self) -> u64 {
    self.0
  }
}

/// Presentation lifecycle of one submitted terminal command block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalBlockStatus {
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

impl TerminalBlockStatus {
  /// Returns whether a new live prompt may be rendered.
  pub const fn is_terminal(self) -> bool {
    !matches!(self, Self::Running)
  }
}

/// One immutable submitted command and its ordered output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalBlock {
  /// Terminal-local block identity.
  pub id: TerminalBlockId,
  /// Prompt captured when this command was submitted.
  pub prompt: TerminalLine,
  /// Exact command text submitted by the user.
  pub input: String,
  /// Replaceable executor output such as progress and artifacts.
  pub output: Vec<TerminalLine>,
  /// Final structured result lines appended after executor output.
  pub result: Vec<TerminalLine>,
  /// Current presentation lifecycle.
  pub status: TerminalBlockStatus,
}

const MAX_TERMINAL_BLOCKS: usize = 512;

/// Profile-aware transcript model shared by built-in and system terminal hosts.
pub struct TerminalBuffer {
  profile: TerminalProfile,
  prompt: TerminalLine,
  blocks: VecDeque<TerminalBlock>,
  active: Option<TerminalBlockId>,
  next_id: u64,
}

impl TerminalBuffer {
  /// Creates an empty transcript for one terminal profile.
  pub fn new(profile: TerminalProfile, prompt: TerminalLine) -> Self {
    Self {
      profile,
      prompt,
      blocks: VecDeque::new(),
      active: None,
      next_id: 1,
    }
  }

  /// Returns the profile that owns this transcript.
  pub const fn profile(&self) -> TerminalProfile {
    self.profile
  }

  /// Returns submitted command blocks in display order.
  pub fn blocks(&self) -> &VecDeque<TerminalBlock> {
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

  /// Returns whether one block currently owns the foreground slot.
  pub fn is_active(&self, id: TerminalBlockId) -> bool {
    self.active == Some(id)
  }

  /// Freezes the live prompt into a running command block.
  pub fn begin_submission(&mut self, input: String) -> TerminalBlockId {
    let id = self.reserve_block();
    self.blocks.push_back(TerminalBlock {
      id,
      prompt: self.prompt.clone(),
      input,
      output: Vec::new(),
      result: Vec::new(),
      status: TerminalBlockStatus::Running,
    });
    self.active = Some(id);
    id
  }

  /// Appends a completed notice without claiming the foreground slot.
  pub fn push_notice(&mut self, input: String, result: Vec<TerminalLine>) {
    let id = self.reserve_block();
    self.blocks.push_back(TerminalBlock {
      id,
      prompt: self.prompt.clone(),
      input,
      output: Vec::new(),
      result,
      status: TerminalBlockStatus::Succeeded,
    });
  }

  /// Replaces transient output for one command block.
  pub fn set_output(&mut self, id: TerminalBlockId, output: Vec<TerminalLine>) {
    if let Some(block) = self.blocks.iter_mut().find(|block| block.id == id) {
      block.output = output;
    }
  }

  /// Finishes one command block and restores the live prompt.
  pub fn finish(&mut self, id: TerminalBlockId, status: TerminalBlockStatus, result: Vec<TerminalLine>) {
    let Some(block) = self.blocks.iter_mut().find(|block| block.id == id) else {
      return;
    };
    block.status = status;
    block.result = result;
    if self.active == Some(id) {
      self.active = None;
    }
  }

  /// Clears submitted blocks while retaining the profile and prompt.
  pub fn clear(&mut self) {
    self.blocks.clear();
    self.active = None;
  }

  /// Replaces the prompt used by subsequent command blocks.
  pub fn set_prompt(&mut self, prompt: TerminalLine) {
    self.prompt = prompt;
  }

  /// Reserves the next block identity and evicts the oldest block at capacity.
  fn reserve_block(&mut self) -> TerminalBlockId {
    let id = TerminalBlockId(self.next_id);
    self.next_id = self.next_id.saturating_add(1);
    if self.blocks.len() == MAX_TERMINAL_BLOCKS {
      self.blocks.pop_front();
    }
    id
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn buffer_should_preserve_profile_and_command_lifecycle() {
    let mut buffer = TerminalBuffer::new(TerminalProfile::BuiltinShell, TerminalLine::plain("chitin ❯ "));
    let id = buffer.begin_submission("help".into());
    assert!(buffer.is_busy());
    buffer.finish(id, TerminalBlockStatus::Succeeded, vec![TerminalLine::plain("ok")]);

    assert_eq!(buffer.profile(), TerminalProfile::BuiltinShell);
    assert!(!buffer.is_busy());
    assert_eq!(buffer.blocks()[0].status, TerminalBlockStatus::Succeeded);
  }
}
