//! RCSB structure-download form rendered inside the command panel.

use std::path::{Path, PathBuf};

use chitin_databases::providers::rcsb::{PdbId, StructureFormat};

use chitin_ui::{
  primitive::{
    button::{Button, ButtonState, ButtonVariant},
    input::{
      select::{
        Select, SelectContent, SelectContentPosition, SelectGroup, SelectInputState, SelectItem, SelectLabel,
        SelectOption, SelectTrigger, SelectValue,
      },
      text::{TextInput, TextInputSize, TextInputState, TextInputVariant},
    },
    progress::{Progress, ProgressLabel},
  },
  themes::UIThemes,
};
use gpui::{
  AppContext, Context, Div, Entity, IntoElement, ParentElement, SharedString, Styled, div, prelude::FluentBuilder,
};

/// Builds the final workspace download path for an RCSB structure.
pub(crate) fn download_path(workspace_root: &Path, pdb_id: &PdbId, format: StructureFormat) -> PathBuf {
  workspace_root
    .join(".chitin")
    .join("download")
    .join(match format {
      StructureFormat::Pdb => "pdb",
      StructureFormat::Mmcif => "mmcif",
    })
    .join(format.filename(pdb_id))
}

/// Persistent primitive state for the RCSB download form.
#[derive(Clone)]
pub(crate) struct RcsbFormPanel {
  /// PDB identifier input state.
  pub(crate) pdb_id: Entity<TextInputState>,
  /// Structure format selector state.
  pub(crate) format: Entity<SelectInputState>,
  /// Download action button state.
  pub(crate) submit: Entity<ButtonState>,
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
  /// Whether the response did not provide a total size.
  pub(crate) indeterminate: bool,
  /// Whether the completed track is animating its final fill.
  pub(crate) finishing: bool,
  /// Percentage at which the completion animation starts.
  pub(crate) finishing_from: f32,
  /// Identity shared by the indeterminate and finishing animation phases.
  pub(crate) animation_id: String,
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
    self.indeterminate = false;
    self.finishing = false;
    self.finishing_from = 0.0;
    self.animation_id.clear();
    self.received_bytes = 0;
    self.error = None;
    cx.notify();
  }

  /// Starts the animation for the next item in a download batch.
  pub(crate) fn start_item(&mut self, index: usize, total: usize, id: String, cx: &mut Context<Self>) {
    self.current_index = index;
    self.total_items = total;
    self.current_id = Some(id);
    self.progress = 0.0;
    self.active = true;
    self.indeterminate = true;
    self.finishing = false;
    self.finishing_from = 0.0;
    self.animation_generation = self.animation_generation.wrapping_add(1);
    self.animation_id = format!("rcsb-progress-{}", self.animation_generation);
    self.received_bytes = 0;
    self.error = None;
    cx.notify();
  }

  /// Applies a received-byte progress update.
  pub(crate) fn set_progress(&mut self, progress: f32, cx: &mut Context<Self>) {
    self.progress = progress.clamp(0.0, 100.0);
    self.indeterminate = false;
    self.finishing = false;
    self.finishing_from = 0.0;
    cx.notify();
  }

  /// Applies progress for a response whose total size is not known.
  pub(crate) fn set_received_bytes(&mut self, received_bytes: u64, cx: &mut Context<Self>) {
    self.received_bytes = received_bytes;
    self.indeterminate = true;
    cx.notify();
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
    self.progress = 0.0;
    self.indeterminate = false;
    self.finishing = false;
    self.finishing_from = 0.0;
    self.received_bytes = 0;
    self.error = Some(error);
    cx.notify();
  }
}

impl RcsbFormPanel {
  /// Creates an empty RCSB form with PDB as the default format.
  pub(crate) fn new(cx: &mut Context<crate::app::ChitinApp>) -> Self {
    let format = cx.new(|cx| {
      let mut state = SelectInputState::new(
        [
          SelectOption::new(StructureFormat::Pdb.id(), StructureFormat::Pdb.label()),
          SelectOption::new(StructureFormat::Mmcif.id(), StructureFormat::Mmcif.label()),
        ],
        cx,
      );
      state.select(StructureFormat::Pdb.id(), cx);
      state
    });

    Self {
      pdb_id: cx.new(TextInputState::new),
      format,
      submit: cx.new(ButtonState::new),
      download: cx.new(|_| RcsbDownloadState::default()),
    }
  }

