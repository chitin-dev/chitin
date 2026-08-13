//! Typed command dispatch for desktop workbench events.
//!
//! Commands are the shared event boundary between UI sources such as key
//! bindings, future command-palette entries, status-bar command input, and
//! direct component controls.

pub(crate) use chitin_command::{ApplicationCommand, ChitinCommand, PanelTabCommand, WorkspaceCommand};
pub(crate) use command_panel::{CommandInvocationKind, CommandRegistry};
use gpui::Context;

use crate::app::ChitinApp;

pub(crate) mod application;
pub(crate) mod command_panel;
pub(crate) mod database;
pub(crate) mod tab;
pub(crate) mod workspace;

impl ChitinApp {
  /// Executes a typed desktop command against the app state.
  ///
  /// This is the main command bus for UI events. Keybindings, future command
  /// palette entries, status-bar command input, and component controls should
  /// all route through this function once they have been translated into a
  /// typed [`ChitinCommand`].
  ///
  /// # Parameters
  ///
  /// * `command` is the typed command to execute.
  /// * `cx` is the GPUI app context used to notify the UI or spawn follow-up
  ///   work from command handlers.
  ///
  /// # Returns
  ///
  /// This function returns `()`. Command handlers mutate app state directly and
  /// use `cx` for any UI notifications or background work.
  pub(crate) fn dispatch_command(&mut self, command: ChitinCommand, cx: &mut Context<Self>) {
    log::debug!("Dispatch command {}", command.id());

    match command {
      ChitinCommand::Workspace(command) => self.dispatch_workspace_command(command, cx),
      ChitinCommand::Database(command) => self.dispatch_database_command(command, cx),
      ChitinCommand::Application(command) => self.dispatch_application_command(command, cx),
    }
  }
}

/// Builds all default keybindings registered by the desktop app.
///
/// The returned bindings are grouped from command modules so each feature area
/// owns its own key defaults while `main` can register a single collection.
///
/// # Parameters
///
/// This function takes no parameters.
///
/// # Returns
///
/// A vector of GPUI keybindings ready to pass to [`gpui::App::bind_keys`].
pub fn default_key_bindings() -> Vec<gpui::KeyBinding> {
  workspace::default_key_bindings()
    .into_iter()
    .chain(tab::default_key_bindings())
    .chain(application::default_key_bindings())
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn command_id_should_include_hierarchy_namespace() {
    let command = ChitinCommand::from(WorkspaceCommand::FocusNext);

    assert_eq!(command.id(), "workspace.focus_next_entry");
  }

  /// Verifies that default keybindings include workspace and panel-tab groups.
  #[test]
  fn default_key_bindings_should_include_panel_tab_bindings() {
    assert_eq!(default_key_bindings().len(), 14);
  }
}
