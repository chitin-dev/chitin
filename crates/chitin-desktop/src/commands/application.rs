//! Application command definitions and global key bindings.

use gpui::{KeyBinding, actions};

use crate::commands::command_panel::{
  CommandCategory, CommandDescriptor, CommandInvocationKind, CommandShortcut, primary_shortcut_label,
};

const TOGGLE_COMMAND_PANEL_SHORTCUTS: [CommandShortcut; 1] =
  [CommandShortcut::new("secondary-shift-p", "Ctrl/Cmd+Shift+P", None)];

actions!(
  application,
  [
    /// Show or hide the command panel.
    ToggleCommandPanel,
  ]
);

/// Application-scoped commands supported by Chitin desktop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ApplicationCommand {
  /// Show or hide the command panel.
  ToggleCommandPanel,
}

impl ApplicationCommand {
  /// Returns the stable dotted identifier for this application command.
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
      Self::ToggleCommandPanel => "application.toggle_command_panel",
    }
  }
}

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
    form_prompt: None,
    form_placeholder: None,
    command: ApplicationCommand::ToggleCommandPanel.into(),
  }]
}

impl crate::app::ChitinApp {
  /// Executes an application command.
  ///
  /// # Parameters
  ///
  /// `command` is the application command to execute.
  ///
  /// `cx` is the GPUI app context notified when state changes.
  pub(crate) fn dispatch_application_command(&mut self, command: ApplicationCommand, cx: &mut gpui::Context<Self>) {
    match command {
      ApplicationCommand::ToggleCommandPanel => self.toggle_command_panel(cx),
    }
  }
}
