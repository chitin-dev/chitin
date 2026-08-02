//! Desktop-owned command-panel state and focus management.

use chitin_ui::primitive::input::text::TextInputState;
use gpui::{AppContext, Context, Entity, FocusHandle, KeyDownEvent, ScrollStrategy, UniformListScrollHandle, Window};

use crate::{
  app::ChitinApp,
  commands::{ChitinCommand, CommandDescriptor, CommandRegistry},
};

/// Current interaction mode of the command panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CommandPanelMode {
  /// Search mode listing matching commands.
  Search,
  /// Form mode for one command, identified without duplicating its descriptor.
  Form(ChitinCommand),
}

/// Result of handling one command-panel key event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CommandPanelEvent {
  /// The event only changed local panel state.
  StateChanged,
  /// The panel should close and restore its prior focus target.
  Close,
  /// A command should be invoked from the current search result.
  Invoke(ChitinCommand),
  /// A command-specific form was submitted.
  SubmitForm { command: ChitinCommand, value: String },
}

/// State and focus owner for the desktop command panel.
pub(crate) struct CommandPanelController {
  /// Searchable metadata for every desktop command.
  registry: CommandRegistry,
  /// Whether the quick-pick overlay is currently visible.
  is_open: bool,
  /// Current search text or form value.
  query: String,
  /// Selected search result index.
  selected_index: usize,
  /// Scroll handle for the virtualized command result list.
  result_scroll: UniformListScrollHandle,
  /// Primitive text-input state for quick-pick search.
  search_input: Option<Entity<TextInputState>>,
  /// Whether desktop routing has subscribed to search input events.
  search_input_subscribed: bool,
  /// Current panel interaction mode.
  mode: CommandPanelMode,
  /// Focus target active before the overlay opened.
  previous_focus: Option<FocusHandle>,
}

impl CommandPanelController {
  /// Creates a closed command panel with the default command registry.
  ///
  /// # Parameters
  ///
  /// This function takes no parameters.
  ///
  /// # Returns
  ///
  /// A controller ready to open and render the desktop command panel.
  pub(crate) fn new() -> Self {
    Self {
      registry: CommandRegistry::new(),
      is_open: false,
      query: String::new(),
      selected_index: 0,
      result_scroll: UniformListScrollHandle::new(),
      search_input: None,
      search_input_subscribed: false,
      mode: CommandPanelMode::Search,
      previous_focus: None,
    }
  }

  /// Returns whether the command panel is visible.
  ///
  /// # Parameters
  ///
  /// This method reads the controller state.
  ///
  /// # Returns
  ///
  /// `true` when the quick-pick overlay should render.
  pub(crate) fn is_open(&self) -> bool {
    self.is_open
  }

  /// Returns the current command metadata registry.
  ///
  /// # Parameters
  ///
  /// This method reads the controller state.
  ///
  /// # Returns
  ///
  /// The registry used for command lookup and search.
  pub(crate) fn registry(&self) -> &CommandRegistry {
    &self.registry
  }

  /// Returns the active panel interaction mode.
  ///
  /// # Parameters
  ///
  /// This method reads the controller state.
  ///
  /// # Returns
  ///
  /// Search mode or the identity of the command whose form is open.
  pub(crate) fn mode(&self) -> &CommandPanelMode {
    &self.mode
  }

  /// Returns the current search text or form input.
  ///
  /// # Parameters
  ///
  /// This method reads the controller state.
  ///
  /// # Returns
  ///
  /// The current input line.
  pub(crate) fn query(&self) -> &str {
    &self.query
  }

  /// Returns the selected command result index.
  ///
  /// # Parameters
  ///
  /// This method reads the controller state.
  ///
  /// # Returns
  ///
  /// The zero-based selected search result index.
  pub(crate) fn selected_index(&self) -> usize {
    self.selected_index
  }

  /// Returns the scroll handle for command result virtualization.
  ///
  /// # Parameters
  ///
  /// This method reads the controller state.
  ///
  /// # Returns
  ///
  /// A cloned GPUI uniform-list handle for the rendered command rows.
  pub(crate) fn result_scroll_handle(&self) -> UniformListScrollHandle {
    self.result_scroll.clone()
  }

  /// Returns persistent primitive state for the quick-pick search input.
  pub(crate) fn search_input(&mut self, cx: &mut Context<ChitinApp>) -> Entity<TextInputState> {
    self
      .search_input
      .get_or_insert_with(|| cx.new(TextInputState::new))
      .clone()
  }

  /// Marks the search input's desktop event subscription as installed.
  pub(crate) fn take_search_input_subscription(&mut self) -> bool {
    if self.search_input_subscribed {
      return false;
    }

    self.search_input_subscribed = true;
    true
  }

