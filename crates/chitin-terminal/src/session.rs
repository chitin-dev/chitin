use std::{
  io::{Read, Write},
  path::Path,
  sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
  },
  thread,
};

use alacritty_terminal::{
  Term,
  event::{Event as AlacrittyEvent, EventListener},
  sync::FairMutex,
  term::Config,
  vte::ansi,
};
use portable_pty::{ChildKiller, CommandBuilder, MasterPty, native_pty_system};
use thiserror::Error;

use crate::{TerminalProfile, TerminalSize, TerminalSnapshot};

type SharedWriter = Arc<Mutex<Box<dyn Write + Send>>>;
type SharedTerminal = Arc<FairMutex<Term<TerminalEventProxy>>>;
type SharedProcessor = Arc<Mutex<ansi::Processor<ansi::StdSyncHandler>>>;

/// Input sent from a terminal emulator to an in-process terminal program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminalProgramInput {
  /// Keyboard, paste, or terminal-response bytes.
  Bytes(Vec<u8>),
  /// Updated character-grid and cell-pixel dimensions.
  Resize(TerminalSize),
  /// The owning terminal session is shutting down.
  Shutdown,
}

/// A state change emitted by a native terminal session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminalEvent {
  /// Visible terminal cells changed.
  Render,
  /// The child changed the terminal title.
  Title(String),
  /// The terminal emitted an audible or visual bell.
  Bell,
  /// The child process exited with the supplied exit code.
  Exited(u32),
  /// A background terminal operation failed.
  Error(String),
}

