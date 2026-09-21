//! Built-in shell grammar composed around shared portable command fragments.

use std::ffi::OsString;

use chitin_command::{
  CommandExecutionDomain, CommandId, CommandSpec, FrontendCommand, PortableCommand, command_spec_by_name,
  command_specs,
  grammar::{PortableCommandArgs, PortableCommandLineError},
};
use clap::{ColorChoice, CommandFactory, Parser, Subcommand, error::ErrorKind};

/// Result of parsing one built-in shell line.
#[derive(Debug)]
pub enum BuiltinCommandLine {
  /// A typed command executable without desktop state.
  Portable(PortableCommand),
  /// A typed command requiring desktop application state.
  Frontend(FrontendCommand),
  /// A command implemented by the shell session itself.
  ShellBuiltin(ShellBuiltin),
  /// Help or version text that should be displayed without execution.
  Display(String),
}

/// Commands whose semantics belong to the built-in shell session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellBuiltin {
  /// Clear visible scrollback without changing command history.
  Clear,
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
  /// Clap rejected the composed built-in shell grammar.
  #[error("{0}")]
  Grammar(String),
  /// A frontend grammar branch has no argument-free command to invoke.
  ///
  /// This is unreachable while every frontend branch stays argument-free; it
  /// exists so the conversion reports a typed failure instead of panicking.
  #[error("command '{command}' has no argument-free frontend form")]
  MissingFrontendCommand {
    /// Stable identity of the frontend command without an invocation form.
    command: CommandId,
  },
  /// A shared command-line value failed domain validation.
  #[error(transparent)]
  Portable(#[from] PortableCommandLineError),
}

/// Complete grammar available inside Chitin's built-in shell.
#[derive(Debug, Parser)]
#[command(
  name = "chitin",
  about = "Chitin structural biology tools",
  color = ColorChoice::Never,
  arg_required_else_help = true
)]
pub struct BuiltinShellGrammar {
  #[command(subcommand)]
  command: BuiltinShellCommandArgs,
}

/// Top-level commands composed specifically for the built-in shell.
#[derive(Debug, Subcommand)]
enum BuiltinShellCommandArgs {
  /// Execute a command shared with non-GUI frontends.
  #[command(flatten)]
  Portable(PortableCommandArgs),
  /// Clear visible terminal scrollback.
  Clear,
}

impl BuiltinShellCommandArgs {
  /// Converts the selected grammar branch into its execution-domain result.
  fn into_line(self) -> Result<BuiltinCommandLine, CommandLineParseError> {
    let line = match self {
      Self::Portable(arguments) => BuiltinCommandLine::Portable(arguments.into_command()?),
      Self::Clear => BuiltinCommandLine::ShellBuiltin(ShellBuiltin::Clear),
    };
    Ok(line)
  }
}

/// Resolves a stable identity into the argument-free frontend command to run.
///
/// # Parameters
///
/// * `id` is the stable identity named by a built-in shell grammar branch.
///
/// # Returns
///
/// The frontend command registered for that identity.
///
/// # Errors
///
/// Returns [`CommandLineParseError::MissingFrontendCommand`] when the identity
/// carries required arguments and therefore cannot be invoked from a grammar
/// branch that accepts none.
fn frontend_command(id: CommandId) -> Result<FrontendCommand, CommandLineParseError> {
  id.frontend_command_without_arguments()
    .ok_or(CommandLineParseError::MissingFrontendCommand { command: id })
}

/// Parses one built-in shell line using the composed shell grammar.
///
/// # Parameters
///
/// * `input` is one complete line without a trailing newline.
///
/// # Returns
///
/// A portable command, frontend command, shell builtin, or display-only help.
///
/// # Errors
///
/// Returns [`CommandLineParseError`] when tokenization, Clap validation, or
/// domain conversion fails.
pub fn parse_builtin_command_line(input: &str) -> Result<BuiltinCommandLine, CommandLineParseError> {
  let tokens = tokenize(input)?;
  if tokens.is_empty() {
    return Err(CommandLineParseError::Empty);
  }
  if let Some(line) = parse_registered_frontend_command(&tokens)? {
    return Ok(line);
  }
  let include_frontend_help = tokens.len() == 1 && matches!(tokens[0].as_str(), "help" | "-h" | "--help");
  parse_grammar(tokens, include_frontend_help)
}

