//! Primary workbench navigation and its project, document, and function views.

use chitin_command::CommandId;
use chitin_ui::workbench::{WorkbenchStyle, panel_surface};
use gpui::{
  Context, Div, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled, Window, div, px,
};
use gpui_kit::component::{
  Icon, Sizable as _,
  button::{Button, ButtonVariants as _},
  collapsible::Collapsible,
  tab::{Tab, TabBar},
  theme::ThemeColor,
};

use crate::{
  app::ChitinApp,
  workbench::{
    documents::layout::{PanelId, PanelTabId},
    explorer::render_project_sidebar,
  },
};

/// Mutually exclusive content views sharing the primary sidebar's width and chrome.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SidebarView {
  /// Filesystem workspace tree and its keyboard navigation.
  #[default]
  Project,
  /// Open scientific entries, their topology, selections, and properties.
  Entries,
  /// Curated functional commands grouped into collapsible categories.
  Functions,
}

impl SidebarView {
  const ALL: [Self; 3] = [Self::Project, Self::Entries, Self::Functions];

  /// Returns the full navigation label, rendered alongside the icon.
  fn label(self) -> &'static str {
    match self {
      Self::Project => "Project",
      Self::Entries => "Entries",
      Self::Functions => "Functions",
    }
  }

  /// Returns the bundled navigation icon for this view.
  fn icon_path(self) -> &'static str {
    match self {
      Self::Project => "icons/folder-closed.svg",
      Self::Entries => "icons/activity-workspace.svg",
      Self::Functions => "icons/activity-jobs.svg",
    }
  }
}

/// Composes shared navigation chrome with the selected sidebar's content.
pub(crate) fn render_primary_sidebar(
  app: &mut ChitinApp,
  theme: ThemeColor,
  window: &mut Window,
  cx: &mut Context<ChitinApp>,
) -> impl IntoElement {
  let focus = app.project_sidebar_focus(cx);
  let selected = SidebarView::ALL
    .iter()
    .position(|view| *view == app.sidebar_view)
    .unwrap_or(0);
  let weak = cx.weak_entity();
  let navigation = TabBar::new("primary-sidebar-navigation")
    .segmented()
    .selected_background(WorkbenchStyle::new(theme).raised_background)
    .small()
    .selected_index(selected)
    .children(SidebarView::ALL.map(sidebar_tab))
    .on_click(move |index, window, cx| {
      if let Some(view) = SidebarView::ALL.get(*index) {
        let _ = weak.update(cx, |app, cx| app.select_sidebar_view(*view, window, cx));
      }
    });
  let body = match app.sidebar_view {
    SidebarView::Project => render_project_sidebar(
      app.workspace.as_ref(),
      &mut app.project_sidebar_state,
      &focus,
      theme,
      cx,
    )
    .into_any_element(),
    SidebarView::Entries => super::entries::render_entries(app, theme, window, cx).into_any_element(),
    SidebarView::Functions => render_functions(app, theme, cx).into_any_element(),
  };
  panel_surface(WorkbenchStyle::new(theme))
    .border_r_1()
    .border_color(WorkbenchStyle::new(theme).separator)
    .child(
      div()
        .flex_none()
        .px_1()
        .py_2()
        .border_b_1()
        .border_color(WorkbenchStyle::new(theme).separator)
        .child(navigation),
    )
    .child(body)
}

/// Composes a Kit tab with both its icon and full navigation label.
fn sidebar_tab(view: SidebarView) -> Tab {
  // Kit's icon() selects an icon-only tab; compose the row to keep both visible.
  Tab::new().aria_label(view.label()).child(
    div()
      .flex()
      .items_center()
      .gap_1()
      .child(Icon::default().path(view.icon_path()).with_size(px(10.0)))
      .child(
        div()
          .debug_selector(move || format!("sidebar-label-{}", view.label()))
          .child(view.label()),
      ),
  )
}

/// Renders curated command categories using retained expansion state.
fn render_functions(app: &ChitinApp, theme: ThemeColor, cx: &mut Context<ChitinApp>) -> impl IntoElement {
  let expanded = app.database_functions_expanded;
  let weak = cx.weak_entity();
  let header = Button::new("sidebar-database-category")
    .debug_selector(|| "sidebar-database-category".into())
    .ghost()
    .small()
    .w_full()
    .justify_start()
    .text_color(theme.muted_foreground)
    .child(left_aligned_label(
      "Biological databases",
      Some(if expanded {
        "icons/tree-expand.svg"
      } else {
        "icons/tree-collapse.svg"
      }),
    ))
    .accessibility_label(if expanded {
      "Collapse Biological databases"
    } else {
      "Expand Biological databases"
    })
    .on_click(move |_, _, cx| {
      let _ = weak.update(cx, |app, cx| {
        app.database_functions_expanded = !app.database_functions_expanded;
        cx.notify();
      });
    });
  div()
    .id("sidebar-functions")
    .flex()
    .flex_col()
    .flex_1()
    .min_h_0()
    .overflow_y_scroll()
    .gap_1()
    .child(
      Collapsible::new().open(expanded).child(header).content(
        div().pl_6().pr_2().child(
          command_button(CommandId::DatabaseDownloadRcsbStructure, "Download RCSB structure", cx)
            .child(left_aligned_label("Download RCSB structure", None))
            .w_full()
            .debug_selector(|| "sidebar-download".into()),
        ),
      ),
    )
}

