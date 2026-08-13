//! Desktop command panel rendering and input handling.

use std::rc::Rc;
use std::sync::{
  Arc, Mutex,
  atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
};
use std::time::Duration;

mod controller;
mod form;

pub(crate) use controller::CommandPanelController;
use controller::{CommandPanelEvent, CommandPanelMode};
use form::rcsb::{RcsbDownloadState, RcsbFormPanel, download_path};

use chitin_command::{ApplicationCommand, ChitinCommand, CommandInvocationKind};
use chitin_databases::{
  Client, ClientConfig,
  providers::rcsb::{PdbId, RcsbBatchDownloadEvent, RcsbBatchDownloadRequest},
};
use chitin_ui::{
  composite::quickpick::{QuickPickItem, QuickPickOverlay, QuickPickSearchInput, render_quick_pick_overlay},
  primitive::input::text::{TextInputEvent, TextInputState},
  themes::UIThemes,
};
use gpui::{
  App, Context, Div, Entity, InteractiveElement, KeyDownEvent, ParentElement, Styled, Subscription, WeakEntity, Window,
  div,
};

use crate::app::ChitinApp;

/// Callback invoked when a rendered command row is selected.
type CommandPanelSelectHandler = dyn for<'a, 'b> Fn(usize, &'a mut Window, &'b mut App);

