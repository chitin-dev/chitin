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
    }
  }

  /// Converts this command into the workspace tree navigation it drives.
  ///
  /// Workspace commands are the command-bus representation, while
  /// [`WorkspaceTreeNavigation`] is the tree renderer's local behavior model.
  /// This conversion keeps those layers separate without duplicating the actual
  /// navigation implementation.
  ///
  /// # Parameters
  ///
  /// This method reads `self`, the workspace command being dispatched.
  ///
  /// # Returns
  ///
  /// The matching [`WorkspaceTreeNavigation`] variant.
  pub(crate) fn tree_navigation(&self) -> WorkspaceTreeNavigation {
    match self {
      Self::FocusPrevious => WorkspaceTreeNavigation::FocusPrevious,
      Self::FocusNext => WorkspaceTreeNavigation::FocusNext,
      Self::ActivateFocused => WorkspaceTreeNavigation::ActivateFocused,
      Self::FocusFirst => WorkspaceTreeNavigation::FocusFirst,
      Self::FocusLast => WorkspaceTreeNavigation::FocusLast,
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
    self.navigate_project_tree(command.tree_navigation(), cx);
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
/// Nine GPUI keybindings for the current workspace tree navigation commands.
pub(crate) fn default_key_bindings() -> [KeyBinding; 9] {
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
      WorkspaceCommand::ActivateFocused.id(),
      "workspace.activate_focused_entry"
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

    assert_eq!(bindings.len(), 9);
    assert!(bindings.iter().all(|binding| binding.predicate().is_some()));
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
      WorkspaceTreeNavigation::FocusPrevious
    );
  }
}