/// Builds a labeled command action shared by the sidebar and welcome surface.
///
/// `icon` customizes presentation only. Activation reuses command-palette
/// invocation, including forms for commands that require user-supplied arguments.
pub(crate) fn function_button(
  id: CommandId,
  label: &'static str,
  icon: Option<&'static str>,
  cx: &Context<ChitinApp>,
) -> Button {
  let button = command_button(id, label, cx).label(label);
  match icon {
    Some(icon) => button.icon(Icon::default().path(icon)),
    None => button,
  }
}

/// Connects a semantic Kit button to desktop command invocation.
fn command_button(id: CommandId, label: &'static str, cx: &Context<ChitinApp>) -> Button {
  let weak = cx.weak_entity();
  Button::new(format!("sidebar-function-{}-{label}", id.as_str()))
    .ghost()
    .small()
    .justify_start()
    .accessibility_label(label)
    .on_click(move |_, window, cx| {
      let _ = weak.update(cx, |app, cx| {
        if id == CommandId::ApplicationToggleCommandPanel {
          if app.command_panel.toggle(window, cx) {
            cx.notify();
          }
        } else {
          app.invoke_command_from_panel(id, window, cx);
        }
      });
    })
}

/// Keeps functional category and action labels aligned within Kit button content.
fn left_aligned_label(label: &'static str, icon: Option<&'static str>) -> Div {
  // Kit centers its inner content independently of the button's outer style.
  let mut row = div().flex().w_full().min_w_0().items_center().justify_start().gap_1();
  if let Some(icon) = icon {
    row = row.child(Icon::default().path(icon).with_size(px(14.0)));
  }
  row.child(
    div()
      .text_xs()
      .debug_selector(move || format!("sidebar-action-label-{label}"))
      .child(label),
  )
}

impl ChitinApp {
  /// Reveals a sidebar view and focuses its appropriate keyboard scope.
  ///
  /// The project tree owns its navigation focus; scientific and functional views
  /// use workbench focus. Switching views retains pane width and document state.
  pub(crate) fn select_sidebar_view(&mut self, view: SidebarView, window: &mut Window, cx: &mut Context<Self>) {
    self.sidebar_view = view;
    self.project_sidebar_visible = true;
    let focus = if view == SidebarView::Project {
      self.project_sidebar_focus(cx)
    } else {
      self.workbench_focus(cx)
    };
    window.focus(&focus, cx);
    cx.notify();
  }

