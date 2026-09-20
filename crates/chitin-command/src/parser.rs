//! Frontend-independent parsing for Chitin's built-in command language.

use std::path::PathBuf;

use chitin_databases::providers::rcsb::{PdbId, PdbIdListError, StructureFormat};

use crate::{
  ChitinCommand, CommandId, CommandOutputFormat, DatabaseCommand, RcsbDownloadArguments, StructureCommand,
  StructureInputArguments, StructureInspectArguments, StructureValidateArguments,
};

/// Failure produced while tokenizing or parsing a built-in command line.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CommandParseError {
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
  /// The top-level command name is not registered.
  #[error("unknown command '{command}'")]
  UnknownCommand {
    /// Unrecognized command name.
    command: String,
  },
  /// A nested command path ended before a required word.
  #[error("'{parent}' requires a {expected} subcommand")]
  MissingSubcommand {
    /// Command path parsed before the missing word.
    parent: String,
    /// Human-readable set of accepted words.
    expected: &'static str,
  },
  /// A nested command word was not recognized.
  #[error("unknown subcommand '{subcommand}' for '{parent}'; expected {expected}")]
  UnknownSubcommand {
    /// Parent command path.
    parent: String,
    /// Unrecognized nested command word.
    subcommand: String,
    /// Human-readable set of accepted words.
    expected: &'static str,
  },
  /// A required positional argument or option was absent.
  #[error("missing required argument {argument}")]
  MissingArgument {
    /// Argument spelling shown to the user.
    argument: &'static str,
  },
  /// An option that requires a value was the final token.
  #[error("option '{option}' requires a value")]
  MissingOptionValue {
    /// Option spelling that lacked a value.
    option: String,
  },
  /// The command line contains an option that its command does not accept.
  #[error("unknown option '{option}' for '{command}'")]
  UnknownOption {
    /// Command path receiving the option.
    command: &'static str,
    /// Unrecognized option spelling.
    option: String,
  },
  /// A single-valued option occurred more than once.
  #[error("option '{option}' may only be specified once")]
  DuplicateOption {
    /// Repeated option spelling.
    option: String,
  },
  /// A value is outside the accepted vocabulary for its option.
  #[error("invalid value '{value}' for '{option}'; expected {expected}")]
  InvalidValue {
    /// Option whose value failed validation.
    option: &'static str,
    /// Original invalid value.
    value: String,
    /// Human-readable set of accepted values.
    expected: &'static str,
  },
  /// A command received an extra positional argument.
  #[error("unexpected argument '{value}' for '{command}'")]
  UnexpectedArgument {
    /// Command path receiving the extra value.
    command: &'static str,
    /// Unexpected value.
    value: String,
  },
  /// A comma-separated PDB identifier list contains an invalid item.
  #[error(transparent)]
  InvalidPdbIds(#[from] PdbIdListError),
}

/// Parses built-in shell text into a complete typed command.
///
/// The grammar deliberately mirrors the public CLI for portable workflows.
/// Commands without arguments may also use their stable dotted identifier,
/// such as `workspace.toggle_workspace`.
///
/// # Parameters
///
/// * `input` is one complete command line without a trailing newline.
///
/// # Returns
///
/// A frontend-independent [`ChitinCommand`] containing all validated arguments.
///
/// # Errors
///
/// Returns [`CommandParseError`] when quoting is incomplete, the command path
/// is unknown, an option is malformed, or a domain value fails validation.
///
/// # Examples
///
/// ```
/// use chitin_command::{ChitinCommand, DatabaseCommand, parse_command_line};
///
/// let command = parse_command_line("db rcsb download --id 4hhb --format pdb")?;
/// assert!(matches!(
///   command,
///   ChitinCommand::Database(DatabaseCommand::DownloadRcsbStructure(_))
/// ));
/// # Ok::<(), chitin_command::CommandParseError>(())
/// ```
pub fn parse_command_line(input: &str) -> Result<ChitinCommand, CommandParseError> {
  let tokens = tokenize(input)?;
  let Some(command) = tokens.first().map(String::as_str) else {
    return Err(CommandParseError::Empty);
  };

  match command {
    "db" => parse_database(&tokens[1..]),
    "structure" => parse_structure(&tokens[1..]),
    stable_id => parse_argumentless_command(stable_id, &tokens[1..]),
  }
}