/// Generates full-line completion candidates from the built-in shell grammar.
///
/// # Parameters
///
/// * `input` is the current editable command line up to the caret.
///
/// # Returns
///
/// Complete replacement lines whose current command or option starts with the
/// unfinished token. Invalid or quoted prefixes return no candidates.
pub fn complete_builtin_shell_line(input: &str) -> Vec<String> {
  let (head, prefix) = completion_prefix(input);
  let Ok(tokens) = tokenize(head) else {
    return Vec::new();
  };
  let mut root = BuiltinShellGrammar::command();
  root.build();
  let mut command = &root;
  for token in &tokens {
    if token.starts_with('-') {
      continue;
    }
    if let Some(subcommand) = command.get_subcommands().find(|subcommand| {
      subcommand.get_name() == token || subcommand.get_all_aliases().any(|alias| alias == token.as_str())
    }) {
      command = subcommand;
    }
  }

  if tokens.len() == 1 && command_spec_by_name(&tokens[0]).is_some() {
    return Vec::new();
  }

  let mut candidates = command
    .get_subcommands()
    .map(|subcommand| subcommand.get_name().to_owned())
    .chain(
      command
        .get_arguments()
        .filter_map(|argument| argument.get_long().map(|long| format!("--{long}"))),
    )
    .chain(
      command
        .get_arguments()
        .filter_map(|argument| argument.get_short().map(|short| format!("-{short}"))),
    )
    .chain(
      tokens
        .is_empty()
        .then(frontend_specs)
        .into_iter()
        .flatten()
        .map(|spec| spec.name.to_owned()),
    )
    .filter(|candidate| candidate.starts_with(prefix))
    .map(|candidate| format!("{head}{candidate}"))
    .collect::<Vec<_>>();
  candidates.sort();
  candidates.dedup();
  candidates
}

/// Separates completed input from the token currently being completed.
fn completion_prefix(input: &str) -> (&str, &str) {
  input
    .char_indices()
    .rfind(|(_, character)| character.is_whitespace())
    .map_or(("", input), |(index, character)| {
      let next = index + character.len_utf8();
      (&input[..next], &input[next..])
    })
}

/// Parses tokenized syntax through Clap without terminating the host process.
fn parse_grammar(
  tokens: Vec<String>,
  include_frontend_help: bool,
) -> Result<BuiltinCommandLine, CommandLineParseError> {
  let arguments = std::iter::once(OsString::from("chitin")).chain(tokens.into_iter().map(OsString::from));
  match BuiltinShellGrammar::try_parse_from(arguments) {
    Ok(parsed) => parsed.command.into_line(),
    Err(error)
      if matches!(
        error.kind(),
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
      ) =>
    {
      let mut help = error.to_string();
      if include_frontend_help {
        append_frontend_help(&mut help);
      }
      Ok(BuiltinCommandLine::Display(help))
    }
    Err(error) => Err(CommandLineParseError::Grammar(normalize_clap_error(error.to_string()))),
  }
}

/// Parses registry-driven frontend names before delegating portable syntax to Clap.
///
/// # Parameters
///
/// * `tokens` is the tokenized built-in shell input.
///
/// # Returns
///
/// A frontend command or its help text when the first token is a registered
/// frontend name; otherwise `None` so portable and shell grammar can continue.
///
/// # Errors
///
/// Returns [`CommandLineParseError`] when a frontend command receives arguments
/// or its catalog entry cannot construct its argument-free command value.
fn parse_registered_frontend_command(tokens: &[String]) -> Result<Option<BuiltinCommandLine>, CommandLineParseError> {
  let (name, help_requested) = match tokens {
    [help, name] if help == "help" => (name.as_str(), true),
    [name, flag] if flag == "-h" || flag == "--help" => (name.as_str(), true),
    [name, ..] => (name.as_str(), false),
    [] => return Ok(None),
  };
  let Some(spec) = command_spec_by_name(name) else {
    return Ok(None);
  };
  if spec.execution_domain != CommandExecutionDomain::Frontend {
    return Ok(None);
  }
  if help_requested {
    return Ok(Some(BuiltinCommandLine::Display(frontend_help(spec))));
  }
  if tokens.len() != 1 {
    return Err(CommandLineParseError::Grammar(format!(
      "unexpected argument '{}'\n\nUsage: chitin {}",
      tokens[1], spec.name
    )));
  }
  Ok(Some(BuiltinCommandLine::Frontend(frontend_command(spec.id)?)))
}

