//! Main document area for opened workspace files.
//!
//! This module owns desktop-specific document presentation. It intentionally
//! starts with a placeholder view so file-tree activation can be wired before a
//! real editor, molecule viewer, or structure viewer is introduced.

use std::path::{Path, PathBuf};

use chitin_core::WorkspaceSummary;
use chitin_ui::themes::UIThemes;
use gpui::{FontWeight, IntoElement, div, prelude::*};

use crate::components::activity_bar::ActiveActivity;

/// File document currently opened from the project workspace tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenedProjectDocument {
  /// Original filesystem path of the opened file.
  pub path: PathBuf,
  /// Display name shown in the document placeholder.
  pub title: String,
}

impl OpenedProjectDocument {
  /// Creates an opened document descriptor from a filesystem path.
  ///
  /// # Parameters
  ///
  /// `path` is the workspace file path opened from the project tree.
  ///
  /// # Returns
  ///
  /// An [`OpenedProjectDocument`] with the original path and a display title
  /// derived from the final path component.
  pub fn new(path: &Path) -> Self {
    Self {
      path: path.to_path_buf(),
      title: path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string()),
    }
  }
}

/// Renders the main workbench document area.
///
/// When a file is opened from the workspace tree, this renders a minimal
/// placeholder document. Otherwise it keeps the existing product placeholder so
/// the main area is still useful before any file is selected.
///
/// # Parameters
///
/// `document` is the currently opened project document, if any.
///
/// `summary` provides product placeholder text for empty states.
///
/// `active_activity` identifies the selected workbench activity for empty
/// placeholder copy.
///
/// `theme` supplies colors for the document area.
///
/// # Returns
///
/// A GPUI element for the main document region.
pub fn render_document_area(
  document: Option<&OpenedProjectDocument>,
  summary: &WorkspaceSummary,
  active_activity: ActiveActivity,
  theme: UIThemes,
) -> impl IntoElement {
  match document {
    Some(document) => render_opened_document(document, theme),
    None => render_empty_document_area(summary, active_activity, theme),
  }
}

/// Renders a placeholder for a file opened from the workspace tree.
///
/// # Parameters
///
/// `document` is the opened file descriptor to display.
///
/// `theme` supplies colors for the placeholder document surface.
///
/// # Returns
///
/// A GPUI `Div` containing the placeholder opened-document view.
fn render_opened_document(document: &OpenedProjectDocument, theme: UIThemes) -> gpui::Div {
  div()
    .flex()
    .flex_col()
    .flex_1()
    .min_w_0()
    .h_full()
    .bg(theme.background.primary)
    .child(
      div()
        .flex()
        .items_center()
        .h_10()
        .px_4()
        .border_b_1()
        .border_color(theme.border.primary)
        .bg(theme.background.secondary)
        .child(
          div()
            .min_w_0()
            .truncate()
            .text_sm()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme.text.primary)
            .child(document.title.clone()),
        ),
    )
    .child(
      div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .p_8()
        .gap_3()
        .child(
          div()
            .text_lg()
            .font_weight(FontWeight::SEMIBOLD)
            .child("Placeholder document"),
        )
        .child(
          div()
            .text_sm()
            .text_color(theme.text.secondary)
            .child(document.path.display().to_string()),
        ),
    )
}

/// Renders the main area before a workspace file is opened.
///
/// # Parameters
///
/// `summary` provides product name and focus text.
///
/// `active_activity` selects the placeholder heading and description.
///
/// `theme` supplies colors for the empty document area.
///
/// # Returns
///
/// A GPUI `Div` containing the empty workbench placeholder.
fn render_empty_document_area(
  summary: &WorkspaceSummary,
  active_activity: ActiveActivity,
  theme: UIThemes,
) -> gpui::Div {
  div()
    .flex()
    .flex_col()
    .flex_1()
    .h_full()
    .p_8()
    .gap_4()
    .child(
      div()
        .text_3xl()
        .font_weight(FontWeight::SEMIBOLD)
        .child(summary.product_name),
    )
    .child(
      div()
        .text_lg()
        .text_color(theme.text.secondary)
        .child(summary.focus),
    )
    .child(
      div()
        .mt_6()
        .p_4()
        .rounded_md()
        .border_1()
        .border_color(theme.border.primary)
        .bg(theme.background.secondary)
        .child(
          div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
              div()
                .text_lg()
                .font_weight(FontWeight::SEMIBOLD)
                .child(active_activity.title()),
            )
            .child(
              div()
                .text_sm()
                .text_color(theme.text.secondary)
                .child(active_activity.description()),
            ),
        ),
    )
}
