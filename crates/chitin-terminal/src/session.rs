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

use crate::{TerminalSize, TerminalSnapshot};

type SharedWriter = Arc<Mutex<Box<dyn Write + Send>>>;
type SharedTerminal = Arc<FairMutex<Term<TerminalEventProxy>>>;

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
  #[error("failed to write terminal input: {0}")]
  Write(#[from] std::io::Error),
}

/// Owns a native PTY child and its frontend-independent VT terminal state.
pub struct TerminalSession {
  terminal: SharedTerminal,
  writer: SharedWriter,
  master: Box<dyn MasterPty + Send>,
  killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
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
    Self::spawn(command, size)
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
      terminal,
      writer,
      master: pair.master,
      killer: Mutex::new(killer),
      events: Mutex::new(event_receiver),
      render_pending,
    })
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
    self
      .master
      .resize(size.pty_size())
      .map_err(|error| pty_error("resize native pseudo-terminal", error))?;
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
    if let Ok(killer) = self.killer.get_mut() {
      let _ = killer.kill();
    }
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
  use std::{thread, time::Duration};

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
}