/// Iterates over argument-free frontend specifications exposed by the shell.
fn frontend_specs() -> impl Iterator<Item = &'static CommandSpec> {
  command_specs().filter(|spec| spec.execution_domain == CommandExecutionDomain::Frontend && !spec.requires_arguments)
}

/// Formats help for one registry-driven frontend command.
fn frontend_help(spec: &CommandSpec) -> String {
  format!("{}\n\nUsage: chitin {}\n", spec.title, spec.name)
}

/// Appends registry-driven frontend commands to Clap's root help.
fn append_frontend_help(help: &mut String) {
  help.push_str("\nFrontend commands:\n");
  for spec in frontend_specs() {
    help.push_str(&format!("  {:<42} {}\n", spec.name, spec.title));
  }
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

  use chitin_command::{CommandId, DatabaseCommand, RcsbDownloadArguments};
  use chitin_databases::providers::rcsb::StructureFormat;

  use super::*;

  #[test]
  fn database_download_should_use_cli_defaults_and_short_options() -> Result<(), CommandLineParseError> {
    let parsed = parse_builtin_command_line("db rcsb download --id 4hhb -o 'saved structure'")?;

    assert!(matches!(
      parsed,
      BuiltinCommandLine::Portable(PortableCommand::Database(DatabaseCommand::DownloadRcsbStructure(
        RcsbDownloadArguments {
          format: StructureFormat::Pdb,
          output: Some(output),
          ..
        }
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

    assert!(matches!(parsed, BuiltinCommandLine::Frontend(command) if command.id() == CommandId::PanelTabClose));
    Ok(())
  }

  #[test]
  fn clear_should_parse_as_a_shell_builtin() -> Result<(), CommandLineParseError> {
    let parsed = parse_builtin_command_line("clear")?;

    assert!(matches!(parsed, BuiltinCommandLine::ShellBuiltin(ShellBuiltin::Clear)));
    Ok(())
  }

  #[test]
  fn root_help_should_include_all_execution_domains() -> Result<(), CommandLineParseError> {
    let parsed = parse_builtin_command_line("help")?;

    assert!(matches!(parsed, BuiltinCommandLine::Display(help)
      if help.contains("db") && help.contains("tab.close") && help.contains("clear")));
    Ok(())
  }

  #[test]
  fn completion_should_follow_the_composed_shell_grammar() {
    assert_eq!(complete_builtin_shell_line("db r"), vec!["db rcsb"]);
    assert!(complete_builtin_shell_line("tab.").contains(&"tab.close".to_owned()));
    assert!(complete_builtin_shell_line("cl").contains(&"clear".to_owned()));
  }

  #[test]
  fn completion_should_list_every_option_of_a_nested_subcommand() {
    // Clap contributes its own `--help` flag, so it is listed alongside the
    // options declared by the shared portable fragment.
    assert_eq!(
      complete_builtin_shell_line("db rcsb download --"),
      vec![
        "db rcsb download --format",
        "db rcsb download --help",
        "db rcsb download --id",
        "db rcsb download --output",
      ]
    );
  }

  #[test]
  fn subcommand_help_should_render_from_the_builtin_grammar() -> Result<(), CommandLineParseError> {
    let parsed = parse_builtin_command_line("help structure")?;

    assert!(matches!(parsed, BuiltinCommandLine::Display(help)
      if help.contains("inspect") && help.contains("validate")));
    Ok(())
  }

  #[test]
  fn nested_subcommand_help_should_render_from_the_builtin_grammar() -> Result<(), CommandLineParseError> {
    let parsed = parse_builtin_command_line("help db rcsb download")?;

    assert!(matches!(parsed, BuiltinCommandLine::Display(help)
      if help.contains("--id") && help.contains("--format") && help.contains("--output")));
    Ok(())
  }

  #[test]
  fn frontend_catalog_names_should_round_trip_through_the_shell() -> Result<(), CommandLineParseError> {
    for spec in frontend_specs() {
      let parsed = parse_builtin_command_line(spec.name)?;
      assert!(matches!(parsed, BuiltinCommandLine::Frontend(command) if command.id() == spec.id));
    }
    Ok(())
  }

  #[test]
  fn frontend_help_should_come_from_the_canonical_spec() -> Result<(), CommandLineParseError> {
    let parsed = parse_builtin_command_line("help tab.close")?;

    assert!(matches!(parsed, BuiltinCommandLine::Display(help)
      if help.contains("Close Active Tab") && help.contains("Usage: chitin tab.close")));
    Ok(())
  }
}
