//! Workspace command definitions and default key bindings.
//!
//! This module owns command IDs and GPUI action types for workspace-sidebar
//! events. The actual behavior remains in the workspace tree implementation so
//! commands stay as a routing layer instead of becoming a second state owner.

use chitin_command::WorkspaceCommand;
use gpui::{KeyBinding, actions};

use crate::{
  app::ChitinApp, commands::command_panel::CommandShortcut, components::workspace_tree::WorkspaceTreeNavigation,
};

/// GPUI key context used by the project workspace tree.
pub(crate) const PROJECT_TREE_KEY_CONTEXT: &str = "ProjectTree";

#[rustfmt::skip]
const FOCUS_PREVIOUS_SHORTCUTS: [CommandShortcut; 2] = [
  CommandShortcut::new(
    "up",
    "Up",
    Some(PROJECT_TREE_KEY_CONTEXT)
  ),
  CommandShortcut::new(
    "k",
    "K",
    Some(PROJECT_TREE_KEY_CONTEXT)
  ),
];

#[rustfmt::skip]
const FOCUS_NEXT_SHORTCUTS: [CommandShortcut; 2] = [
  CommandShortcut::new(
    "down",
    "Down",
    Some(PROJECT_TREE_KEY_CONTEXT)
  ),
  CommandShortcut::new(
    "j",
    "J",
    Some(PROJECT_TREE_KEY_CONTEXT)
  ),
];

#[rustfmt::skip]
const ACTIVATE_FOCUSED_SHORTCUTS: [CommandShortcut; 1] = [
  CommandShortcut::new(
    "enter",
    "Enter",
    Some(PROJECT_TREE_KEY_CONTEXT)
  ),
];

#[rustfmt::skip]
const FOCUS_FIRST_SHORTCUTS: [CommandShortcut; 2] = [
  CommandShortcut::new(
    "home",
    "Home",
    Some(PROJECT_TREE_KEY_CONTEXT)
  ),
  CommandShortcut::new(
    "g g",
    "G G",
    Some(PROJECT_TREE_KEY_CONTEXT)
  ),
];

#[rustfmt::skip]
const FOCUS_LAST_SHORTCUTS: [CommandShortcut; 2] = [
  CommandShortcut::new(
    "end",
    "End",
    Some(PROJECT_TREE_KEY_CONTEXT)
  ),
  CommandShortcut::new(
    "G",
    "Shift+G",
    Some(PROJECT_TREE_KEY_CONTEXT)
  ),
];

#[rustfmt::skip]
const TOGGLE_WORKSPACE_SHORTCUTS: [CommandShortcut; 1] = [
  CommandShortcut::new(
    "shift-e",
    "Shift+E",
    None
)];

actions!(
  workspace,
  [
    /// Move project tree focus to the previous visible entry.
    FocusPreviousEntry,
    /// Move project tree focus to the next visible entry.
    FocusNextEntry,
    /// Open or toggle the currently focused project tree entry.
    ActivateFocusedEntry,
    /// Move project tree focus to the first visible entry.
    FocusFirstEntry,
    /// Move project tree focus to the last visible entry.
    FocusLastEntry,
    /// Show or hide the project workspace sidebar.
    ToggleWorkspace,
  ]
);

