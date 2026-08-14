//! Data models for the reusable quick-pick composite.

use gpui::{App, Entity, ScrollStrategy, SharedString, UniformListScrollHandle, Window};

use crate::primitive::input::text::TextInputState;

/// Callback invoked when a quick-pick row is selected.
pub type QuickPickSelectHandler = dyn for<'a, 'b> Fn(usize, &'a mut Window, &'b mut App);

/// Persistent interaction state for a searchable, selectable list.
#[derive(Clone)]
pub struct QuickPickState {
  query: String,
  selected_index: usize,
  scroll_handle: UniformListScrollHandle,
}

impl QuickPickState {
  /// Creates an empty quick-pick interaction state.
  pub fn new() -> Self {
    Self {
      query: String::new(),
      selected_index: 0,
      scroll_handle: UniformListScrollHandle::new(),
    }
  }

  /// Returns the current query text.
  pub fn query(&self) -> &str {
    &self.query
  }

  /// Returns the currently highlighted item index.
  pub fn selected_index(&self) -> usize {
    self.selected_index
  }

  /// Returns the scroll handle for the rendered result list.
  pub fn scroll_handle(&self) -> UniformListScrollHandle {
    self.scroll_handle.clone()
  }

  /// Replaces the query and returns the selection to the first item.
  pub fn set_query(&mut self, query: impl Into<String>) {
    self.query = query.into();
    self.selected_index = 0;
    self.reveal_selected(ScrollStrategy::Top);
  }

  /// Clears the query and selection before opening a quick-pick.
  pub fn reset(&mut self) {
    self.query.clear();
    self.selected_index = 0;
    self.reveal_selected(ScrollStrategy::Top);
  }

  /// Moves the highlight to the previous item without going below the first.
  pub fn select_previous(&mut self) {
    self.selected_index = self.selected_index.saturating_sub(1);
  }

  /// Moves the highlight to the next item without going past the last item.
  pub fn select_next(&mut self, item_count: usize) {
    if item_count > 0 {
      self.selected_index = (self.selected_index + 1).min(item_count - 1);
    }
  }

  /// Requests that the highlighted item be revealed by the virtual list.
  pub fn reveal_selected(&self, strategy: ScrollStrategy) {
    self.scroll_handle.scroll_to_item(self.selected_index, strategy);
  }
}

impl Default for QuickPickState {
  fn default() -> Self {
    Self::new()
  }
}

/// One visible quick-pick result row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuickPickItem {
  /// Main row label.
  pub title: SharedString,
  /// Optional shortcut text shown on the trailing edge.
  pub shortcut: Option<SharedString>,
}

impl QuickPickItem {
  /// Creates a quick-pick row without secondary metadata.
  ///
  /// # Parameters
  ///
  /// * `title` is the primary row label.
  /// * `shortcut` is optional trailing shortcut text.
  ///
  /// # Returns
  ///
  /// A reusable quick-pick row model.
  pub fn new(title: impl Into<SharedString>, shortcut: Option<impl Into<SharedString>>) -> Self {
    Self {
      title: title.into(),
      shortcut: shortcut.map(Into::into),
    }
  }
}

/// Reusable search input configuration for a quick-pick overlay.
#[derive(Clone)]
pub struct QuickPickSearchInput {
  /// State owned by the application rendering the quick-pick.
  pub state: Entity<TextInputState>,
}

impl QuickPickSearchInput {
  /// Creates a quick-pick search input model.
  ///
  /// # Parameters
  ///
  /// * `state` stores the reusable text input state.
  ///
  /// # Returns
  ///
  /// A search input model with no callbacks.
  pub fn new(state: Entity<TextInputState>) -> Self {
    Self { state }
  }
}

/// Data needed to render one quick-pick overlay.
#[derive(Clone)]
pub struct QuickPickOverlay {
  /// Current query text.
  pub query: SharedString,
  /// Placeholder shown when `query` is empty.
  pub placeholder: SharedString,
  /// Optional reusable text input state for the search field.
  pub search_input: Option<QuickPickSearchInput>,
  /// Ranked selectable rows.
  pub items: Vec<QuickPickItem>,
  /// Currently selected row index.
  pub selected_index: usize,
  /// Optional virtual-list scroll handle used to reveal the selected row.
  pub scroll_handle: Option<UniformListScrollHandle>,
  /// Empty-state text shown when `items` is empty.
  pub empty_message: SharedString,
}

#[cfg(test)]
mod tests {
  use super::QuickPickState;
  use gpui::ScrollStrategy;

  #[test]
  fn query_changes_reset_selection() {
    let mut state = QuickPickState::new();
    state.select_next(4);

    state.set_query("workspace");

    assert_eq!(state.query(), "workspace");
    assert_eq!(state.selected_index(), 0);
  }

  #[test]
  fn navigation_stays_within_item_count() {
    let mut state = QuickPickState::new();
    state.select_next(2);
    state.select_next(2);
    state.select_previous();

    assert_eq!(state.selected_index(), 0);

    state.select_previous();
    state.reveal_selected(ScrollStrategy::Top);
    assert_eq!(state.selected_index(), 0);
  }

  #[test]
  fn reset_clears_query_and_selection() {
    let mut state = QuickPickState::new();
    state.set_query("command");
    state.select_next(3);

    state.reset();

    assert!(state.query().is_empty());
    assert_eq!(state.selected_index(), 0);
  }
}
