//! Typed command identities, arguments, registrations, and search behavior.

mod application;
mod catalog;
mod database;
mod panel_tab;
mod structure;
mod workspace;

pub use application::ApplicationCommand;
pub use catalog::*;
pub use database::{DatabaseCommand, RcsbDownloadArguments};
pub use panel_tab::PanelTabCommand;
pub use structure::{
  CommandOutputFormat, StructureCommand, StructureInputArguments, StructureInspectArguments, StructureValidateArguments,
};
pub use workspace::WorkspaceCommand;

/// Commands executable without application or window state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PortableCommand {
  /// Database-provider commands.
  Database(DatabaseCommand),
  /// Structure parsing and inspection commands.
  Structure(StructureCommand),
}

impl PortableCommand {
  /// Returns the stable identity for this command.
  pub fn id(&self) -> CommandId {
    match self {
      Self::Database(command) => command.id(),
      Self::Structure(command) => command.id(),
    }
  }
}

/// Commands that require state owned by a graphical frontend.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FrontendCommand {
  /// Workspace and document-panel commands.
  Workspace(WorkspaceCommand),
  /// Application-shell commands.
  Application(ApplicationCommand),
}

impl FrontendCommand {
  /// Returns the stable identity for this command.
  pub fn id(&self) -> CommandId {
    match self {
      Self::Workspace(command) => command.id(),
      Self::Application(command) => command.id(),
    }
  }
}

/// Top-level command hierarchy shared by desktop, terminal, and CLI inputs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChitinCommand {
  /// A command executable by the frontend-independent runtime.
  Portable(PortableCommand),
  /// A command requiring state owned by its host frontend.
  Frontend(FrontendCommand),
}

impl ChitinCommand {
  /// Returns the stable identity for this command.
  pub fn id(&self) -> CommandId {
    match self {
      Self::Portable(command) => command.id(),
      Self::Frontend(command) => command.id(),
    }
  }

  /// Returns the execution boundary required by this command.
  pub fn execution_domain(&self) -> CommandExecutionDomain {
    self.id().spec().execution_domain
  }
}

impl From<WorkspaceCommand> for FrontendCommand {
  fn from(command: WorkspaceCommand) -> Self {
    Self::Workspace(command)
  }
}

impl From<DatabaseCommand> for PortableCommand {
  fn from(command: DatabaseCommand) -> Self {
    Self::Database(command)
  }
}

impl From<ApplicationCommand> for FrontendCommand {
  fn from(command: ApplicationCommand) -> Self {
    Self::Application(command)
  }
}

impl From<StructureCommand> for PortableCommand {
  fn from(command: StructureCommand) -> Self {
    Self::Structure(command)
  }
}

impl From<PanelTabCommand> for FrontendCommand {
  fn from(command: PanelTabCommand) -> Self {
    Self::Workspace(WorkspaceCommand::PanelTab(command))
  }
}

impl From<PortableCommand> for ChitinCommand {
  fn from(command: PortableCommand) -> Self {
    Self::Portable(command)
  }
}

impl From<FrontendCommand> for ChitinCommand {
  fn from(command: FrontendCommand) -> Self {
    Self::Frontend(command)
  }
}

impl From<WorkspaceCommand> for ChitinCommand {
  fn from(command: WorkspaceCommand) -> Self {
    FrontendCommand::from(command).into()
  }
}

impl From<DatabaseCommand> for ChitinCommand {
  fn from(command: DatabaseCommand) -> Self {
    PortableCommand::from(command).into()
  }
}

impl From<ApplicationCommand> for ChitinCommand {
  fn from(command: ApplicationCommand) -> Self {
    FrontendCommand::from(command).into()
  }
}

impl From<StructureCommand> for ChitinCommand {
  fn from(command: StructureCommand) -> Self {
    PortableCommand::from(command).into()
  }
}

impl From<PanelTabCommand> for ChitinCommand {
  fn from(command: PanelTabCommand) -> Self {
    FrontendCommand::from(command).into()
  }
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
  fn database_commands_should_use_the_portable_execution_domain() -> Result<(), Box<dyn std::error::Error>> {
    let command = ChitinCommand::from(DatabaseCommand::DownloadRcsbStructure(RcsbDownloadArguments {
      ids: vec![PdbId::new("4hhb")?],
      format: StructureFormat::Pdb,
      output: None,
    }));

    assert_eq!(command.execution_domain(), CommandExecutionDomain::Portable);
    Ok(())
  }

  #[test]
  fn workspace_commands_should_use_the_frontend_execution_domain() {
    let command = ChitinCommand::from(WorkspaceCommand::ToggleWorkspace);

    assert_eq!(command.execution_domain(), CommandExecutionDomain::Frontend);
  }

  #[test]
  fn parameterized_command_id_should_not_create_an_incomplete_command() {
    assert_eq!(
      CommandId::DatabaseDownloadRcsbStructure.frontend_command_without_arguments(),
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
      ChitinCommand::Portable(PortableCommand::Database(DatabaseCommand::DownloadRcsbStructure(
        RcsbDownloadArguments {
        ids,
        format: StructureFormat::Mmcif,
        output: Some(output),
      }))) if ids == vec![id] && output == Path::new("structures")
    ));
    Ok(())
  }
}
