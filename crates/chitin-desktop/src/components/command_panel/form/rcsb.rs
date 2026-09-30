//! RCSB structure-download form rendered inside the command panel.

use chitin_databases::providers::rcsb::StructureFormat;

use chitin_ui::{
  composite::select_item::IconSelectItem,
  primitive::progress::{Progress, ProgressLabel},
};
use gpui_kit::component::theme::ThemeColor;

use gpui::{
  AppContext, Context, Div, Entity, IntoElement, ParentElement, Styled, WeakEntity, Window, div, prelude::FluentBuilder,
};
use gpui_kit::component::{
  Disableable as _, IndexPath,
  button::{Button, ButtonVariant, ButtonVariants as _},
  input::{Input, InputState},
  select::{Select, SelectState},
};

use crate::{
  app::ChitinApp,
  tasks::{TaskSnapshot, TaskState},
};

/// Persistent primitive state for the RCSB download form.
#[derive(Clone)]
pub(crate) struct RcsbFormPanel {
  /// PDB identifier input state.
  pub(crate) pdb_id: Entity<InputState>,
  /// Structure format selector state.
  pub(crate) format: Entity<SelectState<Vec<IconSelectItem>>>,
  /// Background-download status and progress.
  pub(crate) download: Entity<RcsbDownloadState>,
}

/// Visible state for the active RCSB download.
#[derive(Clone, Debug, Default)]
pub(crate) struct RcsbDownloadState {
  /// One-based index of the active download in the batch.
  pub(crate) current_index: usize,
  /// Number of downloads in the active batch.
  pub(crate) total_items: usize,
  /// Identifier of the active download.
  pub(crate) current_id: Option<String>,
  /// Download completion percentage.
  pub(crate) progress: f32,
  /// Whether a download is currently running.
  pub(crate) active: bool,
  /// Whether the most recent background download was cancelled.
  pub(crate) cancelled: bool,
  /// Whether the response did not provide a total size.
  pub(crate) indeterminate: bool,
  /// Whether the completed track is animating its final fill.
  pub(crate) finishing: bool,
  /// Percentage at which the completion animation starts.
  pub(crate) finishing_from: f32,
  /// Identity shared by the indeterminate and finishing animation phases.
  pub(crate) animation_id: String,
  /// Monotonic counter used to restart progress animations for each stage.
  animation_generation: u64,
  /// Bytes received for an indeterminate download.
  pub(crate) received_bytes: u64,
  /// Error message from the most recent failed download.
  pub(crate) error: Option<String>,
}

impl RcsbDownloadState {
  /// Resets the download state before opening the singleton form.
  pub(crate) fn reset(&mut self, cx: &mut Context<Self>) {
    self.current_index = 0;
    self.total_items = 0;
    self.current_id = None;
    self.progress = 0.0;
    self.active = false;
    self.cancelled = false;
    self.indeterminate = false;
    self.finishing = false;
    self.finishing_from = 0.0;
    self.animation_id.clear();
    self.received_bytes = 0;
    self.error = None;
    cx.notify();
  }

  /// Applies one task-center snapshot to the download presentation state.
  pub(crate) fn apply_task_snapshot(&mut self, snapshot: &TaskSnapshot, fallback_total: usize, cx: &mut Context<Self>) {
    match snapshot.state {
      TaskState::Queued => self.start_queue(fallback_total),
      TaskState::Running => self.apply_running_snapshot(snapshot, fallback_total),
      TaskState::Completed => {
        self.apply_running_snapshot(snapshot, fallback_total);
        self.finish(cx);
        return;
      }
      TaskState::Failed => {
        self.apply_running_snapshot(snapshot, fallback_total);
        self.fail(
          snapshot
            .error
            .clone()
            .unwrap_or_else(|| "Background download failed".to_string()),
          cx,
        );
        return;
      }
      TaskState::Cancelled => {
        self.apply_running_snapshot(snapshot, fallback_total);
        self.cancel(cx);
        return;
      }
    }
    cx.notify();
  }

