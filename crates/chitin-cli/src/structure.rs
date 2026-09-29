//! ANSI presentation adapter for shared command reports.

use chitin_command::{
  CommandOutcome, CommandOutputTone, CommandReportStatus, render_outcome as render_command_outcome,
};
use console::Style;

/// Writes a shared command outcome with process-terminal styling.
///
/// # Parameters
///
/// * `outcome` is the frontend-independent result returned by the executor.
///
/// # Returns
///
/// The domain-level completion status used as the process exit status.
pub(crate) fn render_outcome(outcome: &CommandOutcome) -> CommandReportStatus {
  let report = render_command_outcome(outcome);
  for line in &report.lines {
    for span in &line.spans {
      print!("{}", style(span.tone).apply_to(&span.text));
    }
    println!();
  }
  report.status
}

/// Maps a frontend-neutral output tone to ANSI terminal styling.
fn style(tone: CommandOutputTone) -> Style {
  match tone {
    CommandOutputTone::Primary => Style::new(),
    CommandOutputTone::Secondary => Style::new().dim(),
    CommandOutputTone::Heading => Style::new().cyan().bold(),
    CommandOutputTone::Success => Style::new().green().bold(),
    CommandOutputTone::Warning => Style::new().yellow(),
    CommandOutputTone::Error => Style::new().red().bold(),
  }
}
