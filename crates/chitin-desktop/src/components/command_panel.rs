//! Desktop command panel rendering and input handling.

use std::sync::{
  Arc,
  atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::Duration;
use std::{fs, path::PathBuf, rc::Rc};

mod controller;
mod form;

pub(crate) use controller::CommandPanelController;
use controller::{CommandPanelEvent, CommandPanelMode};
use form::rcsb::{RcsbDownloadState, RcsbFormPanel, download_path};

use chitin_command::{ApplicationCommand, ChitinCommand, CommandInvocationKind};
use chitin_databases::{
  Client, ClientConfig, TransportError,
  providers::rcsb::{PdbId, RcsbError},
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

/// A validated RCSB structure download that can outlive the form panel.
struct RcsbDownloadRequest {
  /// Workspace root that owns the hidden download directory.
  workspace_root: PathBuf,
  /// Canonical uppercase RCSB identifier.
  pdb_id: PdbId,
  /// Selected output format and destination layout.
  format: chitin_databases::providers::rcsb::StructureFormat,
}

/// Failure encountered while downloading or persisting an RCSB structure.
#[derive(Debug, thiserror::Error)]
enum RcsbDownloadError {
  /// Tokio could not create the runtime required by the HTTP transport.
  #[error("failed to create download runtime: {0}")]
  Runtime(std::io::Error),
  /// The shared database client could not initialize its transport.
  #[error("failed to create database client: {0}")]
  Client(TransportError),
  /// RCSB rejected the request or the network transfer failed.
  #[error("RCSB download failed: {0}")]
  Provider(RcsbError),
  /// The format-specific destination directory could not be created.
  #[error("failed to create '{path}': {source}")]
  CreateDirectory { path: PathBuf, source: std::io::Error },
  /// The downloaded bytes could not be written to their final path.
  #[error("failed to write '{path}': {source}")]
  WriteFile { path: PathBuf, source: std::io::Error },
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
    let Some(workspace_root) = self.workspace.as_ref().map(|workspace| workspace.root.clone()) else {
      log::warn!("RCSB download ignored because no workspace is open");
      return;
    };
    let raw_pdb_id = form.pdb_id.read(cx).text().trim().to_owned();
    let Ok(pdb_id) = PdbId::new(&raw_pdb_id) else {
      log::warn!("RCSB download ignored because '{raw_pdb_id}' is not a valid PDB ID");
      return;
    };
    let request = RcsbDownloadRequest {
      workspace_root,
      pdb_id,
      format: form.selected_format(cx),
    };
    let destination = download_path(&request.workspace_root, &request.pdb_id, request.format);
    log::info!(
      "RCSB download started: pdb_id={}, format={:?}, destination={}",
      request.pdb_id,
      request.format,
      destination.display()
    );

    form.download.update(cx, RcsbDownloadState::start);
    form.submit.update(cx, |state, cx| state.set_disabled(true, cx));

    let progress = Arc::new(AtomicU64::new(0));
    let received_bytes = Arc::new(AtomicU64::new(0));
    let has_total = Arc::new(AtomicBool::new(false));
    let active = Arc::new(AtomicBool::new(true));
    let background_progress = progress.clone();
    let background_received_bytes = received_bytes.clone();
    let background_has_total = has_total.clone();
    let background_active = active.clone();
    let download_task = cx.background_executor().spawn(async move {
      let result = download_rcsb_structure(request, move |received, total| {
        background_received_bytes.store(received, Ordering::Relaxed);
        if let Some(total) = total {
          if let Some(value) = received.saturating_mul(10_000).checked_div(total) {
            log::debug!("RCSB download progress callback: {} basis points", value);
            background_has_total.store(true, Ordering::Release);
            background_progress.store(value, Ordering::Relaxed);
          }
        } else {
          log::debug!("RCSB download progress callback: received={received} bytes, total=unknown");
        }
      });
      background_active.store(false, Ordering::Release);
      result
    });
    let download_state = form.download.clone();
    let submit_state = form.submit.clone();
    cx.spawn(async move |app, cx| {
      let result = download_task.await;
      let _ = app.update(cx, |_, cx| {
        match result {
          Ok(path) => {
            log::info!("RCSB download completed: {}", path.display());
            download_state.update(cx, RcsbDownloadState::finish);
          }
          Err(error) => {
            log::error!("RCSB download failed: {error}");
            download_state.update(cx, |state, cx| state.fail(error.to_string(), cx));
          }
        }
        submit_state.update(cx, |state, cx| state.set_disabled(false, cx));
      });
    })
    .detach();
    let progress_state = form.download.clone();
    let progress_active = active;
    cx.spawn(async move |app, cx| {
      let mut last_logged_percent = None;
      loop {
        cx.background_executor().timer(Duration::from_millis(50)).await;
        let percent = progress.load(Ordering::Relaxed) as f32 / 100.0;
        let received = received_bytes.load(Ordering::Relaxed);
        let rounded_percent = percent.round() as u8;
        if last_logged_percent != Some(rounded_percent) {
          log::debug!(
            "RCSB download UI progress tick: percent={percent:.2}, active={}",
            progress_active.load(Ordering::Acquire)
          );
          last_logged_percent = Some(rounded_percent);
        }
        let _ = app.update(cx, |_, cx| {
          let active = progress_state.read(cx).active;
          if active {
            if has_total.load(Ordering::Acquire) {
              progress_state.update(cx, |state, cx| state.set_progress(percent, cx));
            } else {
              progress_state.update(cx, |state, cx| state.set_received_bytes(received, cx));
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
fn download_rcsb_structure(
  request: RcsbDownloadRequest,
  report_progress: impl Fn(u64, Option<u64>) + Send + Sync + 'static,
) -> Result<PathBuf, RcsbDownloadError> {
  let destination = download_path(&request.workspace_root, &request.pdb_id, request.format);
  let runtime = tokio::runtime::Builder::new_current_thread()
    .enable_all()
    .build()
    .map_err(RcsbDownloadError::Runtime)?;
  let artifact = runtime.block_on(async {
    let client = Client::new(ClientConfig::default()).map_err(RcsbDownloadError::Client)?;
    client
      .rcsb()
      .download_structure_with_progress(request.pdb_id, request.format, report_progress)
      .await
      .map_err(RcsbDownloadError::Provider)
  })?;

  if let Some(directory) = destination.parent() {
    fs::create_dir_all(directory).map_err(|source| RcsbDownloadError::CreateDirectory {
      path: directory.to_path_buf(),
      source,
    })?;
  }
  fs::write(&destination, artifact.content).map_err(|source| RcsbDownloadError::WriteFile {
    path: destination.clone(),
    source,
  })?;
  Ok(destination)
}
