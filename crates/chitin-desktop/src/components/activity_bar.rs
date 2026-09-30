//! Desktop activity bar composition.
//!
//! This module maps Chitin workbench activities onto the generic
//! `chitin-ui` activity bar component and wires item clicks into `ChitinApp`
//! state.

use chitin_ui::composite::activity_bar::{ActivityBar, ActivityBarItem};
use gpui_kit::component::theme::ThemeColor;

use gpui::{IntoElement, SharedString, WeakEntity, Window};

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
  pub fn title(self) -> &'static str {
    match self {
      Self::Workspace => "Workspace",
      Self::Search => "Search",
      Self::Jobs => "Jobs",
      Self::Agents => "Agents",
      Self::Settings => "Settings",
    }
  }

  /// Resolves the activity carrying `id`, as reported by the activity bar.
  ///
  /// This is the inverse of [`ActiveActivity::id`] and is what routes an item
  /// activation back to the workbench area it represents.
  pub fn from_id(id: &str) -> Option<Self> {
    [Self::Workspace, Self::Search, Self::Jobs, Self::Agents, Self::Settings]
      .into_iter()
      .find(|activity| activity.id() == id)
  }
}

/// Builds one desktop activity bar item.
///
/// # Parameters
///
/// * `activity` is the workbench area selected when the item is clicked.
/// * `icon_path` is the asset-relative SVG path rendered by the item.
///
/// # Returns
///
/// An [`ActivityBarItem`] carrying the activity's stable id.
fn activity_item(activity: ActiveActivity, icon_path: &'static str) -> ActivityBarItem {
  ActivityBarItem::new(activity.id(), activity.title(), icon_path)
}

/// Routes an activated activity item through the desktop workbench state.
///
/// The bar reports the item id, so the workbench area is resolved here rather
/// than being captured per item.
fn route_activity(id: &SharedString, window: &mut Window, cx: &mut gpui::App, app: &WeakEntity<ChitinApp>) {
  let Some(activity) = ActiveActivity::from_id(id.as_ref()) else {
    return;
  };
  let _ = app.update(cx, |this, cx| match activity {
    ActiveActivity::Workspace => {
      this.dispatch_command(WorkspaceCommand::ToggleWorkspace.into(), cx);
      let focus = this.workspace_toggle_focus_target(cx);
      window.focus(&focus, cx);
    }
    activity => {
      this.active_activity = activity;
      let focus = this.document_panel_focus(cx);
      window.focus(&focus, cx);
      cx.notify();
    }
  });
}

/// Renders the desktop activity bar and wires item clicks to app state.
///
/// # Parameters
///
/// * `active_activity` is the currently selected top-level workbench area.
/// * `theme` supplies colors for the activity bar component.
/// * `app` receives the routing caused by an item activation.
///
/// # Returns
///
/// A GPUI element containing the Chitin activity bar.
pub fn render_activity_bar(
  active_activity: ActiveActivity,
  theme: ThemeColor,
  app: WeakEntity<ChitinApp>,
) -> impl IntoElement {
  ActivityBar::new()
    .theme(theme)
    .active_item(active_activity.id())
    .on_item_click(move |id, window, cx| route_activity(id, window, cx, &app))
    .item(activity_item(ActiveActivity::Workspace, "icons/activity-workspace.svg"))
    .item(activity_item(ActiveActivity::Search, "icons/activity-search.svg"))
    .item(activity_item(ActiveActivity::Jobs, "icons/activity-jobs.svg"))
    .item(activity_item(ActiveActivity::Agents, "icons/activity-agents.svg"))
    .bottom_item(activity_item(ActiveActivity::Settings, "icons/activity-settings.svg"))
}