/// Splits command text while preserving quoted whitespace and escaped characters.
///
/// # Parameters
///
/// * `input` is the original command line and may contain single quotes,
///   double quotes, or backslash escapes.
///
/// # Returns
///
/// Tokens with syntax characters removed while retaining empty quoted values.
fn tokenize(input: &str) -> Result<Vec<String>, CommandParseError> {
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
          return Err(CommandParseError::TrailingEscape { position });
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
    return Err(CommandParseError::UnterminatedQuote { quote, position });
  }
  if token_started {
    tokens.push(token);
  }
  Ok(tokens)
}

/// Parses the `db rcsb download` command and its provider arguments.
///
/// # Parameters
///
/// * `tokens` starts after the top-level `db` word.
///
/// # Returns
///
/// A download command containing canonical identifiers and resolved options.
fn parse_database(tokens: &[String]) -> Result<ChitinCommand, CommandParseError> {
  expect_subcommand(tokens.first(), "db", "rcsb", "rcsb")?;
  expect_subcommand(tokens.get(1), "db rcsb", "download", "download")?;

  let mut ids = None;
  let mut format = None;
  let mut output = None;
  let mut cursor = 2;
  while cursor < tokens.len() {
    let token = &tokens[cursor];
    let (option, inline_value) = parse_option(token).ok_or_else(|| CommandParseError::UnexpectedArgument {
      command: "db rcsb download",
      value: token.clone(),
    })?;
    match option {
      "--id" => {
        reject_duplicate(&ids, option)?;
        ids = Some(option_value(tokens, &mut cursor, option, inline_value)?.to_owned());
      }
      "--format" => {
        reject_duplicate(&format, option)?;
        let value = option_value(tokens, &mut cursor, option, inline_value)?;
        format = Some(parse_structure_format(value)?);
      }
      "--output" => {
        reject_duplicate(&output, option)?;
        output = Some(PathBuf::from(option_value(tokens, &mut cursor, option, inline_value)?));
      }
      _ => {
        return Err(CommandParseError::UnknownOption {
          command: "db rcsb download",
          option: option.to_owned(),
        });
      }
    }
    cursor += 1;
  }

  let ids = ids.ok_or(CommandParseError::MissingArgument { argument: "--id" })?;
  let format = format.ok_or(CommandParseError::MissingArgument { argument: "--format" })?;
  Ok(
    DatabaseCommand::DownloadRcsbStructure(RcsbDownloadArguments {
      ids: PdbId::parse_many(&ids)?,
      format,
      output,
    })
    .into(),
  )
}

