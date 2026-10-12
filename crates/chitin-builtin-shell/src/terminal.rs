//! Terminal byte-stream adapter for Chitin's in-process shell.

use chitin_terminal::{
  TerminalProfile, TerminalProgram, TerminalProgramInput, TerminalSession, TerminalSessionError, TerminalSize,
};

/// Semantic input produced by the built-in terminal's line discipline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BuiltinTerminalEvent {
  /// A complete command line was submitted with Enter.
  Submit(String),
  /// The active line or command was interrupted with Ctrl+C.
  Interrupt,
  /// The user requested completion with Tab.
  Complete(String),
  /// The user requested an older history entry.
  PreviousHistory,
  /// The user requested a newer history entry.
  NextHistory,
  /// The terminal grid changed size.
  Resize(TerminalSize),
  /// The terminal frontend disconnected.
  Shutdown,
}

/// Adapts terminal input bytes into shell lines and writes ANSI output back.
///
/// This type performs the same line-discipline role that an interactive Bash
/// or Fish process performs behind a PTY. Command parsing and execution remain
/// owned by [`BuiltinShell`](crate::BuiltinShell) and its desktop host.
pub struct BuiltinTerminalProgram {
  terminal: TerminalProgram,
  prompt: String,
  line: String,
  waiting_for_command: bool,
  running_output_lines: usize,
  escape: Vec<u8>,
}

impl BuiltinTerminalProgram {
  /// Creates an in-process terminal session and displays its first prompt.
  ///
  /// # Parameters
  ///
  /// * `size` is the initial terminal grid and cell geometry.
  /// * `prompt` is ANSI-capable text displayed before each editable line.
  ///
  /// # Returns
  ///
  /// The frontend session and its built-in shell program endpoint.
  pub fn connect(
    size: TerminalSize,
    prompt: impl Into<String>,
  ) -> Result<(TerminalSession, Self), TerminalSessionError> {
    let (session, terminal) = TerminalSession::in_process(TerminalProfile::BuiltinShell, size);
    let program = Self {
      terminal,
      prompt: prompt.into(),
      line: String::new(),
      waiting_for_command: false,
      running_output_lines: 0,
      escape: Vec::new(),
    };
    program.write_prompt()?;
    Ok((session, program))
  }

  /// Processes pending emulator input into semantic shell events.
  pub fn poll(&mut self) -> Result<Vec<BuiltinTerminalEvent>, TerminalSessionError> {
    let mut events = Vec::new();
    for input in self.terminal.drain_input()? {
      match input {
        TerminalProgramInput::Bytes(bytes) => self.process_bytes(&bytes, &mut events)?,
        TerminalProgramInput::Resize(size) => events.push(BuiltinTerminalEvent::Resize(size)),
        TerminalProgramInput::Shutdown => events.push(BuiltinTerminalEvent::Shutdown),
      }
    }
    Ok(events)
  }

  /// Writes command output and restores an editable prompt.
  pub fn finish_command(&mut self, output: &str) -> Result<(), TerminalSessionError> {
    self.clear_running_output()?;
    if !output.is_empty() {
      self.write_normalized_lines(output)?;
    }
    self.line.clear();
    self.waiting_for_command = false;
    self.write_prompt()
  }

  /// Replaces transient output for the command currently holding the prompt.
  pub fn update_command_output(&mut self, output: &str) -> Result<(), TerminalSessionError> {
    self.clear_running_output()?;
    if output.is_empty() {
      return Ok(());
    }
    let normalized = output.replace("\r\n", "\n").replace('\n', "\r\n");
    self.terminal.output().write(normalized.as_bytes())?;
    self.running_output_lines = normalized.lines().count().max(1);
    Ok(())
  }

  /// Replaces the editable line, as used by history and completion.
  pub fn replace_line(&mut self, line: impl Into<String>) -> Result<(), TerminalSessionError> {
    self.line = line.into();
    self.redraw_line()
  }

