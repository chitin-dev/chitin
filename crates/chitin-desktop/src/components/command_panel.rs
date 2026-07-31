//! Desktop command panel rendering and input handling.

use std::rc::Rc;

mod controller;

pub(crate) use controller::CommandPanelController;
use controller::{CommandPanelEvent, CommandPanelMode};

use chitin_ui::{
  components::quick_pick::{
    QuickPickContent, QuickPickForm, QuickPickItem, QuickPickOverlay, render_quick_pick_overlay,
  },
  themes::UIThemes,
};
use gpui::{App, Context, Div, InteractiveElement, KeyDownEvent, SharedString, WeakEntity, Window};

use crate::{
  app::ChitinApp,
  commands::{ApplicationCommand, ChitinCommand, CommandDescriptor, CommandInvocationKind},
};

/// Callback invoked when a rendered command row is selected.
type CommandPanelSelectHandler = dyn for<'a, 'b> Fn(usize, &'a mut Window, &'b mut App);

/// Renders the command panel overlay.
///
/// # Parameters
///
/// `controller` owns the current panel state, command registry, and focus handle.
///
/// `theme` supplies workbench colors.
///
/// `app` is updated by pointer selection.
///
/// # Returns
///
/// A floating quick-pick style overlay.
pub(crate) fn render_command_panel(
  controller: &mut CommandPanelController,
  theme: UIThemes,
  app: WeakEntity<ChitinApp>,
  cx: &mut Context<ChitinApp>,
) -> Div {
  let (overlay, selected_commands) = match controller.mode() {
    CommandPanelMode::Search => {
      let results = controller.registry().search(controller.query());
      let selected_commands = results
        .iter()
        .map(|result| result.descriptor.command.clone())
        .collect::<Vec<_>>();
      let overlay = QuickPickOverlay {
        prompt: ">".into(),
        query: controller.query().into(),
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
          selected_index: controller.selected_index(),
          scroll_handle: Some(controller.result_scroll_handle()),
          empty_message: "No commands found".into(),
        },
      };
      (overlay, selected_commands)
    }
    CommandPanelMode::Form(_) => (render_command_form(controller), Vec::new()),
  };
  let on_select: Rc<CommandPanelSelectHandler> = Rc::new(move |index, window, cx: &mut App| {
    let Some(command) = selected_commands.get(index).cloned() else {
      return;
    };
    let _ = app.update(cx, |app, cx| {
      app.invoke_command_from_panel(command, window, cx);
    });
  });

  let focus_handle = controller.focus_handle(cx);
  render_quick_pick_overlay(overlay, theme, on_select).track_focus(&focus_handle)
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
    let Some(panel_event) = self.command_panel.handle_key(event) else {
      return;
    };

    window.prevent_default();
    match panel_event {
      CommandPanelEvent::StateChanged => cx.notify(),
      CommandPanelEvent::Close => {
        self.command_panel.close(window, cx);
        cx.notify();
      }
      CommandPanelEvent::Invoke(command) => self.invoke_command_from_panel(command, window, cx),
      CommandPanelEvent::SubmitForm { .. } => {
        self.command_panel.close(window, cx);
        cx.notify();
      }
    }
  }

  /// Invokes one command selected from the command panel.
  ///
  /// # Parameters
  ///
  /// `command` is the typed command selected by the controller.
  ///
  /// `window` receives focus restoration when an immediate command closes the panel.
  ///
  /// `cx` is used to dispatch or notify state changes.
  pub(crate) fn invoke_command_from_panel(
    &mut self,
    command: ChitinCommand,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    let Some(invocation) = self
      .command_panel
      .registry()
      .descriptor_for(&command)
      .map(|descriptor| descriptor.invocation)
    else {
      return;
    };

    match invocation {
      CommandInvocationKind::Form => {
        if self.command_panel.open_form(command) {
          cx.notify();
        }
      }
      CommandInvocationKind::Immediate => {
        self.command_panel.close(window, cx);
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
/// `controller` contains form input.
///
/// `descriptor` is the command being configured.
///
/// # Returns
///
/// The current input, or the descriptor placeholder when input is empty.
fn form_value(controller: &CommandPanelController, descriptor: &CommandDescriptor) -> SharedString {
  if controller.query().is_empty() {
    descriptor.form_placeholder.unwrap_or("").into()
  } else {
    controller.query().into()
  }
}

/// Builds the quick-pick model for the active command form.
///
/// # Parameters
///
/// `controller` owns the current form command and input value.
///
/// # Returns
///
/// A quick-pick overlay for the active form, or an empty-state overlay if the command was removed.
fn render_command_form(controller: &CommandPanelController) -> QuickPickOverlay {
  let Some(descriptor) = controller.form_descriptor() else {
    return QuickPickOverlay {
      prompt: "Input".into(),
      query: controller.query().into(),
      placeholder: "".into(),
      content: QuickPickContent::Items {
        items: Vec::new(),
        selected_index: 0,
        scroll_handle: None,
        empty_message: "Command is unavailable".into(),
      },
    };
  };

  QuickPickOverlay {
    prompt: descriptor.form_prompt.unwrap_or("Input").into(),
    query: controller.query().into(),
    placeholder: descriptor.form_placeholder.unwrap_or("").into(),
    content: QuickPickContent::Form(QuickPickForm {
      title: descriptor.title.into(),
      value: form_value(controller, descriptor),
    }),
  }
}