/// Parses structure inspection and validation commands.
///
/// # Parameters
///
/// * `tokens` starts with `inspect` or `validate` and contains its arguments.
///
/// # Returns
///
/// A complete structure command with its input and output preferences.
fn parse_structure(tokens: &[String]) -> Result<ChitinCommand, CommandParseError> {
  let Some(subcommand) = tokens.first().map(String::as_str) else {
    return Err(CommandParseError::MissingSubcommand {
      parent: "structure".to_owned(),
      expected: "inspect or validate",
    });
  };
  if subcommand != "inspect" && subcommand != "validate" {
    return Err(CommandParseError::UnknownSubcommand {
      parent: "structure".to_owned(),
      subcommand: subcommand.to_owned(),
      expected: "inspect or validate",
    });
  }

  let command_name = if subcommand == "inspect" {
    "structure inspect"
  } else {
    "structure validate"
  };
  let mut input = None;
  let mut format = None;
  let mut output = None;
  let mut verbose = false;
  let mut cursor = 1;
  while cursor < tokens.len() {
    let token = &tokens[cursor];
    if let Some((option, inline_value)) = parse_option(token) {
      match option {
        "--format" => {
          reject_duplicate(&format, option)?;
          let value = option_value(tokens, &mut cursor, option, inline_value)?;
          format = Some(parse_structure_format(value)?);
        }
        "--output" => {
          reject_duplicate(&output, option)?;
          let value = option_value(tokens, &mut cursor, option, inline_value)?;
          output = Some(parse_output_format(value)?);
        }
        "--verbose" if subcommand == "inspect" && inline_value.is_none() => {
          if verbose {
            return Err(CommandParseError::DuplicateOption {
              option: option.to_owned(),
            });
          }
          verbose = true;
        }
        _ => {
          return Err(CommandParseError::UnknownOption {
            command: command_name,
            option: option.to_owned(),
          });
        }
      }
    } else if input.is_some() {
      return Err(CommandParseError::UnexpectedArgument {
        command: command_name,
        value: token.clone(),
      });
    } else {
      input = Some(PathBuf::from(token));
    }
    cursor += 1;
  }

  let input = StructureInputArguments {
    input: input.ok_or(CommandParseError::MissingArgument { argument: "<input>" })?,
    format,
  };
  let output = output.unwrap_or_default();
  match subcommand {
    "inspect" => Ok(StructureCommand::Inspect(StructureInspectArguments { input, output, verbose }).into()),
    "validate" => Ok(StructureCommand::Validate(StructureValidateArguments { input, output }).into()),
    _ => Err(CommandParseError::UnknownSubcommand {
      parent: "structure".to_owned(),
      subcommand: subcommand.to_owned(),
      expected: "inspect or validate",
    }),
  }
}

/// Resolves a stable dotted identifier for a command without arguments.
///
/// # Parameters
///
/// * `stable_id` is the canonical command identifier.
/// * `trailing` contains tokens that must be absent for argumentless commands.
///
/// # Returns
///
/// The executable command represented by the stable identifier.
fn parse_argumentless_command(stable_id: &str, trailing: &[String]) -> Result<ChitinCommand, CommandParseError> {
  let id = match stable_id {
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
    _ => {
      return Err(CommandParseError::UnknownCommand {
        command: stable_id.to_owned(),
      });
    }
  };
  if let Some(value) = trailing.first() {
    return Err(CommandParseError::UnexpectedArgument {
      command: id.as_str(),
      value: value.clone(),
    });
  }
  id.command_without_arguments()
    .ok_or_else(|| CommandParseError::UnknownCommand {
      command: stable_id.to_owned(),
    })
}

/// Requires one exact nested command word at the requested position.
fn expect_subcommand(
  token: Option<&String>,
  parent: &str,
  required: &str,
  expected: &'static str,
) -> Result<(), CommandParseError> {
  match token {
    Some(token) if token == required => Ok(()),
    Some(token) => Err(CommandParseError::UnknownSubcommand {
      parent: parent.to_owned(),
      subcommand: token.clone(),
      expected,
    }),
    None => Err(CommandParseError::MissingSubcommand {
      parent: parent.to_owned(),
      expected,
    }),
  }
}

/// Splits a long option into its name and optional inline value.
fn parse_option(token: &str) -> Option<(&str, Option<&str>)> {
  token.strip_prefix("--").map(|option| {
    let (name, value) = option
      .split_once('=')
      .map_or((option, None), |(name, value)| (name, Some(value)));
    (&token[..name.len() + 2], value)
  })
}

/// Returns an option value from `--name=value` or the following token.
fn option_value<'a>(
  tokens: &'a [String],
  cursor: &mut usize,
  option: &str,
  inline_value: Option<&'a str>,
) -> Result<&'a str, CommandParseError> {
  if let Some(value) = inline_value {
    return if value.is_empty() {
      Err(CommandParseError::MissingOptionValue {
        option: option.to_owned(),
      })
    } else {
      Ok(value)
    };
  }
  *cursor += 1;
  tokens
    .get(*cursor)
    .filter(|value| !value.starts_with("--"))
    .map(String::as_str)
    .ok_or_else(|| CommandParseError::MissingOptionValue {
      option: option.to_owned(),
    })
}

