//! Translation from frontend-neutral shell records to terminal presentation.

use std::path::Path;

use chitin_builtin_shell::{BuiltinShellSnapshot, ShellCommandId, ShellExecutionStatus, ShellTranscriptContent};
use chitin_command::CommandMessageLevel;
use chitin_command_runtime::CommandOutcome;
use chitin_ui::{
  composite::command_terminal::{CommandTerminalId, CommandTerminalStatus},
  primitive::terminal::{TerminalLine, TerminalSpan, TerminalTone},
};

use super::TerminalPanelState;

/// Builds the two-tone prompt shared by frozen and live command lines.
pub(super) fn terminal_prompt(working_directory: &Path) -> TerminalLine {
  TerminalLine::new([
    TerminalSpan::accent("chitin "),
    TerminalSpan::secondary(format!("{} ", working_directory.display())),
    TerminalSpan::accent("❯ "),
  ])
}

/// Converts preformatted command help into terminal rows.
pub(super) fn terminal_text_lines(text: &str) -> Vec<TerminalLine> {
  text.lines().map(TerminalLine::plain).collect()
}

/// Converts one shell command's event stream into terminal output rows.
///
/// # Parameters
///
/// * `snapshot` supplies the frontend-neutral session transcript.
/// * `shell_id` selects entries belonging to one submitted command.
///
/// # Returns
///
/// Ordered progress, diagnostic, artifact, failure, and cancellation rows.
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
pub(super) fn terminal_outcome(outcome: CommandOutcome) -> (CommandTerminalStatus, Vec<TerminalLine>) {
  match outcome {
    CommandOutcome::DatabaseDownload { artifact_count } => (
      CommandTerminalStatus::Succeeded,
      vec![TerminalLine::new([TerminalSpan::success(format!(
        "Downloaded {artifact_count} structure artifact(s)."
      ))])],
    ),
    CommandOutcome::StructureValidation(validation) => match validation.error {
      Some(error) => (
        CommandTerminalStatus::Failed,
        vec![TerminalLine::new([TerminalSpan::error(format!(
          "Invalid structure {}: {error}",
          validation.path.display()
        ))])],
      ),
      None => (
        CommandTerminalStatus::Succeeded,
        vec![TerminalLine::new([TerminalSpan::success(format!(
          "Valid structure: {} ({})",
          validation.path.display(),
          validation.format.label()
        ))])],
      ),
    },
    CommandOutcome::StructureInspection(inspection) => {
      let structure = &inspection.parsed.structure;
      (
        CommandTerminalStatus::Succeeded,
        vec![
          TerminalLine::new([
            TerminalSpan::secondary("Structure: "),
            TerminalSpan::primary(inspection.path.display().to_string()),
          ]),
          TerminalLine::new([
            TerminalSpan::secondary("Format: "),
            TerminalSpan::primary(inspection.format.label()),
            TerminalSpan::secondary(" | Bytes: "),
            TerminalSpan::primary(inspection.byte_count.to_string()),
            TerminalSpan::secondary(" | Diagnostics: "),
            TerminalSpan::primary(inspection.parsed.diagnostics.len().to_string()),
          ]),
          TerminalLine::plain(format!(
            "Models: {} | Chains: {} | Residues: {} | Atoms: {} | Bonds: {}",
            structure.models().len(),
            structure.chains().len(),
            structure.residues().len(),
            structure.atoms().len(),
            structure.bonds().len()
          )),
        ],
      )
    }
  }
}

/// Resolves cancellation or failure state for one terminal command mapping.
///
/// # Parameters
///
/// * `snapshot` supplies completed shell execution records.
/// * `terminal_id` identifies the visible command block.
/// * `panel` maps terminal blocks back to shell-local identities.
///
/// # Returns
///
/// The final presentation status when both mapping and execution still exist.
pub(super) fn shell_terminal_status(
  snapshot: &BuiltinShellSnapshot,
  terminal_id: CommandTerminalId,
  panel: &TerminalPanelState,
) -> Option<CommandTerminalStatus> {
  let shell_id = panel
    .bindings
    .iter()
    .find_map(|binding| (binding.terminal_id == terminal_id).then_some(binding.shell_id))?;
  snapshot
    .executions
    .iter()
    .find(|record| record.id == shell_id)
    .map(|record| match record.status {
      ShellExecutionStatus::Cancelled => CommandTerminalStatus::Cancelled,
      ShellExecutionStatus::Failed => CommandTerminalStatus::Failed,
      ShellExecutionStatus::Succeeded => CommandTerminalStatus::Succeeded,
      ShellExecutionStatus::Running | ShellExecutionStatus::Cancelling => CommandTerminalStatus::Failed,
    })
}
