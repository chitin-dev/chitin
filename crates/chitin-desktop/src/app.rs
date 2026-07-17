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
  workspace::{ProjectTreeEntry, ProjectWorkspace, ProjectWorkspaceError},
};
use chitin_ui::themes::builtins;
use gpui::{Context, FontWeight, Render, Task, Window, div, prelude::*};

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
  /// Workspace tree directories currently loading their direct children.
  loading_project_paths: HashSet<PathBuf>,
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
      loading_project_paths: HashSet::new(),
      active_activity: ActiveActivity::Files,
    }
  }

  /// Toggles a project tree entry by filesystem path.
  ///
  /// Directory expansion is lazy: if the entry has no loaded children, this
  /// method schedules loading that directory's direct children on GPUI's
  /// background executor and applies the result back to the app state.
  pub(crate) fn toggle_project_tree_entry(&mut self, path: &Path, cx: &mut Context<Self>) {
    if let ProjectTreeToggle::LoadChildren(path) = self.toggle_project_tree_entry_state(path) {
      spawn_project_children_load(path, cx).detach();
    }
  }

  fn toggle_project_tree_entry_state(&mut self, path: &Path) -> ProjectTreeToggle {
    let Some(workspace) = self.workspace.as_mut() else {
      return ProjectTreeToggle::None;
    };

    let Some(entry) = find_project_entry_mut(&mut workspace.tree.root, path) else {
      return ProjectTreeToggle::None;
    };

    if entry.is_file() {
      return ProjectTreeToggle::None;
    }

    if self.expanded_project_paths.remove(path) {
      return ProjectTreeToggle::None;
    }

    self.expanded_project_paths.insert(path.to_path_buf());

    if entry.children.is_empty() && self.loading_project_paths.insert(path.to_path_buf()) {
      return ProjectTreeToggle::LoadChildren(path.to_path_buf());
    }

    ProjectTreeToggle::None
  }

  fn apply_project_children_load(
    &mut self,
    path: &Path,
    result: Result<Vec<ProjectTreeEntry>, ProjectWorkspaceError>,
  ) {
    self.loading_project_paths.remove(path);

    let Ok(children) = result else {
      return;
    };

    if !self.expanded_project_paths.contains(path) {
      return;
    }

    if let Some(workspace) = self.workspace.as_mut()
      && let Some(entry) = find_project_entry_mut(&mut workspace.tree.root, path)
      && entry.children.is_empty()
    {
      entry.children = children;
    }
  }
}

enum ProjectTreeToggle {
  None,
  /// Expand a directory whose direct children have not been loaded yet.
  LoadChildren(PathBuf),
}

/// Loads one directory's direct children away from the GPUI render path.
fn spawn_project_children_load(path: PathBuf, cx: &mut Context<ChitinApp>) -> Task<()> {
  cx.spawn(async move |app, cx| {
    let load_path = path.clone();
    let result = cx
      .background_executor()
      .spawn(async move { ProjectWorkspace::load_directory_children(&load_path) })
      .await;

    let _ = app.update(cx, |this, cx| {
      this.apply_project_children_load(&path, result);
      cx.notify();
    });
  })
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
      .child(render_window_bar(theme, cx))
      .child(
        div()
          .flex()
          .flex_1()
          .min_h_0()
          .child(render_activity_bar(self.active_activity, theme, cx))
          .when(self.active_activity == ActiveActivity::Files, |layout| {
            layout.child(render_project_sidebar(
              self.workspace.as_ref(),
              &self.expanded_project_paths,
              &self.loading_project_paths,
              theme,
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

    let toggle = app.toggle_project_tree_entry_state(&entry_path);

    assert!(app.expanded_project_paths.contains(&entry_path));
    assert!(app.loading_project_paths.contains(&entry_path));

    let ProjectTreeToggle::LoadChildren(load_path) = toggle else {
      return Err("directory toggle should request lazy child loading".into());
    };
    assert_eq!(load_path, entry_path);

    let children = ProjectWorkspace::load_directory_children(&load_path)?;
    app.apply_project_children_load(&load_path, Ok(children));

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
