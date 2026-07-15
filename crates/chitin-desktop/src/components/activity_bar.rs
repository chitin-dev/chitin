use chitin_ui::{
  components::activity_bar::{ActivityBar, ActivityBarItem},
  themes::builtins,
};
use gpui::{Context, IntoElement};

use crate::app::ChitinApp;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveActivity {
  Files,
  Search,
  Jobs,
  Agents,
  Settings,
}

impl ActiveActivity {
  pub fn id(self) -> &'static str {
    match self {
      Self::Files => "files",
      Self::Search => "search",
      Self::Jobs => "jobs",
      Self::Agents => "agents",
      Self::Settings => "settings",
    }
  }

  pub fn title(self) -> &'static str {
    match self {
      Self::Files => "Files",
      Self::Search => "Search",
      Self::Jobs => "Jobs",
      Self::Agents => "Agents",
      Self::Settings => "Settings",
    }
  }

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