/// Errors produced while creating or controlling a terminal session.
#[derive(Debug, Error)]
pub enum TerminalSessionError {
  #[error("failed to {operation}: {message}")]
  Pty { operation: &'static str, message: String },
  #[error("terminal writer lock is poisoned")]
  WriterPoisoned,
  #[error("terminal output parser lock is poisoned")]
  ParserPoisoned,
  #[error("in-process terminal program is disconnected")]
  ProgramDisconnected,
  #[error("failed to write terminal input: {0}")]
  Write(#[from] std::io::Error),
}

/// Output side of an in-process program connected to a VT terminal emulator.
#[derive(Clone)]
pub struct TerminalProgramOutput {
  terminal: SharedTerminal,
  processor: SharedProcessor,
  events: mpsc::Sender<TerminalEvent>,
  render_pending: Arc<AtomicBool>,
}

impl TerminalProgramOutput {
  /// Parses program output as ANSI/VT bytes and schedules a terminal repaint.
  pub fn write(&self, bytes: &[u8]) -> Result<(), TerminalSessionError> {
    let mut processor = self
      .processor
      .lock()
      .map_err(|_| TerminalSessionError::ParserPoisoned)?;
    processor.advance(&mut *self.terminal.lock(), bytes);
    send_render_event(&self.events, &self.render_pending);
    Ok(())
  }
}

/// Program-facing endpoint paired with an in-process terminal session.
pub struct TerminalProgram {
  input: Mutex<mpsc::Receiver<TerminalProgramInput>>,
  output: TerminalProgramOutput,
}

impl TerminalProgram {
  /// Returns a cloneable handle for writing ANSI/VT output to the emulator.
  pub fn output(&self) -> TerminalProgramOutput {
    self.output.clone()
  }

  /// Drains input currently waiting for the in-process program.
  pub fn drain_input(&self) -> Result<Vec<TerminalProgramInput>, TerminalSessionError> {
    let receiver = self
      .input
      .lock()
      .map_err(|_| TerminalSessionError::ProgramDisconnected)?;
    Ok(receiver.try_iter().collect())
  }

  /// Waits for the next input event from the terminal emulator.
  pub fn recv(&self) -> Result<TerminalProgramInput, TerminalSessionError> {
    let receiver = self
      .input
      .lock()
      .map_err(|_| TerminalSessionError::ProgramDisconnected)?;
    receiver.recv().map_err(|_| TerminalSessionError::ProgramDisconnected)
  }
}

enum TerminalBackend {
  NativePty {
    master: Box<dyn MasterPty + Send>,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
  },
  InProcess {
    input: mpsc::Sender<TerminalProgramInput>,
  },
}

/// Owns one terminal backend and its frontend-independent VT terminal state.
pub struct TerminalSession {
  profile: TerminalProfile,
  terminal: SharedTerminal,
  writer: SharedWriter,
  backend: TerminalBackend,
  events: Mutex<mpsc::Receiver<TerminalEvent>>,
  render_pending: Arc<AtomicBool>,
}

impl TerminalSession {
  /// Spawns the operating system's default interactive shell.
  ///
  /// # Parameters
  ///
  /// * `working_directory` is the initial directory for the child shell.
  /// * `size` is the initial character-grid and cell-pixel size.
  ///
  /// # Returns
  ///
  /// A live PTY session and VT terminal state.
  pub fn spawn_default_shell(
    working_directory: impl AsRef<Path>,
    size: TerminalSize,
  ) -> Result<Self, TerminalSessionError> {
    let mut command = CommandBuilder::new_default_prog();
    command.cwd(working_directory.as_ref());
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    Self::spawn_with_profile(command, TerminalProfile::SystemShell, size)
  }

  /// Spawns a selected operating-system shell profile.
  ///
  /// # Parameters
  ///
  /// * `profile` selects the default shell, Bash, or Fish.
  /// * `working_directory` is the initial directory for the child shell.
  /// * `size` is the initial character-grid and cell-pixel size.
  ///
  /// # Returns
  ///
  /// A live PTY session tagged with the selected profile. The built-in profile
  /// is rejected because it requires an in-process program endpoint.
  pub fn spawn_system_shell(
    profile: TerminalProfile,
    working_directory: impl AsRef<Path>,
    size: TerminalSize,
  ) -> Result<Self, TerminalSessionError> {
    let mut command = match profile {
      TerminalProfile::SystemShell => CommandBuilder::new_default_prog(),
      TerminalProfile::Bash => CommandBuilder::new("bash"),
      TerminalProfile::Fish => CommandBuilder::new("fish"),
      TerminalProfile::BuiltinShell => {
        return Err(TerminalSessionError::Pty {
          operation: "spawn system shell profile",
          message: "the built-in shell uses an in-process terminal program".into(),
        });
      }
    };
    command.cwd(working_directory.as_ref());
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    Self::spawn_with_profile(command, profile, size)
  }

  /// Spawns a command attached to a native pseudo-terminal.
  ///
  /// # Parameters
  ///
  /// * `command` configures the child program, environment, and working directory.
  /// * `size` is the initial character-grid and cell-pixel size.
  ///
  /// # Returns
  ///
  /// A live PTY session and VT terminal state.
  pub fn spawn(command: CommandBuilder, size: TerminalSize) -> Result<Self, TerminalSessionError> {
    Self::spawn_with_profile(command, TerminalProfile::SystemShell, size)
  }

  /// Spawns a native PTY command and records its terminal profile.
  fn spawn_with_profile(
    command: CommandBuilder,
    profile: TerminalProfile,
    size: TerminalSize,
  ) -> Result<Self, TerminalSessionError> {
    let pty_system = native_pty_system();
    let pair = pty_system
      .openpty(size.pty_size())
      .map_err(|error| pty_error("open native pseudo-terminal", error))?;
    let child = pair
      .slave
      .spawn_command(command)
      .map_err(|error| pty_error("spawn terminal child", error))?;
    let killer = child.clone_killer();
    drop(pair.slave);

    let reader = pair
      .master
      .try_clone_reader()
      .map_err(|error| pty_error("clone terminal reader", error))?;
    let writer = Arc::new(Mutex::new(
      pair
        .master
        .take_writer()
        .map_err(|error| pty_error("take terminal writer", error))?,
    ));
    let (event_sender, event_receiver) = mpsc::channel();
    let render_pending = Arc::new(AtomicBool::new(false));
    let proxy = TerminalEventProxy {
      sender: event_sender.clone(),
      writer: Arc::clone(&writer),
      render_pending: Arc::clone(&render_pending),
    };
    let terminal = Arc::new(FairMutex::new(Term::new(Config::default(), &size, proxy)));

    spawn_reader_thread(&terminal, &event_sender, &render_pending, reader)?;
    spawn_wait_thread(&event_sender, child)?;

    Ok(Self {
      profile,
      terminal,
      writer,
      backend: TerminalBackend::NativePty {
        master: pair.master,
        killer: Mutex::new(killer),
      },
      events: Mutex::new(event_receiver),
      render_pending,
    })
  }

  /// Creates a VT terminal connected to an in-process program endpoint.
  ///
  /// # Parameters
  ///
  /// * `profile` identifies the program hosted by the terminal surface.
  /// * `size` supplies the initial character-grid and cell-pixel dimensions.
  ///
  /// # Returns
  ///
  /// A terminal session for the frontend and a program endpoint that receives
  /// input bytes and writes ANSI/VT output.
  pub fn in_process(profile: TerminalProfile, size: TerminalSize) -> (Self, TerminalProgram) {
    let (input_sender, input_receiver) = mpsc::channel();
    let writer: SharedWriter = Arc::new(Mutex::new(Box::new(ProgramInputWriter {
      sender: input_sender.clone(),
    })));
    let (event_sender, event_receiver) = mpsc::channel();
    let render_pending = Arc::new(AtomicBool::new(false));
    let proxy = TerminalEventProxy {
      sender: event_sender.clone(),
      writer: Arc::clone(&writer),
      render_pending: Arc::clone(&render_pending),
    };
    let terminal = Arc::new(FairMutex::new(Term::new(Config::default(), &size, proxy)));
    let program = TerminalProgram {
      input: Mutex::new(input_receiver),
      output: TerminalProgramOutput {
        terminal: Arc::clone(&terminal),
        processor: Arc::new(Mutex::new(ansi::Processor::<ansi::StdSyncHandler>::new())),
        events: event_sender,
        render_pending: Arc::clone(&render_pending),
      },
    };
    let session = Self {
      profile,
      terminal,
      writer,
      backend: TerminalBackend::InProcess { input: input_sender },
      events: Mutex::new(event_receiver),
      render_pending,
    };
    (session, program)
  }

  /// Returns the profile represented by this terminal backend.
  pub const fn profile(&self) -> TerminalProfile {
    self.profile
  }

  /// Writes encoded keyboard or paste bytes to the terminal child.
  pub fn write(&self, bytes: &[u8]) -> Result<(), TerminalSessionError> {
    let mut writer = self.writer.lock().map_err(|_| TerminalSessionError::WriterPoisoned)?;
    writer.write_all(bytes)?;
    writer.flush()?;
    Ok(())
  }

  /// Updates both the native PTY and the VT screen dimensions.
  pub fn resize(&self, size: TerminalSize) -> Result<(), TerminalSessionError> {
    match &self.backend {
      TerminalBackend::NativePty { master, .. } => master
        .resize(size.pty_size())
        .map_err(|error| pty_error("resize native pseudo-terminal", error))?,
      TerminalBackend::InProcess { input } => input
        .send(TerminalProgramInput::Resize(size))
        .map_err(|_| TerminalSessionError::ProgramDisconnected)?,
    }
    self.terminal.lock().resize(size);
    Ok(())
  }

  /// Copies the currently visible terminal grid.
  pub fn snapshot(&self) -> TerminalSnapshot {
    TerminalSnapshot::from_term(&self.terminal.lock())
  }

  /// Drains terminal events accumulated since the previous call.
  pub fn drain_events(&self) -> Vec<TerminalEvent> {
    // Clear before draining so a concurrent parser update can enqueue the next
    // repaint instead of being hidden behind the event currently in the queue.
    self.render_pending.store(false, Ordering::Release);
    let Ok(receiver) = self.events.lock() else {
      return vec![TerminalEvent::Error("terminal event receiver lock is poisoned".into())];
    };
    receiver.try_iter().collect()
  }
}

impl Drop for TerminalSession {
  fn drop(&mut self) {
    match &mut self.backend {
      TerminalBackend::NativePty { killer, .. } => {
        if let Ok(killer) = killer.get_mut() {
          let _ = killer.kill();
        }
      }
      TerminalBackend::InProcess { input } => {
        let _ = input.send(TerminalProgramInput::Shutdown);
      }
    }
  }
}

struct ProgramInputWriter {
  sender: mpsc::Sender<TerminalProgramInput>,
}

impl Write for ProgramInputWriter {
  fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
    self
      .sender
      .send(TerminalProgramInput::Bytes(buffer.to_vec()))
      .map_err(|_| {
        std::io::Error::new(
          std::io::ErrorKind::BrokenPipe,
          "in-process terminal program disconnected",
        )
      })?;
    Ok(buffer.len())
  }

  fn flush(&mut self) -> std::io::Result<()> {
    Ok(())
  }
}

#[derive(Clone)]
struct TerminalEventProxy {
  sender: mpsc::Sender<TerminalEvent>,
  writer: SharedWriter,
  render_pending: Arc<AtomicBool>,
}

impl EventListener for TerminalEventProxy {
  fn send_event(&self, event: AlacrittyEvent) {
    match event {
      AlacrittyEvent::Wakeup | AlacrittyEvent::MouseCursorDirty => {
        send_render_event(&self.sender, &self.render_pending);
      }
      AlacrittyEvent::Title(title) => {
        let _ = self.sender.send(TerminalEvent::Title(title));
      }
      AlacrittyEvent::Bell => {
        let _ = self.sender.send(TerminalEvent::Bell);
      }
      AlacrittyEvent::PtyWrite(text) => match self.writer.lock() {
        Ok(mut writer) => {
          if let Err(error) = writer.write_all(text.as_bytes()).and_then(|()| writer.flush()) {
            let _ = self.sender.send(TerminalEvent::Error(error.to_string()));
          }
        }
        Err(_) => {
          let _ = self
            .sender
            .send(TerminalEvent::Error("terminal writer lock is poisoned".into()));
        }
      },
      _ => {}
    }
  }
}

/// Starts the blocking PTY reader without holding the terminal lock while waiting for bytes.
fn spawn_reader_thread(
  terminal: &SharedTerminal,
  sender: &mpsc::Sender<TerminalEvent>,
  render_pending: &Arc<AtomicBool>,
  mut reader: Box<dyn Read + Send>,
) -> Result<(), TerminalSessionError> {
  let terminal = Arc::clone(terminal);
  let sender = sender.clone();
  let render_pending = Arc::clone(render_pending);
  thread::Builder::new()
    .name("chitin-terminal-reader".into())
    .spawn(move || {
      let mut processor = ansi::Processor::<ansi::StdSyncHandler>::new();
      let mut buffer = [0_u8; 64 * 1024];
      loop {
        match reader.read(&mut buffer) {
          Ok(0) => break,
          Ok(read) => {
            processor.advance(&mut *terminal.lock(), &buffer[..read]);
            send_render_event(&sender, &render_pending);
          }
          Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
          Err(error) => {
            let _ = sender.send(TerminalEvent::Error(error.to_string()));
            break;
          }
        }
      }
    })
    .map(|_| ())
    .map_err(TerminalSessionError::Write)
}

/// Coalesces repeated parser wakeups into at most one queued repaint request.
fn send_render_event(sender: &mpsc::Sender<TerminalEvent>, pending: &AtomicBool) {
  if !pending.swap(true, Ordering::AcqRel) {
    let _ = sender.send(TerminalEvent::Render);
  }
}

/// Starts a waiter that reports the child exit code without blocking a UI executor.
fn spawn_wait_thread(
  sender: &mpsc::Sender<TerminalEvent>,
  mut child: Box<dyn portable_pty::Child + Send + Sync>,
) -> Result<(), TerminalSessionError> {
  let sender = sender.clone();
  thread::Builder::new()
    .name("chitin-terminal-wait".into())
    .spawn(move || match child.wait() {
      Ok(status) => {
        let _ = sender.send(TerminalEvent::Exited(status.exit_code()));
      }
      Err(error) => {
        let _ = sender.send(TerminalEvent::Error(error.to_string()));
      }
    })
    .map(|_| ())
    .map_err(TerminalSessionError::Write)
}

fn pty_error(operation: &'static str, error: impl std::fmt::Display) -> TerminalSessionError {
  TerminalSessionError::Pty {
    operation,
    message: error.to_string(),
  }
}

#[cfg(all(test, unix))]
mod tests {
  use std::{path::PathBuf, thread, time::Duration};