  /// Resets form fields and progress before opening the singleton panel.
  pub(crate) fn reset(&self, cx: &mut Context<crate::app::ChitinApp>) {
    if self.download.read(cx).active {
      return;
    }
    self.pdb_id.update(cx, |state, cx| state.set_text("", cx));
    self.format.update(cx, |state, cx| {
      state.select(StructureFormat::Pdb.id(), cx);
      state.set_disabled(false, cx);
    });
    self.submit.update(cx, |state, cx| state.set_disabled(false, cx));
    self.download.update(cx, RcsbDownloadState::reset);
  }

  /// Returns the selected output format.
  pub(crate) fn selected_format(&self, cx: &gpui::App) -> StructureFormat {
    match self.format.read(cx).selected_id() {
      Some("mmcif") => StructureFormat::Mmcif,
      _ => StructureFormat::Pdb,
    }
  }

  /// Renders the PDB ID input, format selector, progress, and submit action.
  pub(crate) fn render(&self, theme: UIThemes, cx: &mut Context<crate::app::ChitinApp>) -> Div {
    let selected_format = self
      .format
      .read(cx)
      .selected_option()
      .map(|option| SharedString::from(option.label()));
    let download = self.download.read(cx);
    let progress_label = if download.total_items > 0 {
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
    };

    div()
      .flex()
      .flex_col()
      .gap_3()
      .p_4()
      .child(
        div()
          .text_sm()
          .text_color(theme.text.primary)
          .child("Download RCSB Structure"),
      )
      .child(div().text_xs().text_color(theme.text.secondary).child("PDB ID"))
      .child(
        TextInput::new(self.pdb_id.clone())
          .theme(theme)
          .size(TextInputSize::Medium)
          .variant(TextInputVariant::Secondary)
          .full_width(true)
          .placeholder("Enter a PDB ID, e.g. 1YTH"),
      )
      .child(div().text_xs().text_color(theme.text.secondary).child("Format"))
      .child(
        Select::new(self.format.clone())
          .theme(theme)
          .full_width(true)
          .trigger(
            SelectTrigger::new().value(SelectValue::new().placeholder(selected_format.unwrap_or_else(|| "PDB".into()))),
          )
          .content(
            SelectContent::new().position(SelectContentPosition::Popper).group(
              SelectGroup::new()
                .label(SelectLabel::new("RCSB structure format"))
                .item(SelectItem::new(StructureFormat::Pdb.id(), StructureFormat::Pdb.label()))
                .item(SelectItem::new(
                  StructureFormat::Mmcif.id(),
                  StructureFormat::Mmcif.label(),
                )),
            ),
          ),
      )
      .child(if download.indeterminate {
        Progress::new(download.progress)
          .theme(theme)
          .animation_id(download.animation_id.clone())
          .indeterminate()
          .label(ProgressLabel::new(progress_label))
          .into_any_element()
      } else if download.finishing {
        Progress::new(download.progress)
          .theme(theme)
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
          .theme(theme)
          .animation_id(download.animation_id.clone())
          .label(ProgressLabel::new(progress_label))
          .into_any_element()
      })
      .when_some(download.error.as_ref(), |element, error| {
        element.child(div().text_xs().text_color(theme.text.error).child(error.clone()))
      })
      .child(
        Button::new(self.submit.clone())
          .theme(theme)
          .variant(ButtonVariant::Primary)
          .full_width(true)
          .child(if download.active { "Downloading…" } else { "Download" }),
      )
  }
}

#[cfg(test)]
mod tests {
  use std::path::Path;

  use chitin_databases::providers::rcsb::PdbId;

  use super::{StructureFormat, download_path};

  #[test]
  fn pdb_download_path_should_use_pdb_directory_and_extension() -> Result<(), Box<dyn std::error::Error>> {
    let id = PdbId::new("1yth")?;

    let path = download_path(Path::new("/workspace"), &id, StructureFormat::Pdb);

    assert_eq!(path, Path::new("/workspace/.chitin/download/pdb/1YTH.pdb"));
    Ok(())
  }

  #[test]
  fn mmcif_download_path_should_use_mmcif_directory_and_cif_extension() -> Result<(), Box<dyn std::error::Error>> {
    let id = PdbId::new("1yth")?;

    let path = download_path(Path::new("/workspace"), &id, StructureFormat::Mmcif);

    assert_eq!(path, Path::new("/workspace/.chitin/download/mmcif/1YTH.cif"));
    Ok(())
  }
}
