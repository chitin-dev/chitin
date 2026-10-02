//! Built-in shell grammar composed around shared portable command fragments.

use std::ffi::OsString;

use chitin_command::{
  CommandExecutionDomain, CommandId, CommandSpec, FrontendCommand, PortableCommand, command_spec_by_name,
  command_specs,
  grammar::{PortableCommandArgs, PortableCommandLineError},
};
use clap::{ColorChoice, CommandFactory, Parser, Subcommand, error::ErrorKind};

use crate::RenderingPanelCommand;

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
  /// Navigate the host's rendering views within this shell session.
  RenderingPanel(RenderingPanelCommand),
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
  /// Clap rejected the composed built-in shell grammar.
  #[error("{0}")]
  Grammar(String),
  /// A frontend grammar branch has no argument-free command to invoke.
  ///
  /// Parameterized commands must use their composed grammar branch instead.
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
  /// Inspect or change presentation in the selected panel context.
  Render {
    #[command(subcommand)]
    command: crate::render::RenderArgs,
  },
  /// List and enter open protein rendering views.
  Panel {
    #[command(subcommand)]
    command: RenderingPanelArgs,
  },
}

#[derive(Debug, Subcommand)]
enum RenderingPanelArgs {
  /// List open protein rendering views and their stable identifiers.
  List,
  /// Select a rendering view as this shell's operation context.
  Enter {
    /// Stable identifier shown by `panel list`.
    #[arg(value_parser = clap::value_parser!(u64).range(1..))]
    id: u64,
  },
  /// Leave the rendering context without closing the view.
  Leave,
}

