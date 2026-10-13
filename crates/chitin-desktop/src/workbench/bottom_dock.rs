//! Desktop-owned controls for the reusable workbench bottom dock.

use chitin_ui::workbench::{WorkbenchStyle, bottom_dock::BottomDock};
use gpui::{Context, IntoElement, Styled, Window};
use gpui_kit::component::{
  Sizable as _,
  tab::{Tab, TabBar},
  theme::ThemeColor,
};

use crate::app::ChitinApp;

impl ChitinApp {
  /// Hides the overlay without destroying a tool's runtime or view state.
  pub(crate) fn close_bottom_dock(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    self.bottom_dock.close();
    self.terminal_panel.request_focus(false);
    let focus = self.workbench_focus(cx);
    window.focus(&focus, cx);
    cx.notify();
  }
}

/// Composes workbench chrome independently of any utility's session model.
pub(crate) fn render_bottom_dock(
  app: &ChitinApp,
  content: impl IntoElement,
  theme: ThemeColor,
  cx: &Context<ChitinApp>,
) -> impl IntoElement {
  // Only implemented tools are exposed; the UI layer does not know these IDs.
  let tools = [(crate::features::terminal::TERMINAL_DOCK_ITEM_ID, "Terminal")];
  let selected = tools
    .iter()
    .position(|(id, _)| app.bottom_dock.is_active(id))
    .unwrap_or(0);
  let navigation_app = cx.weak_entity();
  let navigation = TabBar::new("bottom-dock-tools")
    .segmented()
    .selected_background(WorkbenchStyle::new(theme).raised_background)
    .small()
    .bg(WorkbenchStyle::new(theme).panel_background)
    .menu(false)
    .selected_index(selected)
    .children(tools.map(|(_, label)| Tab::new().label(label)))
    .on_click(move |index, _, cx| {
      if let Some((id, _)) = tools.get(*index) {
        let _ = navigation_app.update(cx, |this, cx| {
          this.bottom_dock.show(*id);
          this
            .terminal_panel
            .request_focus(*id == crate::features::terminal::TERMINAL_DOCK_ITEM_ID);
          cx.notify();
        });
      }
    });
  let close_app = cx.weak_entity();
  let resize_app = cx.weak_entity();
  let maximize_app = cx.weak_entity();
  BottomDock::new("Utilities")
    .navigation(navigation)
    .height(app.bottom_dock.height())
    .maximized(app.bottom_dock.is_maximized())
    .on_maximize(move |_, _, cx| {
      let _ = maximize_app.update(cx, |this, cx| {
        this.bottom_dock.toggle_maximized();
        cx.notify();
      });
    })
    .on_close(move |_, window, cx| {
      let _ = close_app.update(cx, |this, cx| this.close_bottom_dock(window, cx));
    })
    .on_resize(move |height, _, cx| {
      let _ = resize_app.update(cx, |this, cx| {
        if this.bottom_dock.set_height(height) {
          cx.notify();
        }
      });
    })
    .theme(theme)
    .child(content)
}