  use super::*;

  #[test]
  fn session_should_parse_output_from_a_native_pty() {
    let mut command = CommandBuilder::new("sh");
    command.args(["-c", "printf '\\033[32mnative-pty\\033[0m'"]);
    let Ok(session) = TerminalSession::spawn(command, TerminalSize::new(40, 4, 8, 16)) else {
      panic!("native PTY should be available during a Unix test");
    };

    let mut visible_text = String::new();
    for _ in 0..100 {
      visible_text = session.snapshot().cells.iter().map(|cell| cell.character).collect();
      if visible_text.contains("native-pty") {
        break;
      }
      thread::sleep(Duration::from_millis(10));
    }

    assert!(visible_text.contains("native-pty"));
    assert_eq!(
      session.snapshot().cell(0, 0).map(|cell| cell.foreground),
      Some(crate::TerminalColor::Named(crate::TerminalNamedColor::Green)),
    );
  }

  #[test]
  fn native_pty_should_report_the_child_exit_code() {
    let mut command = CommandBuilder::new("sh");
    command.args(["-c", "exit 3"]);
    let Ok(session) = TerminalSession::spawn(command, TerminalSize::new(40, 4, 8, 16)) else {
      panic!("native PTY should be available during a Unix test");
    };

    let mut exit_code = None;
    for _ in 0..200 {
      exit_code = session.drain_events().into_iter().find_map(|event| match event {
        TerminalEvent::Exited(code) => Some(code),
        _ => None,
      });
      if exit_code.is_some() {
        break;
      }
      thread::sleep(Duration::from_millis(10));
    }

    assert_eq!(exit_code, Some(3));
  }

