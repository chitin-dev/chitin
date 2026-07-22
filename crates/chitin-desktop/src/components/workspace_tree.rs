//! Desktop workspace tree renderer.
//!
//! The generic tree data model lives in `chitin-ui`, but this renderer is
//! intentionally desktop-specific: it uses workspace SVG assets and dispatches
//! row clicks into `ChitinApp` so directories can be loaded lazily.

use std::path::{Path, PathBuf};

use chitin_core::workspace::{
  ProjectTreeEntry, ProjectTreeEntryKind, ProjectWorkspace, ProjectWorkspaceError,
};
use chitin_ui::{
  components::tree::{
    DEFAULT_TREE_INDENT, DEFAULT_TREE_ROW_HEIGHT, TreeItemRow, TreeMessageRow, TreeRow,
    virtual_tree_rows,
  },
  themes::UIThemes,
};
use gpui::{
  Context, InteractiveElement, IntoElement, MouseButton, ParentElement, SharedString, Styled, Task,
  WeakEntity, div, prelude::*, px, svg,
};

use crate::{
  app::ChitinApp,
  components::{document_area::OpenedProjectDocument, project_sidebar::ProjectSidebarState},
};

const TREE_ICON_SIZE_VALUE: f32 = 16.0;
const TREE_ICON_SIZE: gpui::Pixels = px(TREE_ICON_SIZE_VALUE);

const FILE_ICON: &str = "icons/workspace/catppuccin-default-file.svg";
const FOLDER_CLOSED_ICON: &str = "icons/workspace/catppuccin-default-folder-close.svg";
const FOLDER_OPEN_ICON: &str = "icons/workspace/catppuccin-default-folder-open.svg";
const LIST_CLOSED_ICON: &str = "icons/workspace/codicon-list-close.svg";
const LIST_OPEN_ICON: &str = "icons/workspace/codicon-list-open.svg";

impl ChitinApp {
  /// Activates a project tree entry from pointer or keyboard input.
  ///
  /// Directory activation toggles expansion and may schedule lazy child loading.
  /// File activation opens the file in the main document area. Both paths
  /// update sidebar focus, but only files update sidebar selection because
  /// selection tracks the active document.
  pub(crate) fn activate_project_tree_entry(&mut self, path: &Path, cx: &mut Context<Self>) {
    if let ProjectTreeActivation::LoadChildren(path) = self.activate_project_tree_entry_state(path)
    {
      spawn_project_children_load(path, cx).detach();
    }
  }

  /// Applies tree activation without spawning follow-up asynchronous work.
  fn activate_project_tree_entry_state(&mut self, path: &Path) -> ProjectTreeActivation {
    let Some(kind) = self.project_tree_entry_kind(path) else {
      return ProjectTreeActivation::None;
    };

    self.project_sidebar_state.focus_entry(path);

    match kind {
      ProjectTreeEntryKind::Directory => self.toggle_project_tree_entry_state(path).into(),
      ProjectTreeEntryKind::File => {
        self.open_project_file(path);
        ProjectTreeActivation::OpenFile
      }
    }
  }

  /// Opens a workspace file in the main document area.
  fn open_project_file(&mut self, path: &Path) {
    self.project_sidebar_state.select_entry(path);
    self.active_document = Some(OpenedProjectDocument::new(path));
  }

  /// Finds the project tree entry kind for a filesystem path.
  fn project_tree_entry_kind(&self, path: &Path) -> Option<ProjectTreeEntryKind> {
    self
      .workspace
      .as_ref()
      .and_then(|workspace| find_project_entry(&workspace.tree.root, path))
      .map(|entry| entry.kind)
  }

  /// Toggles directory expansion state for one project tree entry.
  fn toggle_project_tree_entry_state(&mut self, path: &Path) -> ProjectTreeToggle {
    let Some(workspace) = self.workspace.as_mut() else {
      return ProjectTreeToggle::None;
    };

    // find the entry that matches the path
    let Some(entry) = find_project_entry_mut(&mut workspace.tree.root, path) else {
      return ProjectTreeToggle::None;
    };

    if entry.is_file() {
      return ProjectTreeToggle::None;
    }

    if self.project_sidebar_state.expanded_paths.remove(path) {
      // if move the paths from expanded paths successfully, then it means we
      // toggle the paths from expanded to collapsed.
      return ProjectTreeToggle::None;
    }

    self
      .project_sidebar_state
      .expanded_paths
      .insert(path.to_path_buf());
    // insert the toggled path into expanded path hash set
    log::debug!("Newly expanded path: {:?}", path);

    if entry.children.is_empty()
      && self
        .project_sidebar_state
        .loading_paths
        .insert(path.to_path_buf())
    {
      return ProjectTreeToggle::LoadChildren(path.to_path_buf());
    }

    ProjectTreeToggle::None
  }

