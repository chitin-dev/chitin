//! Desktop command panel rendering and input handling.

mod controller;
mod form;

pub(crate) use controller::CommandPanelController;
use controller::{CommandPanelEvent, CommandPanelMode};
use form::rcsb::{RcsbDownloadState, RcsbFormPanel};

use chitin_command::{CommandExecutionContext, CommandId, DatabaseCommand, PortableCommand, RcsbDownloadArguments};
use chitin_databases::providers::rcsb::PdbId;
use chitin_ui::composite::toast::{Toast, ToastVariant};
use gpui_kit::component::theme::ThemeColor;

use gpui::{
  AppContext, Context, Div, Entity, Focusable, InteractiveElement, KeyDownEvent, ParentElement, Styled, WeakEntity,
  Window, div, prelude::FluentBuilder, px,
};
use gpui_kit::component::{
  command::{Command, CommandItem, CommandState},
  input::InputEvent,
  select::SelectEvent,
};

use crate::{
  app::ChitinApp,
  components::document_area::state::OpenedProjectDocument,
  portable_command::PortableCommandSubmission,
  tasks::{TaskKind, TaskOutput, TaskState},
};

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
  theme: ThemeColor,
  app: WeakEntity<ChitinApp>,
  cx: &mut Context<ChitinApp>,
  palette: Entity<CommandState>,
  window: &mut Window,
) -> Div {
  let rcsb_form = matches!(
    controller.mode(),
    CommandPanelMode::Form(CommandId::DatabaseDownloadRcsbStructure)
  )
  .then(|| controller.rcsb_form(window, cx));
  if let Some(form) = rcsb_form {
    let focus_handle = form.pdb_id.read(cx).focus_handle(cx);
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
          .border_color(theme.border)
          .bg(theme.popover)
          .flex_none()
          .child(form.render(theme, app.clone(), cx)),
      )
      .track_focus(&focus_handle);
  }

  let results = controller.registry().search(controller.query());
  let selected_ids = results.iter().map(|result| result.spec.id).collect::<Vec<_>>();
  let query_app = app.clone();
  let cancel_app = app.clone();
  let command = Command::new(&palette)
    // Registry scoring remains domain logic; Kit must not filter a second time.
    .filterable(false)
    .placeholder("Type a command")
    .max_h(px(360.0))
    .items(results.iter().map(|result| {
      let title = result.spec.title;
      let shortcut = result.spec.shortcut;
      CommandItem::new().label(title).child(move |_, _| {
        div()
          .flex()
          .items_center()
          .justify_between()
          .w_full()
          .child(title)
          .when_some(shortcut, |row, shortcut| {
            row.child(div().text_xs().text_color(theme.muted_foreground).child(shortcut))
          })
      })
    }))
    .on_query(move |query, _, cx| {
      let _ = query_app.update(cx, |this, cx| {
        this.command_panel.set_query(query);
        cx.notify();
      });
    })
    .on_confirm(move |index, window, cx| {
      if let Some(id) = selected_ids.get(index.row).copied() {
        let _ = app.update(cx, |this, cx| this.invoke_command_from_panel(id, window, cx));
      }
    })
    .on_cancel(move |window, cx| {
      // Kit calls cancellation while its state is leased. Closing only changes
      // host visibility/focus; it never updates the leased palette recursively.
      let _ = cancel_app.update(cx, |this, cx| {
        this.command_panel.close(window, cx);
        cx.notify();
      });
    });

  div()
    .absolute()
    .inset_0()
    .flex()
    .justify_center()
    .items_start()
    .child(div().mt(px(30.0)).w(px(520.0)).max_w_full().flex_none().child(command))
}

impl ChitinApp {
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
    let form = self.command_panel.rcsb_form(window, cx);
    if self.command_panel.take_rcsb_form_subscription() {
      let pdb_subscription = cx.subscribe_in(&form.pdb_id, window, move |this, _, event, window, cx| match event {
        InputEvent::PressEnter { .. } => this.submit_rcsb_form(window, cx),
        InputEvent::Change => {
          if let Some(form) = this.command_panel.rcsb_form_if_created() {
            form.download.update(cx, RcsbDownloadState::clear_error);
          }
          cx.notify();
        }
        _ => {}
      });
      pdb_subscription.detach();

      let select_subscription = cx.subscribe_in(&form.format, window, move |_, _, event, _, cx| {
        if matches!(event, SelectEvent::Confirm(_)) {
          cx.notify();
        }
      });
      select_subscription.detach();
    }
    if self.command_panel.take_rcsb_focus_request() {
      form.reset(window, cx);
      let focus = form.pdb_id.read(cx).focus_handle(cx);
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
    let raw_pdb_ids = form.pdb_id.read(cx).value().trim().to_owned();
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
    let initial = task.latest();
    form
      .download
      .update(cx, |state, cx| state.apply_task_snapshot(&initial, total_items, cx));

    let download_state = form.download.clone();
    let pdb_id_state = form.pdb_id.clone();
    let window_handle = window.window_handle();
    cx.spawn(async move |app, cx| {
      while let Some(snapshot) = task.changed().await {
        let terminal = snapshot.state.is_terminal();
        let _ = cx.update_window(window_handle, |_, window, cx| {
          let _ = app.update(cx, |this, cx| {
            download_state.update(cx, |state, cx| state.apply_task_snapshot(&snapshot, total_items, cx));
            if terminal {
              pdb_id_state.update(cx, |state, cx| state.set_disabled(false, cx));
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
      CommandPanelEvent::Close => {
        self.command_panel.close(window, cx);
        cx.notify();
      }
    }
  }

  /// Invokes one command selected from the command panel.
  /// # Parameters
  ///
  /// * `id` is the command identity selected by the controller.
  /// * `window` receives focus restoration when an immediate command closes the panel.
  /// * `cx` is used to dispatch or notify state changes.
  pub(crate) fn invoke_command_from_panel(&mut self, id: CommandId, window: &mut Window, cx: &mut Context<Self>) {
    let Some(spec) = self.command_panel.registry().spec_for(id) else {
      return;
    };

    if spec.requires_arguments {
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