  /// Updates prompt context, preserving input and deferring display during execution.
  pub fn set_prompt(&mut self, prompt: impl Into<String>) -> Result<(), TerminalSessionError> {
    let prompt = prompt.into();
    if self.prompt == prompt {
      return Ok(());
    }
    self.prompt = prompt;
    if !self.waiting_for_command {
      self.redraw_line()?;
    }
    Ok(())
  }

  /// Clears the VT screen and displays a fresh prompt.
  pub fn clear_screen(&mut self) -> Result<(), TerminalSessionError> {
    self.line.clear();
    self.waiting_for_command = false;
    self.terminal.output().write(b"\x1b[2J\x1b[H")?;
    self.write_prompt()
  }

  /// Displays non-command output above the current editable line.
  pub fn show_notice(&self, output: &str) -> Result<(), TerminalSessionError> {
    self.terminal.output().write(b"\r\x1b[2K")?;
    self.write_normalized_lines(output)?;
    self.redraw_line()
  }

  /// Returns the line currently owned by the in-process line discipline.
  pub fn line(&self) -> &str {
    &self.line
  }

  fn process_bytes(
    &mut self,
    bytes: &[u8],
    events: &mut Vec<BuiltinTerminalEvent>,
  ) -> Result<(), TerminalSessionError> {
    let mut printable = Vec::new();
    for &byte in bytes {
      if !self.escape.is_empty() || byte == 0x1b {
        self.flush_printable(&mut printable)?;
        self.process_escape_byte(byte, events);
        continue;
      }
      match byte {
        b'\r' | b'\n' if !self.waiting_for_command => {
          self.flush_printable(&mut printable)?;
          self.terminal.output().write(b"\r\n")?;
          self.waiting_for_command = true;
          self.running_output_lines = 0;
          events.push(BuiltinTerminalEvent::Submit(self.line.clone()));
        }
        0x03 => {
          self.flush_printable(&mut printable)?;
          self.terminal.output().write(b"^C\r\n")?;
          events.push(BuiltinTerminalEvent::Interrupt);
          if !self.waiting_for_command {
            self.line.clear();
            self.write_prompt()?;
          }
        }
        0x7f | 0x08 if !self.waiting_for_command => {
          self.flush_printable(&mut printable)?;
          self.line.pop();
          self.redraw_line()?;
        }
        b'\t' if !self.waiting_for_command => {
          self.flush_printable(&mut printable)?;
          events.push(BuiltinTerminalEvent::Complete(self.line.clone()));
        }
        0x20..=0x7e if !self.waiting_for_command => printable.push(byte),
        0x80..=0xff if !self.waiting_for_command => printable.push(byte),
        _ => {}
      }
    }
    self.flush_printable(&mut printable)
  }

  fn process_escape_byte(&mut self, byte: u8, events: &mut Vec<BuiltinTerminalEvent>) {
    self.escape.push(byte);
    match self.escape.as_slice() {
      b"\x1b[A" => {
        events.push(BuiltinTerminalEvent::PreviousHistory);
        self.escape.clear();
      }
      b"\x1b[B" => {
        events.push(BuiltinTerminalEvent::NextHistory);
        self.escape.clear();
      }
      sequence if sequence.len() >= 3 || (sequence.len() == 2 && sequence[1] != b'[') => {
        self.escape.clear();
      }
      _ => {}
    }
  }

  fn flush_printable(&mut self, bytes: &mut Vec<u8>) -> Result<(), TerminalSessionError> {
    if bytes.is_empty() {
      return Ok(());
    }
    let text = String::from_utf8_lossy(bytes);
    self.line.push_str(&text);
    self.terminal.output().write(text.as_bytes())?;
    bytes.clear();
    Ok(())
  }

  fn redraw_line(&self) -> Result<(), TerminalSessionError> {
    let redraw = format!("\r\x1b[2K{}{}", self.prompt, self.line);
    self.terminal.output().write(redraw.as_bytes())
  }

  fn write_prompt(&self) -> Result<(), TerminalSessionError> {
    self.terminal.output().write(self.prompt.as_bytes())
  }