  /// Applies loaded directory children back to the project workspace tree.
  ///
  /// The loading flag is always cleared first. Successful results are only
  /// inserted when the directory is still expanded and does not already have
  /// loaded children, which prevents stale async results from re-opening a
  /// collapsed directory or duplicating children after repeated toggles.
  fn apply_project_children_load(
    &mut self,
    path: &Path,
    result: Result<Vec<ProjectTreeEntry>, ProjectWorkspaceError>,
  ) {
    self.project_sidebar_state.loading_paths.remove(path);

    let Ok(children) = result else {
      log::error!("Failed to load path: {:?}", path);
      return;
    };

    if !self.project_sidebar_state.expanded_paths.contains(path) {
      log::debug!(
        "User collapsed this path before loading completed: {:?}",
        path
      );
      return;
    }

    if let Some(workspace) = self.workspace.as_mut()
      && let Some(entry) = find_project_entry_mut(&mut workspace.tree.root, path)
      && entry.children.is_empty()
    {
      log::debug!("Update the expanded state: {:?}", entry.path);
      entry.children = children;
    }
  }
}

/// Result of applying directory expansion state.
enum ProjectTreeToggle {
  /// No asynchronous work is required.
  None,
  /// Expand a directory whose direct children have not been loaded yet.
  LoadChildren(PathBuf),
}

/// Result of activating a workspace tree entry.
enum ProjectTreeActivation {
  /// No visible state change was applied.
  None,
  /// Open a file in the main document area.
  OpenFile,
  /// Expand a directory whose direct children have not been loaded yet.
  LoadChildren(PathBuf),
}

impl From<ProjectTreeToggle> for ProjectTreeActivation {
  /// Converts directory toggle results into row activation results.
  fn from(value: ProjectTreeToggle) -> Self {
    match value {
      ProjectTreeToggle::None => Self::None,
      ProjectTreeToggle::LoadChildren(path) => Self::LoadChildren(path),
    }
  }
}

/// Spawns a background task to load a directory's direct children.
///
/// This function offloads filesystem I/O to GPUI's background executor so
/// expanding a large or slow directory does not block the render thread. When
/// loading finishes, the result is applied back on the app entity and the UI is
/// notified.
///
/// # Example
///
/// ```ignore
/// if let ProjectTreeActivation::LoadChildren(path) = activation {
///   spawn_project_children_load(path, cx).detach();
/// }
/// ```
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

/// Finds a mutable project tree entry by filesystem path.
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

/// Finds an immutable project tree entry by filesystem path.
fn find_project_entry<'a>(
  entry: &'a ProjectTreeEntry,
  path: &Path,
) -> Option<&'a ProjectTreeEntry> {
  if entry.path == path {
    return Some(entry);
  }

  entry
    .children
    .iter()
    .find_map(|child| find_project_entry(child, path))
}

/// Desktop-specific payload for one workspace tree item row.
///
/// The payload deliberately stores the original [`PathBuf`] so row events never
/// round-trip through lossy display strings for non-UTF-8 filesystem paths.
#[derive(Clone, Debug, PartialEq, Eq)]
struct WorkspaceEntryRow {
  /// Original filesystem path used for non-lossy click handling.
  path: PathBuf,
  /// Display name shown in the tree.
  name: String,
  /// Whether the entry represents a directory or file.
  kind: ProjectTreeEntryKind,
  /// Whether this entry backs the active opened document.
  selected: bool,
  /// Whether this entry is focused for pointer or keyboard navigation.
  focused: bool,
}

/// Renders a workspace tree rooted at `root`.
///
/// `state` controls which directory rows are expanded, loading, selected, or
/// focused. Clicking a row delegates to
/// `ChitinApp::activate_project_tree_entry` with the original [`PathBuf`],
/// which avoids lossy string round trips for non-UTF-8 filesystem paths.
pub fn render_workspace_tree(
  root: &ProjectTreeEntry,
  state: &ProjectSidebarState,
  theme: UIThemes,
  cx: &mut Context<ChitinApp>,
) -> impl IntoElement {
  let app = cx.weak_entity();

  virtual_tree_rows(
    "project-workspace-tree-rows",
    visible_workspace_tree_rows(root, state),
    move |row, _, _| render_workspace_row(row, theme, &app),
  )
}

