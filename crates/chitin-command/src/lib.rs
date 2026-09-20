//! Shared command definitions and search behavior.

mod application;
mod database;
mod panel_tab;
mod structure;
mod workspace;

pub use application::ApplicationCommand;
pub use database::{DatabaseCommand, RcsbDownloadArguments};
pub use panel_tab::PanelTabCommand;
pub use structure::{
  CommandOutputFormat, StructureCommand, StructureInputArguments, StructureInspectArguments, StructureValidateArguments,
};
pub use workspace::WorkspaceCommand;

/// Stable identity of a command independent of its invocation arguments.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CommandId {
  /// Focus the previous project entry.
  WorkspaceFocusPrevious,
  /// Focus the next project entry.
  WorkspaceFocusNext,
  /// Activate the focused project entry.
  WorkspaceActivateFocused,
  /// Focus the first project entry.
  WorkspaceFocusFirst,
  /// Focus the last project entry.
  WorkspaceFocusLast,
  /// Show or hide the workspace sidebar.
  WorkspaceToggle,
  /// Focus the previous document tab.
  PanelTabFocusPrevious,
  /// Focus the next document tab.
  PanelTabFocusNext,
  /// Close the active document tab.
  PanelTabClose,
  /// Download one or more RCSB structures.
  DatabaseDownloadRcsbStructure,
  /// Show or hide the command panel.
  ApplicationToggleCommandPanel,
  /// Inspect a local structure file.
  StructureInspect,
  /// Validate a local structure file.
  StructureValidate,
}

impl CommandId {
  /// Returns the stable dotted identifier.
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::WorkspaceFocusPrevious => "workspace.focus_previous_entry",
      Self::WorkspaceFocusNext => "workspace.focus_next_entry",
      Self::WorkspaceActivateFocused => "workspace.activate_focused_entry",
      Self::WorkspaceFocusFirst => "workspace.focus_first_entry",
      Self::WorkspaceFocusLast => "workspace.focus_last_entry",
      Self::WorkspaceToggle => "workspace.toggle_workspace",
      Self::PanelTabFocusPrevious => "tab.focus_previous",
      Self::PanelTabFocusNext => "tab.focus_next",
      Self::PanelTabClose => "tab.close",
      Self::DatabaseDownloadRcsbStructure => "database.rcsb.download_structure",
      Self::ApplicationToggleCommandPanel => "application.toggle_command_panel",
      Self::StructureInspect => "structure.inspect",
      Self::StructureValidate => "structure.validate",
    }
  }

  /// Builds an executable command when this identity needs no arguments.
  pub fn command_without_arguments(self) -> Option<ChitinCommand> {
    match self {
      Self::WorkspaceFocusPrevious => Some(WorkspaceCommand::FocusPrevious.into()),
      Self::WorkspaceFocusNext => Some(WorkspaceCommand::FocusNext.into()),
      Self::WorkspaceActivateFocused => Some(WorkspaceCommand::ActivateFocused.into()),
      Self::WorkspaceFocusFirst => Some(WorkspaceCommand::FocusFirst.into()),
      Self::WorkspaceFocusLast => Some(WorkspaceCommand::FocusLast.into()),
      Self::WorkspaceToggle => Some(WorkspaceCommand::ToggleWorkspace.into()),
      Self::PanelTabFocusPrevious => Some(PanelTabCommand::FocusPrevious.into()),
      Self::PanelTabFocusNext => Some(PanelTabCommand::FocusNext.into()),
      Self::PanelTabClose => Some(PanelTabCommand::Close.into()),
      Self::ApplicationToggleCommandPanel => Some(ApplicationCommand::ToggleCommandPanel.into()),
      Self::DatabaseDownloadRcsbStructure | Self::StructureInspect | Self::StructureValidate => None,
    }
  }
}

impl std::fmt::Display for CommandId {
  /// Formats the stable dotted identifier.
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter.write_str(self.as_str())
  }
}

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
  /// Returns the stable identity for this command.
  pub fn id(&self) -> CommandId {
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

/// Frontend-independent description of one available command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandDescriptor {
  /// Stable command identifier.
  pub id: CommandId,
  /// User-facing title.
  pub title: &'static str,
  /// Whether an input adapter must collect arguments before execution.
  pub requires_arguments: bool,
}

