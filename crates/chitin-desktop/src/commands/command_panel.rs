//! Searchable command metadata and registry.

use super::{ChitinCommand, application, database, tab, workspace};

/// A shortcut declared once for keybinding and command-panel display.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CommandShortcut {
  /// GPUI keystroke string.
  pub(crate) keystrokes: &'static str,
  /// User-facing shortcut label.
  pub(crate) label: &'static str,
  /// Optional GPUI key context.
  pub(crate) context: Option<&'static str>,
}

/// Command category shown in the command panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandCategory {
  /// Project workspace and document-panel commands.
  Workspace,
  /// External database commands.
  Database,
  /// Application-level commands.
  Application,
}

/// Command invocation behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandInvocationKind {
  /// Execute immediately when selected.
  Immediate,
  /// Open a command-specific form before execution.
  Form,
}

/// Metadata used by the command panel to search and invoke commands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CommandDescriptor {
  /// Stable command identifier.
  pub(crate) id: &'static str,
  /// User-facing command title.
  pub(crate) title: &'static str,
  /// Command category.
  pub(crate) category: CommandCategory,
  /// Search keywords.
  pub(crate) keywords: &'static [&'static str],
  /// Default shortcut shown in the list.
  pub(crate) shortcut: Option<&'static str>,
  /// Invocation behavior.
  pub(crate) invocation: CommandInvocationKind,
  /// Optional prompt used by form-mode command input.
  pub(crate) form_prompt: Option<&'static str>,
  /// Optional placeholder used by form-mode command input.
  pub(crate) form_placeholder: Option<&'static str>,
  /// Typed command payload.
  pub(crate) command: ChitinCommand,
}

/// Registry of available desktop commands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CommandRegistry {
  /// Registered command descriptors.
  commands: Vec<CommandDescriptor>,
}

/// Search result with score and descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CommandSearchResult<'a> {
  /// Ranking score. Higher scores sort first.
  pub(crate) score: i32,
  /// Matching command descriptor.
  pub(crate) descriptor: &'a CommandDescriptor,
}

impl CommandCategory {
  /// Returns a display label for this category.
  ///
  /// # Parameters
  ///
  /// This method reads `self`.
  ///
  /// # Returns
  ///
  /// A static category label.
  pub(crate) fn label(&self) -> &'static str {
    match self {
      Self::Workspace => "Workspace",
      Self::Database => "Database",
      Self::Application => "Application",
    }
  }
}

impl CommandShortcut {
  /// Creates a shortcut spec.
  ///
  /// # Parameters
  ///
  /// * `keystrokes` is the GPUI keybinding syntax.
  /// * `label` is the command-panel display text.
  /// * `context` limits dispatch to a GPUI key context when present.
  ///
  /// # Returns
  ///
  /// A shortcut spec shared by keybindings and descriptors.
  pub(crate) const fn new(keystrokes: &'static str, label: &'static str, context: Option<&'static str>) -> Self {
    Self {
      keystrokes,
      label,
      context,
    }
  }

  /// Builds a GPUI keybinding for this shortcut.
  ///
  /// # Parameters
  ///
  /// * `action` is the GPUI action dispatched by this shortcut.
  ///
  /// # Returns
  ///
  /// A GPUI keybinding using this shortcut's keystrokes and context.
  pub(crate) fn binding<A: gpui::Action>(&self, action: A) -> gpui::KeyBinding {
    gpui::KeyBinding::new(self.keystrokes, action, self.context)
  }
}

/// Returns the primary display label from a shortcut collection.
///
/// # Parameters
///
/// * `shortcuts` is the shortcut collection owned by a command module.
///
/// # Returns
///
/// The first shortcut label, or `None` when the command has no shortcut.
pub(crate) fn primary_shortcut_label(shortcuts: &[CommandShortcut]) -> Option<&'static str> {
  shortcuts.first().map(|shortcut| shortcut.label)
}

