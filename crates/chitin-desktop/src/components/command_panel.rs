//! Desktop command panel rendering and input handling.

use std::rc::Rc;

mod controller;
mod form;

pub(crate) use controller::CommandPanelController;
use controller::{CommandPanelEvent, CommandPanelMode};
use form::rcsb::{RcsbDownloadState, RcsbFormPanel};

use chitin_command::{CommandExecutionContext, CommandId, DatabaseCommand, PortableCommand, RcsbDownloadArguments};
use chitin_databases::providers::rcsb::PdbId;
use chitin_ui::{
  composite::{
    quickpick::{QuickPickItem, QuickPickOverlay, QuickPickSearchInput, render_quick_pick_overlay},
    toast::{Toast, ToastVariant},
  },
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
  portable_command::PortableCommandSubmission,
  tasks::{TaskKind, TaskOutput, TaskState},
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
  let rcsb_form = matches!(
    controller.mode(),
    CommandPanelMode::Form(CommandId::DatabaseDownloadRcsbStructure)
  )
  .then(|| controller.rcsb_form(cx));
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
  let selected_ids = results.iter().map(|result| result.descriptor.id).collect::<Vec<_>>();
  let overlay = QuickPickOverlay {
    query: controller.query().into(),
    placeholder: "Type a command".into(),
    search_input: Some(QuickPickSearchInput::new(search_input)),
    items: results
      .iter()
      .map(|result| QuickPickItem::new(result.descriptor.title, result.shortcut))
      .collect(),
    selected_index: controller.selected_index(),
    scroll_handle: Some(controller.result_scroll_handle()),
    empty_message: "No commands found".into(),
  };
  let on_select: Rc<CommandPanelSelectHandler> = Rc::new(move |index, window, cx: &mut App| {
    let Some(id) = selected_ids.get(index).copied() else {
      return;
    };
    let _ = app.update(cx, |app, cx| {
      app.invoke_command_from_panel(id, window, cx);
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
    if !matches!(
      self.command_panel.mode(),
      CommandPanelMode::Form(CommandId::DatabaseDownloadRcsbStructure)
    ) {
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

  /// Validates the current RCSB form and dispatches a typed download command.
  fn submit_rcsb_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let Some(form) = self.command_panel.rcsb_form_if_created() else {
      return;
    };
    if form.download.read(cx).active {
      log::debug!("RCSB download submission ignored because a batch is already active");
      return;
    }
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
    self.execute_rcsb_download(
      RcsbDownloadArguments {
        ids,
        format,
        output: None,
      },
      window,
      cx,
    );
  }

  /// Executes a typed RCSB download against the active desktop workspace.
  ///
  /// # Parameters
  ///
  /// * `arguments` contains validated identifiers, format, and output override.
  /// * `window` identifies the window that receives completed documents.
  /// * `cx` submits the background task and updates form presentation state.
  ///
  /// # Returns
  ///
  /// This function returns after the background task has been submitted.
  pub(crate) fn execute_rcsb_download(
    &mut self,
    arguments: RcsbDownloadArguments,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    let Some(form) = self.command_panel.rcsb_form_if_created() else {
      return;
    };
    let Some(workspace_root) = self.workspace.as_ref().map(|workspace| workspace.root.clone()) else {
      let error = "RCSB download requires an open workspace".to_string();
      log::warn!("{error}");
      form.download.update(cx, |state, cx| state.fail(error, cx));
      return;
    };
    let execution_context = CommandExecutionContext::new(&workspace_root)
      .with_workspace_root(&workspace_root)
      .with_default_download_root(workspace_root.join(".chitin").join("download"));
    let command = PortableCommand::from(DatabaseCommand::DownloadRcsbStructure(arguments));
    let running = match self.portable_commands.submit(
      &self.tasks,
      PortableCommandSubmission::new(
        TaskKind::DatabaseDownload {
          provider: "RCSB".to_string(),
        },
        command,
        execution_context,
      ),
    ) {
      Ok(submission) => submission,
      Err(error) => {
        log::error!("failed to submit RCSB download task: {error}");
        form.download.update(cx, |state, cx| state.fail(error.to_string(), cx));
        return;
      }
    };
    let total_items = running.target_count();
    let mut task = running.into_task_handle();
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
                this.show_toast(
                  Toast::new("RCSB download complete")
                    .description(format!("Downloaded {} structure file(s).", snapshot.outputs.len()))
                    .variant(ToastVariant::Success),
                  cx,
                );
              } else if snapshot.state == TaskState::Failed {
                this.show_toast(
                  Toast::new("RCSB download failed")
                    .description(
                      snapshot
                        .error
                        .clone()
                        .unwrap_or_else(|| "The download task failed.".to_string()),
                    )
                    .variant(ToastVariant::Error),
                  cx,
                );
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
      CommandPanelEvent::Invoke(id) => self.invoke_command_from_panel(id, window, cx),
    }
  }

  /// Invokes one command selected from the command panel.
  /// # Parameters
  ///
  /// * `id` is the command identity selected by the controller.
  /// * `window` receives focus restoration when an immediate command closes the panel.
  /// * `cx` is used to dispatch or notify state changes.
  pub(crate) fn invoke_command_from_panel(&mut self, id: CommandId, window: &mut Window, cx: &mut Context<Self>) {
    let Some(descriptor) = self.command_panel.registry().descriptor_for(id) else {
      return;
    };

    if descriptor.requires_arguments {
      if self.command_panel.open_form(id) {
        cx.notify();
      }
      return;
    }

    let Some(command) = id.frontend_command_without_arguments() else {
      return;
    };
    self.command_panel.close(window, cx);
    if id == CommandId::ApplicationToggleCommandPanel {
      cx.notify();
    } else {
      self.dispatch_command_with_window(command, window, cx);
    }
  }
}
