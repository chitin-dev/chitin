use gpui::{App, Entity, SharedString, UniformListScrollHandle, Window};

use crate::primitive::input::text::TextInputState;

/// Callback invoked when a quick-pick row is selected.
pub type QuickPickSelectHandler = dyn for<'a, 'b> Fn(usize, &'a mut Window, &'b mut App);

/// One visible quick-pick result row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuickPickItem {
  /// Main row label.
  pub title: SharedString,
  /// Secondary row metadata.
  pub subtitle: SharedString,
  /// Optional shortcut text shown on the trailing edge.
  pub shortcut: Option<SharedString>,
}

impl QuickPickItem {
  /// Creates a quick-pick row.
  ///
  /// # Parameters
  ///
  /// `title` is the primary row label.
  ///
  /// `subtitle` is the secondary row metadata.
  ///
  /// `shortcut` is optional trailing shortcut text.
  ///
  /// # Returns
  ///
  /// A reusable quick-pick row model.
  pub fn new(
    title: impl Into<SharedString>,
    subtitle: impl Into<SharedString>,
    shortcut: Option<impl Into<SharedString>>,
  ) -> Self {
    Self {
      title: title.into(),
      subtitle: subtitle.into(),
      shortcut: shortcut.map(Into::into),
    }
  }
}

/// Content for a compact form shown inside the quick-pick shell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuickPickForm {
  /// Form title.
  pub title: SharedString,
  /// Current form value or placeholder.
  pub value: SharedString,
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
  /// `state` stores the reusable text input state.
  ///
  /// # Returns
  ///
  /// A search input model with no callbacks.
  pub fn new(state: Entity<TextInputState>) -> Self {
    Self { state }
  }
}

/// Body content rendered below the query line.
#[derive(Clone, Debug)]
pub enum QuickPickContent {
  /// Ranked selectable rows.
  Items {
    /// Rows to render.
    items: Vec<QuickPickItem>,
    /// Currently selected row index.
    selected_index: usize,
    /// Optional virtual-list scroll handle used to reveal the selected row.
    scroll_handle: Option<UniformListScrollHandle>,
    /// Empty-state text.
    empty_message: SharedString,
  },
  /// A command-specific form.
  Form(QuickPickForm),
}

/// Data needed to render one quick-pick overlay.
#[derive(Clone)]
pub struct QuickPickOverlay {
  /// Leading prompt label.
  pub prompt: SharedString,
  /// Current query text.
  pub query: SharedString,
  /// Placeholder shown when `query` is empty.
  pub placeholder: SharedString,
  /// Optional reusable text input state for the search field.
  pub search_input: Option<QuickPickSearchInput>,
  /// Overlay body content.
  pub content: QuickPickContent,
}
