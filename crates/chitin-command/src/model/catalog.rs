//! Canonical command metadata and command-panel search.

use super::{ApplicationCommand, FrontendCommand, PanelTabCommand, WorkspaceCommand};

/// Execution boundary required by a typed command.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CommandExecutionDomain {
  /// The command can execute without application or window state.
  Portable,
  /// The command requires state owned by its graphical frontend.
  Frontend,
}

/// Command category used for discovery, search grouping, and presentation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandCategory {
  /// Workspace and document-panel commands.
  Workspace,
  /// External database commands.
  Database,
  /// Structure parsing and analysis commands.
  Structure,
  /// Application-shell commands.
  Application,
}

impl CommandCategory {
  /// Returns the display label for this category.
  pub const fn label(self) -> &'static str {
    match self {
      Self::Workspace => "Workspace",
      Self::Database => "Database",
      Self::Structure => "Structure",
      Self::Application => "Application",
    }
  }
}

/// Canonical metadata for one typed command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandSpec {
  /// Stable command identity.
  pub id: CommandId,
  /// Stable dotted name accepted by registry-driven textual frontends.
  pub name: &'static str,
  /// User-facing command title.
  pub title: &'static str,
  /// Discovery and presentation category.
  pub category: CommandCategory,
  /// Execution boundary required by the command.
  pub execution_domain: CommandExecutionDomain,
  /// Whether an input adapter must collect arguments before execution.
  pub requires_arguments: bool,
  /// Additional search keywords.
  pub keywords: &'static [&'static str],
  /// Optional shortcut label shown by discovery frontends.
  pub shortcut: Option<&'static str>,
  /// Whether the desktop command panel can currently invoke the command.
  pub visible_in_command_panel: bool,
}

macro_rules! frontend_command {
  () => {
    None
  };
  ($command:expr) => {
    Some(FrontendCommand::from($command))
  };
}

macro_rules! define_commands {
  (
    $(
      $(#[$variant_meta:meta])*
      $variant:ident {
        name: $name:literal,
        title: $title:literal,
        category: $category:expr,
        domain: $domain:expr,
        requires_arguments: $requires_arguments:literal,
        keywords: $keywords:expr,
        shortcut: $shortcut:expr,
        command_panel: $command_panel:literal,
        frontend: [$($frontend:expr)?]
      }
    ),* $(,)?
  ) => {
    /// Stable identity of a command independent of its invocation arguments.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub enum CommandId {
      $(
        $(#[$variant_meta])*
        $variant,
      )*
    }

    impl CommandId {
      /// Every stable command identity, in declaration order.
      pub const ALL: &'static [Self] = &[$(Self::$variant),*];

      /// Returns the stable dotted identifier.
      pub const fn as_str(self) -> &'static str {
        match self {
          $(Self::$variant => $name),*
        }
      }

      /// Returns the canonical metadata for this identity.
      pub const fn spec(self) -> &'static CommandSpec {
        match self {
          $(
            Self::$variant => &CommandSpec {
              id: Self::$variant,
              name: $name,
              title: $title,
              category: $category,
              execution_domain: $domain,
              requires_arguments: $requires_arguments,
              keywords: $keywords,
              shortcut: $shortcut,
              visible_in_command_panel: $command_panel,
            },
          )*
        }
      }

      /// Builds a frontend command when this identity needs no arguments.
      pub fn frontend_command_without_arguments(self) -> Option<FrontendCommand> {
        match self {
          $(Self::$variant => frontend_command!($($frontend)?)),*
        }
      }
    }
  };
}

