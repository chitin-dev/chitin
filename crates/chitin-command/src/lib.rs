//! Shared command definitions and search behavior.

mod application;
mod database;
mod panel_tab;
mod structure;
mod workspace;

pub use application::ApplicationCommand;
pub use database::DatabaseCommand;
pub use panel_tab::PanelTabCommand;
pub use structure::StructureCommand;
pub use workspace::WorkspaceCommand;

/// Top-level command hierarchy shared by desktop, terminal, and CLI inputs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChitinCommand {
  /// Workspace and document-panel commands.
  Workspace(WorkspaceCommand),
  /// Database-provider commands.
  Database(DatabaseCommand),
  /// Application-shell commands.
  Application(ApplicationCommand),
  /// Structure parsing and inspection commands.
  Structure(StructureCommand),
}

impl ChitinCommand {
  /// Returns the stable dotted identifier for this command.
  pub fn id(&self) -> &'static str {
    match self {
      Self::Workspace(command) => command.id(),
      Self::Database(command) => command.id(),
      Self::Application(command) => command.id(),
      Self::Structure(command) => command.id(),
    }
  }
}

impl From<WorkspaceCommand> for ChitinCommand {
  fn from(command: WorkspaceCommand) -> Self {
    Self::Workspace(command)
  }
}

impl From<DatabaseCommand> for ChitinCommand {
  fn from(command: DatabaseCommand) -> Self {
    Self::Database(command)
  }
}

impl From<ApplicationCommand> for ChitinCommand {
  fn from(command: ApplicationCommand) -> Self {
    Self::Application(command)
  }
}

impl From<StructureCommand> for ChitinCommand {
  fn from(command: StructureCommand) -> Self {
    Self::Structure(command)
  }
}

impl From<PanelTabCommand> for ChitinCommand {
  fn from(command: PanelTabCommand) -> Self {
    Self::Workspace(WorkspaceCommand::PanelTab(command))
  }
}

/// Command category used for search grouping and presentation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandCategory {
  /// Workspace and document-panel commands.
  Workspace,
  /// External database commands.
  Database,
  /// Application-shell commands.
  Application,
}

impl CommandCategory {
  /// Returns the display label for this category.
  pub fn label(self) -> &'static str {
    match self {
      Self::Workspace => "Workspace",
      Self::Database => "Database",
      Self::Application => "Application",
    }
  }
}

/// Describes what happens after a command is selected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandInvocationKind {
  /// Execute immediately.
  Immediate,
  /// Request a command-specific interactive workflow.
  Form,
}

/// Searchable metadata for a command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandDescriptor {
  /// Stable command identifier.
  pub id: &'static str,
  /// User-facing title.
  pub title: &'static str,
  /// Search category.
  pub category: CommandCategory,
  /// Search keywords.
  pub keywords: &'static [&'static str],
  /// Optional shortcut label.
  pub shortcut: Option<&'static str>,
  /// Invocation behavior.
  pub invocation: CommandInvocationKind,
  /// Typed command payload.
  pub command: ChitinCommand,
}

/// A ranked command result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandSearchResult<'a> {
  /// Match score, where larger values rank first.
  pub score: i32,
  /// Matching descriptor.
  pub descriptor: &'a CommandDescriptor,
}

/// Registry and deterministic search implementation for command descriptors.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CommandRegistry {
  commands: Vec<CommandDescriptor>,
}

impl CommandRegistry {
  /// Creates a registry from command descriptors.
  pub fn new(commands: impl IntoIterator<Item = CommandDescriptor>) -> Self {
    Self {
      commands: commands.into_iter().collect(),
    }
  }

  /// Finds a descriptor by its typed command.
  pub fn descriptor_for(&self, command: &ChitinCommand) -> Option<&CommandDescriptor> {
    self.commands.iter().find(|descriptor| &descriptor.command == command)
  }

  /// Searches commands using deterministic scoring.
  pub fn search(&self, query: &str) -> Vec<CommandSearchResult<'_>> {
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
}

/// Creates the built-in registry from all shared command descriptors.
pub fn default_registry() -> CommandRegistry {
  CommandRegistry::new(
    workspace::command_descriptors()
      .into_iter()
      .chain(panel_tab::command_descriptors())
      .chain(database::command_descriptors())
      .chain(application::command_descriptors()),
  )
}

/// Scores a command descriptor against a normalized search query.
///
/// Exact command identifiers receive the highest score, followed by title,
/// identifier-prefix, title-prefix, and category matches. The ordering keeps
/// command IDs predictable while still allowing users to search by the label
/// shown in the command panel.
///
/// # Parameters
///
/// * `descriptor` is the command metadata being matched.
/// * `query` is a trimmed, lowercase search query.
///
/// # Returns
///
/// A score when the descriptor matches, or `None` when it should be omitted.
fn score_descriptor(descriptor: &CommandDescriptor, query: &str) -> Option<i32> {
  if query.is_empty() {
    return Some(1);
  }
  let id = normalize(descriptor.id);
  let title = normalize(descriptor.title);
  let category = normalize(descriptor.category.label());
  // Check stronger matches first so a broad prefix cannot hide an exact hit.
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

/// Trims surrounding whitespace and lowercases a command-search value.
///
/// # Parameters
///
/// * `value` is the raw command identifier, title, category, or user query.
///
/// # Returns
///
/// A canonical value suitable for case-insensitive matching.
fn normalize(value: &str) -> String {
  value.trim().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn command_ids_should_include_scope() {
    assert_eq!(
      ChitinCommand::from(WorkspaceCommand::FocusNext).id(),
      "workspace.focus_next_entry"
    );
  }
}