  fn write_normalized_lines(&self, output: &str) -> Result<(), TerminalSessionError> {
    let normalized = output.replace("\r\n", "\n").replace('\n', "\r\n");
    self.terminal.output().write(normalized.as_bytes())?;
    if !normalized.ends_with("\r\n") {
      self.terminal.output().write(b"\r\n")?;
    }
    Ok(())
  }

  fn clear_running_output(&mut self) -> Result<(), TerminalSessionError> {
    if self.running_output_lines == 0 {
      return Ok(());
    }
    let mut clear = String::from("\r\x1b[2K");
    for _ in 1..self.running_output_lines {
      clear.push_str("\x1b[1A\r\x1b[2K");
    }
    self.terminal.output().write(clear.as_bytes())?;
    self.running_output_lines = 0;
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn changing_prompt_should_preserve_the_editable_command() -> Result<(), TerminalSessionError> {
    let (session, mut program) = connected_program();
    session.write(b"panel li")?;
    program.poll()?;
    program.set_prompt("chitin [panel #2 4HHB.cif] ❯ ")?;
    assert_eq!(program.line(), "panel li");
    let text: String = session.snapshot().cells.iter().map(|cell| cell.character).collect();
    assert!(text.contains("[panel #2 4HHB.cif] ❯ panel li"));
    Ok(())
  }

  #[test]
  fn changing_prompt_during_execution_should_wait_for_completion() -> Result<(), TerminalSessionError> {
    let (session, mut program) = connected_program();
    session.write(b"panel enter 2\r")?;
    program.poll()?;
    program.set_prompt("chitin [panel #2 4HHB.cif] ❯ ")?;
    let before: String = session.snapshot().cells.iter().map(|cell| cell.character).collect();
    assert!(!before.contains("[panel #2"));
    program.finish_command("Entered rendering panel #2")?;
    let after: String = session.snapshot().cells.iter().map(|cell| cell.character).collect();
    assert!(after.contains("[panel #2 4HHB.cif] ❯"));
    Ok(())
  }

  fn connected_program() -> (TerminalSession, BuiltinTerminalProgram) {
    let result = BuiltinTerminalProgram::connect(TerminalSize::new(80, 24, 8, 16), "chitin ❯ ");
    let Ok(connected) = result else {
      panic!("in-process terminal should connect");
    };
    connected
  }

  #[test]
  fn enter_should_submit_the_line_received_as_terminal_bytes() {
    let (session, mut program) = connected_program();
    assert!(session.write(b"structure validate file.cif\r").is_ok());

    let events = program.poll();

    assert_eq!(
      events.ok(),
      Some(vec![BuiltinTerminalEvent::Submit("structure validate file.cif".into())]),
    );
  }

  #[test]
  fn backspace_should_edit_and_redraw_the_terminal_line() {
    let (session, mut program) = connected_program();
    assert!(session.write(b"hellp\x7fo").is_ok());

    let poll_result = program.poll();

    assert!(poll_result.is_ok());
    assert_eq!(program.line(), "hello");
  }

  #[test]
  fn finish_should_write_output_through_the_shared_vt_emulator() {
    let (session, mut program) = connected_program();

    let finish_result = program.finish_command("command completed");
    let text = session
      .snapshot()
      .cells
      .iter()
      .map(|cell| cell.character)
      .collect::<String>();

    assert!(finish_result.is_ok());
    assert!(text.contains("command completed"));
  }

  #[test]
  fn running_output_should_be_replaced_before_final_output() {
    let (session, mut program) = connected_program();
    assert!(program.update_command_output("Downloading 1 / 2").is_ok());
    assert!(program.update_command_output("Downloading 2 / 2").is_ok());

    let finish_result = program.finish_command("Download complete");
    let text = session
      .snapshot()
      .cells
      .iter()
      .map(|cell| cell.character)
      .collect::<String>();

    assert!(finish_result.is_ok());
    assert!(text.contains("Download complete"));
    assert!(!text.contains("Downloading 1 / 2"));
  }
}
