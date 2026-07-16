//! Desktop activity bar composition.
//!
//! This module maps Chitin workbench activities onto the generic
//! `chitin-ui` activity bar component and wires item clicks into `ChitinApp`
//! state.

use chitin_ui::{
  components::activity_bar::{ActivityBar, ActivityBarItem},
  themes::builtins,
};
use gpui::{Context, IntoElement};

use crate::app::ChitinApp;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Top-level workbench area selected from the activity bar.
pub enum ActiveActivity {
  /// Project files and workspace tree.
  Files,
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
  pub fn id(self) -> &'static str {
    match self {
      Self::Files => "files",
      Self::Search => "search",
      Self::Jobs => "jobs",
      Self::Agents => "agents",
      Self::Settings => "settings",
    }
  }

  /// Human-readable activity label.
  pub fn title(self) -> &'static str {
    match self {
      Self::Files => "Files",
      Self::Search => "Search",
      Self::Jobs => "Jobs",
      Self::Agents => "Agents",
      Self::Settings => "Settings",
    }
  }

  /// Short placeholder description for the main content area.
  pub fn description(self) -> &'static str {
    match self {
      Self::Files => "Project file tree will appear here.",
      Self::Search => "Search across molecules, sequences, workflows, and notes.",
      Self::Jobs => "Local tool runs and workflow jobs will be tracked here.",
      Self::Agents => "Agent sessions and scientific task plans will appear here.",
      Self::Settings => "Workspace and tool configuration will be edited here.",
    }
  }
}

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

/// Renders the desktop activity bar and wires item clicks to app state.
pub fn render_activity_bar(
  active_activity: ActiveActivity,
  cx: &mut Context<ChitinApp>,
) -> impl IntoElement {
  let theme = builtins::dark();

  ActivityBar::new()
    .theme(theme)
    .active_item(active_activity.id())
    .item(activity_item(
      cx,
      ActiveActivity::Files,
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
