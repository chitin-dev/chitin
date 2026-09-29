//! Desktop execution adapters for shared commands.

use chitin_command::{ApplicationCommand, FrontendCommand, PanelTabCommand, WorkspaceCommand};
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
    command: FrontendCommand,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    match command {
      FrontendCommand::Workspace(WorkspaceCommand::ActivateFocused) => {
        self.activate_focused_project_tree_entry_with_window(window, cx);
      }
      command => self.dispatch_command(command, cx),
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
  pub(crate) fn dispatch_command(&mut self, command: FrontendCommand, cx: &mut Context<Self>) {
    log::debug!("Dispatch command {}", command.id());

    match command {
      FrontendCommand::Workspace(command) => self.dispatch_workspace_command(command, cx),
      FrontendCommand::Application(command) => self.dispatch_application_command(command, cx),
    }
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
      ApplicationCommand::ToggleTerminal => self.toggle_terminal(cx),
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
