//! Desktop activity bar composition.
//!
//! This module maps Chitin workbench activities onto the generic
//! `chitin-ui` activity bar component and wires item clicks into `ChitinApp`
//! state.

use chitin_ui::{
  components::activity_bar::{ActivityBar, ActivityBarItem},
  themes::UIThemes,
};
use gpui::{Context, IntoElement};

use crate::{app::ChitinApp, commands::WorkspaceCommand};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Top-level workbench area selected from the activity bar.
pub enum ActiveActivity {
  /// Project files and workspace tree.
  Workspace,
  /// Search across project and scientific assets.
  Search,
  /// Local and external job execution status.
  Jobs,
  /// Agent sessions and planning views.
  Agents,
  /// Application and workspace settings.
  Settings,
}

impl ActiveActivity {
  /// Stable id used for activity bar selection.
  ///
  /// # Parameters
  ///
  /// This method reads `self`, the active workbench area.
  ///
  /// # Returns
  ///
  /// A stable lowercase identifier used for selection comparisons.
  pub fn id(self) -> &'static str {
    match self {
      Self::Workspace => "workspace",
      Self::Search => "search",
      Self::Jobs => "jobs",
      Self::Agents => "agents",
      Self::Settings => "settings",
    }
  }

  /// Human-readable activity label.
  ///
  /// # Parameters
  ///
  /// This method reads `self`, the active workbench area.
  ///
  /// # Returns
  ///
  /// A user-facing label suitable for activity bar tooltips and placeholders.
  pub fn title(self) -> &'static str {
    match self {
      Self::Workspace => "Workspace",
      Self::Search => "Search",
      Self::Jobs => "Jobs",
      Self::Agents => "Agents",
      Self::Settings => "Settings",
    }
  }

  /// Short placeholder description for the main content area.
  ///
  /// # Parameters
  ///
  /// This method reads `self`, the active workbench area.
  ///
  /// # Returns
  ///
  /// A short description shown while the selected activity has no richer panel.
  pub fn description(self) -> &'static str {
    match self {
      Self::Workspace => "Project workspace file tree will appear here.",
      Self::Search => "Search across molecules, sequences, workflows, and notes.",
      Self::Jobs => "Local tool runs and workflow jobs will be tracked here.",
      Self::Agents => "Agent sessions and scientific task plans will appear here.",
      Self::Settings => "Workspace and tool configuration will be edited here.",
    }
  }
}

/// Builds one desktop activity bar item and its click behavior.
///
/// # Parameters
///
/// `cx` is the GPUI context used to create an app-state listener.
///
/// `activity` is the workbench area selected when the item is clicked.
///
/// `icon_path` is the asset-relative SVG path rendered by the item.
///
/// # Returns
///
/// An [`ActivityBarItem`] configured for Chitin desktop state updates.
fn activity_item(
  cx: &mut Context<ChitinApp>,
  activity: ActiveActivity,
  icon_path: &'static str,
) -> ActivityBarItem {
  ActivityBarItem::new(activity.id(), activity.title(), icon_path).on_click(cx.listener(
    move |this, _, _, cx| {
      this.active_activity = activity;
      cx.notify();
    },
  ))
}

/// Builds the Workspace activity item and routes clicks through commands.
///
/// # Parameters
///
/// `cx` is the GPUI context used to create an app-state listener.
///
/// `icon_path` is the asset-relative SVG path rendered by the item.
///
/// # Returns
///
/// An [`ActivityBarItem`] that toggles the project workspace sidebar through
/// the same command used by keyboard shortcuts.
fn workspace_activity_item(
  cx: &mut Context<ChitinApp>,
  icon_path: &'static str,
) -> ActivityBarItem {
  ActivityBarItem::new(
    ActiveActivity::Workspace.id(),
    ActiveActivity::Workspace.title(),
    icon_path,
  )
  .on_click(cx.listener(move |this, _, window, cx| {
    this.dispatch_command(WorkspaceCommand::ToggleWorkspace.into(), cx);
    let focus = this.workspace_toggle_focus_target(cx);
    window.focus(&focus);
  }))
}

/// Renders the desktop activity bar and wires item clicks to app state.
///
/// # Parameters
///
/// `active_activity` is the currently selected top-level workbench area.
///
/// `theme` supplies colors for the activity bar component.
///
/// `cx` is the GPUI context used to create item click listeners.
///
/// # Returns
///
/// A GPUI element containing the Chitin activity bar.
pub fn render_activity_bar(
  active_activity: ActiveActivity,
  theme: UIThemes,
  cx: &mut Context<ChitinApp>,
) -> impl IntoElement {
  ActivityBar::new()
    .theme(theme)
    .active_item(active_activity.id())
    .item(workspace_activity_item(
      cx,
      "icons/activity-bar/codicon-workspace.svg",
    ))
    .item(activity_item(
      cx,
      ActiveActivity::Search,
      "icons/activity-bar/codicon-search.svg",
    ))
    .item(activity_item(
      cx,
      ActiveActivity::Jobs,
      "icons/activity-bar/codicon-job.svg",
    ))
    .item(activity_item(
      cx,
      ActiveActivity::Agents,
      "icons/activity-bar/codicon-agent.svg",
    ))
    .bottom_item(activity_item(
      cx,
      ActiveActivity::Settings,
      "icons/activity-bar/codicon-settings.svg",
    ))
}
