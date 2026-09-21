//! Built-in shell parsing around the shared portable command fragments.

use std::ffi::OsString;

use chitin_command::{ChitinCommand, CommandId};
use clap::{ColorChoice, Parser, error::ErrorKind};

use crate::{PortableCommandArgs, PortableCommandLineError};

/// Result of parsing one built-in shell line.
#[derive(Debug)]
pub enum BuiltinCommandLine {
  /// A typed command ready for execution routing.
  Command(ChitinCommand),
  /// Help or version text that should be displayed without execution.
  Display(String),
}

/// Failure while tokenizing or parsing built-in shell text.
#[derive(Debug, thiserror::Error)]
pub enum CommandLineParseError {
  /// No command was present after whitespace was removed.
  #[error("command line is empty")]
  Empty,
  /// A quoted token reached the end of the input without a closing quote.
  #[error("unterminated {quote} quote starting at byte {position}")]
  UnterminatedQuote {
    /// Quote character that was not closed.
    quote: char,
    /// Byte offset of the opening quote.
    position: usize,
  },
  /// A trailing escape character had no following character.
  #[error("trailing escape character at byte {position}")]
  TrailingEscape {
    /// Byte offset of the escape character.
    position: usize,
  },
  /// Clap rejected the portable command grammar.
  #[error("{0}")]
  Grammar(String),
  /// A shared command-line value failed domain validation.
  #[error(transparent)]
  Portable(#[from] PortableCommandLineError),
  /// A stable frontend command identifier was not registered.
  #[error("unknown command '{0}'")]
  UnknownCommand(String),
  /// A frontend command without arguments received trailing values.
  #[error("unexpected argument '{value}' for '{command}'")]
  UnexpectedArgument {
    /// Stable command identifier receiving the extra value.
    command: &'static str,
    /// Unexpected argument value.
    value: String,
  },
}

/// Parses one built-in shell line using shared portable command grammar.
///
/// # Parameters
///
/// * `input` is one complete line without a trailing newline.
///
/// # Returns
///
/// A typed command or display-only help text.
///
/// # Errors
///
/// Returns [`CommandLineParseError`] when tokenization, Clap validation, or
/// domain conversion fails.
pub fn parse_builtin_command_line(input: &str) -> Result<BuiltinCommandLine, CommandLineParseError> {
  let tokens = tokenize(input)?;
  let Some(command) = tokens.first().map(String::as_str) else {
    return Err(CommandLineParseError::Empty);
  };

  if matches!(command, "db" | "databases" | "structure" | "help") || command.starts_with('-') {
    return parse_portable(tokens);
  }
  parse_frontend_identifier(command, &tokens[1..]).map(BuiltinCommandLine::Command)
}

#[derive(Debug, Parser)]
#[command(
  name = "chitin",
  about = "Chitin structural biology tools",
  color = ColorChoice::Never,
  arg_required_else_help = true
)]
struct BuiltinPortableRoot {
  #[command(subcommand)]
  command: PortableCommandArgs,
}

/// Parses tokenized portable syntax through Clap without terminating the host.
fn parse_portable(tokens: Vec<String>) -> Result<BuiltinCommandLine, CommandLineParseError> {
  let arguments = std::iter::once(OsString::from("chitin")).chain(tokens.into_iter().map(OsString::from));
  match BuiltinPortableRoot::try_parse_from(arguments) {
    Ok(parsed) => parsed
      .command
      .into_command()
      .map(ChitinCommand::from)
      .map(BuiltinCommandLine::Command)
      .map_err(CommandLineParseError::from),
    Err(error)
      if matches!(
        error.kind(),
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
      ) =>
    {
      Ok(BuiltinCommandLine::Display(error.to_string()))
    }
    Err(error) => Err(CommandLineParseError::Grammar(normalize_clap_error(error.to_string()))),
  }
}