  /// Activates an existing document tab and transfers focus to its panel.
  ///
  /// Missing panel/tab pairs are ignored rather than creating a new document.
  pub(crate) fn activate_sidebar_entry(
    &mut self,
    panel: PanelId,
    tab: PanelTabId,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    if self.document_panels.activate_tab(panel, tab) {
      let focus = self.document_panel_focus(cx);
      window.focus(&focus, cx);
      cx.notify();
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::workbench::documents::OpenedProjectDocument;
  use gpui::AppContext;
  use std::path::{Path, PathBuf};

  struct NavigationProbe;

  struct FunctionsProbe(gpui::Entity<ChitinApp>);

  impl gpui::Render for FunctionsProbe {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
      self.0.update(cx, |app, cx| {
        render_functions(app, *ThemeColor::dark(), cx).into_any_element()
      })
    }
  }

  #[gpui::test]
  fn database_category_should_toggle_download_visibility(cx: &mut gpui::TestAppContext) {
    cx.update(gpui_kit::init);
    let app = cx.new(|_| {
      let mut app = ChitinApp::new(Some(PathBuf::from("/tmp")));
      app.database_functions_expanded = false;
      app
    });
    let (probe, cx) = cx.add_window_view(|_, _| FunctionsProbe(app.clone()));
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(cx.debug_bounds("sidebar-download").is_none());
    for expanded in [true, false] {
      let Some(header) = cx.debug_bounds("sidebar-database-category") else {
        panic!("category header must remain visible");
      };
      cx.simulate_click(header.center(), gpui::Modifiers::default());
      cx.run_until_parked();
      cx.update(|_, cx| {
        assert_eq!(app.read(cx).database_functions_expanded, expanded);
        probe.update(cx, |_, cx| cx.notify());
      });
      cx.run_until_parked();
      cx.update(|window, cx| window.draw(cx).clear());
      // GPUI's debug-bounds map retains old entries after children unmount.
      if expanded {
        assert!(cx.debug_bounds("sidebar-download").is_some());
        for (button, label, inset) in [
          (
            "sidebar-database-category",
            "sidebar-action-label-Biological databases",
            px(40.0),
          ),
          (
            "sidebar-download",
            "sidebar-action-label-Download RCSB structure",
            px(16.0),
          ),
        ] {
          let Some(button) = cx.debug_bounds(button) else {
            panic!("button must be rendered");
          };
          let Some(label) = cx.debug_bounds(label) else {
            panic!("complete label must be rendered");
          };
          assert!(
            label.origin.x <= button.origin.x + inset,
            "label must be left-aligned: {label:?}, {button:?}"
          );
        }
      }
    }
  }

  impl gpui::Render for NavigationProbe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
      div().w(px(272.0)).child(
        TabBar::new("sidebar-navigation-probe")
          .segmented()
          .xsmall()
          .selected_index(0)
          .children(SidebarView::ALL.map(sidebar_tab)),
      )
    }
  }

  #[gpui::test]
  fn navigation_should_render_all_three_labels_alongside_icons(cx: &mut gpui::TestAppContext) {
    cx.update(gpui_kit::init);
    let (_, cx) = cx.add_window_view(|_, _| NavigationProbe);
    cx.update(|window, cx| window.draw(cx).clear());
    for (label, selector) in [
      ("Project", "sidebar-label-Project"),
      ("Entries", "sidebar-label-Entries"),
      ("Functions", "sidebar-label-Functions"),
    ] {
      let bounds = cx.debug_bounds(selector);
      assert!(
        bounds.is_some_and(|bounds| bounds.size.width > px(0.0)
          && bounds.size.height > px(0.0)
          && bounds.origin.x + bounds.size.width <= px(272.0)),
        "{} must have visible text bounds: {:?}",
        label,
        bounds
      );
    }
  }

  #[test]
  fn sidebar_navigation_should_pair_full_labels_with_bundled_icons() -> gpui::Result<()> {
    use gpui::AssetSource;
    for view in SidebarView::ALL {
      assert!(!view.label().is_empty());
      assert!(chitin_ui::assets::ChitinAssets.load(view.icon_path())?.is_some());
    }
    assert_eq!(
      SidebarView::ALL.map(SidebarView::label),
      ["Project", "Entries", "Functions"]
    );
    Ok(())
  }

  #[gpui::test]
  fn switching_sidebar_should_preserve_open_entries_width_and_bottom_dock(cx: &mut gpui::TestAppContext) {
    cx.update(gpui_kit::init);
    let (app, cx) = cx.add_window_view(|_, _| ChitinApp::new(Some(PathBuf::from("/tmp"))));
    cx.update(|window, cx| {
      app.update(cx, |app, cx| {
        app.open_project_document(OpenedProjectDocument::new(Path::new("/tmp/one.pdb")));
        app.project_sidebar_state.set_width(px(320.0));
        app.bottom_dock.show("terminal");
        let tabs = app.document_panels.tree.tabs_in_order();
        app.project_sidebar_visible = false;
        app.database_functions_expanded = false;
        app.select_sidebar_view(SidebarView::Entries, window, cx);
        assert_eq!(app.sidebar_view, SidebarView::Entries);
        assert!(app.project_sidebar_visible);
        assert_eq!(app.document_panels.tree.tabs_in_order(), tabs);
        app.select_sidebar_view(SidebarView::Functions, window, cx);
        app.select_sidebar_view(SidebarView::Project, window, cx);
        assert_eq!(app.project_sidebar_state.width(), px(320.0));
        assert!(app.bottom_dock.is_active("terminal"));
        assert!(!app.database_functions_expanded);
        assert_eq!(app.document_panels.tree.tabs_in_order(), tabs);
      });
    });
  }

  #[gpui::test]
  fn entry_activation_should_reuse_an_inactive_document_tab(cx: &mut gpui::TestAppContext) {
    cx.update(gpui_kit::init);
    let (app, cx) = cx.add_window_view(|_, _| ChitinApp::new(Some(PathBuf::from("/tmp"))));
    cx.update(|window, cx| {
      app.update(cx, |app, cx| {
        app.open_project_document(OpenedProjectDocument::new(Path::new("/tmp/one.pdb")));
        let (panel, tab) = app.document_panels.tree.tabs_in_order()[0];
        app.open_project_document(OpenedProjectDocument::new(Path::new("/tmp/two.pdb")));
        app.activate_sidebar_entry(panel, tab, window, cx);
        assert_eq!(app.document_panels.focused_panel_id, panel);
        assert_eq!(
          app.document_panels.tree.leaf(panel).and_then(|leaf| leaf.active_tab),
          Some(tab)
        );
        assert_eq!(app.document_panels.tree.tabs_in_order().len(), 2);
      });
    });
  }
}
