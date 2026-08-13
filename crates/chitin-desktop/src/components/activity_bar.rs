//! Desktop activity bar composition.
//!
//! This module maps Chitin workbench activities onto the generic
//! `chitin-ui` activity bar component and wires item clicks into `ChitinApp`
//! state.

use chitin_ui::{
  composite::activity_bar::{ActivityBar, ActivityBarItem},
  primitive::{button::ButtonEvent, button::ButtonState},
  themes::UIThemes,
};
use gpui::{AppContext, Context, Entity, IntoElement, Subscription, Window};

use chitin_command::WorkspaceCommand;

use crate::app::ChitinApp;

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
}

/// Persistent primitive button states for desktop activity controls.
#[derive(Clone)]
pub(crate) struct ActivityBarControls {
  workspace: Entity<ButtonState>,
  search: Entity<ButtonState>,
  jobs: Entity<ButtonState>,
  agents: Entity<ButtonState>,
  settings: Entity<ButtonState>,
}

impl ActivityBarControls {
  /// Creates one button state for each desktop activity.
  pub(crate) fn new(cx: &mut Context<ChitinApp>) -> Self {
    Self {
      workspace: cx.new(ButtonState::new),
      search: cx.new(ButtonState::new),
      jobs: cx.new(ButtonState::new),
      agents: cx.new(ButtonState::new),
      settings: cx.new(ButtonState::new),
    }
  }

  /// Subscribes desktop routing to semantic primitive button events.
  pub(crate) fn subscribe(&self, window: &mut Window, cx: &mut Context<ChitinApp>) {
    subscribe_workspace_activity(&self.workspace, window, cx);
    for (button, activity) in [
      (&self.search, ActiveActivity::Search),
      (&self.jobs, ActiveActivity::Jobs),
      (&self.agents, ActiveActivity::Agents),
      (&self.settings, ActiveActivity::Settings),
    ] {
      subscribe_activity(button, activity, window, cx);
    }
  }
}

/// Builds one desktop activity bar item from a primitive button state.
///
/// # Parameters
///
/// * `activity` is the workbench area selected when the item is clicked.
/// * `icon_path` is the asset-relative SVG path rendered by the item.
///
/// # Returns
///
/// An [`ActivityBarItem`] configured for Chitin desktop state updates.
fn activity_item(
  activity: ActiveActivity,
  icon_path: &'static str,
  button_state: Entity<ButtonState>,
) -> ActivityBarItem {
  ActivityBarItem::new(activity.id(), activity.title(), icon_path, button_state)
}

/// Routes a non-workspace activity through the desktop workbench state.
fn subscribe_activity(
  button: &Entity<ButtonState>,
  activity: ActiveActivity,
  window: &mut Window,
  cx: &mut Context<ChitinApp>,
) {
  let subscription: Subscription = cx.subscribe_in(button, window, move |this, _, event, window, cx| {
    if matches!(event, ButtonEvent::Click) {
      this.active_activity = activity;
      let focus = this.document_panel_focus(cx);
      window.focus(&focus, cx);
      cx.notify();
    }
  });
  subscription.detach();
}

/// Routes the Workspace activity through the existing workspace command.
///
fn subscribe_workspace_activity(button: &Entity<ButtonState>, window: &mut Window, cx: &mut Context<ChitinApp>) {
  let subscription: Subscription = cx.subscribe_in(button, window, move |this, _, event, window, cx| {
    if matches!(event, ButtonEvent::Click) {
      this.dispatch_command(WorkspaceCommand::ToggleWorkspace.into(), cx);
      let focus = this.workspace_toggle_focus_target(cx);
      window.focus(&focus, cx);
    }
  });
  subscription.detach();
}

/// Renders the desktop activity bar and wires item clicks to app state.
///
/// # Parameters
///
/// * `active_activity` is the currently selected top-level workbench area.
/// * `theme` supplies colors for the activity bar component.
///
/// # Returns
///
/// A GPUI element containing the Chitin activity bar.
pub fn render_activity_bar(
  active_activity: ActiveActivity,
  theme: UIThemes,
  controls: ActivityBarControls,
) -> impl IntoElement {
  ActivityBar::new()
    .theme(theme)
    .active_item(active_activity.id())
    .item(activity_item(
      ActiveActivity::Workspace,
      "icons/activity-workspace.svg",
      controls.workspace,
    ))
    .item(activity_item(
      ActiveActivity::Search,
      "icons/activity-search.svg",
      controls.search,
    ))
    .item(activity_item(
      ActiveActivity::Jobs,
      "icons/activity-jobs.svg",
      controls.jobs,
    ))
    .item(activity_item(
      ActiveActivity::Agents,
      "icons/activity-agents.svg",
      controls.agents,
    ))
    .bottom_item(activity_item(
      ActiveActivity::Settings,
      "icons/activity-settings.svg",
      controls.settings,
    ))
}