trait WorkspaceCommandDesktopExt {
  /// Converts this command into workspace tree navigation when applicable.
  ///
  /// Workspace commands are the command-bus representation, while
  /// [`WorkspaceTreeNavigation`] is the tree renderer's local behavior model.
  /// Commands that affect broader workbench state, such as toggling the
  /// sidebar shell, return `None` because they are not tree navigation.
  ///
  /// # Parameters
  ///
  /// This method reads `self`, the workspace command being dispatched.
  ///
  /// # Returns
  ///
  /// `Some(WorkspaceTreeNavigation)` for tree commands, or `None` for
  /// workspace commands handled by the broader app state.
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
  /// Executes a workspace command against workspace-sidebar state.
  ///
  /// This handler adapts command-bus events to the current workspace tree
  /// implementation. It intentionally does not parse command IDs; callers
  /// should parse external strings into [`WorkspaceCommand`] before dispatch.
  ///
  /// # Parameters
  ///
  /// * `command` is the workspace command to execute.
  /// * `cx` is the GPUI app context used by tree navigation to notify the UI and
  ///   spawn lazy directory loading when needed.
  ///
  /// # Returns
  ///
  /// This function returns `()`. The command mutates [`ChitinApp`] state
  /// directly through the workspace tree behavior.
  pub(crate) fn dispatch_workspace_command(&mut self, command: WorkspaceCommand, cx: &mut gpui::Context<Self>) {
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

/// Builds default keybindings for the project workspace tree.
///
/// Arrow keys and `j`/`k` move focus, `Enter` activates the focused row, and
/// `Home`/`End` jump to the first or last visible row. Vim-style `g g` and
/// `G` provide alternate first/last navigation. Every binding is scoped
/// to [`PROJECT_TREE_KEY_CONTEXT`] so text inputs and future editors can
/// override the same keystrokes in their own contexts.
///
/// # Parameters
///
/// This function takes no parameters.
///
/// # Returns
///
/// Ten GPUI keybindings for the current workspace tree navigation commands.
pub(crate) fn default_key_bindings() -> [KeyBinding; 10] {
  [
    FOCUS_PREVIOUS_SHORTCUTS[0].binding(FocusPreviousEntry),
    FOCUS_PREVIOUS_SHORTCUTS[1].binding(FocusPreviousEntry),
    FOCUS_NEXT_SHORTCUTS[0].binding(FocusNextEntry),
    FOCUS_NEXT_SHORTCUTS[1].binding(FocusNextEntry),
    ACTIVATE_FOCUSED_SHORTCUTS[0].binding(ActivateFocusedEntry),
    FOCUS_FIRST_SHORTCUTS[0].binding(FocusFirstEntry),
    FOCUS_LAST_SHORTCUTS[0].binding(FocusLastEntry),
    FOCUS_FIRST_SHORTCUTS[1].binding(FocusFirstEntry),
    FOCUS_LAST_SHORTCUTS[1].binding(FocusLastEntry),
    TOGGLE_WORKSPACE_SHORTCUTS[0].binding(ToggleWorkspace),
  ]
}

#[cfg(test)]
mod tests {
  use super::*;

  /// Verifies that workspace command IDs use stable dotted names.
  #[test]
  fn workspace_command_id_should_match_config_name() {
    assert_eq!(WorkspaceCommand::ToggleWorkspace.id(), "workspace.toggle_workspace");
  }

  /// Verifies that key bindings stay scoped to project tree focus.
  #[test]
  fn default_key_bindings_should_use_project_tree_context() {
    let bindings = default_key_bindings();

    assert_eq!(bindings.len(), 10);
    assert_eq!(
      bindings.iter().filter(|binding| binding.predicate().is_some()).count(),
      9
    );
    assert_eq!(
      bindings.iter().filter(|binding| binding.predicate().is_none()).count(),
      1
    );
  }

  /// Verifies that Vim-style tree navigation bindings are registered.
  #[test]
  fn default_key_bindings_should_include_vim_bounds_navigation() {
    let bindings = default_key_bindings();

    assert!(bindings.iter().any(|binding| {
      let keystrokes = binding.keystrokes();
      keystrokes.len() == 2
        && keystrokes.iter().all(|keystroke| {
          keystroke.key() == "g"
            && !keystroke.modifiers().shift
            && !keystroke.modifiers().control
            && !keystroke.modifiers().alt
        })
    }));
    assert!(bindings.iter().any(|binding| {
      let keystrokes = binding.keystrokes();
      keystrokes.len() == 1 && keystrokes[0].key() == "g" && keystrokes[0].modifiers().shift
    }));
  }

  /// Verifies that the workspace toggle has a global Shift+E binding.
  #[test]
  fn default_key_bindings_should_include_global_workspace_toggle() {
    let bindings = default_key_bindings();

    assert!(bindings.iter().any(|binding| {
      let keystrokes = binding.keystrokes();
      binding.predicate().is_none()
        && keystrokes.len() == 1
        && keystrokes[0].key() == "e"
        && keystrokes[0].modifiers().shift
    }));
  }

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