  /// Initializes the visible state for a newly queued batch.
  fn start_queue(&mut self, total: usize) {
    if self.active {
      return;
    }
    self.current_index = 1;
    self.total_items = total;
    self.current_id = None;
    self.progress = 0.0;
    self.active = true;
    self.cancelled = false;
    self.indeterminate = true;
    self.finishing = false;
    self.finishing_from = 0.0;
    self.animation_generation = self.animation_generation.wrapping_add(1);
    self.animation_id = format!("rcsb-progress-{}", self.animation_generation);
    self.received_bytes = 0;
    self.error = None;
  }

  /// Projects task-center progress into the form's per-file presentation state.
  fn apply_running_snapshot(&mut self, snapshot: &TaskSnapshot, fallback_total: usize) {
    self.start_queue(fallback_total);
    let Some(progress) = snapshot.progress.as_ref() else {
      return;
    };
    if self.current_index != progress.stage_index {
      self.animation_generation = self.animation_generation.wrapping_add(1);
      self.animation_id = format!("rcsb-progress-{}", self.animation_generation);
      self.progress = 0.0;
    }
    self.current_index = progress.stage_index;
    self.total_items = if progress.stage_count > 0 {
      progress.stage_count
    } else {
      fallback_total
    };
    self.current_id = progress.stage_label.clone();
    self.received_bytes = progress.completed;
    self.active = true;
    self.cancelled = false;
    self.finishing = false;
    self.finishing_from = 0.0;
    if let Some(total) = progress.total.filter(|total| *total > 0) {
      self.progress = progress
        .completed
        .saturating_mul(10_000)
        .checked_div(total)
        .unwrap_or_default() as f32
        / 100.0;
      self.indeterminate = false;
    } else {
      self.indeterminate = true;
    }
  }

  /// Clears the most recent validation or download error.
  pub(crate) fn clear_error(&mut self, cx: &mut Context<Self>) {
    if self.error.take().is_some() {
      cx.notify();
    }
  }

  /// Stores a validation error without starting a download.
  pub(crate) fn set_validation_error(&mut self, error: String, cx: &mut Context<Self>) {
    self.error = Some(error);
    cx.notify();
  }

  /// Starts the short completion animation after a successful download.
  pub(crate) fn finish(&mut self, cx: &mut Context<Self>) {
    let starting_progress = self.progress;
    self.progress = 100.0;
    self.active = false;
    self.cancelled = false;
    self.indeterminate = false;
    self.finishing = true;
    // An indeterminate track is 30% wide, so preserve that visible amount
    // while it transitions into the completed track.
    self.finishing_from = starting_progress.max(30.0);
    cx.notify();
  }

  /// Marks the download as failed while keeping the form open.
  pub(crate) fn fail(&mut self, error: String, cx: &mut Context<Self>) {
    self.active = false;
    self.cancelled = false;
    self.progress = 0.0;
    self.indeterminate = false;
    self.finishing = false;
    self.finishing_from = 0.0;
    self.received_bytes = 0;
    self.error = Some(error);
    cx.notify();
  }

  /// Clears active progress after cooperative task cancellation.
  fn cancel(&mut self, cx: &mut Context<Self>) {
    self.active = false;
    self.cancelled = true;
    self.progress = 0.0;
    self.indeterminate = false;
    self.finishing = false;
    self.finishing_from = 0.0;
    self.received_bytes = 0;
    self.error = None;
    cx.notify();
  }
}

/// Builds a progress label that reflects cancellation before stale batch metadata.
fn download_progress_label(download: &RcsbDownloadState) -> String {
  if download.cancelled {
    return "Download cancelled".to_string();
  }
  if download.total_items > 0 {
    let current = download.current_index.max(1);
    let id = download.current_id.as_deref().unwrap_or("structure");
    if download.indeterminate {
      format!(
        "Downloading {current}/{} · {id} · {} KB",
        download.total_items,
        download.received_bytes / 1024
      )
    } else {
      format!("Downloading {current}/{} · {id}", download.total_items)
    }
  } else if download.indeterminate {
    format!("Downloading… {} KB", download.received_bytes / 1024)
  } else {
    "Download progress".to_string()
  }
}

