//! Typed command dispatch for desktop workbench events.
//!
//! Commands are the shared event boundary between UI sources such as key
//! bindings, future command-palette entries, status-bar command input, and
//! direct component controls.

use gpui::Context;

use crate::app::ChitinApp;

pub(crate) mod workspace;

pub(crate) use workspace::WorkspaceCommand;

/// Top-level command hierarchy for Chitin desktop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ChitinCommand {
  /// Commands owned by the project workspace sidebar and tree.
  Workspace(WorkspaceCommand),
}

impl ChitinCommand {
  /// Returns the stable dotted identifier for this command.
  ///
  /// Command IDs are intended for text/config boundaries such as a future
  /// command palette, status-bar command input, keybinding file, or plugin API.
  /// Internal desktop code should prefer dispatching the typed enum variant
  /// instead of parsing this string.
  ///
  /// # Parameters
  ///
  /// This method reads `self`, the typed command value whose external ID is
  /// requested.
  ///
  /// # Returns
  ///
  /// A stable static command ID such as `"workspace.focus_next_entry"`.
  pub(crate) fn id(&self) -> &'static str {
    match self {
      Self::Workspace(command) => command.id(),
    }
  }
}

impl From<WorkspaceCommand> for ChitinCommand {
  /// Wraps a workspace command in the top-level desktop command hierarchy.
  ///
  /// # Parameters
  ///
  /// `command` is the workspace-scoped command to expose as a generic
  /// [`ChitinCommand`].
  ///
  /// # Returns
  ///
  /// [`ChitinCommand::Workspace`] containing the provided workspace command.
  fn from(command: WorkspaceCommand) -> Self {
    Self::Workspace(command)
  }
}

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
  /// `command` is the typed command to execute.
  ///
  /// `cx` is the GPUI app context used to notify the UI or spawn follow-up
  /// work from command handlers.
  ///
  /// # Returns
  ///
  /// This function returns `()`. Command handlers mutate app state directly and
  /// use `cx` for any UI notifications or background work.
  pub(crate) fn dispatch_command(&mut self, command: ChitinCommand, cx: &mut Context<Self>) {
    log::debug!("Dispatch command {}", command.id());

    match command {
      ChitinCommand::Workspace(command) => self.dispatch_workspace_command(command, cx),
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
pub(crate) fn default_key_bindings() -> Vec<gpui::KeyBinding> {
  workspace::default_key_bindings().into()
}

#[cfg(test)]
mod tests {
  use super::*;

  /// Verifies that nested command IDs stay stable for text boundaries.
  #[test]
  fn command_id_should_include_hierarchy_namespace() {
    let command = ChitinCommand::from(WorkspaceCommand::FocusNext);

    assert_eq!(command.id(), "workspace.focus_next_entry");
  }
}
