//! Desktop workspace tree renderer.
//!
//! The generic tree data model lives in `chitin-ui`, but this renderer is
//! intentionally desktop-specific: it uses workspace SVG assets and dispatches
//! row clicks into `ChitinApp` so directories can be loaded lazily.

use std::{collections::HashSet, ops::Range, path::PathBuf, rc::Rc};

use chitin_core::workspace::{ProjectTreeEntry, ProjectTreeEntryKind};
use chitin_ui::themes::UIThemes;
use gpui::{
  Context, InteractiveElement, IntoElement, MouseButton, ParentElement, Styled, div, prelude::*,
  px, svg, uniform_list,
};

use crate::app::ChitinApp;

const TREE_INDENT: f32 = 12.0;
const TREE_ICON_SIZE_VALUE: f32 = 16.0;
const TREE_ROW_HEIGHT: gpui::Pixels = px(24.0);
const TREE_ICON_SIZE: gpui::Pixels = px(TREE_ICON_SIZE_VALUE);

const FILE_ICON: &str = "icons/workspace/catppuccin-default-file.svg";
const FOLDER_CLOSED_ICON: &str = "icons/workspace/catppuccin-default-folder-close.svg";
const FOLDER_OPEN_ICON: &str = "icons/workspace/catppuccin-default-folder-open.svg";
const LIST_CLOSED_ICON: &str = "icons/workspace/codicon-list-close.svg";
const LIST_OPEN_ICON: &str = "icons/workspace/codicon-list-open.svg";

/// A desktop-specific row in the flattened workspace tree.
///
/// The workspace tree is flattened before it is handed to GPUI's virtual list.
/// This keeps rendering bounded by the viewport while preserving Chitin's
/// desktop-specific icons and `PathBuf` click payloads outside `chitin-ui`.
#[derive(Clone)]
enum WorkspaceTreeRow {
  /// A real filesystem entry row.
  Entry {
    /// Original filesystem path used for non-lossy click handling.
    path: PathBuf,
    /// Display name shown in the tree.
    name: String,
    /// Whether the entry represents a directory or file.
    kind: ProjectTreeEntryKind,
    /// Whether this directory is currently expanded.
    expanded: bool,
    /// Zero-based nesting level used for indentation.
    depth: usize,
  },
  /// A non-interactive status row, such as a loading placeholder.
  Message {
    /// Status text shown in the row.
    label: String,
    /// Zero-based nesting level used for indentation.
    depth: usize,
  },
}

/// Renders a workspace tree rooted at `root`.
///
/// `expanded_paths` controls which directory rows display their loaded children.
/// `loading_paths` controls which directory rows display a loading placeholder.
/// Clicking a row delegates to `ChitinApp::toggle_project_tree_entry` with the
/// original [`PathBuf`], which avoids lossy string round trips for non-UTF-8
/// filesystem paths.
pub fn render_workspace_tree(
  root: &ProjectTreeEntry,
  expanded_paths: &HashSet<PathBuf>,
  loading_paths: &HashSet<PathBuf>,
  theme: UIThemes,
  cx: &mut Context<ChitinApp>,
) -> impl IntoElement {
  let rows = Rc::new(visible_workspace_tree_rows(
    root,
    expanded_paths,
    loading_paths,
  ));
  let row_count = rows.len();

  div().flex().flex_1().min_h_0().w_full().child(
    uniform_list(
      "project-workspace-tree-rows",
      row_count,
      cx.processor(move |_, range: Range<usize>, _, cx| {
        range
          .filter_map(|index| rows.get(index).cloned())
          .map(|row| render_workspace_row(row, theme, cx))
          .collect::<Vec<_>>()
      }),
    )
    .size_full(),
  )
}

fn visible_workspace_tree_rows(
  entry: &ProjectTreeEntry,
  expanded_paths: &HashSet<PathBuf>,
  loading_paths: &HashSet<PathBuf>,
) -> Vec<WorkspaceTreeRow> {
  let mut rows = Vec::new();
  collect_visible_workspace_tree_rows(entry, expanded_paths, loading_paths, 0, &mut rows);
  rows
}