/// Builds the flattened workspace rows consumed by `chitin-ui`.
///
/// This adapts [`ProjectTreeEntry`] into generic [`TreeRow`] values while keeping
/// filesystem-specific identity in [`WorkspaceEntryRow`].
fn visible_workspace_tree_rows(
  entry: &ProjectTreeEntry,
  state: &ProjectSidebarState,
) -> Vec<TreeRow<WorkspaceEntryRow>> {
  let mut rows = Vec::new();
  collect_visible_workspace_tree_rows(entry, state, 0, &mut rows);
  rows
}

/// Collects only rows that are visible under the current expansion state.
///
/// Collapsed descendants are skipped. Expanded directories that are still
/// loading receive a placeholder row instead of stale or empty children.
fn collect_visible_workspace_tree_rows(
  entry: &ProjectTreeEntry,
  state: &ProjectSidebarState,
  depth: usize,
  rows: &mut Vec<TreeRow<WorkspaceEntryRow>>,
) {
  // Expansion and loading state are owned by `ChitinApp`, not by `chitin-ui`.
  let expanded = state.expanded_paths.contains(&entry.path);
  let loading = state.loading_paths.contains(&entry.path);
  let selected = state.selected_path.as_deref() == Some(entry.path.as_path());
  let focused = state.focused_path.as_deref() == Some(entry.path.as_path());
  rows.push(TreeRow::Item(TreeItemRow {
    data: WorkspaceEntryRow {
      path: entry.path.clone(),
      name: entry.name.clone(),
      kind: entry.kind,
      selected,
      focused,
    },
    expanded,
    depth,
  }));

  if expanded && loading {
    rows.push(TreeRow::Message(TreeMessageRow {
      label: "Loading...".into(),
      depth: depth + 1,
    }));
  }

  // Loaded expanded directories contribute their visible descendants.
  if expanded && !loading {
    for child in &entry.children {
      collect_visible_workspace_tree_rows(child, state, depth + 1, rows);
    }
  }
}

/// Render the workspace row according to its type.
///
/// Item rows render file and directory icons; message rows render status
/// placeholders such as loading states.
fn render_workspace_row(
  row: TreeRow<WorkspaceEntryRow>,
  theme: UIThemes,
  app: &WeakEntity<ChitinApp>,
) -> gpui::Div {
  match row {
    TreeRow::Item(row) => render_workspace_entry_row(row, theme, app),
    TreeRow::Message(TreeMessageRow { label, depth }) => {
      render_workspace_tree_message(label, theme, depth)
    }
  }
}