/// Failure encountered while downloading or persisting an RCSB structure.
#[derive(Debug, thiserror::Error)]
enum DesktopRcsbDownloadError {
  /// Tokio could not create the runtime required by the HTTP transport.
  #[error("failed to create download runtime: {0}")]
  Runtime(std::io::Error),
  /// The shared database client or local persistence step failed.
  #[error("RCSB download failed: {0}")]
  Download(#[from] chitin_databases::providers::rcsb::RcsbDownloadError),
}

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
          .mt(gpui::px(30.0))
          .w(gpui::px(420.0))
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
        TextInputEvent::Change { .. } => cx.notify(),
        _ => {}
      });
      pdb_subscription.detach();

      let select_subscription = cx.subscribe_in(&form.format, window, move |_, _, event, _, cx| {
        if matches!(
          event,
          chitin_ui::primitive::input::select::SelectInputEvent::SelectionChange { .. }
        ) {
          cx.notify();
        }
      });
      select_subscription.detach();

      let submit_subscription = cx.subscribe_in(&form.submit, window, move |this, _, event, window, cx| {
        if matches!(event, chitin_ui::primitive::button::ButtonEvent::Click) {
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
  fn submit_rcsb_form(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
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
    let Ok(ids) = PdbId::parse_many(&raw_pdb_ids) else {
      log::warn!("RCSB download ignored because the PDB ID list is invalid: {raw_pdb_ids}");
      return;
    };
    let format = form.selected_format(cx);
    let requests = ids
      .into_iter()
      .map(|pdb_id| RcsbBatchDownloadRequest {
        destination: download_path(&workspace_root, &pdb_id, format),
        id: pdb_id,
        format,
      })
      .collect::<Vec<_>>();
    let total_items = requests.len();
    log::info!(
      "RCSB batch download started: items={}, format={:?}",
      total_items,
      format
    );

    form
      .download
      .update(cx, |state, cx| state.start_item(1, total_items, String::new(), cx));
    form.pdb_id.update(cx, |state, cx| state.set_disabled(true, cx));
    form.format.update(cx, |state, cx| state.set_disabled(true, cx));
    form.submit.update(cx, |state, cx| state.set_disabled(true, cx));

    let progress = Arc::new(AtomicU64::new(0));
    let received_bytes = Arc::new(AtomicU64::new(0));
    let has_total = Arc::new(AtomicBool::new(false));
    let active = Arc::new(AtomicBool::new(true));
    let background_progress = progress.clone();
    let background_received_bytes = received_bytes.clone();
    let background_has_total = has_total.clone();
    let background_active = active.clone();
    let batch_index = Arc::new(AtomicUsize::new(0));
    let batch_total = Arc::new(AtomicUsize::new(total_items));
    let current_id = Arc::new(Mutex::new(String::new()));
    let background_batch_index = batch_index.clone();
    let background_batch_total = batch_total.clone();
    let background_current_id = current_id.clone();
    let download_task = cx.background_executor().spawn(async move {
      let result = download_rcsb_structures(requests, move |event| match event {
        RcsbBatchDownloadEvent::Started { index, total, id } => {
          background_batch_index.store(index, Ordering::Release);
          background_batch_total.store(total, Ordering::Release);
          if let Ok(mut current) = background_current_id.lock() {
            *current = id.to_string();
          }
        }
        RcsbBatchDownloadEvent::Progress {
          received, total_bytes, ..
        } => {
          background_received_bytes.store(received, Ordering::Relaxed);
          if let Some(total) = total_bytes
            && let Some(value) = received.saturating_mul(10_000).checked_div(total)
          {
            background_has_total.store(true, Ordering::Release);
            background_progress.store(value, Ordering::Relaxed);
          }
        }
        RcsbBatchDownloadEvent::Completed { .. } => {}
      });
      background_active.store(false, Ordering::Release);
      result
    });
    let download_state = form.download.clone();
    let pdb_id_state = form.pdb_id.clone();
    let format_state = form.format.clone();
    let submit_state = form.submit.clone();
    cx.spawn(async move |app, cx| {
      let result = download_task.await;
      let _ = app.update(cx, |_, cx| {
        match result {
          Ok(()) => {
            log::info!("RCSB batch download completed");
            download_state.update(cx, RcsbDownloadState::finish);
          }
          Err(error) => {
            log::error!("RCSB download failed: {error}");
            download_state.update(cx, |state, cx| state.fail(error.to_string(), cx));
          }
        }
        pdb_id_state.update(cx, |state, cx| state.set_disabled(false, cx));
        format_state.update(cx, |state, cx| state.set_disabled(false, cx));
        submit_state.update(cx, |state, cx| state.set_disabled(false, cx));
      });
    })
    .detach();
    let progress_state = form.download.clone();
    let progress_active = active;
    let progress_batch_index = batch_index;
    let progress_batch_total = batch_total;
    let progress_current_id = current_id;
    cx.spawn(async move |app, cx| {
      let mut last_logged_percent = None;
      loop {
        cx.background_executor().timer(Duration::from_millis(50)).await;
        let percent = progress.load(Ordering::Relaxed) as f32 / 100.0;
        let received = received_bytes.load(Ordering::Relaxed);
        let rounded_percent = percent.round() as u8;
        if last_logged_percent != Some(rounded_percent) {
          log::debug!(
            "RCSB download UI progress tick: percent={percent:.2}, received={received}, has_total={}, active={}",
            has_total.load(Ordering::Acquire),
            progress_active.load(Ordering::Acquire)
          );
          last_logged_percent = Some(rounded_percent);
        }
        let _ = app.update(cx, |_, cx| {
          let active = progress_state.read(cx).active;
          if active {
            let item_index = progress_batch_index.load(Ordering::Acquire);
            let item_total = progress_batch_total.load(Ordering::Acquire);
            let item_id = progress_current_id.lock().map(|id| id.clone()).unwrap_or_default();
            if item_index > 0 && progress_state.read(cx).current_index != item_index {
              progress_state.update(cx, |state, cx| state.start_item(item_index, item_total, item_id, cx));
            }
            progress_state.update(cx, |state, cx| state.set_received_bytes(received, cx));
            if has_total.load(Ordering::Acquire) {
              progress_state.update(cx, |state, cx| state.set_progress(percent, cx));
            }
          }
        });
        if !progress_active.load(Ordering::Acquire) {
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
          self.dispatch_command(command, cx);
        }
      }
    }
  }
}

/// Downloads one RCSB structure and persists it under the workspace metadata directory.
///
/// # Parameters
///
/// * `request` contains the validated identifier, selected format, and workspace root.
///
/// # Returns
///
/// The final artifact path after the file has been written successfully.
fn download_rcsb_structures(
  requests: Vec<RcsbBatchDownloadRequest>,
  report_progress: impl Fn(RcsbBatchDownloadEvent) + Send + Sync + 'static,
) -> Result<(), DesktopRcsbDownloadError> {
  let runtime = tokio::runtime::Builder::new_current_thread()
    .enable_all()
    .build()
    .map_err(DesktopRcsbDownloadError::Runtime)?;
  let download_result: Result<(), chitin_databases::providers::rcsb::RcsbDownloadError> = runtime.block_on(async {
    let client = Client::new(ClientConfig::default()).map_err(|error| {
      chitin_databases::providers::rcsb::RcsbDownloadError::Provider(
        chitin_databases::providers::rcsb::RcsbError::Transport(error),
      )
    })?;
    client
      .rcsb()
      .download_structures_to_paths(&requests, report_progress)
      .await?;
    Ok(())
  });
  download_result.map_err(DesktopRcsbDownloadError::Download)?;
  Ok(())
}