impl RcsbFormPanel {
  /// Creates an empty RCSB form with PDB as the default format.
  pub(crate) fn new(window: &mut Window, cx: &mut Context<crate::app::ChitinApp>) -> Self {
    let format = cx.new(|cx| {
      SelectState::new(
        vec![
          IconSelectItem::new(StructureFormat::Pdb.id(), StructureFormat::Pdb.label()),
          IconSelectItem::new(StructureFormat::Mmcif.id(), StructureFormat::Mmcif.label()),
        ],
        Some(IndexPath::default()),
        window,
        cx,
      )
    });

    Self {
      pdb_id: cx.new(|cx| InputState::new(window, cx).placeholder("Enter PDB IDs, e.g. 1YTH, 4HHB")),
      format,
      download: cx.new(|_| RcsbDownloadState::default()),
    }
  }

  /// Resets form fields and progress before opening the singleton panel.
  pub(crate) fn reset(&self, window: &mut Window, cx: &mut Context<crate::app::ChitinApp>) {
    if self.download.read(cx).active {
      return;
    }
    self.pdb_id.update(cx, |state, cx| state.set_value("", window, cx));
    self.format.update(cx, |state, cx| {
      state.set_selected_index(Some(IndexPath::default()), window, cx);
    });
    self.download.update(cx, RcsbDownloadState::reset);
  }

  /// Returns the selected output format.
  pub(crate) fn selected_format(&self, cx: &gpui::App) -> StructureFormat {
    match self.format.read(cx).selected_value().map(|value| value.as_ref()) {
      Some("mmcif") => StructureFormat::Mmcif,
      _ => StructureFormat::Pdb,
    }
  }

  /// Renders the PDB ID input, format selector, progress, and submit action.
  ///
  /// `app` receives the download submission triggered by the submit button.
  pub(crate) fn render(&self, theme: ThemeColor, app: WeakEntity<ChitinApp>, cx: &mut Context<ChitinApp>) -> Div {
    let download = self.download.read(cx);
    let progress_label = download_progress_label(download);

    div()
      .flex()
      .flex_col()
      .gap_3()
      .p_4()
      .child(
        div()
          .text_sm()
          .text_color(theme.foreground)
          .child("Download RCSB Structure"),
      )
      .child(div().text_xs().text_color(theme.muted_foreground).child("PDB ID"))
      .child(Input::new(&self.pdb_id).disabled(download.active).w_full())
      .child(div().text_xs().text_color(theme.muted_foreground).child("Format"))
      .child(
        Select::new(&self.format)
          .accessibility_label("RCSB structure format")
          .disabled(download.active)
          .w_full(),
      )
      .child(if download.indeterminate {
        Progress::new(download.progress)
          .animation_id(download.animation_id.clone())
          .indeterminate()
          .label(ProgressLabel::new(progress_label))
          .into_any_element()
      } else if download.finishing {
        Progress::new(download.progress)
          .animation_id(download.animation_id.clone())
          .finishing_from(download.finishing_from)
          .label(ProgressLabel::new(format!(
            "Downloaded {}/{} · {} KB",
            download.current_index,
            download.total_items,
            download.received_bytes / 1024
          )))
          .into_any_element()
      } else {
        Progress::new(download.progress)
          .animation_id(download.animation_id.clone())
          .label(ProgressLabel::new(progress_label))
          .into_any_element()
      })
      .when_some(download.error.as_ref(), |element, error| {
        element.child(div().text_xs().text_color(theme.danger).child(error.clone()))
      })
      .child(
        Button::new("rcsb-submit")
          .with_variant(ButtonVariant::Primary)
          .disabled(download.active)
          .w_full()
          .on_click(move |_, window, cx| {
            let _ = app.update(cx, |this, cx| this.submit_rcsb_form(window, cx));
          })
          .child(if download.active { "Downloading…" } else { "Download" }),
      )
  }
}

#[cfg(test)]
mod tests {
  use super::{RcsbDownloadState, download_progress_label};

  #[test]
  fn cancelled_download_should_render_cancelled_label() {
    let state = RcsbDownloadState {
      cancelled: true,
      current_index: 2,
      total_items: 5,
      current_id: Some("1CRN".to_string()),
      ..RcsbDownloadState::default()
    };

    assert_eq!(download_progress_label(&state), "Download cancelled");
  }
}