  #[cfg(target_os = "linux")]
  #[test]
  fn dropping_a_session_should_terminate_the_pty_child() {
    let pid_file = std::env::temp_dir().join("chitin-terminal-dropped-child.pid");
    let _ = std::fs::remove_file(&pid_file);
    let mut command = CommandBuilder::new("sh");
    command.args(["-c", &format!("echo $$ > {}; exec sleep 300", pid_file.display())]);
    let Ok(session) = TerminalSession::spawn(command, TerminalSize::new(40, 4, 8, 16)) else {
      panic!("native PTY should be available during a Unix test");
    };
    let reported = (0..200)
      .find_map(|_| {
        thread::sleep(Duration::from_millis(10));
        std::fs::read_to_string(&pid_file).ok()
      })
      .and_then(|pid| pid.trim().parse::<i32>().ok());
    let Some(child_pid) = reported else {
      panic!("child should report its process id");
    };
    assert!(PathBuf::from(format!("/proc/{child_pid}")).exists());

    drop(session);

    let exited = (0..500).any(|_| {
      thread::sleep(Duration::from_millis(10));
      !PathBuf::from(format!("/proc/{child_pid}")).exists()
    });
    let _ = std::fs::remove_file(&pid_file);
    assert!(exited, "child {child_pid} outlived the session that owned it");
  }

