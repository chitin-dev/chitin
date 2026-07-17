//! Desktop workspace tree renderer.
//!
//! The generic tree data model lives in `chitin-ui`, but this renderer is
//! intentionally desktop-specific: it uses workspace SVG assets and dispatches
//! row clicks into `ChitinApp` so directories can be loaded lazily.

use std::{collections::HashSet, path::PathBuf};

use chitin_core::workspace::{ProjectTreeEntry, ProjectTreeEntryKind};
use chitin_ui::themes::builtins;
use gpui::{
  Context, InteractiveElement, IntoElement, MouseButton, ParentElement, Styled, div, prelude::*,
  px, svg,
};

use crate::app::ChitinApp;

const TREE_INDENT: f32 = 12.0;
const TREE_ROW_HEIGHT: gpui::Pixels = px(24.0);
const TREE_ICON_SIZE: gpui::Pixels = px(16.0);

const FILE_ICON: &str = "icons/workspace/catppuccin-default-file.svg";
const FOLDER_CLOSED_ICON: &str = "icons/workspace/catppuccin-default-folder-close.svg";
const FOLDER_OPEN_ICON: &str = "icons/workspace/catppuccin-default-folder-open.svg";
const LIST_CLOSED_ICON: &str = "icons/workspace/codicon-list-close.svg";
const LIST_OPEN_ICON: &str = "icons/workspace/codicon-list-open.svg";

/// Renders a workspace tree rooted at `root`.
///
/// `expanded_paths` controls which directory rows display their loaded children.
/// Clicking a row delegates to `ChitinApp::toggle_project_tree_entry` with the
/// original [`PathBuf`], which avoids lossy string round trips for non-UTF-8
/// filesystem paths.
pub fn render_workspace_tree(
  root: &ProjectTreeEntry,
  expanded_paths: &HashSet<PathBuf>,
  cx: &mut Context<ChitinApp>,
) -> impl IntoElement {
  div()
    .flex()
    .flex_col()
    .w_full()
    .child(render_workspace_entry(root, expanded_paths, 0, cx))
}

fn render_workspace_entry(
  entry: &ProjectTreeEntry,
  expanded_paths: &HashSet<PathBuf>,
  depth: usize,
  cx: &mut Context<ChitinApp>,
) -> gpui::Div {
  let theme = builtins::dark();
  let is_dir = entry.kind == ProjectTreeEntryKind::Directory;
  let expanded = expanded_paths.contains(&entry.path);
  let path = entry.path.clone();
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
        .child(entry.name.clone()),
    );

  row = row.on_mouse_up(
    MouseButton::Left,
    cx.listener(move |this, _, _, cx| {
      this.toggle_project_tree_entry(&path);
      cx.notify();
    }),
  );

  let mut node = div().flex().flex_col().w_full().child(row);

  if expanded {
    node = node.children(
      entry
        .children
        .iter()
        .map(|child| render_workspace_entry(child, expanded_paths, depth + 1, cx)),
    );
  }

  node
}
