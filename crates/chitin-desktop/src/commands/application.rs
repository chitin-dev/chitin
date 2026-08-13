//! Application command definitions and global key bindings.

use gpui::{KeyBinding, actions};

use crate::commands::command_panel::CommandShortcut;
use chitin_command::ApplicationCommand;

const TOGGLE_COMMAND_PANEL_SHORTCUTS: [CommandShortcut; 1] =
  [CommandShortcut::new("secondary-shift-p", "Ctrl/Cmd+Shift+P", None)];

actions!(
  application,
  [
    /// Show or hide the command panel.
    ToggleCommandPanel,
  ]
);

/// Builds application-level keybindings.
///
/// # Parameters
///
/// This function takes no parameters.
///
/// # Returns
///
/// Global keybindings for application commands.
pub(crate) fn default_key_bindings() -> [KeyBinding; 1] {
  [TOGGLE_COMMAND_PANEL_SHORTCUTS[0].binding(ToggleCommandPanel)]
}

impl crate::app::ChitinApp {
  /// Executes an application command.
  ///
  /// # Parameters
  ///
  /// * `command` is the application command to execute.
  /// * `cx` is the GPUI app context notified when state changes.
  pub(crate) fn dispatch_application_command(&mut self, command: ApplicationCommand, cx: &mut gpui::Context<Self>) {
    match command {
      ApplicationCommand::ToggleCommandPanel => self.toggle_command_panel(cx),
    }
  }
}