  /// Updates the query after a primitive text-input change event.
  pub(crate) fn set_query(&mut self, query: impl Into<String>) {
    self.query = query.into();
    self.selected_index = 0;
    self.reveal_selected(ScrollStrategy::Top);
  }

  /// Resolves the current search selection or form value into a panel event.
  pub(crate) fn submit_current(&self) -> CommandPanelEvent {
    self.submit()
  }

  /// Returns the descriptor for the command whose form is open.
  ///
  /// # Parameters
  ///
  /// This method reads the controller state and command registry.
  ///
  /// # Returns
  ///
  /// The form command descriptor when the current mode identifies one.
  pub(crate) fn form_descriptor(&self) -> Option<&CommandDescriptor> {
    let CommandPanelMode::Form(command) = &self.mode else {
      return None;
    };
    self.registry.descriptor_for(command)
  }

  /// Opens the command panel and focuses its overlay.
  ///
  /// # Parameters
  ///
  /// `window` supplies the focus snapshot and receives the focus request.
  ///
  /// `cx` allocates the overlay focus handle when necessary.
  ///
  /// # Returns
  ///
  /// `true` when the panel changed from closed to open.
  pub(crate) fn open(&mut self, window: &mut Window, cx: &mut Context<ChitinApp>) -> bool {
    if self.is_open {
      return false;
    }

    self.reset_for_open();
    self.previous_focus = window.focused(cx);
    let search_input = self.search_input(cx);
    search_input.update(cx, |state, cx| {
      state.set_text("", cx);
    });
    let focus = search_input.read(cx).focus_handle().clone();
    window.focus(&focus, cx);
    true
  }

  /// Toggles the command panel and maintains focus ownership.
  ///
  /// # Parameters
  ///
  /// `window` supplies focus state for opening and closing.
  ///
  /// `cx` allocates or restores GPUI focus handles.
  ///
  /// # Returns
  ///
  /// `true` when panel visibility changed.
  pub(crate) fn toggle(&mut self, window: &mut Window, cx: &mut Context<ChitinApp>) -> bool {
    if self.is_open {
      self.close(window, cx)
    } else {
      self.open(window, cx)
    }
  }

  /// Toggles panel visibility when no GPUI window is available.
  ///
  /// # Parameters
  ///
  /// This method mutably borrows the controller.
  ///
  /// # Returns
  ///
  /// `true` when panel visibility changed. Window-aware callers should prefer [`Self::toggle`].
  pub(crate) fn toggle_without_focus(&mut self) -> bool {
    if self.is_open {
      self.is_open = false;
      self.query.clear();
      self.selected_index = 0;
      self.mode = CommandPanelMode::Search;
      self.previous_focus = None;
    } else {
      self.reset_for_open();
    }
    true
  }

  /// Closes the command panel and restores its previous focus target.
  ///
  /// # Parameters
  ///
  /// `window` receives the focus restoration request.
  ///
  /// `cx` is used by GPUI focus APIs.
  ///
  /// # Returns
  ///
  /// `true` when the panel changed from open to closed.
  pub(crate) fn close(&mut self, window: &mut Window, cx: &mut Context<ChitinApp>) -> bool {
    if !self.is_open {
      return false;
    }

    self.is_open = false;
    self.query.clear();
    if let Some(search_input) = self.search_input.as_ref() {
      search_input.update(cx, |state, cx| {
        state.set_text("", cx);
      });
    }
    self.selected_index = 0;
    self.mode = CommandPanelMode::Search;
    if let Some(previous_focus) = self.previous_focus.take() {
      window.focus(&previous_focus, cx);
    }
    true
  }

  /// Opens a form for a registered command identity.
  ///
  /// # Parameters
  ///
  /// `command` identifies the command that owns the form metadata.
  ///
  /// # Returns
  ///
  /// `true` when the command is registered and the panel entered form mode.
  pub(crate) fn open_form(&mut self, command: ChitinCommand) -> bool {
    if self.registry.descriptor_for(&command).is_none() {
      return false;
    }

    self.mode = CommandPanelMode::Form(command);
    self.query.clear();
    self.selected_index = 0;
    self.reveal_selected(ScrollStrategy::Top);
    true
  }

  /// Handles one key event while the command panel is open.
  ///
  /// # Parameters
  ///
  /// `event` is the GPUI key event received by the workbench root.
  ///
  /// # Returns
  ///
  /// The local state change or command operation requested by the event.
  pub(crate) fn handle_key(&mut self, event: &KeyDownEvent) -> Option<CommandPanelEvent> {
    if !self.is_open {
      return None;
    }

    match event.keystroke.key.as_str() {
      "escape" => Some(CommandPanelEvent::Close),
      "up" => {
        self.select_previous();
        self.reveal_selected(ScrollStrategy::Top);
        Some(CommandPanelEvent::StateChanged)
      }
      "down" => {
        self.select_next();
        self.reveal_selected(ScrollStrategy::Bottom);
        Some(CommandPanelEvent::StateChanged)
      }
      "enter" => Some(self.submit()),
      "backspace" => {
        self.query.pop();
        self.selected_index = 0;
        self.reveal_selected(ScrollStrategy::Top);
        Some(CommandPanelEvent::StateChanged)
      }
      _ => {
        let character = printable_character(event)?;
        self.query.push_str(character);
        self.selected_index = 0;
        self.reveal_selected(ScrollStrategy::Top);
        Some(CommandPanelEvent::StateChanged)
      }
    }
  }