impl CommandRegistry {
  /// Registers every command currently exposed by the desktop app.
  ///
  /// # Parameters
  ///
  /// This function takes no parameters.
  ///
  /// # Returns
  ///
  /// A registry containing all command descriptors.
  pub(crate) fn new() -> Self {
    let mut registry = Self { commands: Vec::new() };
    registry.register_many(workspace::command_descriptors());
    registry.register_many(tab::command_descriptors());
    registry.register_many(database::command_descriptors());
    registry.register_many(application::command_descriptors());
    registry
  }

  /// Finds a descriptor by typed command.
  ///
  /// # Parameters
  ///
  /// * `command` is the typed command payload to locate.
  ///
  /// # Returns
  ///
  /// A descriptor borrowed from this registry when the command is registered.
  pub(crate) fn descriptor_for(&self, command: &ChitinCommand) -> Option<&CommandDescriptor> {
    self.commands.iter().find(|descriptor| &descriptor.command == command)
  }

  /// Searches commands using simple deterministic scoring.
  ///
  /// # Parameters
  ///
  /// * `query` is the user-entered search text.
  ///
  /// # Returns
  ///
  /// Results ordered by descending score, then title.
  pub(crate) fn search(&self, query: &str) -> Vec<CommandSearchResult<'_>> {
    let query = normalize(query);
    let mut results = self
      .commands
      .iter()
      .filter_map(|descriptor| {
        score_descriptor(descriptor, &query).map(|score| CommandSearchResult { score, descriptor })
      })
      .collect::<Vec<_>>();
    results.sort_by(|left, right| {
      right
        .score
        .cmp(&left.score)
        .then_with(|| left.descriptor.title.cmp(right.descriptor.title))
    });
    results
  }

  /// Registers one command descriptor.
  ///
  /// # Parameters
  ///
  /// * `descriptor` is appended to the registry.
  fn register(&mut self, descriptor: CommandDescriptor) {
    self.commands.push(descriptor);
  }

  /// Registers a descriptor collection.
  /// # Parameters
  ///
  /// * `descriptors` is the command metadata collection to append.
  fn register_many(&mut self, descriptors: impl IntoIterator<Item = CommandDescriptor>) {
    descriptors.into_iter().for_each(|descriptor| self.register(descriptor));
  }
}

impl Default for CommandRegistry {
  /// Creates a command registry with all available commands.
  fn default() -> Self {
    Self::new()
  }
}

/// Scores one command descriptor against a normalized query.
/// # Parameters
///
/// * `descriptor` is the command being scored.
/// * `query` is the normalized query.
///
/// # Returns
///
/// `Some(score)` for matches, otherwise `None`.
fn score_descriptor(descriptor: &CommandDescriptor, query: &str) -> Option<i32> {
  if query.is_empty() {
    return Some(1);
  }

  let id = normalize(descriptor.id);
  let title = normalize(descriptor.title);
  let category = normalize(descriptor.category.label());
  if id == query {
    return Some(1000);
  }
  if id.starts_with(query) {
    return Some(850);
  }
  if title.starts_with(query) {
    return Some(700);
  }
  if title.contains(query) {
    return Some(500);
  }
  if descriptor.keywords.iter().any(|keyword| normalize(keyword) == query) {
    return Some(420);
  }
  if descriptor
    .keywords
    .iter()
    .any(|keyword| normalize(keyword).contains(query))
  {
    return Some(360);
  }
  if category == query || category.starts_with(query) {
    return Some(280);
  }
  None
}

/// Normalizes a string for command search.
///
/// # Parameters
///
/// * `value` is the string to normalize.
///
/// # Returns
///
/// Lowercase trimmed text.
fn normalize(value: &str) -> String {
  value.trim().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn command_registry_should_register_all_categories() {
    let registry = CommandRegistry::new();

    assert!(
      registry
        .commands
        .iter()
        .any(|command| command.category == CommandCategory::Workspace)
    );
    assert!(
      registry
        .commands
        .iter()
        .any(|command| command.category == CommandCategory::Database)
    );
    assert!(
      registry
        .commands
        .iter()
        .any(|command| command.category == CommandCategory::Application)
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