define_commands! {
  /// Focus the previous project entry.
  WorkspaceFocusPrevious {
    name: "workspace.focus_previous_entry",
    title: "Focus Previous Project Entry",
    category: CommandCategory::Workspace,
    domain: CommandExecutionDomain::Frontend,
    requires_arguments: false,
    keywords: &["tree", "up", "previous"],
    shortcut: Some("Up"),
    command_panel: true,
    frontend: [WorkspaceCommand::FocusPrevious]
  },
  /// Focus the next project entry.
  WorkspaceFocusNext {
    name: "workspace.focus_next_entry",
    title: "Focus Next Project Entry",
    category: CommandCategory::Workspace,
    domain: CommandExecutionDomain::Frontend,
    requires_arguments: false,
    keywords: &["tree", "down", "next"],
    shortcut: Some("Down"),
    command_panel: true,
    frontend: [WorkspaceCommand::FocusNext]
  },
  /// Activate the focused project entry.
  WorkspaceActivateFocused {
    name: "workspace.activate_focused_entry",
    title: "Activate Focused Project Entry",
    category: CommandCategory::Workspace,
    domain: CommandExecutionDomain::Frontend,
    requires_arguments: false,
    keywords: &["open", "tree", "file", "directory"],
    shortcut: Some("Enter"),
    command_panel: true,
    frontend: [WorkspaceCommand::ActivateFocused]
  },
  /// Focus the first project entry.
  WorkspaceFocusFirst {
    name: "workspace.focus_first_entry",
    title: "Focus First Project Entry",
    category: CommandCategory::Workspace,
    domain: CommandExecutionDomain::Frontend,
    requires_arguments: false,
    keywords: &["tree", "home", "first"],
    shortcut: Some("Home"),
    command_panel: true,
    frontend: [WorkspaceCommand::FocusFirst]
  },
  /// Focus the last project entry.
  WorkspaceFocusLast {
    name: "workspace.focus_last_entry",
    title: "Focus Last Project Entry",
    category: CommandCategory::Workspace,
    domain: CommandExecutionDomain::Frontend,
    requires_arguments: false,
    keywords: &["tree", "end", "last"],
    shortcut: Some("End"),
    command_panel: true,
    frontend: [WorkspaceCommand::FocusLast]
  },
  /// Show or hide the workspace sidebar.
  WorkspaceToggle {
    name: "workspace.toggle_workspace",
    title: "Toggle Workspace Sidebar",
    category: CommandCategory::Workspace,
    domain: CommandExecutionDomain::Frontend,
    requires_arguments: false,
    keywords: &["files", "project", "sidebar", "explorer"],
    shortcut: Some("Shift+E"),
    command_panel: true,
    frontend: [WorkspaceCommand::ToggleWorkspace]
  },
  /// Focus the previous document tab.
  PanelTabFocusPrevious {
    name: "tab.focus_previous",
    title: "Focus Previous Tab",
    category: CommandCategory::Workspace,
    domain: CommandExecutionDomain::Frontend,
    requires_arguments: false,
    keywords: &["document", "panel", "tab", "previous"],
    shortcut: Some("Shift+J"),
    command_panel: true,
    frontend: [PanelTabCommand::FocusPrevious]
  },
  /// Focus the next document tab.
  PanelTabFocusNext {
    name: "tab.focus_next",
    title: "Focus Next Tab",
    category: CommandCategory::Workspace,
    domain: CommandExecutionDomain::Frontend,
    requires_arguments: false,
    keywords: &["document", "panel", "tab", "next"],
    shortcut: Some("Shift+K"),
    command_panel: true,
    frontend: [PanelTabCommand::FocusNext]
  },
  /// Close the active document tab.
  PanelTabClose {
    name: "tab.close",
    title: "Close Active Tab",
    category: CommandCategory::Workspace,
    domain: CommandExecutionDomain::Frontend,
    requires_arguments: false,
    keywords: &["document", "panel", "tab", "close"],
    shortcut: Some("Shift+X"),
    command_panel: true,
    frontend: [PanelTabCommand::Close]
  },
  /// Download one or more RCSB structures.
  DatabaseDownloadRcsbStructure {
    name: "database.rcsb.download_structure",
    title: "Download RCSB Structure",
    category: CommandCategory::Database,
    domain: CommandExecutionDomain::Portable,
    requires_arguments: true,
    keywords: &["pdb", "rcsb", "mmcif", "structure"],
    shortcut: None,
    command_panel: true,
    frontend: []
  },
  /// Show or hide the command panel.
  ApplicationToggleCommandPanel {
    name: "application.toggle_command_panel",
    title: "Toggle Command Panel",
    category: CommandCategory::Application,
    domain: CommandExecutionDomain::Frontend,
    requires_arguments: false,
    keywords: &["quick pick", "palette", "commands"],
    shortcut: Some("Ctrl/Cmd+Shift+P"),
    command_panel: true,
    frontend: [ApplicationCommand::ToggleCommandPanel]
  },
  /// Show or hide the built-in terminal panel.
  ApplicationToggleTerminal {
    name: "application.toggle_terminal",
    title: "Toggle Terminal",
    category: CommandCategory::Application,
    domain: CommandExecutionDomain::Frontend,
    requires_arguments: false,
    keywords: &["built-in shell", "console", "command line"],
    shortcut: Some("Shift+T"),
    command_panel: true,
    frontend: [ApplicationCommand::ToggleTerminal]
  },
  /// Inspect a local structure file.
  StructureInspect {
    name: "structure.inspect",
    title: "Inspect Structure",
    category: CommandCategory::Structure,
    domain: CommandExecutionDomain::Portable,
    requires_arguments: true,
    keywords: &["pdb", "mmcif", "summary", "metadata"],
    shortcut: None,
    command_panel: false,
    frontend: []
  },
  /// Validate a local structure file.
  StructureValidate {
    name: "structure.validate",
    title: "Validate Structure",
    category: CommandCategory::Structure,
    domain: CommandExecutionDomain::Portable,
    requires_arguments: true,
    keywords: &["pdb", "mmcif", "check", "invariants"],
    shortcut: None,
    command_panel: false,
    frontend: []
  },
}

