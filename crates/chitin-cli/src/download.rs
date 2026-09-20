//! Terminal presentation for portable command execution events.

use std::{
  sync::{Arc, Mutex},
  time::Duration,
};

use chitin_command::{CommandEventSink, CommandExecutionEvent, CommandMessageLevel, CommandProgress};
use indicatif::{ProgressBar, ProgressStyle};

/// Mutable terminal progress retained across command event callbacks.
#[derive(Default)]
struct DownloadProgressState {
  /// Active progress bar for the current artifact.
  bar: Option<ProgressBar>,
  /// One-based stage currently represented by the progress bar.
  stage_index: Option<usize>,
}

/// Creates a command-event sink that renders download progress in the terminal.
pub(crate) fn terminal_event_sink() -> CommandEventSink {
  let state = Arc::new(Mutex::new(DownloadProgressState::default()));
  CommandEventSink::new(move |event| {
    let Ok(mut state) = state.lock() else {
      return;
    };
    render_event(&mut state, event);
  })
}

/// Projects one frontend-neutral event into terminal progress and messages.
fn render_event(state: &mut DownloadProgressState, event: CommandExecutionEvent) {
  match event {
    CommandExecutionEvent::Progress(progress) => render_progress(state, progress),
    CommandExecutionEvent::Artifact(artifact) => {
      finish_progress_bar(state);
      println!("  ✓ Saved to {}", artifact.path.display());
    }
    CommandExecutionEvent::Message(message) => match message.level {
      CommandMessageLevel::Info => println!("  ✓ {}", message.text),
      CommandMessageLevel::Warning => eprintln!("  ! {}", message.text),
    },
    CommandExecutionEvent::Completed { .. } => finish_progress_bar(state),
    CommandExecutionEvent::Started { .. } => {}
  }
}

/// Updates or replaces the progress bar for the active download stage.
fn render_progress(state: &mut DownloadProgressState, progress: CommandProgress) {
  if state.stage_index != Some(progress.stage_index) {
    finish_progress_bar(state);
    let bar = ProgressBar::new_spinner();
    bar.enable_steady_tick(Duration::from_millis(100));
    if let Ok(style) = ProgressStyle::with_template("  {spinner} Downloading {bytes} ({elapsed})") {
      bar.set_style(style);
    }
    let label = progress.stage_label.as_deref().unwrap_or("structure");
    eprintln!(
      "  Downloading {}/{}: {label}",
      progress.stage_index, progress.stage_count
    );
    state.stage_index = Some(progress.stage_index);
    state.bar = Some(bar);
  }

  let Some(bar) = state.bar.as_ref() else {
    return;
  };
  if let Some(total) = progress.total {
    if bar.length().is_none() {
      if let Ok(style) =
        ProgressStyle::with_template("  Downloading [{bar:32.cyan/blue}] {bytes}/{total_bytes} ({eta})")
      {
        bar.set_style(style.progress_chars("##-"));
      }
      bar.set_length(total);
    }
  }
  bar.set_position(progress.completed);
}

/// Clears the current progress bar after an artifact or command completes.
fn finish_progress_bar(state: &mut DownloadProgressState) {
  if let Some(bar) = state.bar.take() {
    bar.finish_and_clear();
  }
}
