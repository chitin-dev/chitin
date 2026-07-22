//! Project sidebar composition.
//!
//! This module combines generic `chitin-ui` sidebar primitives with the
//! desktop-specific workspace tree renderer.

use std::{
  collections::HashSet,
  path::{Path, PathBuf},
};

use chitin_core::workspace::ProjectWorkspace;
use chitin_ui::{
  components::sidebar::{Sidebar, SidebarBody, SidebarHeader, SidebarSection, SidebarTitle},
  themes::UIThemes,
};
use gpui::{Context, IntoElement, div, prelude::*};

use crate::{app::ChitinApp, components::workspace_tree::render_workspace_tree};

/// Default title shown at the top of the project workspace sidebar.
pub const DEFAULT_PROJECT_WORKSPACE_TITLE: &str = "EXPLORER";

/// App state managed by the project workspace sidebar.
///
/// `ChitinApp` owns this value as one grouped workbench state field. The
/// sidebar and workspace tree borrow it during rendering so new sidebar state
/// can be added without widening every component function signature.
#[derive(Clone, Debug, Default)]
pub struct ProjectSidebarState {
  /// Directory paths whose children are visible in the workspace tree.
  pub expanded_paths: HashSet<PathBuf>,
  /// Directory paths currently loading their direct children.
  pub loading_paths: HashSet<PathBuf>,
  /// Workspace tree entry selected as the active project item.
  pub selected_path: Option<PathBuf>,
  /// Workspace tree entry focused for keyboard navigation.
  pub focused_path: Option<PathBuf>,
}

impl ProjectSidebarState {
  /// Creates sidebar state with the workspace root expanded when present.
  pub fn with_workspace_root(root: Option<&Path>) -> Self {
    Self {
      expanded_paths: root
        .map(|root| HashSet::from([root.to_path_buf()]))
        .unwrap_or_default(),
      loading_paths: HashSet::new(),
      selected_path: None,
      focused_path: None,
    }
  }

  /// Selects a workspace tree entry, that's caused by opening the entry, for
  /// directory node, that means toggling the directory, for file node, that
  /// means opening the file in richer panel.
  pub fn select_entry(&mut self, path: &Path) {
    self.selected_path = Some(path.to_path_buf());
  }

  /// Focuses a workspace tree entry, that' caused by keyboard navigation
  pub fn focus_entry(&mut self, path: &Path) {
    self.focused_path = Some(path.to_path_buf());
  }
}

/// Renders the project workspace sidebar.
///
/// The sidebar itself is generic composition, while the file tree inside it is
/// desktop-specific because it uses Chitin's workspace SVG icon assets and
/// dispatches expansion events to [`ChitinApp`].
pub fn render_project_sidebar(
  workspace: Option<&ProjectWorkspace>,
  state: &ProjectSidebarState,
  theme: UIThemes,
  cx: &mut Context<ChitinApp>,
) -> impl IntoElement {
  Sidebar::new()
    .theme(theme)
    .child(
      SidebarHeader::new()
        .theme(theme)
        .child(SidebarTitle::new(DEFAULT_PROJECT_WORKSPACE_TITLE).theme(theme)),
    )
    .child(
      SidebarBody::new().theme(theme).child(match workspace {
        Some(workspace) => {
          SidebarSection::new()
            .theme(theme)
            .fill(true)
            .child(render_workspace_tree(
              &workspace.tree.root,
              state,
              theme,
              cx,
            ))
        }
        None => SidebarSection::new().theme(theme).child(
          div()
            .p_3()
            .text_xs()
            .text_color(theme.text.secondary)
            .child("Open a project path to show files."),
        ),
      }),
    )
}
