//! Desktop-owned command-panel state and focus management.

use chitin_command::{ChitinCommand, CommandRegistry};
use chitin_ui::{composite::quickpick::QuickPickState, primitive::input::text::TextInputState};
use gpui::{AppContext, Context, Entity, FocusHandle, KeyDownEvent, ScrollStrategy, UniformListScrollHandle, Window};

use crate::{app::ChitinApp, components::command_panel::form::rcsb::RcsbFormPanel};

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
}

/// State and focus owner for the desktop command panel.
pub(crate) struct CommandPanelController {
  /// Searchable metadata for every desktop command.
  registry: CommandRegistry,
  /// Whether the quick-pick overlay is currently visible.
  is_open: bool,
  /// Search and selection state for the command result quick-pick.
  quickpick: QuickPickState,
  /// Primitive text-input state for quick-pick search.
  search_input: Option<Entity<TextInputState>>,
  /// Whether desktop routing has subscribed to search input events.
  search_input_subscribed: bool,
  /// Current panel interaction mode.
  mode: CommandPanelMode,
  /// Focus target active before the overlay opened.
  previous_focus: Option<FocusHandle>,
  /// Singleton RCSB form state used by the database command.
  rcsb_form: Option<RcsbFormPanel>,
  /// Whether the next RCSB form render should claim focus.
  rcsb_focus_pending: bool,
  /// Whether RCSB form event subscriptions have been installed.
  rcsb_form_subscribed: bool,
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
      registry: chitin_command::default_registry(),
      is_open: false,
      quickpick: QuickPickState::new(),
      search_input: None,
      search_input_subscribed: false,
      mode: CommandPanelMode::Search,
      previous_focus: None,
      rcsb_form: None,
      rcsb_focus_pending: false,
      rcsb_form_subscribed: false,
    }
  }

  /// Returns whether the command panel is visible.
  pub(crate) fn is_open(&self) -> bool {
    self.is_open
  }

  /// Returns the current command metadata registry.
  pub(crate) fn registry(&self) -> &CommandRegistry {
    &self.registry
  }

  /// Returns the active panel interaction mode.
  pub(crate) fn mode(&self) -> &CommandPanelMode {
    &self.mode
  }

  /// Returns the current search text.
  pub(crate) fn query(&self) -> &str {
    self.quickpick.query()
  }

  /// Returns the selected command result index.
  pub(crate) fn selected_index(&self) -> usize {
    self.quickpick.selected_index()
  }

  /// Returns the scroll handle for command result virtualization.
  pub(crate) fn result_scroll_handle(&self) -> UniformListScrollHandle {
    self.quickpick.scroll_handle()
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
    self.quickpick.set_query(query);
  }

  /// Resolves the current search selection or form value into a panel event.
  pub(crate) fn submit_current(&self) -> CommandPanelEvent {
    self.submit()
  }

  /// Opens the command panel and focuses its overlay.
  ///
  /// # Parameters
  ///
  /// * `window` supplies the focus snapshot and receives the focus request.
  /// * `cx` allocates the overlay focus handle when necessary.
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
  /// * `window` supplies focus state for opening and closing.
  /// * `cx` allocates or restores GPUI focus handles.
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
      self.quickpick.reset();
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
  /// * `window` receives the focus restoration request.
  /// * `cx` is used by GPUI focus APIs.
  ///
  /// # Returns
  ///
  /// `true` when the panel changed from open to closed.
  pub(crate) fn close(&mut self, window: &mut Window, cx: &mut Context<ChitinApp>) -> bool {
    if !self.is_open {
      return false;
    }

    self.is_open = false;
    self.quickpick.reset();
    if let Some(search_input) = self.search_input.as_ref() {
      search_input.update(cx, |state, cx| {
        state.set_text("", cx);
      });
    }
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
  /// * `command` identifies the command that owns the form metadata.
  ///
  /// # Returns
  ///
  /// `true` when the command is registered and the panel entered form mode.
  pub(crate) fn open_form(&mut self, command: ChitinCommand) -> bool {
    if self.registry.descriptor_for(&command).is_none() {
      return false;
    }

    self.mode = CommandPanelMode::Form(command);
    self.rcsb_focus_pending = true;
    self.quickpick.reset();
    true
  }

  /// Returns or creates the singleton RCSB form state.
  pub(crate) fn rcsb_form(&mut self, cx: &mut Context<ChitinApp>) -> RcsbFormPanel {
    self.rcsb_form.get_or_insert_with(|| RcsbFormPanel::new(cx)).clone()
  }

  /// Returns the already-created RCSB form state.
  pub(crate) fn rcsb_form_if_created(&self) -> Option<RcsbFormPanel> {
    self.rcsb_form.clone()
  }

  /// Marks the RCSB form focus request as handled.
  pub(crate) fn take_rcsb_focus_request(&mut self) -> bool {
    std::mem::take(&mut self.rcsb_focus_pending)
  }

  /// Marks the RCSB form event subscriptions as installed.
  pub(crate) fn take_rcsb_form_subscription(&mut self) -> bool {
    if self.rcsb_form_subscribed {
      return false;
    }
    self.rcsb_form_subscribed = true;
    true
  }

  /// Handles one key event while the command panel is open.
  ///
  /// # Parameters
  ///
  /// * `event` is the GPUI key event received by the workbench root.
  ///
  /// # Returns
  ///
  /// The local state change or command operation requested by the event.
  pub(crate) fn handle_key(&mut self, event: &KeyDownEvent) -> Option<CommandPanelEvent> {
    if !self.is_open {
      return None;
    }

    if self.mode == CommandPanelMode::Search {
      return self.handle_search_key(event);
    }

    (event.keystroke.key == "escape").then_some(CommandPanelEvent::Close)
  }

  /// Handles navigation keys while the search input owns query editing.
  ///
  /// # Parameters
  ///
  /// * `event` is the GPUI key event received by the workbench root.
  ///
  /// # Returns
  ///
  /// A panel event for navigation or cancellation, or `None` for text editing.
  fn handle_search_key(&mut self, event: &KeyDownEvent) -> Option<CommandPanelEvent> {
    match event.keystroke.key.as_str() {
      "escape" => Some(CommandPanelEvent::Close),
      "up" => {
        self.quickpick.select_previous();
        self.quickpick.reveal_selected(ScrollStrategy::Top);
        Some(CommandPanelEvent::StateChanged)
      }
      "down" => {
        let result_count = self.registry.search(self.quickpick.query()).len();
        self.quickpick.select_next(result_count);
        self.quickpick.reveal_selected(ScrollStrategy::Bottom);
        Some(CommandPanelEvent::StateChanged)
      }
      // TextInputState owns Search-mode typing, deletion, cursor movement,
      // and submission. It reports changes through TextInputEvent.
      _ => None,
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
    self.quickpick.reset();
    self.mode = CommandPanelMode::Search;
    self.quickpick.reveal_selected(ScrollStrategy::Top);
  }

  /// Resolves the current input line into a command-panel operation.
  ///
  /// # Parameters
  ///
  /// This method reads the controller state and command registry.
  ///
  /// # Returns
  ///
  /// An invocation request or local state-change event.
  fn submit(&self) -> CommandPanelEvent {
    self
      .registry
      .search(self.quickpick.query())
      .get(self.quickpick.selected_index())
      .map(|result| CommandPanelEvent::Invoke(result.descriptor.command.clone()))
      .unwrap_or(CommandPanelEvent::StateChanged)
  }
}

impl Default for CommandPanelController {
  /// Creates the default closed command panel controller.
  fn default() -> Self {
    Self::new()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gpui::Keystroke;

  fn key_down(key: &str) -> KeyDownEvent {
    KeyDownEvent {
      keystroke: Keystroke {
        key: key.to_owned(),
        ..Default::default()
      },
      is_held: false,
      prefer_character_input: false,
    }
  }

  #[test]
  fn reset_for_open_should_clear_query_and_selection() {
    let mut controller = CommandPanelController::new();
    controller.set_query("tab");
    controller.quickpick.select_next(3);

    controller.reset_for_open();

    assert!(controller.is_open);
    assert!(controller.query().is_empty());
    assert_eq!(controller.selected_index(), 0);
    assert_eq!(controller.mode, CommandPanelMode::Search);
  }

  #[test]
  fn open_form_should_store_only_command_identity() {
    let mut controller = CommandPanelController::new();
    let command = ChitinCommand::from(chitin_command::DatabaseCommand::DownloadRcsbStructure);

    assert!(controller.open_form(command.clone()));
    assert_eq!(controller.mode, CommandPanelMode::Form(command));
  }

  #[test]
  fn set_query_should_reset_result_selection() {
    let mut controller = CommandPanelController::new();
    controller.quickpick.select_next(4);

    controller.set_query("workspace");

    assert_eq!(controller.query(), "workspace");
    assert_eq!(controller.selected_index(), 0);
  }

  #[test]
  fn search_backspace_should_not_mutate_query_outside_text_input() {
    let mut controller = CommandPanelController::new();
    controller.is_open = true;
    controller.set_query("workspace");

    assert_eq!(controller.handle_key(&key_down("backspace")), None);
    assert_eq!(controller.query(), "workspace");
  }
}
