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

    // workspace is [`Option<ProjectWorkspace>`], so the `expanded_project_paths`
    // will first be only a root of workspace
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

    // find the entry matches the path
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

  /// Applies the result of an asynchronous directory children load operation.
  ///
  /// Clears the loading state for the given path, and if the load succeeded and
  /// the directory is still expanded, updates the tree with the loaded children.
  ///
  /// # Arguments
  /// * `path` - The directory path whose children were loaded
  /// * `result` - The load result containing children or an error
  ///
  /// # Behavior
  /// * On error: Clears loading state and returns without changes
  /// * On success: Adds children to the tree if the directory is still expanded
  ///   and currently has no children (prevents duplicate updates)
  /// * If the user collapsed the directory during loading, children are discarded
  fn apply_project_children_load(
    &mut self,
    path: &Path,
    result: Result<Vec<ProjectTreeEntry>, ProjectWorkspaceError>,
  ) {
    // remove the path from [`loading_project_paths`], that means no matter the
    // loading succeeds or fails, it removes the loading path
    self.loading_project_paths.remove(path);

    let Ok(children) = result else {
      // if the loading fails, return directly
      log::error!("Failed to load path: {:?}", path);
      return;
    };

    if !self.expanded_project_paths.contains(path) {
      // if the user collapsed this path before loading completed (no matter
      // succeeded or failed), return directly
      log::debug!(
        "User collapsed this path before loading completed: {:?}",
        path
      );
      return;
    }

    if let Some(workspace) = self.workspace.as_mut()
      // get the mutable reference to `self.workspace`
      && let Some(entry) = find_project_entry_mut(&mut workspace.tree.root, path)
      // find the entry that matches the path
      && entry.children.is_empty()
    // only load entry which doesn't have children nodes, avoid duplicate loading
    {
      log::debug!("Update the expanded state: {:?}", entry.path);
      entry.children = children;
    }
  }
}

enum ProjectTreeToggle {
  None,
  /// Expand a directory whose direct children have not been loaded yet.
  LoadChildren(PathBuf),
}

/// Spawns a background task to load a directory's direct children.
///
/// This function offloads filesystem I/O to a background executor to avoid
/// blocking the UI thread. The loaded children are automatically applied
/// to the workspace tree and the UI is refreshed upon completion.
///
/// # Arguments
/// * `path` - The directory path whose children should be loaded
/// * `cx` - GPUI context for spawning the task and updating the UI
///
/// # Returns
/// A `Task<()>` that can be detached to run in the background or awaited
/// if the result is needed synchronously.
///
/// # Behavior
/// * The loading operation runs on `background_executor()`
/// * Results are applied via `apply_project_children_load`
/// * `cx.notify()` is called to refresh the UI after update
/// * Errors are logged and handled gracefully without crashing
///
/// # Example
/// ```no_run
/// if let ProjectTreeToggle::LoadChildren(path) = toggle_state {
///   spawn_project_children_load(path, cx).detach();
/// }
/// ```
///
/// # Note
/// The spawned task is typically detached to run asynchronously, allowing
/// the UI to remain responsive while directory contents are being loaded.
fn spawn_project_children_load(path: PathBuf, cx: &mut Context<ChitinApp>) -> Task<()> {
  // create an async task from current GPUI context
  cx.spawn(async move |app, cx| {
    let load_path = path.clone();
    // execute time-consuming filesystem tasks in background
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

/// Finds and returns a mutable reference to the project tree entry at the
/// specified filesystem path.
///
/// This function performs a depth-first search through the project tree,
/// starting from the given root entry, and returns a mutable reference to
/// the first entry whose `path` field matches the provided `path`.
///
/// # Parameters
/// - `entry`: A mutable reference to the root of the project tree subtree
///   to search within.
/// - `path`: The filesystem path to locate within the project tree.
///
/// # Returns
/// `Some(&mut ProjectTreeEntry)` containing a mutable reference to the
/// matching entry if found, or `None` if no entry matches the given path.
///
/// # Notes
/// - The search is recursive and visits all children of each directory entry.
/// - Only the first matching entry is returned; duplicate paths are not supported.
/// - This function does not follow symlinks or resolve path canonicalization.
fn find_project_entry_mut<'a>(
  entry: &'a mut ProjectTreeEntry,
  path: &Path,
) -> Option<&'a mut ProjectTreeEntry> {
  // using this lifecycle notation, it marks the lifecycle of returned reference
  // has same lifecycle with the `entry` because it refers the path in entry,
  // otherwise Rust doesn't know the lifecycle of returned reference.

  if entry.path == path {
    return Some(entry);
  }

  // it uses recursive function to find the path in entry
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