  #[test]
  fn in_process_program_should_receive_terminal_input_bytes() {
    let (session, program) =
      TerminalSession::in_process(TerminalProfile::BuiltinShell, TerminalSize::new(40, 4, 8, 16));

    let write_result = session.write(b"help\r");
    let input = program.recv();

    assert!(write_result.is_ok());
    assert_eq!(input.ok(), Some(TerminalProgramInput::Bytes(b"help\r".to_vec())));
  }

  #[test]
  fn in_process_program_output_should_update_the_shared_vt_snapshot() {
    let (session, program) =
      TerminalSession::in_process(TerminalProfile::BuiltinShell, TerminalSize::new(40, 4, 8, 16));

    let write_result = program.output().write(b"\x1b[32mbuiltin-shell\x1b[0m");
    let visible_text = session
      .snapshot()
      .cells
      .iter()
      .map(|cell| cell.character)
      .collect::<String>();

    assert!(write_result.is_ok());
    assert!(visible_text.contains("builtin-shell"));
  }

  #[test]
  fn in_process_program_should_receive_terminal_resize() {
    let (session, program) =
      TerminalSession::in_process(TerminalProfile::BuiltinShell, TerminalSize::new(40, 4, 8, 16));
    let expected = TerminalSize::new(80, 24, 9, 18);

    let resize_result = session.resize(expected);
    let input = program.recv();

    assert!(resize_result.is_ok());
    assert_eq!(input.ok(), Some(TerminalProgramInput::Resize(expected)));
  }
}
