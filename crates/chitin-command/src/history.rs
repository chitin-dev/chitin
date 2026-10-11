//! Bounded linear history of committed state changes, independent of execution.

use std::collections::VecDeque;

/// One reversible change and its retained-memory estimate.
struct Record<T> {
  change: T,
  bytes: usize,
}

/// Successful edits form one timeline; new edits invalidate the redo branch.
pub struct CommandHistory<T> {
  undo: VecDeque<Record<T>>,
  redo: Vec<Record<T>>,
  bytes: usize,
  max_entries: usize,
  max_bytes: usize,
}

impl<T> CommandHistory<T> {
  /// Creates history with shared undo/redo count and memory limits.
  pub fn new(max_entries: usize, max_bytes: usize) -> Self {
    Self {
      undo: VecDeque::new(),
      redo: Vec::new(),
      bytes: 0,
      max_entries,
      max_bytes,
    }
  }

  /// Commits one nonempty change, clearing the abandoned redo branch.
  pub fn record(&mut self, change: T, bytes: usize) {
    self.bytes = self
      .bytes
      .saturating_sub(self.redo.iter().map(|record| record.bytes).sum::<usize>());
    self.redo.clear();
    while self.undo.len() >= self.max_entries || self.bytes.saturating_add(bytes) > self.max_bytes {
      let Some(record) = self.undo.pop_front() else { break };
      self.bytes = self.bytes.saturating_sub(record.bytes);
    }
    // An oversized edit still invalidates redo, but must not exceed the budget.
    if self.max_entries == 0 || bytes > self.max_bytes {
      return;
    }
    self.bytes += bytes;
    self.undo.push_back(Record { change, bytes });
  }

  /// Returns the next undo change without consuming it before application succeeds.
  pub fn undo_change(&self) -> Option<&T> {
    self.undo.back().map(|record| &record.change)
  }
  /// Returns the next redo change without consuming it before application succeeds.
  pub fn redo_change(&self) -> Option<&T> {
    self.redo.last().map(|record| &record.change)
  }
  /// Advances the timeline after a successful undo.
  pub fn did_undo(&mut self) {
    if let Some(record) = self.undo.pop_back() {
      self.redo.push(record);
    }
  }
  /// Advances the timeline after a successful redo.
  pub fn did_redo(&mut self) {
    if let Some(record) = self.redo.pop() {
      self.undo.push_back(record);
    }
  }
  /// Removes changes whose target or structure revision is no longer live.
  pub fn retain(&mut self, mut live: impl FnMut(&T) -> bool) {
    self.undo.retain(|record| live(&record.change));
    self.redo.retain(|record| live(&record.change));
    self.bytes = self.undo.iter().chain(&self.redo).map(|record| record.bytes).sum();
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  #[test]
  fn new_edits_should_drop_redo_and_evict_oldest_records() {
    let mut history = CommandHistory::new(2, 8);
    history.record(1, 4);
    history.record(2, 4);
    history.did_undo();
    assert_eq!(history.redo_change(), Some(&2));
    history.record(3, 4);
    assert_eq!(history.redo_change(), None);
    history.record(4, 4);
    history.did_undo();
    history.did_undo();
    assert_eq!(history.undo_change(), None);
    assert_eq!(history.redo_change(), Some(&3));
  }
  #[test]
  fn invalid_targets_should_be_removed_from_both_branches() {
    let mut history = CommandHistory::new(10, 100);
    history.record(1, 4);
    history.record(2, 4);
    history.did_undo();
    history.retain(|value| *value != 2);
    assert_eq!(history.redo_change(), None);
    assert_eq!(history.undo_change(), Some(&1));
  }
  #[test]
  fn oversized_edit_should_not_allow_undo_across_an_unrecorded_change() {
    let mut history = CommandHistory::new(10, 8);
    history.record(1, 4);
    history.record(2, 4);
    history.did_undo();
    history.record(3, 9);
    assert_eq!(history.undo_change(), None);
    assert_eq!(history.redo_change(), None);
  }
}