/// Renders one interactive filesystem row in the virtual workspace tree.
///
/// The row must occupy the full available width so hover backgrounds and click
/// hitboxes span the sidebar instead of shrinking to icon and label content.
fn render_workspace_entry_row(
  row: TreeItemRow<WorkspaceEntryRow>,
  theme: UIThemes,
  app: &WeakEntity<ChitinApp>,
) -> gpui::Div {
  let path = row.data.path;
  let name = row.data.name;
  let kind = row.data.kind;
  let selected = row.data.selected;
  let focused = row.data.focused;
  let expanded = row.expanded;
  let depth = row.depth;

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
    .h(DEFAULT_TREE_ROW_HEIGHT)
    .pl(px(depth as f32 * DEFAULT_TREE_INDENT))
    .pr_2()
    .gap_1()
    .when(selected, |row| {
      row.border_2().bg(theme.background.selection)
    })
    .when(focused, |row| {
      row.border_2().border_color(theme.border.focus)
    })
    .text_xs()
    .cursor_pointer()
    .text_color(theme.text.secondary)
    .hover(move |style| {
      if selected {
        style
          .bg(theme.background.selection)
          .text_color(theme.text.primary)
      } else {
        style
          .bg(theme.background.hover)
          .text_color(theme.text.primary)
      }
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

  row = row.on_mouse_up(MouseButton::Left, {
    let app = app.clone();
    move |_, _, cx| {
      let _ = app.update(cx, |this, cx| {
        this.activate_project_tree_entry(&path, cx);
        cx.notify();
      });
    }
  });

  row
}

/// Renders one non-interactive status row in the virtual workspace tree.
///
/// Message rows share the same fixed height as entry rows so `uniform_list`
/// can virtualize them with the same measurement.
fn render_workspace_tree_message(
  message: impl Into<SharedString>,
  theme: UIThemes,
  depth: usize,
) -> gpui::Div {
  div()
    .flex()
    .items_center()
    // Match entry row width so status-row backgrounds align with tree rows.
    .w_full()
    .h(DEFAULT_TREE_ROW_HEIGHT)
    .pl(px(
      depth as f32 * DEFAULT_TREE_INDENT + TREE_ICON_SIZE_VALUE * 2.0,
    ))
    .pr_2()
    .text_xs()
    .text_color(theme.text.disabled)
    .child(message.into())
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

  /// Temporary filesystem project used by workspace tree tests.
  struct TestProject {
    /// Root directory removed when the test helper is dropped.
    root: PathBuf,
  }

  impl TestProject {
    /// Creates an empty temporary project directory.
    fn new(name: &str) -> Result<Self, Box<dyn Error>> {
      let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
      let root =
        std::env::temp_dir().join(format!("chitin-{name}-{}-{timestamp}", std::process::id()));

      fs::create_dir(&root)?;

      Ok(Self { root })
    }

    /// Returns the temporary project root path.
    fn path(&self) -> &Path {
      &self.root
    }
  }

  impl Drop for TestProject {
    /// Removes the temporary project directory after each test.
    fn drop(&mut self) {
      let _ = fs::remove_dir_all(&self.root);
    }
  }

  /// Builds a non-UTF-8 path component for Unix filesystem tests.
  #[cfg(unix)]
  fn non_utf8_name() -> OsString {
    use std::os::unix::ffi::OsStringExt;

    OsString::from_vec(b"non-utf8-\xFF".to_vec())
  }

  /// Verifies that display strings are not safe tree identifiers.
  #[cfg(unix)]
  #[test]
  fn display_path_string_should_not_be_used_as_project_tree_id() {
    let path = PathBuf::from(non_utf8_name());
    let displayed = path.display().to_string();

    assert_ne!(PathBuf::from(displayed), path);
  }

  /// Verifies that file activation opens and selects a document.
  #[cfg(unix)]
  #[test]
  fn activate_project_tree_file_should_open_document_and_select_path() -> Result<(), Box<dyn Error>>
  {
    let project = TestProject::new("open-tree-file")?;
    let entry_path = project.path().join(non_utf8_name());
    fs::write(&entry_path, "")?;

    let mut app = ChitinApp::new(Some(project.path().to_path_buf()));
    let activation = app.activate_project_tree_entry_state(&entry_path);

    assert_eq!(
      app.project_sidebar_state.selected_path.as_deref(),
      Some(entry_path.as_path())
    );
    assert_eq!(
      app.project_sidebar_state.focused_path.as_deref(),
      Some(entry_path.as_path())
    );
    assert_eq!(
      app
        .active_document
        .as_ref()
        .map(|document| document.path.as_path()),
      Some(entry_path.as_path())
    );

    let ProjectTreeActivation::OpenFile = activation else {
      return Err("file activation should open a document".into());
    };

    Ok(())
  }

  /// Verifies that directory activation focuses without opening a document.
  #[cfg(unix)]
  #[test]
  fn activate_project_tree_directory_should_focus_without_selecting_document()
  -> Result<(), Box<dyn Error>> {
    let project = TestProject::new("activate-tree-directory")?;
    let child_dir = project.path().join(non_utf8_name());
    fs::create_dir(&child_dir)?;

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

    let activation = app.activate_project_tree_entry_state(&entry_path);

    assert_eq!(app.project_sidebar_state.selected_path, None);
    assert_eq!(app.active_document, None);
    assert_eq!(
      app.project_sidebar_state.focused_path.as_deref(),
      Some(entry_path.as_path())
    );
    assert!(
      app
        .project_sidebar_state
        .expanded_paths
        .contains(&entry_path)
    );

    let ProjectTreeActivation::LoadChildren(load_path) = activation else {
      return Err("directory activation should request lazy child loading".into());
    };
    assert_eq!(load_path, entry_path);

    Ok(())
  }

  /// Verifies that non-UTF-8 directory paths can be toggled and loaded.
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

    assert!(
      app
        .project_sidebar_state
        .expanded_paths
        .contains(&entry_path)
    );
    assert!(
      app
        .project_sidebar_state
        .loading_paths
        .contains(&entry_path)
    );

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
