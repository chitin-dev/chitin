//! Completion-candidate handling for the desktop terminal prompt.

/// Action the live prompt takes for one set of completion candidates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum CompletionAction {
  /// Replace the line with the only match, followed by a word separator.
  Insert(String),
  /// List candidates without changing the editable line.
  List(Vec<String>),
  /// Leave the line untouched.
  None,
}

/// Decides what one completion result does to the live prompt.
///
/// # Parameters
///
/// * `candidates` are the full replacement lines reported by shell completion.
///
/// # Returns
///
/// A single candidate is inserted so repeated Tab presses converge on one
/// command. Several candidates are listed instead, because the grammar offers
/// no unambiguous choice and overwriting the line would discard user input.
pub(super) fn completion_action(candidates: Vec<String>) -> CompletionAction {
  match candidates.as_slice() {
    [] => CompletionAction::None,
    [line] => CompletionAction::Insert(format!("{line} ")),
    _ => CompletionAction::List(candidates),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn an_empty_completion_should_leave_the_line_untouched() {
    assert_eq!(completion_action(Vec::new()), CompletionAction::None);
  }

  #[test]
  fn a_unique_completion_should_be_inserted_with_a_separator() {
    assert_eq!(
      completion_action(vec!["clear".to_owned()]),
      CompletionAction::Insert("clear ".to_owned())
    );
  }

  #[test]
  fn ambiguous_completion_should_list_candidates_without_editing() {
    let candidates = vec!["tab.close".to_owned(), "tab.focus_next".to_owned()];

    assert_eq!(
      completion_action(candidates.clone()),
      CompletionAction::List(candidates)
    );
  }
}
