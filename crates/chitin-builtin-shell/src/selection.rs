//! Scientific selection syntax within the shell's current panel context.

use chitin_command::{AtomSpec, SelectionCommand, SelectionMode};

pub(crate) fn parse(tokens: &[String]) -> Result<SelectionCommand, String> {
  match tokens {
    [verb] if verb == "clear" => Ok(SelectionCommand::Clear),
    [verb] if verb == "all" => Ok(SelectionCommand::All),
    [verb] if verb == "status" => Ok(SelectionCommand::Status),
    [spec] => Ok(SelectionCommand::Match {
      spec: spec.parse::<AtomSpec>()?,
      mode: SelectionMode::Replace,
    }),
    [verb, spec] if verb == "add" || verb == "subtract" => Ok(SelectionCommand::Match {
      spec: spec.parse::<AtomSpec>()?,
      mode: if verb == "add" {
        SelectionMode::Add
      } else {
        SelectionMode::Subtract
      },
    }),
    _ => Err(
      "Usage: select <spec> | select add <spec> | select subtract <spec> | select all | select clear | select status"
        .into(),
    ),
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{BuiltinCommandLine, complete_builtin_shell_line, parse_builtin_command_line};
  use chitin_command::{ApplicationCommand, FrontendCommand};

  #[test]
  fn selection_and_history_should_use_typed_frontend_requests() -> Result<(), Box<dyn std::error::Error>> {
    for (line, mode) in [
      ("select /AA:-5--1@CA", SelectionMode::Replace),
      ("select add /A:42A", SelectionMode::Add),
      ("select subtract :LYS", SelectionMode::Subtract),
    ] {
      assert!(
        matches!(parse_builtin_command_line(line)?, BuiltinCommandLine::Frontend(FrontendCommand::Selection(SelectionCommand::Match { mode: actual, .. })) if actual == mode)
      );
    }
    for (line, command) in [
      ("select all", SelectionCommand::All),
      ("select clear", SelectionCommand::Clear),
      ("select status", SelectionCommand::Status),
    ] {
      assert!(
        matches!(parse_builtin_command_line(line)?, BuiltinCommandLine::Frontend(FrontendCommand::Selection(actual)) if actual == command)
      );
    }
    assert!(matches!(
      parse_builtin_command_line("undo")?,
      BuiltinCommandLine::Frontend(FrontendCommand::Application(ApplicationCommand::Undo))
    ));
    assert!(matches!(
      parse_builtin_command_line("redo")?,
      BuiltinCommandLine::Frontend(FrontendCommand::Application(ApplicationCommand::Redo))
    ));
    assert_eq!(
      complete_builtin_shell_line("select s"),
      ["select status", "select subtract"]
    );
    assert!(parse_builtin_command_line("select #1/A:42").is_err());
    assert!(
      matches!(parse_builtin_command_line("select --help")?, BuiltinCommandLine::Display(text) if text.contains("Usage:"))
    );
    Ok(())
  }
}
