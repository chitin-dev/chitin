//! Desktop command panel rendering and input handling.

use std::rc::Rc;

use chitin_ui::{
  components::quick_pick::{
    QuickPickContent, QuickPickForm, QuickPickItem, QuickPickOverlay, render_quick_pick_overlay,
  },
  themes::UIThemes,
};
use gpui::{App, Context, Div, FocusHandle, InteractiveElement, KeyDownEvent, SharedString, WeakEntity, Window};

use crate::{
  app::ChitinApp,
  commands::{
    ApplicationCommand, ChitinCommand, CommandDescriptor, CommandInvocationKind, CommandPanelMode, CommandPanelState,
    CommandRegistry,
  },
};

/// Callback invoked when a rendered command row is selected.
type CommandPanelSelectHandler = dyn Fn(usize, &mut App);

/// Renders the command panel overlay.
///
/// # Parameters
///
/// `state` provides the current panel mode, query, and selection.
///
/// `registry` provides searchable command descriptors.
///
/// `focus_handle` receives keyboard focus when the panel opens.
///
/// `theme` supplies workbench colors.
///
/// `app` is updated by pointer selection.
///
/// `_cx` is present for render API symmetry.
///
/// # Returns
///
/// A floating quick-pick style overlay.
pub(crate) fn render_command_panel(
  state: &CommandPanelState,
  registry: &CommandRegistry,
  focus_handle: &FocusHandle,
  theme: UIThemes,
  app: WeakEntity<ChitinApp>,
  _cx: &mut Context<ChitinApp>,
) -> Div {
  let results = registry.search(&state.query);
  let selected_descriptors = results
    .iter()
    .map(|result| result.descriptor.clone())
    .collect::<Vec<_>>();
  let overlay = match &state.mode {
    CommandPanelMode::Search => QuickPickOverlay {
      prompt: ">".into(),
      query: state.query.clone().into(),
      placeholder: "Type a command".into(),
      content: QuickPickContent::Items {
        items: results
          .iter()
          .map(|result| {
            QuickPickItem::new(
              result.descriptor.title,
              format!("{} · {}", result.descriptor.category.label(), result.descriptor.id),
              result.descriptor.shortcut,
            )
          })
          .collect(),
        selected_index: state.selected_index,
        empty_message: "No commands found".into(),
      },
    },
    CommandPanelMode::Form(descriptor) => QuickPickOverlay {
      prompt: descriptor.form_prompt.unwrap_or("Input").into(),
      query: state.query.clone().into(),
      placeholder: descriptor.form_placeholder.unwrap_or("").into(),
      content: QuickPickContent::Form(QuickPickForm {
        title: descriptor.title.into(),
        value: form_value(state, descriptor),
      }),
    },
  };
  let on_select: Rc<CommandPanelSelectHandler> = Rc::new(move |index, cx: &mut App| {
    let Some(descriptor) = selected_descriptors.get(index).cloned() else {
      return;
    };
    let _ = app.update(cx, |app, cx| {
      app.invoke_command_descriptor(descriptor, cx);
    });
  });

  render_quick_pick_overlay(overlay, theme, on_select).track_focus(focus_handle)
}

impl ChitinApp {
  /// Handles command panel keyboard input.
  ///
  /// # Parameters
  ///
  /// `event` is the GPUI key-down event.
  ///
  /// `window` is used to suppress default key handling after panel input.
  ///
  /// `cx` is notified when panel state or app state changes.
  pub(crate) fn handle_command_panel_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
    if !self.command_panel.is_open {
      return;
    }

    window.prevent_default();
    let key = event.keystroke.key.as_str();
    match key {
      "escape" => self.close_command_panel_and_restore_focus(window, cx),
      "up" => {
        let count = self.command_registry.search(&self.command_panel.query).len();
        self.command_panel.select_previous(count);
        cx.notify();
      }
      "down" => {
        let count = self.command_registry.search(&self.command_panel.query).len();
        self.command_panel.select_next(count);
        cx.notify();
      }
      "enter" => self.submit_command_panel(cx),
      "backspace" => {
        let mut query = self.command_panel.query.clone();
        query.pop();
        self.command_panel.set_query(query);
        cx.notify();
      }
      _ => {
        if let Some(character) = printable_character(event) {
          let mut query = self.command_panel.query.clone();
          query.push_str(character);
          self.command_panel.set_query(query);
          cx.notify();
        }
      }
    }
  }

  /// Invokes the selected command panel row or submits the current form.
  ///
  /// # Parameters
  ///
  /// `cx` is notified after panel state changes.
  fn submit_command_panel(&mut self, cx: &mut Context<Self>) {
    match self.command_panel.mode.clone() {
      CommandPanelMode::Search => {
        let results = self.command_registry.search(&self.command_panel.query);
        let Some(result) = results.get(self.command_panel.selected_index).cloned() else {
          return;
        };
        self.invoke_command_descriptor(result.descriptor, cx);
      }
      CommandPanelMode::Form(_) => {
        self.command_panel.close();
        cx.notify();
      }
    }
  }

  /// Invokes one command descriptor from search results.
  ///
  /// # Parameters
  ///
  /// `descriptor` is the selected command metadata.
  ///
  /// `cx` is used to dispatch or notify state changes.
  pub(crate) fn invoke_command_descriptor(&mut self, descriptor: CommandDescriptor, cx: &mut Context<Self>) {
    match descriptor.invocation {
      CommandInvocationKind::Form => {
        self.command_panel.open_form(descriptor);
        cx.notify();
      }
      CommandInvocationKind::Immediate => {
        let command = descriptor.command.clone();
        self.command_panel.close();
        if command == ChitinCommand::Application(ApplicationCommand::ToggleCommandPanel) {
          cx.notify();
        } else {
          self.dispatch_command(command, cx);
        }
      }
    }
  }
}

/// Returns the visible form value.
///
/// # Parameters
///
/// `state` contains form input.
///
/// `descriptor` is the command being configured.
///
/// # Returns
///
/// The current input, or the descriptor placeholder when input is empty.
fn form_value(state: &CommandPanelState, descriptor: &CommandDescriptor) -> SharedString {
  if state.query.is_empty() {
    descriptor.form_placeholder.unwrap_or("").into()
  } else {
    state.query.clone().into()
  }
}

/// Extracts printable text input from a key event.
///
/// # Parameters
///
/// `event` is the key event.
///
/// # Returns
///
/// `Some(&str)` for printable text input.
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