impl std::fmt::Display for CommandId {
  /// Formats the stable dotted identifier.
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    formatter.write_str(self.as_str())
  }
}

/// Iterates over every canonical command specification.
pub fn command_specs() -> impl ExactSizeIterator<Item = &'static CommandSpec> {
  CommandId::ALL.iter().map(|id| id.spec())
}

/// Finds canonical command metadata by its stable dotted name.
pub fn command_spec_by_name(name: &str) -> Option<&'static CommandSpec> {
  command_specs().find(|spec| spec.name == name)
}

/// A ranked command result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandSearchResult {
  /// Match score, where larger values rank first.
  pub score: i32,
  /// Matching canonical command specification.
  pub spec: &'static CommandSpec,
}

/// Registry and deterministic search implementation for command specifications.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CommandRegistry {
  commands: Vec<&'static CommandSpec>,
}

impl CommandRegistry {
  /// Creates a registry from canonical command specifications.
  pub fn new(commands: impl IntoIterator<Item = &'static CommandSpec>) -> Self {
    Self {
      commands: commands.into_iter().collect(),
    }
  }

  /// Finds a specification by stable command identity.
  pub fn spec_for(&self, id: CommandId) -> Option<&'static CommandSpec> {
    self.commands.iter().copied().find(|spec| spec.id == id)
  }

  /// Searches commands using deterministic scoring.
  pub fn search(&self, query: &str) -> Vec<CommandSearchResult> {
    let query = normalize(query);
    let mut results = self
      .commands
      .iter()
      .filter_map(|spec| score_spec(spec, &query).map(|score| CommandSearchResult { score, spec }))
      .collect::<Vec<_>>();
    results.sort_by(|left, right| {
      right
        .score
        .cmp(&left.score)
        .then_with(|| left.spec.title.cmp(right.spec.title))
    });
    results
  }
}

/// Creates the registry of commands currently invocable from the desktop panel.
pub fn default_registry() -> CommandRegistry {
  CommandRegistry::new(command_specs().filter(|spec| spec.visible_in_command_panel))
}

/// Scores a command specification against a normalized search query.
///
/// Exact command identifiers receive the highest score, followed by title,
/// identifier-prefix, title-prefix, keyword, and category matches. This keeps
/// stable names predictable while still supporting human-facing discovery.
///
/// # Parameters
///
/// * `spec` is the canonical command metadata being matched.
/// * `query` is a trimmed, lowercase search query.
///
/// # Returns
///
/// A score when the command matches, or `None` when it should be omitted.
fn score_spec(spec: &CommandSpec, query: &str) -> Option<i32> {
  if query.is_empty() {
    return Some(1);
  }
  let name = normalize(spec.name);
  let title = normalize(spec.title);
  let category = normalize(spec.category.label());
  if name == query {
    return Some(1000);
  }
  if name.starts_with(query) {
    return Some(850);
  }
  if title.starts_with(query) {
    return Some(700);
  }
  if title.contains(query) {
    return Some(500);
  }
  if spec.keywords.iter().any(|keyword| normalize(keyword) == query) {
    return Some(420);
  }
  if spec.keywords.iter().any(|keyword| normalize(keyword).contains(query)) {
    return Some(360);
  }
  if category == query || category.starts_with(query) {
    return Some(280);
  }
  None
}

/// Trims surrounding whitespace and lowercases a command-search value.
fn normalize(value: &str) -> String {
  value.trim().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn command_specs_should_cover_every_command_id() {
    assert_eq!(command_specs().count(), CommandId::ALL.len());
  }

  #[test]
  fn stable_names_should_resolve_to_their_original_identity() {
    for id in CommandId::ALL {
      assert_eq!(command_spec_by_name(id.as_str()).map(|spec| spec.id), Some(*id));
    }
  }

  #[test]
  fn default_registry_should_hide_commands_without_panel_argument_ui() {
    assert_eq!(default_registry().spec_for(CommandId::StructureInspect), None);
  }

  #[test]
  fn frontend_specs_should_construct_commands_in_the_frontend_domain() {
    for spec in command_specs().filter(|spec| spec.execution_domain == CommandExecutionDomain::Frontend) {
      let command = spec.id.frontend_command_without_arguments();
      assert_eq!(command.as_ref().map(FrontendCommand::id), Some(spec.id));
    }
  }
}
