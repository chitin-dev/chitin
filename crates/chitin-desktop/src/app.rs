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

  /// Toggles a project tree entry by filesystem path.
  ///
  /// Directory expansion is lazy: if the entry has no loaded children, this
  /// method asks `chitin-core` to load only that directory's direct children.
  pub(crate) fn toggle_project_tree_entry(&mut self, path: &Path) {
    let Some(workspace) = self.workspace.as_mut() else {
      return;
    };

    let Some(entry) = find_project_entry_mut(&mut workspace.tree.root, path) else {
      return;
    };

    if entry.is_file() {
      return;
    }

    if self.expanded_project_paths.remove(path) {
      return;
    }

    self.expanded_project_paths.insert(path.to_path_buf());

    if entry.children.is_empty()
      && let Ok(children) = ProjectWorkspace::load_directory_children(path)
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

#[cfg(test)]
mod tests {
  use std::{
    error::Error,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
  };

  use super::*;

  struct TestProject {
    root: PathBuf,
  }

  impl TestProject {
    fn new(name: &str) -> Result<Self, Box<dyn Error>> {
      let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
      let root =
        std::env::temp_dir().join(format!("chitin-{name}-{}-{timestamp}", std::process::id()));

      fs::create_dir(&root)?;

      Ok(Self { root })
    }

    fn path(&self) -> &Path {
      &self.root
    }
  }

  impl Drop for TestProject {
    fn drop(&mut self) {
      let _ = fs::remove_dir_all(&self.root);
    }
  }

  #[cfg(unix)]
  fn non_utf8_name() -> OsString {
    use std::os::unix::ffi::OsStringExt;

    OsString::from_vec(b"non-utf8-\xFF".to_vec())
  }

  #[cfg(unix)]
  #[test]
  fn display_path_string_should_not_be_used_as_project_tree_id() {
    let path = PathBuf::from(non_utf8_name());
    let displayed = path.display().to_string();

    assert_ne!(PathBuf::from(displayed), path);
  }

  #[cfg(unix)]
  #[test]
  fn toggle_project_tree_entry_should_support_non_utf8_paths() -> Result<(), Box<dyn Error>> {
    let project = TestProject::new("non-utf8-toggle")?;
    let child_dir = project.path().join(non_utf8_name());
    fs::create_dir(&child_dir)?;
    fs::write(child_dir.join("child.txt"), "")?;

    let mut app = ChitinApp::new(Some(project.path().to_path_buf()));
    let workspace = app.workspace.as_ref().ok_or("workspace should open")?;
    let entry_path = workspace
      .tree
      .root
      .children
      .iter()
      .find(|entry| entry.path == child_dir)
      .map(|entry| entry.path.clone())
      .ok_or("non-UTF-8 child directory should be present")?;

    app.toggle_project_tree_entry(&entry_path);

    assert!(app.expanded_project_paths.contains(&entry_path));

    let workspace = app.workspace.as_ref().ok_or("workspace should stay open")?;
    let entry = workspace
      .tree
      .root
      .children
      .iter()
      .find(|entry| entry.path == entry_path)
      .ok_or("expanded non-UTF-8 directory should stay present")?;

    assert_eq!(entry.children.len(), 1);
    assert_eq!(entry.children[0].name, "child.txt");

    Ok(())
  }
}
