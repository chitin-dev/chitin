//! Translation from frontend-neutral shell records to terminal presentation.

use std::path::Path;

use chitin_builtin_shell::{BuiltinShellSnapshot, RenderingPanel, ShellCommandId, ShellTranscriptContent};
use chitin_command::{
  CommandMessageLevel, CommandOutcome, CommandOutputTone, CommandReportStatus, render_outcome as render_command_outcome,
};
use chitin_terminal::TerminalBlockStatus;
use chitin_ui::views::terminal::{TerminalLine, TerminalSpan, TerminalTone};

/// Builds an ANSI-colored prompt for the in-process terminal program.
pub(super) fn terminal_prompt_ansi(working_directory: &Path, panel: Option<&RenderingPanel>) -> String {
  let context = panel.map_or_else(String::new, |panel| {
    format!(
      " \x1b[35m[panel #{} {}]\x1b[0m",
      panel.id(),
      crate::commands::terminal_label(panel.title())
    )
  });
  format!(
    "\x1b[38;2;0;145;220mchitin\x1b[0m \x1b[90m{}\x1b[0m{context} \x1b[38;2;0;145;220m❯\x1b[0m ",
    crate::commands::terminal_label(&working_directory.to_string_lossy()),
  )
}

/// Converts one command's structured event stream into terminal rows.
///
/// # Parameters
///
/// * `snapshot` supplies the current shell transcript.
/// * `shell_id` selects entries belonging to one submitted command.
///
/// # Returns
///
/// Ordered progress, message, artifact, failure, and cancellation rows.
pub(super) fn shell_output_lines(snapshot: &BuiltinShellSnapshot, shell_id: ShellCommandId) -> Vec<TerminalLine> {
  snapshot
    .transcript
    .iter()
    .filter(|entry| entry.command_id == shell_id)
    .filter_map(|entry| match &entry.content {
      ShellTranscriptContent::Input(_) | ShellTranscriptContent::Completed => None,
      ShellTranscriptContent::Progress(progress) => {
        let stage = progress
          .stage_label
          .clone()
          .unwrap_or_else(|| format!("Stage {}/{}", progress.stage_index, progress.stage_count));
        let amount = progress.total.map_or_else(
          || progress.completed.to_string(),
          |total| format!("{} / {}", progress.completed, total),
        );
        Some(TerminalLine::new([TerminalSpan::info(format!("{stage}  {amount}"))]))
      }
      ShellTranscriptContent::Message(message) => Some(TerminalLine::new([TerminalSpan::new(
        message.text.clone(),
        match message.level {
          CommandMessageLevel::Info => TerminalTone::Info,
          CommandMessageLevel::Warning => TerminalTone::Warning,
        },
      )])),
      ShellTranscriptContent::Artifact(artifact) => Some(TerminalLine::new([
        TerminalSpan::secondary("→ "),
        TerminalSpan::primary(format!("{} ({} bytes)", artifact.path.display(), artifact.bytes)),
      ])),
      ShellTranscriptContent::Failed(message) => {
        Some(TerminalLine::new([TerminalSpan::error(format!("error: {message}"))]))
      }
      ShellTranscriptContent::Cancelled => Some(TerminalLine::new([TerminalSpan::warning("^C cancelled")])),
    })
    .collect()
}

/// Converts a portable command result into rows placed before the next prompt.
///
/// # Parameters
///
/// * `outcome` is the structured result returned by the shared command runtime.
///
/// # Returns
///
/// The terminal status and final rows for the corresponding command block.
pub(super) fn terminal_outcome(outcome: CommandOutcome) -> (TerminalBlockStatus, Vec<TerminalLine>) {
  let report = render_command_outcome(&outcome);
  let status = match report.status {
    CommandReportStatus::Succeeded => TerminalBlockStatus::Succeeded,
    CommandReportStatus::Failed => TerminalBlockStatus::Failed,
  };
  let lines = report
    .lines
    .into_iter()
    .map(|line| {
      TerminalLine::new(
        line
          .spans
          .into_iter()
          .map(|span| TerminalSpan::new(span.text, terminal_tone(span.tone))),
      )
    })
    .collect();
  (status, lines)
}

/// Maps a shared output tone to the command terminal theme.
fn terminal_tone(tone: CommandOutputTone) -> TerminalTone {
  match tone {
    CommandOutputTone::Primary => TerminalTone::Primary,
    CommandOutputTone::Secondary => TerminalTone::Secondary,
    CommandOutputTone::Heading => TerminalTone::Accent,
    CommandOutputTone::Success => TerminalTone::Success,
    CommandOutputTone::Warning => TerminalTone::Warning,
    CommandOutputTone::Error => TerminalTone::Error,
  }
}

/// Converts semantic terminal lines into ANSI-colored program output.
pub(super) fn terminal_lines_ansi(lines: &[TerminalLine]) -> String {
  lines
    .iter()
    .map(|line| {
      line
        .spans()
        .iter()
        .map(|span| format!("{}{}\x1b[0m", tone_ansi(span.tone), span.text))
        .collect::<String>()
    })
    .collect::<Vec<_>>()
    .join("\n")
}

/// Returns the ANSI SGR prefix for a semantic terminal tone.
fn tone_ansi(tone: TerminalTone) -> &'static str {
  match tone {
    TerminalTone::Primary => "\x1b[39m",
    TerminalTone::Secondary => "\x1b[90m",
    TerminalTone::Accent | TerminalTone::Info => "\x1b[36m",
    TerminalTone::Success => "\x1b[32m",
    TerminalTone::Warning => "\x1b[33m",
    TerminalTone::Error => "\x1b[31m",
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn prompt_should_show_selected_rendering_identity_and_title() {
    let panel = RenderingPanel::new(7, "4HHB.cif");
    let prompt = terminal_prompt_ansi(Path::new("."), Some(&panel));
    assert!(prompt.contains("[panel #7 4HHB.cif]"));
    assert!(!terminal_prompt_ansi(Path::new("."), None).contains("[panel"));
  }

  #[test]
  fn prompt_metadata_should_not_inject_terminal_control_sequences() {
    let panel = RenderingPanel::new(7, "file\x1b[2J\n.cif");
    let prompt = terminal_prompt_ansi(Path::new("."), Some(&panel));
    assert!(!prompt.contains("\x1b[2J"));
    assert!(!prompt.contains('\n'));
  }

  #[test]
  fn terminal_adapter_should_preserve_shared_report_text() {
    let outcome = CommandOutcome::DatabaseDownload { artifact_count: 2 };
    let expected = render_command_outcome(&outcome).plain_text();

    let (_, lines) = terminal_outcome(outcome);
    let actual = lines
      .iter()
      .map(|line| line.spans().iter().map(|span| span.text.as_str()).collect::<String>())
      .collect::<Vec<_>>()
      .join("\n");

    assert_eq!(actual, expected);
  }
}