/// Resolves a stable identifier for a frontend command without arguments.
fn parse_frontend_identifier(command: &str, trailing: &[String]) -> Result<ChitinCommand, CommandLineParseError> {
  let id = match command {
    "workspace.focus_previous_entry" => CommandId::WorkspaceFocusPrevious,
    "workspace.focus_next_entry" => CommandId::WorkspaceFocusNext,
    "workspace.activate_focused_entry" => CommandId::WorkspaceActivateFocused,
    "workspace.focus_first_entry" => CommandId::WorkspaceFocusFirst,
    "workspace.focus_last_entry" => CommandId::WorkspaceFocusLast,
    "workspace.toggle_workspace" => CommandId::WorkspaceToggle,
    "tab.focus_previous" => CommandId::PanelTabFocusPrevious,
    "tab.focus_next" => CommandId::PanelTabFocusNext,
    "tab.close" => CommandId::PanelTabClose,
    "application.toggle_command_panel" => CommandId::ApplicationToggleCommandPanel,
    "application.toggle_terminal" => CommandId::ApplicationToggleTerminal,
    _ => return Err(CommandLineParseError::UnknownCommand(command.to_owned())),
  };
  if let Some(value) = trailing.first() {
    return Err(CommandLineParseError::UnexpectedArgument {
      command: id.as_str(),
      value: value.clone(),
    });
  }
  id.frontend_command_without_arguments()
    .map(ChitinCommand::from)
    .ok_or_else(|| CommandLineParseError::UnknownCommand(command.to_owned()))
}

/// Splits command text while preserving quoted whitespace and escapes.
fn tokenize(input: &str) -> Result<Vec<String>, CommandLineParseError> {
  let mut tokens = Vec::new();
  let mut token = String::new();
  let mut token_started = false;
  let mut quote = None;
  let mut characters = input.char_indices().peekable();

  while let Some((position, character)) = characters.next() {
    match quote {
      Some((active, _)) if character == active => quote = None,
      Some(('\'', _)) => {
        token.push(character);
        token_started = true;
      }
      Some(_) | None if character == '\\' => {
        let Some((_, escaped)) = characters.next() else {
          return Err(CommandLineParseError::TrailingEscape { position });
        };
        token.push(escaped);
        token_started = true;
      }
      None if character == '\'' || character == '"' => {
        quote = Some((character, position));
        token_started = true;
      }
      None if character.is_whitespace() => {
        if token_started {
          tokens.push(std::mem::take(&mut token));
          token_started = false;
        }
      }
      Some(_) | None => {
        token.push(character);
        token_started = true;
      }
    }
  }

  if let Some((quote, position)) = quote {
    return Err(CommandLineParseError::UnterminatedQuote { quote, position });
  }
  if token_started {
    tokens.push(token);
  }
  Ok(tokens)
}

/// Removes Clap's own prefix because terminal presenters supply one.
fn normalize_clap_error(message: String) -> String {
  message
    .trim()
    .strip_prefix("error: ")
    .unwrap_or(message.trim())
    .to_owned()
}

#[cfg(test)]
mod tests {
  use std::path::Path;

  use chitin_command::{DatabaseCommand, PortableCommand, RcsbDownloadArguments};
  use chitin_databases::providers::rcsb::StructureFormat;

  use super::*;

  #[test]
  fn database_download_should_use_cli_defaults_and_short_options() -> Result<(), CommandLineParseError> {
    let parsed = parse_builtin_command_line("db rcsb download --id 4hhb -o 'saved structure'")?;

    assert!(matches!(
      parsed,
      BuiltinCommandLine::Command(ChitinCommand::Portable(PortableCommand::Database(
        DatabaseCommand::DownloadRcsbStructure(RcsbDownloadArguments {
          format: StructureFormat::Pdb,
          output: Some(output),
          ..
        })
      ))) if output == Path::new("saved structure")
    ));
    Ok(())
  }

  #[test]
  fn missing_database_subcommand_should_return_clap_help() -> Result<(), CommandLineParseError> {
    let parsed = parse_builtin_command_line("db")?;

    assert!(matches!(parsed, BuiltinCommandLine::Display(help) if help.contains("Usage: chitin db <COMMAND>")));
    Ok(())
  }

  #[test]
  fn stable_frontend_identifier_should_still_parse() -> Result<(), CommandLineParseError> {
    let parsed = parse_builtin_command_line("tab.close")?;

    assert!(matches!(parsed, BuiltinCommandLine::Command(command) if command.id() == CommandId::PanelTabClose));
    Ok(())
  }
}
