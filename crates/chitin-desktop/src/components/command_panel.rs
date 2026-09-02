//! Desktop command panel rendering and input handling.

use std::{collections::BTreeSet, rc::Rc};

mod controller;
mod form;

pub(crate) use controller::CommandPanelController;
use controller::{CommandPanelEvent, CommandPanelMode};
use form::rcsb::{RcsbDownloadState, RcsbFormPanel, download_path};

use chitin_command::{ApplicationCommand, ChitinCommand, CommandInvocationKind};
use chitin_databases::providers::rcsb::{PdbId, RcsbBatchDownloadRequest};
use chitin_ui::{
  composite::quickpick::{QuickPickItem, QuickPickOverlay, QuickPickSearchInput, render_quick_pick_overlay},
  primitive::{
    button::ButtonEvent,
    input::{
      select::SelectInputEvent,
      text::{TextInputEvent, TextInputState},
    },
  },
  themes::UIThemes,
};
use gpui::{
  App, AppContext, Context, Div, Entity, InteractiveElement, KeyDownEvent, ParentElement, Styled, Subscription,
  WeakEntity, Window, div, px,
};

use crate::{
  app::ChitinApp,
  components::document_area::state::OpenedProjectDocument,
  tasks::{TaskOutput, TaskState, rcsb::submit_downloads},
};

/// Callback invoked when a rendered command row is selected.
type CommandPanelSelectHandler = dyn for<'a, 'b> Fn(usize, &'a mut Window, &'b mut App);

/// Renders the command panel overlay.
///
/// # Parameters
///
/// * `controller` owns the current panel state, command registry, and focus handle.
/// * `theme` supplies workbench colors.
/// * `app` is updated by pointer selection.
///
/// # Returns
///
/// A floating quick-pick style overlay.
pub(crate) fn render_command_panel(
  controller: &mut CommandPanelController,
  theme: UIThemes,
  app: WeakEntity<ChitinApp>,
  cx: &mut Context<ChitinApp>,
  search_input: Entity<TextInputState>,
) -> Div {
  let rcsb_form = matches!(controller.mode(), CommandPanelMode::Form(_)).then(|| controller.rcsb_form(cx));
  let focus_handle = rcsb_form
    .as_ref()
    .map(|form| form.pdb_id.read(cx).focus_handle().clone())
    .unwrap_or_else(|| search_input.read(cx).focus_handle().clone());
  if let Some(form) = rcsb_form {
    return div()
      .absolute()
      .inset_0()
      .flex()
      .items_start()
      .justify_center()
      .child(
        div()
          .mt(px(30.0))
          .w(px(420.0))
          .rounded_md()
          .border_1()
          .border_color(theme.border.primary)
          .bg(theme.background.secondary)
          .flex_none()
          .child(form.render(theme, cx)),
      )
      .track_focus(&focus_handle);
  }

  let results = controller.registry().search(controller.query());
  let selected_commands = results
    .iter()
    .map(|result| result.descriptor.command.clone())
    .collect::<Vec<_>>();
  let overlay = QuickPickOverlay {
    query: controller.query().into(),
    placeholder: "Type a command".into(),
    search_input: Some(QuickPickSearchInput::new(search_input)),
    items: results
      .iter()
      .map(|result| QuickPickItem::new(result.descriptor.title, result.descriptor.shortcut))
      .collect(),
    selected_index: controller.selected_index(),
    scroll_handle: Some(controller.result_scroll_handle()),
    empty_message: "No commands found".into(),
  };
  let on_select: Rc<CommandPanelSelectHandler> = Rc::new(move |index, window, cx: &mut App| {
    let Some(command) = selected_commands.get(index).cloned() else {
      return;
    };
    let _ = app.update(cx, |app, cx| {
      app.invoke_command_from_panel(command, window, cx);
    });
  });

  render_quick_pick_overlay(overlay, theme, on_select).track_focus(&focus_handle)
}

impl ChitinApp {
  /// Returns the command panel's input state and subscribes desktop behavior once.
  pub(crate) fn command_panel_search_input(
    &mut self,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) -> Entity<TextInputState> {
    let search_input = self.command_panel.search_input(cx);
    if !self.command_panel.take_search_input_subscription() {
      return search_input;
    }

    let subscription: Subscription =
      cx.subscribe_in(&search_input, window, move |this, _, event, window, cx| match event {
        TextInputEvent::Change { value } => {
          this.command_panel.set_query(value.to_string());
          cx.notify();
        }
        TextInputEvent::Submit { .. } => match this.command_panel.submit_current() {
          CommandPanelEvent::Invoke(command) => this.invoke_command_from_panel(command, window, cx),
          CommandPanelEvent::StateChanged | CommandPanelEvent::Close => {}
        },
        TextInputEvent::Cancel => {
          this.command_panel.close(window, cx);
          cx.notify();
        }
        _ => {}
      });
    subscription.detach();
    search_input
  }

