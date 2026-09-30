//! Desktop-owned command-panel state and focus management.

use chitin_command::{CommandId, CommandRegistry};
use gpui::{AppContext, Context, Entity, FocusHandle, KeyDownEvent, Window};
use gpui_kit::component::command::CommandState;

use crate::{app::ChitinApp, components::command_panel::form::rcsb::RcsbFormPanel};

/// Current interaction mode of the command panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CommandPanelMode {
  /// Search mode listing matching commands.
  Search,
  /// Form mode for one command, identified without duplicating its descriptor.
  Form(CommandId),
}

/// Result of handling one command-panel key event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CommandPanelEvent {
  /// The panel should close and restore its prior focus target.
  Close,
}

/// State and focus owner for the desktop command panel.
pub(crate) struct CommandPanelController {
  /// Searchable metadata for every desktop command.
  registry: CommandRegistry,
  /// Whether the quick-pick overlay is currently visible.
  is_open: bool,
  /// Query mirrored solely for domain ranking; Kit owns editing and selection.
  query: String,
  /// Kit palette state owns focus, selection, scrolling, and submission.
  palette: Option<Entity<CommandState>>,
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
  pub(crate) fn new() -> Self {
    Self {
      registry: chitin_command::default_registry(),
      is_open: false,
      query: String::new(),
      palette: None,
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
    &self.query
  }

  /// Returns the persistent Kit palette, creating it on first use.
  pub(crate) fn palette(&mut self, window: &mut Window, cx: &mut Context<ChitinApp>) -> Entity<CommandState> {
    self
      .palette
      .get_or_insert_with(|| cx.new(|cx| CommandState::new(window, cx)))
      .clone()
  }

  /// Mirrors Kit's query for command-registry ranking without owning editing.
  pub(crate) fn set_query(&mut self, query: impl Into<String>) {
    self.query = query.into();
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
    let palette = self.palette(window, cx);
    palette.update(cx, |state, cx| state.set_query("", window, cx));
    palette.update(cx, |state, cx| state.focus(window, cx));
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
  pub(crate) fn toggle_without_focus(&mut self) -> bool {
    if self.is_open {
      self.is_open = false;
      self.query.clear();
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
    self.query.clear();
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
  /// * `id` identifies the command that owns the form metadata.
  ///
  /// # Returns
  ///
  /// `true` when the command is registered and the panel entered form mode.
  pub(crate) fn open_form(&mut self, id: CommandId) -> bool {
    let Some(spec) = self.registry.spec_for(id) else {
      return false;
    };
    if !spec.requires_arguments {
      return false;
    }

    self.mode = CommandPanelMode::Form(id);
    self.rcsb_focus_pending = true;
    self.query.clear();
    true
  }

  /// Returns or creates the singleton RCSB form state.
  pub(crate) fn rcsb_form(&mut self, window: &mut Window, cx: &mut Context<ChitinApp>) -> RcsbFormPanel {
    self
      .rcsb_form
      .get_or_insert_with(|| RcsbFormPanel::new(window, cx))
      .clone()
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

    // Search-mode cancellation/navigation belong to Kit Command. The form
    // remains a distinct desktop presentation with its own Escape boundary.
    (matches!(self.mode, CommandPanelMode::Form(_)) && event.keystroke.key == "escape")
      .then_some(CommandPanelEvent::Close)
  }

  /// Resets transient state before an open transition.
  fn reset_for_open(&mut self) {
    self.is_open = true;
    self.query.clear();
    self.mode = CommandPanelMode::Search;
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
  fn reset_for_open_should_clear_domain_query() {
    let mut controller = CommandPanelController::new();
    controller.set_query("tab");

    controller.reset_for_open();

    assert!(controller.is_open);
    assert!(controller.query().is_empty());
    assert_eq!(controller.mode, CommandPanelMode::Search);
  }

  #[test]
  fn open_form_should_store_only_command_identity() {
    let mut controller = CommandPanelController::new();
    let id = CommandId::DatabaseDownloadRcsbStructure;

    assert!(controller.open_form(id));
    assert_eq!(controller.mode, CommandPanelMode::Form(id));
  }

  #[test]
  fn set_query_should_mirror_registry_query() {
    let mut controller = CommandPanelController::new();

    controller.set_query("workspace");

    assert_eq!(controller.query(), "workspace");
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
