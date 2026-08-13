//! Desktop adapter for the shared command registry.

use chitin_command::{ChitinCommand, CommandSearchResult};

use super::{application, database, tab, workspace};

pub(crate) use chitin_command::{CommandCategory, CommandDescriptor, CommandInvocationKind};

/// A GPUI shortcut declared once for keybinding and command-panel display.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CommandShortcut {
  /// GPUI keystroke string.
  pub(crate) keystrokes: &'static str,
  /// User-facing shortcut label.
  pub(crate) label: &'static str,
  /// Optional GPUI key context.
  pub(crate) context: Option<&'static str>,
}

impl CommandShortcut {
  /// Creates a shortcut specification.
  pub(crate) const fn new(keystrokes: &'static str, label: &'static str, context: Option<&'static str>) -> Self {
    Self {
      keystrokes,
      label,
      context,
    }
  }

  /// Builds a GPUI keybinding for this shortcut.
  pub(crate) fn binding<A: gpui::Action>(&self, action: A) -> gpui::KeyBinding {
    gpui::KeyBinding::new(self.keystrokes, action, self.context)
  }
}

/// Returns the primary display label from a shortcut collection.
pub(crate) fn primary_shortcut_label(shortcuts: &[CommandShortcut]) -> Option<&'static str> {
  shortcuts.first().map(|shortcut| shortcut.label)
}

/// Desktop-owned view over the shared command registry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CommandRegistry {
  inner: chitin_command::CommandRegistry,
}

impl CommandRegistry {
  /// Creates a registry from all desktop command descriptors.
  pub(crate) fn new() -> Self {
    Self {
      inner: chitin_command::CommandRegistry::new(
        workspace::command_descriptors()
          .into_iter()
          .chain(tab::command_descriptors())
          .chain(database::command_descriptors())
          .chain(application::command_descriptors()),
      ),
    }
  }

  /// Finds a descriptor by typed command.
  pub(crate) fn descriptor_for(&self, command: &ChitinCommand) -> Option<&CommandDescriptor> {
    self.inner.descriptor_for(command)
  }

  /// Searches commands using the shared deterministic scoring rules.
  pub(crate) fn search(&self, query: &str) -> Vec<CommandSearchResult<'_>> {
    self.inner.search(query)
  }
}

impl Default for CommandRegistry {
  fn default() -> Self {
    Self::new()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn command_registry_should_register_all_categories() {
    let registry = CommandRegistry::new();
    assert!(
      registry
        .search("")
        .iter()
        .any(|result| result.descriptor.category == CommandCategory::Workspace)
    );
    assert!(
      registry
        .search("")
        .iter()
        .any(|result| result.descriptor.category == CommandCategory::Database)
    );
    assert!(
      registry
        .search("")
        .iter()
        .any(|result| result.descriptor.category == CommandCategory::Application)
    );
  }

  #[test]
  fn search_should_rank_exact_id_before_title_match() {
    let registry = CommandRegistry::new();
    let results = registry.search("workspace.toggle_workspace");
    assert_eq!(
      results.first().map(|result| result.descriptor.id),
      Some("workspace.toggle_workspace")
    );
  }

  #[test]
  fn search_should_match_database_keyword() {
    let registry = CommandRegistry::new();
    let results = registry.search("pdb");
    assert_eq!(
      results.first().map(|result| result.descriptor.id),
      Some("database.rcsb.download_structure")
    );
  }
}