  /// Creates the singleton RCSB form and installs its semantic event routes.
  pub(crate) fn command_panel_rcsb_form(
    &mut self,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) -> Option<RcsbFormPanel> {
    if !matches!(self.command_panel.mode(), CommandPanelMode::Form(_)) {
      return None;
    }
    let form = self.command_panel.rcsb_form(cx);
    if self.command_panel.take_rcsb_form_subscription() {
      let pdb_subscription = cx.subscribe_in(&form.pdb_id, window, move |this, _, event, window, cx| match event {
        TextInputEvent::Submit { .. } => this.submit_rcsb_form(window, cx),
        TextInputEvent::Change { .. } => {
          if let Some(form) = this.command_panel.rcsb_form_if_created() {
            form.download.update(cx, RcsbDownloadState::clear_error);
          }
          cx.notify();
        }
        _ => {}
      });
      pdb_subscription.detach();

      let select_subscription = cx.subscribe_in(&form.format, window, move |_, _, event, _, cx| {
        if matches!(event, SelectInputEvent::SelectionChange { .. }) {
          cx.notify();
        }
      });
      select_subscription.detach();

      let submit_subscription = cx.subscribe_in(&form.submit, window, move |this, _, event, window, cx| {
        if matches!(event, ButtonEvent::Click) {
          this.submit_rcsb_form(window, cx);
        }
      });
      submit_subscription.detach();
    }
    if self.command_panel.take_rcsb_focus_request() {
      form.reset(cx);
      let focus = form.pdb_id.read(cx).focus_handle().clone();
      window.focus(&focus, cx);
    }
    Some(form)
  }

  /// Submits the current RCSB form and closes its modal panel.
  fn submit_rcsb_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let Some(form) = self.command_panel.rcsb_form_if_created() else {
      return;
    };
    if form.download.read(cx).active {
      log::debug!("RCSB download submission ignored because a batch is already active");
      return;
    }
    let Some(workspace_root) = self.workspace.as_ref().map(|workspace| workspace.root.clone()) else {
      log::warn!("RCSB download ignored because no workspace is open");
      return;
    };
    let raw_pdb_ids = form.pdb_id.read(cx).text().trim().to_owned();
    let ids = match PdbId::parse_many(&raw_pdb_ids) {
      Ok(ids) => ids,
      Err(error) => {
        let error = error.to_string();
        log::warn!("RCSB download ignored because the PDB ID list is invalid: {error}");
        form
          .download
          .update(cx, |state, cx| state.set_validation_error(error, cx));
        return;
      }
    };
    let format = form.selected_format(cx);
    let mut unique_ids = BTreeSet::new();
    let requests = ids
      .into_iter()
      .filter(|pdb_id| unique_ids.insert(pdb_id.clone()))
      .map(|pdb_id| RcsbBatchDownloadRequest {
        destination: download_path(&workspace_root, &pdb_id, format),
        id: pdb_id,
        format,
      })
      .collect::<Vec<_>>();
    let total_items = requests.len();
    let mut task = match submit_downloads(&self.tasks, requests) {
      Ok(task) => task,
      Err(error) => {
        log::error!("failed to submit RCSB download task: {error}");
        form.download.update(cx, |state, cx| state.fail(error.to_string(), cx));
        return;
      }
    };
    log::info!(
      "RCSB batch download task submitted: id={}, items={total_items}",
      task.id()
    );

    form.pdb_id.update(cx, |state, cx| state.set_disabled(true, cx));
    form.format.update(cx, |state, cx| state.set_disabled(true, cx));
    form.submit.update(cx, |state, cx| state.set_disabled(true, cx));
    let initial = task.latest();
    form
      .download
      .update(cx, |state, cx| state.apply_task_snapshot(&initial, total_items, cx));

    let download_state = form.download.clone();
    let pdb_id_state = form.pdb_id.clone();
    let format_state = form.format.clone();
    let submit_state = form.submit.clone();
    let window_handle = window.window_handle();
    cx.spawn(async move |app, cx| {
      while let Some(snapshot) = task.changed().await {
        let terminal = snapshot.state.is_terminal();
        let _ = cx.update_window(window_handle, |_, window, cx| {
          let _ = app.update(cx, |this, cx| {
            download_state.update(cx, |state, cx| state.apply_task_snapshot(&snapshot, total_items, cx));
            if terminal {
              pdb_id_state.update(cx, |state, cx| state.set_disabled(false, cx));
              format_state.update(cx, |state, cx| state.set_disabled(false, cx));
              submit_state.update(cx, |state, cx| state.set_disabled(false, cx));
              if snapshot.state == TaskState::Completed {
                for output in &snapshot.outputs {
                  let TaskOutput::PersistedArtifact(artifact) = output;
                  this.open_project_document_with_window(OpenedProjectDocument::new(&artifact.path), window, cx);
                }
              }
            }
          });
        });
        if terminal {
          break;
        }
      }
    })
    .detach();
    cx.notify();
  }

  /// Handles command panel keyboard input.
  ///
  /// # Parameters
  ///
  /// * `event` is the GPUI key-down event.
  /// * `window` is used to suppress default key handling after panel input.
  /// * `cx` is notified when panel state or app state changes.
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
    }
  }

  /// Invokes one command selected from the command panel.
  /// # Parameters
  ///
  /// * `command` is the typed command selected by the controller.
  /// * `window` receives focus restoration when an immediate command closes the panel.
  /// * `cx` is used to dispatch or notify state changes.
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
          self.dispatch_command_with_window(command, window, cx);
        }
      }
    }
  }
}
