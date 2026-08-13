//! Database command definitions for provider workflows.

use crate::commands::ChitinCommand;
use chitin_command::DatabaseCommand;

impl crate::app::ChitinApp {
  /// Executes a database command.
  ///
  /// # Parameters
  ///
  /// * `command` is the database command to execute.
  /// * `cx` is the GPUI app context notified when state changes.
  pub(crate) fn dispatch_database_command(&mut self, command: DatabaseCommand, cx: &mut gpui::Context<Self>) {
    match command {
      DatabaseCommand::DownloadRcsbStructure => {
        let command = ChitinCommand::from(DatabaseCommand::DownloadRcsbStructure);
        if self.command_panel.open_form(command) {
          cx.notify();
        }
      }
    }
  }
}
