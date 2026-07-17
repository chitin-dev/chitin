//! Project sidebar composition.
//!
//! This module combines generic `chitin-ui` sidebar primitives with the
//! desktop-specific workspace tree renderer.

use std::{collections::HashSet, path::PathBuf};

use chitin_core::workspace::ProjectWorkspace;
use chitin_ui::{
  components::sidebar::{Sidebar, SidebarBody, SidebarHeader, SidebarSection, SidebarTitle},
  themes::UIThemes,
};
use gpui::{Context, IntoElement, div, prelude::*};

use crate::{app::ChitinApp, components::workspace_tree::render_workspace_tree};

/// Default title shown at the top of the project workspace sidebar.
pub const DEFAULT_PROJECT_WORKSPACE_TITLE: &str = "EXPLORER";

/// Renders the project workspace sidebar.
///
/// The sidebar itself is generic composition, while the file tree inside it is
/// desktop-specific because it uses Chitin's workspace SVG icon assets and
/// dispatches expansion events to [`ChitinApp`].
pub fn render_project_sidebar(
  workspace: Option<&ProjectWorkspace>,
  expanded_paths: &HashSet<PathBuf>,
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
      SidebarBody::new()
        .theme(theme)
        .id("project-sidebar-tree-scroll")
        .scrollable(true)
        .child(match workspace {
          Some(workspace) => SidebarSection::new()
            .theme(theme)
            .child(render_workspace_tree(
              &workspace.tree.root,
              expanded_paths,
              theme,
              cx,
            )),
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