  /// Resets transient state before an open transition.
  ///
  /// # Parameters
  ///
  /// This method mutably borrows the controller.
  ///
  /// # Returns
  ///
  /// This function returns `()` after preparing search mode.
  fn reset_for_open(&mut self) {
    self.is_open = true;
    self.query.clear();
    self.selected_index = 0;
    self.mode = CommandPanelMode::Search;
    self.reveal_selected(ScrollStrategy::Top);
  }

  /// Moves selection to the previous visible search result.
  ///
  /// # Parameters
  ///
  /// This method mutably borrows the controller.
  ///
  /// # Returns
  ///
  /// This function returns `()` after clamping the selected index.
  fn select_previous(&mut self) {
    if !matches!(self.mode, CommandPanelMode::Search) {
      return;
    }

    self.selected_index = self.selected_index.saturating_sub(1);
  }

  /// Moves selection to the next visible search result.
  ///
  /// # Parameters
  ///
  /// This method mutably borrows the controller.
  ///
  /// # Returns
  ///
  /// This function returns `()` after clamping the selected index.
  fn select_next(&mut self) {
    if !matches!(self.mode, CommandPanelMode::Search) {
      return;
    }

    let result_count = self.registry.search(&self.query).len();
    if result_count > 0 {
      self.selected_index = (self.selected_index + 1).min(result_count - 1);
    }
  }

  /// Scrolls the virtual result list so the selected row remains visible.
  ///
  /// # Parameters
  ///
  /// `strategy` controls which viewport edge GPUI should use when scrolling is needed.
  ///
  /// # Returns
  ///
  /// This function returns `()` after recording a deferred GPUI scroll request.
  fn reveal_selected(&self, strategy: ScrollStrategy) {
    self.result_scroll.scroll_to_item(self.selected_index, strategy);
  }

  /// Resolves the current input line into a command-panel operation.
  ///
  /// # Parameters
  ///
  /// This method reads the controller state and command registry.
  ///
  /// # Returns
  ///
  /// An invocation request, form submission, or local state-change event.
  fn submit(&self) -> CommandPanelEvent {
    match &self.mode {
      CommandPanelMode::Search => self
        .registry
        .search(&self.query)
        .get(self.selected_index)
        .map(|result| CommandPanelEvent::Invoke(result.descriptor.command.clone()))
        .unwrap_or(CommandPanelEvent::StateChanged),
      CommandPanelMode::Form(command) => CommandPanelEvent::SubmitForm {
        command: command.clone(),
        value: self.query.clone(),
      },
    }
  }
}

impl Default for CommandPanelController {
  /// Creates the default closed command panel controller.
  fn default() -> Self {
    Self::new()
  }
}

/// Extracts printable text input from a key event.
///
/// # Parameters
///
/// `event` is the key event received by the command panel.
///
/// # Returns
///
/// `Some(&str)` for printable text input, otherwise `None`.
fn printable_character(event: &KeyDownEvent) -> Option<&str> {
  let modifiers = event.keystroke.modifiers;
  if modifiers.control || modifiers.platform || modifiers.alt || modifiers.function {
    return None;
  }
  event
    .keystroke
    .key_char
    .as_deref()
    .or_else(|| (event.keystroke.key.len() == 1).then_some(event.keystroke.key.as_str()))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn reset_for_open_should_clear_query_and_selection() {
    let mut controller = CommandPanelController::new();
    controller.query = "tab".to_string();
    controller.selected_index = 2;

    controller.reset_for_open();

    assert!(controller.is_open);
    assert!(controller.query.is_empty());
    assert_eq!(controller.selected_index, 0);
    assert_eq!(controller.mode, CommandPanelMode::Search);
  }

  #[test]
  fn open_form_should_store_only_command_identity() {
    let mut controller = CommandPanelController::new();
    let command = ChitinCommand::from(crate::commands::DatabaseCommand::DownloadRcsbStructure);

    assert!(controller.open_form(command.clone()));
    assert_eq!(controller.mode, CommandPanelMode::Form(command));
  }

  #[test]
  fn set_query_should_reset_result_selection() {
    let mut controller = CommandPanelController::new();
    controller.selected_index = 3;

    controller.set_query("workspace");

    assert_eq!(controller.query(), "workspace");
    assert_eq!(controller.selected_index(), 0);
  }
}