impl BuiltinShellCommandArgs {
  /// Converts the selected grammar branch into its execution-domain result.
  fn into_line(self) -> Result<BuiltinCommandLine, CommandLineParseError> {
    let line = match self {
      Self::Portable(arguments) => BuiltinCommandLine::Portable(arguments.into_command()?),
      Self::Clear => BuiltinCommandLine::ShellBuiltin(ShellBuiltin::Clear),
      Self::Render { command } => BuiltinCommandLine::Frontend(command.into_command().into()),
      Self::Panel { command } => BuiltinCommandLine::ShellBuiltin(ShellBuiltin::RenderingPanel(match command {
        RenderingPanelArgs::List => RenderingPanelCommand::List,
        RenderingPanelArgs::Enter { id } => RenderingPanelCommand::Enter { id },
        RenderingPanelArgs::Leave => RenderingPanelCommand::Leave,
      })),
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
  let mut positional_count = 0;
  for token in &tokens {
    if token.starts_with('-') {
      continue;
    }
    if let Some(subcommand) = command.get_subcommands().find(|subcommand| {
      subcommand.get_name() == token || subcommand.get_all_aliases().any(|alias| alias == token.as_str())
    }) {
      command = subcommand;
      positional_count = 0;
    } else {
      positional_count += 1;
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
      command
        .get_positionals()
        .nth(positional_count)
        .into_iter()
        .flat_map(|argument| argument.get_possible_values())
        .map(|value| value.get_name().to_owned()),
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
        // Preserve path separators unless the next character is actually escapable.
        let escaped = characters.next_if(|(_, next)| match quote {
          Some(('"', _)) => matches!(next, '"' | '\\'),
          None => matches!(next, '\'' | '"' | '\\') || next.is_whitespace(),
          Some(_) => false,
        });
        token.push(escaped.map_or('\\', |(_, character)| character));
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

  use chitin_command::{CommandId, DatabaseCommand, RcsbDownloadArguments, StructureCommand};
  use chitin_databases::providers::rcsb::StructureFormat;

  use super::*;

  #[test]
  fn appearance_should_parse_typed_requests_and_complete_schemes() -> Result<(), CommandLineParseError> {
    use chitin_command::{RenderColorScheme, RenderCommand, RenderLayer, RenderRgb};
    assert!(matches!(
      parse_builtin_command_line("render polymer color chain")?,
      BuiltinCommandLine::Frontend(FrontendCommand::Render(RenderCommand::Color {
        layer: RenderLayer::Polymer,
        scheme: RenderColorScheme::Chain,
        value: None
      }))
    ));
    assert!(matches!(
      parse_builtin_command_line("render atom color uniform --value \"#B8B8B8\"")?,
      BuiltinCommandLine::Frontend(FrontendCommand::Render(RenderCommand::Color {
        value: Some(RenderRgb([184, 184, 184])),
        ..
      }))
    ));
    assert!(
      matches!(parse_builtin_command_line("render surface opacity 0.35")?, BuiltinCommandLine::Frontend(FrontendCommand::Render(RenderCommand::Opacity { layer: RenderLayer::Surface, value })) if value.value() == 0.35)
    );
    for value in ["NaN", "-1", "1.1"] {
      assert!(parse_builtin_command_line(&format!("render surface opacity {value}")).is_err());
    }
    assert_eq!(
      complete_builtin_shell_line("render polymer color c"),
      vec!["render polymer color chain", "render polymer color chain-element"]
    );
    Ok(())
  }

  #[test]
  fn render_requests_should_be_typed_frontend_commands() -> Result<(), CommandLineParseError> {
    use chitin_command::{
      RenderAtomStyle, RenderCommand, RenderPolymerStyle, RenderSurfaceBackend, RenderSurfaceStyle,
    };
    for (input, expected) in [
      ("render status", RenderCommand::Status),
      (
        "render atom style stick",
        RenderCommand::AtomStyle(RenderAtomStyle::Stick),
      ),
      (
        "render atom style sphere",
        RenderCommand::AtomStyle(RenderAtomStyle::Sphere),
      ),
      (
        "render atom style none",
        RenderCommand::AtomStyle(RenderAtomStyle::None),
      ),
      (
        "render atom style ball-and-stick",
        RenderCommand::AtomStyle(RenderAtomStyle::BallAndStick),
      ),
      (
        "render surface backend msms",
        RenderCommand::SurfaceBackend(RenderSurfaceBackend::Msms),
      ),
      (
        "render polymer style none",
        RenderCommand::PolymerStyle(RenderPolymerStyle::None),
      ),
      (
        "render polymer style cartoon",
        RenderCommand::PolymerStyle(RenderPolymerStyle::Cartoon),
      ),
      (
        "render surface style none",
        RenderCommand::SurfaceStyle(RenderSurfaceStyle::None),
      ),
      (
        "render surface style solid",
        RenderCommand::SurfaceStyle(RenderSurfaceStyle::Solid),
      ),
      (
        "render surface backend implicit-scalar-field",
        RenderCommand::SurfaceBackend(RenderSurfaceBackend::ImplicitScalarField),
      ),
    ] {
      assert!(
        matches!(parse_builtin_command_line(input)?, BuiltinCommandLine::Frontend(FrontendCommand::Render(command)) if command == expected)
      );
    }
    assert!(parse_builtin_command_line("render atom style invalid").is_err());
    Ok(())
  }

  #[test]
  fn render_help_and_value_completion_should_follow_clap() -> Result<(), CommandLineParseError> {
    for input in ["render", "render atom", "render surface"] {
      assert!(
        matches!(parse_builtin_command_line(input)?, BuiltinCommandLine::Display(help) if help.contains("Usage:"))
      );
    }
    assert_eq!(
      complete_builtin_shell_line("render surface backend m"),
      vec!["render surface backend msms"]
    );
    assert_eq!(
      complete_builtin_shell_line("render atom style s"),
      vec!["render atom style sphere", "render atom style stick"]
    );
    Ok(())
  }

  #[test]
  fn rendering_navigation_should_parse_as_session_commands() -> Result<(), CommandLineParseError> {
    for (input, expected) in [
      ("panel list", RenderingPanelCommand::List),
      ("panel enter 42", RenderingPanelCommand::Enter { id: 42 }),
      ("panel leave", RenderingPanelCommand::Leave),
    ] {
      assert!(matches!(parse_builtin_command_line(input)?,
        BuiltinCommandLine::ShellBuiltin(ShellBuiltin::RenderingPanel(command)) if command == expected));
    }
    Ok(())
  }

  #[test]
  fn rendering_navigation_should_reject_invalid_identifiers() {
    for input in ["panel enter", "panel enter 0", "panel enter -1", "panel enter protein"] {
      assert!(parse_builtin_command_line(input).is_err(), "{input}");
    }
  }

  #[test]
  fn panel_help_and_completion_should_come_from_the_shell_grammar() -> Result<(), CommandLineParseError> {
    assert!(
      matches!(parse_builtin_command_line("panel")?, BuiltinCommandLine::Display(help)
      if help.contains("list") && help.contains("enter") && help.contains("leave"))
    );
    assert_eq!(complete_builtin_shell_line("panel e"), vec!["panel enter"]);
    Ok(())
  }

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
  fn structure_validate_should_preserve_unquoted_windows_path() -> Result<(), CommandLineParseError> {
    let parsed = parse_builtin_command_line(r"structure validate C:\data\x.pdb")?;

    assert!(matches!(
      parsed,
      BuiltinCommandLine::Portable(PortableCommand::Structure(StructureCommand::Validate(arguments)))
        if arguments.input.input == Path::new(r"C:\data\x.pdb")
    ));
    Ok(())
  }

  #[test]
  fn database_download_should_preserve_quoted_windows_output_path() -> Result<(), CommandLineParseError> {
    let parsed = parse_builtin_command_line(r#"db rcsb download --id 4hhb -o "C:\out""#)?;

    assert!(matches!(
      parsed,
      BuiltinCommandLine::Portable(PortableCommand::Database(DatabaseCommand::DownloadRcsbStructure(
        RcsbDownloadArguments { output: Some(output), .. }
      ))) if output == Path::new(r"C:\out")
    ));
    Ok(())
  }

  #[test]
  fn tokenize_should_preserve_trailing_unquoted_backslash() -> Result<(), CommandLineParseError> {
    assert_eq!(
      tokenize("structure validate C:\\")?,
      vec!["structure", "validate", "C:\\"]
    );
    Ok(())
  }

  #[test]
  fn tokenize_should_still_escape_unquoted_whitespace() -> Result<(), CommandLineParseError> {
    assert_eq!(
      tokenize(r"structure validate C:\my\ file.pdb")?,
      vec!["structure", "validate", r"C:\my file.pdb"]
    );
    Ok(())
  }

  #[test]
  fn tokenize_should_still_escape_double_quote_inside_double_quotes() -> Result<(), CommandLineParseError> {
    assert_eq!(tokenize(r#""a\"b""#)?, vec!["a\"b"]);
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
    for spec in frontend_specs().filter(|spec| !spec.requires_arguments) {
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