/// Rejects a second occurrence of a single-valued option.
fn reject_duplicate<T>(value: &Option<T>, option: &str) -> Result<(), CommandParseError> {
  if value.is_some() {
    Err(CommandParseError::DuplicateOption {
      option: option.to_owned(),
    })
  } else {
    Ok(())
  }
}

/// Parses a structure format shared by local and RCSB commands.
fn parse_structure_format(value: &str) -> Result<StructureFormat, CommandParseError> {
  match value.to_ascii_lowercase().as_str() {
    "pdb" => Ok(StructureFormat::Pdb),
    "mmcif" | "cif" => Ok(StructureFormat::Mmcif),
    _ => Err(CommandParseError::InvalidValue {
      option: "--format",
      value: value.to_owned(),
      expected: "pdb or mmcif",
    }),
  }
}

/// Parses the requested command output representation.
fn parse_output_format(value: &str) -> Result<CommandOutputFormat, CommandParseError> {
  match value.to_ascii_lowercase().as_str() {
    "text" => Ok(CommandOutputFormat::Text),
    "json" => Ok(CommandOutputFormat::Json),
    _ => Err(CommandParseError::InvalidValue {
      option: "--output",
      value: value.to_owned(),
      expected: "text or json",
    }),
  }
}

#[cfg(test)]
mod tests {
  use std::path::Path;

  use super::*;

  #[test]
  fn database_download_should_parse_complete_typed_arguments() -> Result<(), CommandParseError> {
    let command = parse_command_line("db rcsb download --id 4hhb,1yth --format mmcif --output 'saved structures'")?;

    assert!(matches!(
      command,
      ChitinCommand::Database(DatabaseCommand::DownloadRcsbStructure(RcsbDownloadArguments {
        ids,
        format: StructureFormat::Mmcif,
        output: Some(output),
      })) if ids.iter().map(PdbId::as_str).collect::<Vec<_>>() == ["4HHB", "1YTH"]
        && output == Path::new("saved structures")
    ));
    Ok(())
  }

  #[test]
  fn structure_inspect_should_parse_options_in_any_order() -> Result<(), CommandParseError> {
    let command = parse_command_line("structure inspect --verbose --output=json 'models/one.cif' --format cif")?;

    assert!(matches!(
      command,
      ChitinCommand::Structure(StructureCommand::Inspect(StructureInspectArguments {
        input: StructureInputArguments { input, format: Some(StructureFormat::Mmcif) },
        output: CommandOutputFormat::Json,
        verbose: true,
      })) if input == Path::new("models/one.cif")
    ));
    Ok(())
  }

  #[test]
  fn stable_identifier_should_create_argumentless_command() -> Result<(), CommandParseError> {
    let command = parse_command_line("tab.close")?;

    assert_eq!(command.id(), CommandId::PanelTabClose);
    Ok(())
  }

  #[test]
  fn invalid_pdb_list_should_preserve_item_details() {
    let result = parse_command_line("db rcsb download --id 4hhb,invalid --format pdb");

    assert!(matches!(
      result,
      Err(CommandParseError::InvalidPdbIds(PdbIdListError {
        index: 2,
        value,
        ..
      })) if value == "invalid"
    ));
  }

  #[test]
  fn unterminated_quote_should_report_opening_byte() {
    let result = parse_command_line("structure inspect 'model.cif");

    assert_eq!(
      result,
      Err(CommandParseError::UnterminatedQuote {
        quote: '\'',
        position: 18,
      })
    );
  }

  #[test]
  fn duplicate_option_should_be_rejected() {
    let result = parse_command_line("structure validate one.pdb --format pdb --format mmcif");

    assert_eq!(
      result,
      Err(CommandParseError::DuplicateOption {
        option: "--format".to_owned(),
      })
    );
  }
}
