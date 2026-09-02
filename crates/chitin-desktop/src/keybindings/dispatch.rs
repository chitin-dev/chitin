//! Desktop execution adapters for shared commands.

use chitin_command::{
  ApplicationCommand, ChitinCommand, DatabaseCommand, PanelTabCommand, StructureCommand, WorkspaceCommand,
};
use gpui::{Context, Window};

use crate::{app::ChitinApp, components::workspace_tree::WorkspaceTreeNavigation};

trait WorkspaceCommandDesktopExt {
  /// Converts a workspace command into tree navigation when applicable.
  fn tree_navigation(&self) -> Option<WorkspaceTreeNavigation>;
}

impl WorkspaceCommandDesktopExt for WorkspaceCommand {
  fn tree_navigation(&self) -> Option<WorkspaceTreeNavigation> {
    match self {
      Self::FocusPrevious => Some(WorkspaceTreeNavigation::FocusPrevious),
      Self::FocusNext => Some(WorkspaceTreeNavigation::FocusNext),
      Self::ActivateFocused => Some(WorkspaceTreeNavigation::ActivateFocused),
      Self::FocusFirst => Some(WorkspaceTreeNavigation::FocusFirst),
      Self::FocusLast => Some(WorkspaceTreeNavigation::FocusLast),
      Self::ToggleWorkspace | Self::PanelTab(_) => None,
    }
  }
}

impl ChitinApp {
  /// Executes a command from a UI path that can provide a window context.
  ///
  /// Project-tree activation needs the window to create specialized document
  /// views, such as the molecular PDB/mmCIF renderer. Other commands retain
  /// the regular context-only dispatch path.
  pub(crate) fn dispatch_command_with_window(
    &mut self,
    command: ChitinCommand,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    if matches!(command, ChitinCommand::Workspace(WorkspaceCommand::ActivateFocused)) {
      self.activate_focused_project_tree_entry_with_window(window, cx);
    } else {
      self.dispatch_command(command, cx);
    }
  }

  /// Executes a typed command against desktop state.
  ///
  /// # Parameters
  ///
  /// * `command` is the shared command translated from a UI input source.
  /// * `cx` is the GPUI context used by the command handler.
  ///
  /// # Returns
  ///
  /// This function returns `()` after routing the command to its feature handler.
  pub(crate) fn dispatch_command(&mut self, command: ChitinCommand, cx: &mut Context<Self>) {
    log::debug!("Dispatch command {}", command.id());

    match command {
      ChitinCommand::Workspace(command) => self.dispatch_workspace_command(command, cx),
      ChitinCommand::Database(command) => self.dispatch_database_command(command, cx),
      ChitinCommand::Application(command) => self.dispatch_application_command(command, cx),
      ChitinCommand::Structure(command) => self.dispatch_structure_command(command),
    }
  }

  /// Reports structure commands that are currently CLI-only.
  pub(crate) fn dispatch_structure_command(&mut self, command: StructureCommand) {
    log::debug!("Structure command {} is not a desktop action yet", command.id());
  }

  /// Executes an application command.
  ///
  /// # Parameters
  ///
  /// * `command` identifies the application-level action.
  /// * `cx` is the GPUI context used to refresh the application state.
  ///
  /// # Returns
  ///
  /// This function returns `()` after applying the application action.
  pub(crate) fn dispatch_application_command(&mut self, command: ApplicationCommand, cx: &mut Context<Self>) {
    match command {
      ApplicationCommand::ToggleCommandPanel => self.toggle_command_panel(cx),
    }
  }

  /// Executes a database command.
  ///
  /// # Parameters
  ///
  /// * `command` identifies the database workflow to start.
  /// * `cx` is the GPUI context notified when the form is opened.
  ///
  /// # Returns
  ///
  /// This function returns `()` after opening the corresponding form.
  pub(crate) fn dispatch_database_command(&mut self, command: DatabaseCommand, cx: &mut Context<Self>) {
    match command {
      DatabaseCommand::DownloadRcsbStructure => {
        let command = ChitinCommand::from(DatabaseCommand::DownloadRcsbStructure);
        if self.command_panel.open_form(command) {
          cx.notify();
        }
      }
    }
  }

  /// Executes a document panel-tab command.
  ///
  /// # Parameters
  ///
  /// * `command` identifies the tab operation to perform.
  /// * `cx` is the GPUI context notified when panel state changes.
  ///
  /// # Returns
  ///
  /// This function returns `()` after applying the tab operation.
  pub(crate) fn dispatch_panel_tab_command(&mut self, command: PanelTabCommand, cx: &mut Context<Self>) {
    let changed = match command {
      PanelTabCommand::FocusPrevious => self.focus_previous_document_panel_tab(),
      PanelTabCommand::FocusNext => self.focus_next_document_panel_tab(),
      PanelTabCommand::Close => self.close_focused_document_panel_tab(),
    };

    if changed {
      cx.notify();
    }
  }

  /// Executes a workspace command against workspace-sidebar state.
  ///
  /// # Parameters
  ///
  /// * `command` identifies workspace or nested panel-tab navigation.
  /// * `cx` is the GPUI context used for focus, loading, and redraw updates.
  ///
  /// # Returns
  ///
  /// This function returns `()` after applying the workspace operation.
  pub(crate) fn dispatch_workspace_command(&mut self, command: WorkspaceCommand, cx: &mut Context<Self>) {
    match command {
      WorkspaceCommand::ToggleWorkspace => self.toggle_workspace(cx),
      WorkspaceCommand::PanelTab(command) => self.dispatch_panel_tab_command(command, cx),
      command => {
        if let Some(navigation) = command.tree_navigation() {
          self.navigate_project_tree(navigation, cx);
        }
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  /// Verifies that workspace commands map onto workspace tree navigation.
  #[test]
  fn workspace_command_should_map_to_tree_navigation() {
    assert_eq!(
      WorkspaceCommand::FocusPrevious.tree_navigation(),
      Some(WorkspaceTreeNavigation::FocusPrevious)
    );
    assert_eq!(WorkspaceCommand::ToggleWorkspace.tree_navigation(), None);
  }
}
