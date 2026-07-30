//! Database command definitions for provider workflows.

use crate::commands::{
  ChitinCommand,
  command_panel::{CommandCategory, CommandDescriptor, CommandInvocationKind},
};

/// Database-scoped commands supported by Chitin desktop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DatabaseCommand {
  /// Open a form for downloading an RCSB structure.
  DownloadRcsbStructure,
}

impl crate::app::ChitinApp {
  /// Executes a database command.
  ///
  /// # Parameters
  ///
  /// `command` is the database command to execute.
  ///
  /// `cx` is the GPUI app context notified when state changes.
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

/// Builds command panel descriptors for database commands.
///
/// # Parameters
///
/// This function takes no parameters.
///
/// # Returns
///
/// Database command metadata used by the command registry.
pub(crate) fn command_descriptors() -> Vec<CommandDescriptor> {
  vec![CommandDescriptor {
    id: DatabaseCommand::DownloadRcsbStructure.id(),
    title: "Download RCSB Structure",
    category: CommandCategory::Database,
    keywords: &["pdb", "rcsb", "mmcif", "structure"],
    shortcut: None,
    invocation: CommandInvocationKind::Form,
    form_prompt: Some("RCSB PDB ID"),
    form_placeholder: None,
    command: DatabaseCommand::DownloadRcsbStructure.into(),
  }]
}

impl DatabaseCommand {
  /// Returns the stable dotted identifier for this database command.
  ///
  /// # Parameters
  ///
  /// This method reads `self`.
  ///
  /// # Returns
  ///
  /// A static command ID.
  pub(crate) fn id(&self) -> &'static str {
    match self {
      Self::DownloadRcsbStructure => "database.rcsb.download_structure",
    }
  }
}
