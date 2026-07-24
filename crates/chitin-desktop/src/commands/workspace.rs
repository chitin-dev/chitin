//! Workspace command definitions and default key bindings.
//!
//! This module owns command IDs and GPUI action types for workspace-sidebar
//! events. The actual behavior remains in the workspace tree implementation so
//! commands stay as a routing layer instead of becoming a second state owner.

use gpui::{KeyBinding, actions};

use crate::{app::ChitinApp, components::workspace_tree::WorkspaceTreeNavigation};

/// GPUI key context used by the project workspace tree.
pub(crate) const PROJECT_TREE_KEY_CONTEXT: &str = "ProjectTree";

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

/// Workspace-scoped commands supported by Chitin desktop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum WorkspaceCommand {
  /// Move project tree focus to the previous visible entry.
  FocusPrevious,
  /// Move project tree focus to the next visible entry.
  FocusNext,
  /// Open a focused file or toggle a focused directory.
  ActivateFocused,
  /// Move project tree focus to the first visible entry.
  FocusFirst,
  /// Move project tree focus to the last visible entry.
  FocusLast,
  /// Show or hide the project workspace sidebar.
  ToggleWorkspace,
}

impl WorkspaceCommand {
  /// Returns the stable dotted identifier for this workspace command.
  ///
  /// The ID is the string form that future text/config entry points should use
  /// when they need to refer to workspace behavior without linking directly to
  /// Rust enum variants.
  ///
  /// # Parameters
  ///
  /// This method reads `self`, the workspace command variant being identified.
  ///
  /// # Returns
  ///
  /// A static command ID such as `"workspace.activate_focused_entry"`.
  pub(crate) fn id(&self) -> &'static str {
    match self {
      Self::FocusPrevious => "workspace.focus_previous_entry",
      Self::FocusNext => "workspace.focus_next_entry",
      Self::ActivateFocused => "workspace.activate_focused_entry",
      Self::FocusFirst => "workspace.focus_first_entry",
      Self::FocusLast => "workspace.focus_last_entry",
      Self::ToggleWorkspace => "workspace.toggle_workspace",
    }
  }

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
  pub(crate) fn tree_navigation(&self) -> Option<WorkspaceTreeNavigation> {
    match self {
      Self::FocusPrevious => Some(WorkspaceTreeNavigation::FocusPrevious),
      Self::FocusNext => Some(WorkspaceTreeNavigation::FocusNext),
      Self::ActivateFocused => Some(WorkspaceTreeNavigation::ActivateFocused),
      Self::FocusFirst => Some(WorkspaceTreeNavigation::FocusFirst),
      Self::FocusLast => Some(WorkspaceTreeNavigation::FocusLast),
      Self::ToggleWorkspace => None,
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
  /// `command` is the workspace command to execute.
  ///
  /// `cx` is the GPUI app context used by tree navigation to notify the UI and
  /// spawn lazy directory loading when needed.
  ///
  /// # Returns
  ///
  /// This function returns `()`. The command mutates [`ChitinApp`] state
  /// directly through the workspace tree behavior.
  pub(crate) fn dispatch_workspace_command(
    &mut self,
    command: WorkspaceCommand,
    cx: &mut gpui::Context<Self>,
  ) {
    if let Some(navigation) = command.tree_navigation() {
      self.navigate_project_tree(navigation, cx);
    } else {
      self.toggle_workspace(cx);
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
    KeyBinding::new("up", FocusPreviousEntry, Some(PROJECT_TREE_KEY_CONTEXT)),
    KeyBinding::new("k", FocusPreviousEntry, Some(PROJECT_TREE_KEY_CONTEXT)),
    KeyBinding::new("down", FocusNextEntry, Some(PROJECT_TREE_KEY_CONTEXT)),
    KeyBinding::new("j", FocusNextEntry, Some(PROJECT_TREE_KEY_CONTEXT)),
    KeyBinding::new(
      "enter",
      ActivateFocusedEntry,
      Some(PROJECT_TREE_KEY_CONTEXT),
    ),
    KeyBinding::new("home", FocusFirstEntry, Some(PROJECT_TREE_KEY_CONTEXT)),
    KeyBinding::new("end", FocusLastEntry, Some(PROJECT_TREE_KEY_CONTEXT)),
    KeyBinding::new("g g", FocusFirstEntry, Some(PROJECT_TREE_KEY_CONTEXT)),
    KeyBinding::new("G", FocusLastEntry, Some(PROJECT_TREE_KEY_CONTEXT)),
    KeyBinding::new("shift-e", ToggleWorkspace, None),
  ]
}

#[cfg(test)]
mod tests {
  use super::*;

  /// Verifies that workspace command IDs use stable dotted names.
  #[test]
  /// # Parameters
  ///
  /// This test takes no parameters.
  ///
  /// # Returns
  ///
  /// This test returns `()` and panics if command IDs change unexpectedly.
  fn workspace_command_id_should_match_config_name() {
    assert_eq!(
      WorkspaceCommand::ToggleWorkspace.id(),
      "workspace.toggle_workspace"
    );
  }

  /// Verifies that key bindings stay scoped to project tree focus.
  #[test]
  /// # Parameters
  ///
  /// This test takes no parameters.
  ///
  /// # Returns
  ///
  /// This test returns `()` and panics if a keybinding loses its context.
  fn default_key_bindings_should_use_project_tree_context() {
    let bindings = default_key_bindings();

    assert_eq!(bindings.len(), 10);
    assert_eq!(
      bindings
        .iter()
        .filter(|binding| binding.predicate().is_some())
        .count(),
      9
    );
    assert_eq!(
      bindings
        .iter()
        .filter(|binding| binding.predicate().is_none())
        .count(),
      1
    );
  }

  /// Verifies that Vim-style tree navigation bindings are registered.
  #[test]
  /// # Parameters
  ///
  /// This test takes no parameters.
  ///
  /// # Returns
  ///
  /// This test returns `()` and panics if the multi-key `g g` sequence or
  /// uppercase `G` binding is not registered.
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
  /// # Parameters
  ///
  /// This test takes no parameters.
  ///
  /// # Returns
  ///
  /// This test returns `()` and panics if the workspace toggle shortcut is not
  /// registered as a global shifted `e` binding.
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
  /// # Parameters
  ///
  /// This test takes no parameters.
  ///
  /// # Returns
  ///
  /// This test returns `()` and panics if command-to-navigation mapping changes.
  fn workspace_command_should_map_to_tree_navigation() {
    assert_eq!(
      WorkspaceCommand::FocusPrevious.tree_navigation(),
      Some(WorkspaceTreeNavigation::FocusPrevious)
    );
    assert_eq!(WorkspaceCommand::ToggleWorkspace.tree_navigation(), None);
  }
}