/// Collects only rows that are visible under the current expansion state.
///
/// Collapsed descendants are skipped. Expanded directories that are still
/// loading receive a placeholder row instead of stale or empty children.
fn collect_visible_workspace_tree_rows(
  entry: &ProjectTreeEntry,
  expanded_paths: &HashSet<PathBuf>,
  loading_paths: &HashSet<PathBuf>,
  depth: usize,
  rows: &mut Vec<WorkspaceTreeRow>,
) {
  let expanded = expanded_paths.contains(&entry.path);
  let loading = loading_paths.contains(&entry.path);
  rows.push(WorkspaceTreeRow::Entry {
    path: entry.path.clone(),
    name: entry.name.clone(),
    kind: entry.kind,
    expanded,
    depth,
  });

  if expanded && loading {
    rows.push(WorkspaceTreeRow::Message {
      label: "Loading...".to_string(),
      depth: depth + 1,
    });
  }

  if expanded && !loading {
    for child in &entry.children {
      collect_visible_workspace_tree_rows(child, expanded_paths, loading_paths, depth + 1, rows);
    }
  }
}

fn render_workspace_row(
  row: WorkspaceTreeRow,
  theme: UIThemes,
  cx: &mut Context<ChitinApp>,
) -> gpui::Div {
  match row {
    WorkspaceTreeRow::Entry { .. } => render_workspace_entry_row(row, theme, cx),
    WorkspaceTreeRow::Message { label, depth } => {
      render_workspace_tree_message(label, theme, depth)
    }
  }
}

/// Renders one interactive filesystem row in the virtual workspace tree.
///
/// The row must occupy the full available width so hover backgrounds and click
/// hitboxes span the sidebar instead of shrinking to icon and label content.
fn render_workspace_entry_row(
  row: WorkspaceTreeRow,
  theme: UIThemes,
  cx: &mut Context<ChitinApp>,
) -> gpui::Div {
  let WorkspaceTreeRow::Entry {
    path,
    name,
    kind,
    expanded,
    depth,
  } = row
  else {
    return div().hidden();
  };

  let is_dir = kind == ProjectTreeEntryKind::Directory;
  let item_icon = if is_dir {
    if expanded {
      FOLDER_OPEN_ICON
    } else {
      FOLDER_CLOSED_ICON
    }
  } else {
    FILE_ICON
  };
  let list_icon = if expanded {
    LIST_OPEN_ICON
  } else {
    LIST_CLOSED_ICON
  };

  let mut row = div()
    .flex()
    .items_center()
    // Keep hover background and pointer hitbox full-width inside uniform_list.
    .w_full()
    .h(TREE_ROW_HEIGHT)
    .pl(px(depth as f32 * TREE_INDENT))
    .pr_2()
    .gap_1()
    .text_xs()
    .cursor_pointer()
    .text_color(theme.text.secondary)
    .hover(move |style| {
      style
        .bg(theme.background.hover)
        .text_color(theme.text.primary)
    })
    .child(
      div()
        .flex()
        .items_center()
        .justify_center()
        .size(TREE_ICON_SIZE)
        .when(is_dir, |toggle| {
          toggle.child(
            svg()
              .path(list_icon)
              .size(TREE_ICON_SIZE)
              .text_color(theme.text.secondary),
          )
        }),
    )
    .child(
      div()
        .flex()
        .items_center()
        .justify_center()
        .size(TREE_ICON_SIZE)
        .child(
          svg()
            .path(item_icon)
            .size(TREE_ICON_SIZE)
            .text_color(theme.text.secondary),
        ),
    )
    .child(
      div()
        .flex_1()
        .min_w_0()
        .truncate()
        .text_color(theme.text.primary)
        .child(name),
    );

  row = row.on_mouse_up(
    MouseButton::Left,
    cx.listener(move |this, _, _, cx| {
      this.toggle_project_tree_entry(&path, cx);
      cx.notify();
    }),
  );

  row
}

/// Renders one non-interactive status row in the virtual workspace tree.
///
/// Message rows share the same fixed height as entry rows so `uniform_list`
/// can virtualize them with the same measurement.
fn render_workspace_tree_message(
  message: impl Into<gpui::SharedString>,
  theme: UIThemes,
  depth: usize,
) -> gpui::Div {
  div()
    .flex()
    .items_center()
    // Match entry row width so status-row backgrounds align with tree rows.
    .w_full()
    .h(TREE_ROW_HEIGHT)
    .pl(px(depth as f32 * TREE_INDENT + TREE_ICON_SIZE_VALUE * 2.0))
    .pr_2()
    .text_xs()
    .text_color(theme.text.disabled)
    .child(message.into())
}
