//! Application command definitions and global key bindings.

use gpui::{KeyBinding, actions};

use crate::commands::command_panel::{CommandShortcut, primary_shortcut_label};
use chitin_command::ApplicationCommand;
use chitin_command::{CommandCategory, CommandDescriptor, CommandInvocationKind};

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

/// Builds command panel descriptors for application commands.
///
/// # Parameters
///
/// This function takes no parameters.
///
/// # Returns
///
/// Application command metadata used by the command registry.
pub(crate) fn command_descriptors() -> Vec<CommandDescriptor> {
  vec![CommandDescriptor {
    id: ApplicationCommand::ToggleCommandPanel.id(),
    title: "Toggle Command Panel",
    category: CommandCategory::Application,
    keywords: &["quick pick", "palette", "commands"],
    shortcut: primary_shortcut_label(&TOGGLE_COMMAND_PANEL_SHORTCUTS),
    invocation: CommandInvocationKind::Immediate,
    command: ApplicationCommand::ToggleCommandPanel.into(),
  }]
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
