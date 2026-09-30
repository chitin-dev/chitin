//! Workbench appearance over Kit's retained docking and tab interactions.

use std::{rc::Rc, sync::Arc};

use gpui::{AnyElement, AnyView, App, Axis, Div, SharedString, Stateful, Window, div, prelude::*};
use gpui_kit::base::ResizeHandleContext;
use gpui_kit::component::dock::{
  BasePanelView, DockArea, DockAreaRenderer, DockContext, DockSkin, DropIndicator, NodeId, PanelState, TabGroupContext,
  TabGroupRenderer,
};

use super::{WorkbenchStyle, resize_handle_appearance, surface_slot};

/// Creates a Kit dock area with independent workbench surfaces for each tab group.
///
/// The decorator changes presentation only; Kit still owns layout, resizing,
/// tab activation, focus, drag/drop, and persisted panel identity.
pub fn dock_area(
  id: impl Into<SharedString>,
  configure: impl FnOnce(&DockSkin, &mut App) + 'static,
  window: &mut Window,
  cx: &mut App,
) -> gpui::Entity<DockArea> {
  cx.new(|cx| {
    let inner = DockSkin::new(cx);
    let skin = inner.clone();
    // Skin setters notify the area. Run them only after its construction lease ends.
    cx.defer(move |cx| {
      configure(&skin, cx);
      skin.set_tab_border_color(Some(gpui::transparent_black()), cx);
    });
    DockArea::new(id, None, window, cx).with_renderer(Rc::new(WorkbenchDockSkin { inner }))
  })
}

struct WorkbenchDockSkin {
  inner: Rc<DockSkin>,
}

impl DockAreaRenderer for WorkbenchDockSkin {
  fn frame(&self, window: &mut Window, cx: &mut App) -> Stateful<Div> {
    self.inner.frame(window, cx)
  }

  fn center_frame(&self, window: &mut Window, cx: &mut App) -> Stateful<Div> {
    self.inner.center_frame(window, cx)
  }

  fn split_frame(&self, node: NodeId, _: Axis, _: &mut Window, cx: &mut App) -> Stateful<Div> {
    div()
      .id(("workbench-split", node.as_u64()))
      // Keep percentage-sized nested groups in a definite flex content box.
      .flex()
      .flex_col()
      .min_w_0()
      .bg(WorkbenchStyle::global(cx).background)
  }

  fn render_split_handle(&self, handle: &ResizeHandleContext, window: &mut Window, cx: &mut App) -> Option<AnyElement> {
    resize_handle_appearance()(handle, window, cx)
  }

  fn render_dock(&self, dock: &DockContext, content: AnyElement, window: &mut Window, cx: &mut App) -> AnyElement {
    self.inner.render_dock(dock, content, window, cx)
  }

  fn build_placeholder(&self, state: &PanelState, window: &mut Window, cx: &mut App) -> Option<Arc<dyn BasePanelView>> {
    self.inner.build_placeholder(state, window, cx)
  }

  fn tab_group_renderer(&self) -> Rc<dyn TabGroupRenderer> {
    Rc::new(WorkbenchTabGroup {
      inner: self.inner.tab_group_renderer(),
    })
  }
}

struct WorkbenchTabGroup {
  inner: Rc<dyn TabGroupRenderer>,
}

impl TabGroupRenderer for WorkbenchTabGroup {
  fn frame(&self, group: &TabGroupContext, window: &mut Window, cx: &mut App) -> Stateful<Div> {
    let style = WorkbenchStyle::global(cx);
    surface_slot(self.inner.frame(group, window, cx), style)
  }

  fn content_frame(&self, group: &TabGroupContext, window: &mut Window, cx: &mut App) -> Stateful<Div> {
    self.inner.content_frame(group, window, cx)
  }

  fn render_tab_bar(&self, group: &TabGroupContext, window: &mut Window, cx: &mut App) -> AnyElement {
    self.inner.render_tab_bar(group, window, cx)
  }

  fn render_active_panel(
    &self,
    panel: AnyView,
    group: &TabGroupContext,
    window: &mut Window,
    cx: &mut App,
  ) -> AnyElement {
    self.inner.render_active_panel(panel, group, window, cx)
  }

  fn render_drop_indicator(&self, indicator: DropIndicator, window: &mut Window, cx: &mut App) -> Option<AnyElement> {
    self.inner.render_drop_indicator(indicator, window, cx)
  }

  fn render_empty(&self, group: &TabGroupContext, window: &mut Window, cx: &mut App) -> Option<AnyElement> {
    self.inner.render_empty(group, window, cx)
  }
}
