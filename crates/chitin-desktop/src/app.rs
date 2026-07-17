//! Root GPUI application state and rendering.
//!
//! `ChitinApp` owns desktop-level state such as the active activity, currently
//! opened project workspace, and the set of expanded workspace tree paths.

use std::{
  collections::HashSet,
  path::{Path, PathBuf},
};

use chitin_core::{
  WorkspaceSummary,
  workspace::{ProjectTreeEntry, ProjectWorkspace},
};
use chitin_ui::themes::builtins;
use gpui::{Context, FontWeight, Render, Window, div, prelude::*};

use crate::components::{
  activity_bar::{ActiveActivity, render_activity_bar},
  project_sidebar::render_project_sidebar,
  window_bar::render_window_bar,
};

/// Root state object rendered into the main GPUI window.
pub struct ChitinApp {
  /// Static product summary used by the placeholder main panel.
  summary: WorkspaceSummary,
  /// Currently opened project workspace, if a path was accepted.
  workspace: Option<ProjectWorkspace>,
  /// Workspace tree directories currently expanded in the project sidebar.
  expanded_project_paths: HashSet<PathBuf>,
  /// Currently selected top-level workbench activity.
  pub(crate) active_activity: ActiveActivity,
}

impl ChitinApp {
  /// Creates app state from an optional project path.
  ///
  /// If no path is provided, the current working directory is used. Workspace
  /// loading is shallow; child directories are loaded later when expanded.
  ///
  /// Workspace loading failures are logged so they can be distinguished from
  /// the "no workspace" state.
  pub fn new(project_path: Option<PathBuf>) -> Self {
    let (_, workspace) = match project_path {
      Some(path) => match ProjectWorkspace::open(path.clone()) {
        Ok(workspace) => (Some(path), Some(workspace)),
        Err(err) => {
          eprintln!(
            "Failed to open workspace for project path '{}': {}",
            path.display(),
            err
          );
          (Some(path), None)
        }
      },
      None => {
        // when no path is provided, current working directory is used
        match std::env::current_dir() {
          Ok(path) => match ProjectWorkspace::open(path.clone()) {
            Ok(workspace) => (Some(path), Some(workspace)),
            Err(err) => {
              eprintln!(
                "Failed to open workspace for current directory '{}': {}",
                path.display(),
                err
              );
              (Some(path), None)
            }
          },
          Err(err) => {
            eprintln!(
              "Failed to determine current directory for workspace: {}",
              err
            );
            (None, None)
          }
        }
      }
    };

    let expanded_project_paths = workspace
      .as_ref()
      .map(|workspace| HashSet::from([workspace.tree.root.path.clone()]))
      .unwrap_or_default();

    Self {
      summary: WorkspaceSummary::default(),
      workspace,
      expanded_project_paths,
      active_activity: ActiveActivity::Files,
    }
  }

  /// Toggles a project tree entry by filesystem path string.
  ///
  /// Directory expansion is lazy: if the entry has no loaded children, this
  /// method asks `chitin-core` to load only that directory's direct children.
  pub(crate) fn toggle_project_tree_entry(&mut self, id: &str) {
    let path = PathBuf::from(id);

    let Some(workspace) = self.workspace.as_mut() else {
      return;
    };

    let Some(entry) = find_project_entry_mut(&mut workspace.tree.root, &path) else {
      return;
    };

    if entry.is_file() {
      return;
    }

    if self.expanded_project_paths.remove(&path) {
      return;
    }

    self.expanded_project_paths.insert(path.clone());

    if entry.children.is_empty()
      && let Ok(children) = ProjectWorkspace::load_directory_children(&path)
    {
      entry.children = children;
    }
  }
}

fn find_project_entry_mut<'a>(
  entry: &'a mut ProjectTreeEntry,
  path: &Path,
) -> Option<&'a mut ProjectTreeEntry> {
  if entry.path == path {
    return Some(entry);
  }

  entry
    .children
    .iter_mut()
    .find_map(|child| find_project_entry_mut(child, path))
}

impl Render for ChitinApp {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
    let theme = builtins::dark();

    div()
      .flex()
      .flex_col()
      .size_full()
      .bg(theme.background.primary)
      .text_color(theme.text.primary)
      .child(render_window_bar(cx))
      .child(
        div()
          .flex()
          .flex_1()
          .min_h_0()
          .child(render_activity_bar(self.active_activity, cx))
          .when(self.active_activity == ActiveActivity::Files, |layout| {
            layout.child(render_project_sidebar(
              self.workspace.as_ref(),
              &self.expanded_project_paths,
              cx,
            ))
          })
          .child(
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
                  .child(self.summary.product_name),
              )
              .child(
                div()
                  .text_lg()
                  .text_color(theme.text.secondary)
                  .child(self.summary.focus),
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
                          .child(self.active_activity.title()),
                      )
                      .child(
                        div()
                          .text_sm()
                          .text_color(theme.text.secondary)
                          .child(self.active_activity.description()),
                      ),
                  ),
              ),
          ),
      )
  }
}