/// Search and presentation metadata attached to a command registration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandRegistration {
  /// Frontend-independent command description.
  pub descriptor: CommandDescriptor,
  /// Search result category.
  pub category: CommandCategory,
  /// Additional search keywords.
  pub keywords: &'static [&'static str],
  /// Optional shortcut label shown by search frontends.
  pub shortcut: Option<&'static str>,
}

/// A ranked command result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandSearchResult<'a> {
  /// Match score, where larger values rank first.
  pub score: i32,
  /// Matching descriptor.
  pub descriptor: &'a CommandDescriptor,
  /// Optional shortcut label associated with the registration.
  pub shortcut: Option<&'static str>,
}

/// Registry and deterministic search implementation for command descriptors.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CommandRegistry {
  commands: Vec<CommandRegistration>,
}

impl CommandRegistry {
  /// Creates a registry from command descriptors.
  pub fn new(commands: impl IntoIterator<Item = CommandRegistration>) -> Self {
    Self {
      commands: commands.into_iter().collect(),
    }
  }

  /// Finds a descriptor by stable command identity.
  pub fn descriptor_for(&self, id: CommandId) -> Option<&CommandDescriptor> {
    self
      .commands
      .iter()
      .map(|registration| &registration.descriptor)
      .find(|descriptor| descriptor.id == id)
  }

  /// Searches commands using deterministic scoring.
  pub fn search(&self, query: &str) -> Vec<CommandSearchResult<'_>> {
    let query = normalize(query);
    let mut results = self
      .commands
      .iter()
      .filter_map(|registration| {
        score_registration(registration, &query).map(|score| CommandSearchResult {
          score,
          descriptor: &registration.descriptor,
          shortcut: registration.shortcut,
        })
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
    workspace::command_registrations()
      .into_iter()
      .chain(panel_tab::command_registrations())
      .chain(database::command_registrations())
      .chain(application::command_registrations()),
  )
}

/// Scores a command registration against a normalized search query.
///
/// Exact command identifiers receive the highest score, followed by title,
/// identifier-prefix, title-prefix, and category matches. The ordering keeps
/// command IDs predictable while still allowing users to search by the label
/// shown in the command panel.
///
/// # Parameters
///
/// * `registration` is the command and search metadata being matched.
/// * `query` is a trimmed, lowercase search query.
///
/// # Returns
///
/// A score when the descriptor matches, or `None` when it should be omitted.
fn score_registration(registration: &CommandRegistration, query: &str) -> Option<i32> {
  if query.is_empty() {
    return Some(1);
  }
  let descriptor = &registration.descriptor;
  let id = normalize(descriptor.id.as_str());
  let title = normalize(descriptor.title);
  let category = normalize(registration.category.label());
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
  if registration.keywords.iter().any(|keyword| normalize(keyword) == query) {
    return Some(420);
  }
  if registration
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
  use std::path::{Path, PathBuf};

  use chitin_databases::providers::rcsb::{PdbId, StructureFormat};

  use super::*;

  #[test]
  fn command_ids_should_include_scope() {
    assert_eq!(
      ChitinCommand::from(WorkspaceCommand::FocusNext).id(),
      CommandId::WorkspaceFocusNext
    );
  }

  #[test]
  fn parameterized_command_id_should_not_create_an_incomplete_command() {
    assert_eq!(
      CommandId::DatabaseDownloadRcsbStructure.command_without_arguments(),
      None
    );
  }

  #[test]
  fn rcsb_download_command_should_retain_validated_arguments() -> Result<(), Box<dyn std::error::Error>> {
    let id = PdbId::new("4hhb")?;
    let command = ChitinCommand::from(DatabaseCommand::DownloadRcsbStructure(RcsbDownloadArguments {
      ids: vec![id.clone()],
      format: StructureFormat::Mmcif,
      output: Some(PathBuf::from("structures")),
    }));

    assert!(matches!(
      command,
      ChitinCommand::Database(DatabaseCommand::DownloadRcsbStructure(RcsbDownloadArguments {
        ids,
        format: StructureFormat::Mmcif,
        output: Some(output),
      })) if ids == vec![id] && output == Path::new("structures")
    ));
    Ok(())
  }
}
