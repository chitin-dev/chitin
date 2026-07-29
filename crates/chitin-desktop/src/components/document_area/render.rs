//! GPUI rendering for the document panel area.

use std::{rc::Rc, time::Instant};

use chitin_ui::{
  components::panel::{
    PanelContainerConfig, PanelId, PanelResizeConfig, PanelSplitAxis, PanelTabActivateHandler,
    PanelTabCloseHandler, PanelTabCloseIconRenderer, PanelTabDragConfig, PanelTabDragStartHandler,
    PanelTabDragTargetHandler, PanelTabDropHandler, render_panel_container,
  },
  themes::UIThemes,
};
use gpui::{
  AnyElement, App, AsyncApp, Context, FocusHandle, FontWeight, InteractiveElement, IntoElement,
  MouseButton, ParentElement, Pixels, Styled, WeakEntity, Window, div, px, svg,
};

use crate::{
  app::ChitinApp,
  commands::{
    PanelTabCommand,
    tab::{CloseTab, FocusNextPanelTab, FocusPreviousPanelTab, PANEL_CONTAINER_KEY_CONTEXT},
  },
};

use super::state::{DocumentPanelState, OpenedProjectDocument};

/// Asset path for the horizontal split panel action.
const SPLIT_HORIZONTAL_ICON_PATH: &str = "icons/panel/codicon-split-horizontal.svg";
/// Asset path for the vertical split panel action.
const SPLIT_VERTICAL_ICON_PATH: &str = "icons/panel/codicon-split-vertical.svg";
/// Asset path for document tab close buttons.
const TAB_CLOSE_ICON_PATH: &str = "icons/panel/lucide-x.svg";
/// Size used by tab strip action icons.
const PANEL_ACTION_ICON_SIZE: Pixels = px(16.0);
/// Size used by close tab button icons.
const TAB_CLOSE_ICON_SIZE: Pixels = px(12.0);

/// Renders the main workbench document area.
///
/// The document area always renders the panel container. Before any file is
/// opened, the container shows its empty root panel instead of the legacy
/// workspace summary placeholder.
///
/// # Parameters
///
/// `document_panels` contains the current document panel layout.
///
/// `theme` supplies colors for the document area.
///
/// `focus_handle` is the focus handle used to track and direct keyboard focus
/// for the document area element.
///
/// `app` is the weak app entity used by document panel callbacks.
///
/// `cx` is the GPUI context used by action listeners and callbacks to update
/// application state.
///
/// # Returns
///
/// A GPUI element for the main document region.
pub fn render_document_area(
  document_panels: &DocumentPanelState,
  theme: UIThemes,
  focus_handle: &FocusHandle,
  app: WeakEntity<ChitinApp>,
  cx: &mut Context<ChitinApp>,
) -> impl IntoElement {
  render_opened_document_panels(document_panels, theme, focus_handle, app, cx)
}

/// Renders document panels for files opened from the workspace tree.
///
/// # Parameters
///
/// `document_panels` contains the panel tree and tab state to render.
///
/// `theme` supplies colors for the placeholder document surface.
///
/// `focus_handle` is the focus handle used to track and direct keyboard focus
/// for the panel container element.
///
/// `app` is the weak app entity used by tab and split button callbacks.
///
/// `cx` is the GPUI context used by action listeners and callbacks to update
/// application state.
///
/// # Returns
///
/// A GPUI `Div` containing the placeholder opened-document view.
fn render_opened_document_panels(
  document_panels: &DocumentPanelState,
  theme: UIThemes,
  focus_handle: &FocusHandle,
  app: WeakEntity<ChitinApp>,
  cx: &mut Context<ChitinApp>,
) -> gpui::Div {
  let activate_app = app.clone();
  let on_activate_tab: Rc<PanelTabActivateHandler> = Rc::new(
    move |panel_id: PanelId, tab_id, _: &mut Window, cx: &mut App| {
      let _ = activate_app.update(cx, |app, cx| {
        if app.activate_document_panel_tab(panel_id, tab_id) {
          cx.notify();
        }
      });
    },
  );
  let close_app = app.clone();
  let on_close_tab: Rc<PanelTabCloseHandler> = Rc::new(
    move |panel_id: PanelId, tab_id, _: &mut Window, cx: &mut App| {
      let _ = close_app.update(cx, |app, cx| {
        if app.close_document_panel_tab(panel_id, tab_id) {
          cx.notify();
        }
      });
    },
  );
  let render_tab_close_icon: Rc<PanelTabCloseIconRenderer> = Rc::new(move |theme| {
    svg()
      .path(TAB_CLOSE_ICON_PATH)
      .size(TAB_CLOSE_ICON_SIZE)
      .text_color(theme.text.primary)
      .into_any_element()
  });
  let drag_start_app = app.clone();
  let on_tab_drag_start: Rc<PanelTabDragStartHandler> =
    Rc::new(move |drag, _: &mut Window, cx: &mut App| {
      let _ = drag_start_app.update(cx, |app, cx| {
        if app.start_document_panel_tab_drag(drag) {
          cx.notify();
        }
      });
    });
  let drag_target_app = app.clone();
  let on_tab_drag_target: Rc<PanelTabDragTargetHandler> =
    Rc::new(move |target, _: &mut Window, cx: &mut App| {
      let _ = drag_target_app.update(cx, |app, cx| {
        if app.update_document_panel_tab_drag_target(target) {
          cx.notify();
        }
      });
    });
  let drop_app = app.clone();
  let on_tab_drop: Rc<PanelTabDropHandler> =
    Rc::new(move |drag, panel_id, _: &mut Window, cx: &mut App| {
      let _ = drop_app.update(cx, |app, cx| {
        app.drop_document_panel_tab(drag, panel_id);
        cx.notify();
      });
    });
  let tab_drag = PanelTabDragConfig::new(on_tab_drag_start, on_tab_drag_target, on_tab_drop)
    .state(document_panels.tab_drag.clone());

  let actions_app = app.clone();
  let render_tab_strip_actions = Rc::new(move |panel_id| {
    render_panel_tab_strip_actions(panel_id, theme, actions_app.clone()).into_any_element()
  });
  let resize_app = app.clone();
  let resize = PanelResizeConfig::new(move |path, axis, start_position, _, cx| {
    let _ = resize_app.update(cx, |app, cx| {
      if app.start_document_panel_resize(path, axis, start_position) {
        cx.notify();
      }
    });
  });
  let now = Instant::now();
  let panel_config = PanelContainerConfig::new()
    .resize(resize)
    .focused_panel_id(document_panels.focused_panel_id)
    .on_activate_tab(on_activate_tab)
    .on_close_tab(on_close_tab)
    .render_tab_close_icon(render_tab_close_icon)
    .render_tab_strip_actions(render_tab_strip_actions)
    .tab_drag(tab_drag)
    .tab_scroll(document_panels.tab_scroll.clone())
    .now(now);

  let mouse_focus_handle = focus_handle.clone();
  let panel_container =
    render_panel_container(&document_panels.tree, theme, panel_config, &|tab| {
      render_opened_document_body(&tab.payload, theme).into_any_element()
    });
  schedule_tab_scroll_indicator_hide(document_panels, now, cx);

  div()
    .flex()
    .flex_1()
    .min_w_0()
    .min_h_0()
    .track_focus(focus_handle)
    .key_context(PANEL_CONTAINER_KEY_CONTEXT)
    .on_action(cx.listener(|this, _: &FocusPreviousPanelTab, _, cx| {
      this.dispatch_command(PanelTabCommand::FocusPrevious.into(), cx);
    }))
    .on_action(cx.listener(|this, _: &FocusNextPanelTab, _, cx| {
      this.dispatch_command(PanelTabCommand::FocusNext.into(), cx);
    }))
    .on_action(cx.listener(|this, _: &CloseTab, _, cx| {
      this.dispatch_command(PanelTabCommand::Close.into(), cx);
    }))
    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
      window.focus(&mouse_focus_handle, cx);
    })
    .child(panel_container)
}

/// Schedules redraws for expiring tab-scroll indicators.
///
/// # Parameters
///
/// `document_panels` owns the presentation-only scroll indicator state.
///
/// `now` is the current UI clock timestamp.
///
/// `cx` schedules the wake-up task and receives the redraw notification.
///
/// # Returns
///
/// This function has no return value.
fn schedule_tab_scroll_indicator_hide(
  document_panels: &DocumentPanelState,
  now: Instant,
  cx: &mut Context<ChitinApp>,
) {
  for expires_at in document_panels
    .tab_scroll
    .indicator_redraws_to_schedule(now)
  {
    let delay = expires_at.saturating_duration_since(now);
    cx.spawn(
      move |this: WeakEntity<ChitinApp>, async_cx: &mut AsyncApp| {
        let mut async_cx = async_cx.clone();
        async move {
          let timer = async_cx.background_executor().timer(delay);
          timer.await;
          let _ = this.update(&mut async_cx, |_, cx| cx.notify());
        }
      },
    )
    .detach();
  }
}

/// Renders split controls at the right end of one document panel tab strip.
///
/// # Parameters
///
/// `panel_id` identifies the panel controlled by the split buttons.
///
/// `theme` supplies colors for the action buttons.
///
/// `app` is the weak app entity updated by button clicks.
///
/// # Returns
///
/// A GPUI element containing horizontal and vertical split buttons.
fn render_panel_tab_strip_actions(
  panel_id: PanelId,
  theme: UIThemes,
  app: WeakEntity<ChitinApp>,
) -> gpui::Div {
  div()
    .flex()
    .items_center()
    .h_full()
    .flex_none()
    .border_l_1()
    .border_color(theme.border.primary)
    .bg(theme.background.primary)
    .child(render_panel_split_button(
      panel_id,
      PanelSplitAxis::Horizontal,
      SPLIT_HORIZONTAL_ICON_PATH,
      theme,
      app.clone(),
    ))
    .child(render_panel_split_button(
      panel_id,
      PanelSplitAxis::Vertical,
      SPLIT_VERTICAL_ICON_PATH,
      theme,
      app,
    ))
}

/// Renders one split button for a document panel.
///
/// # Parameters
///
/// `panel_id` identifies the panel that should be split when clicked.
///
/// `axis` controls the orientation of the created split.
///
/// `icon_path` is the desktop asset path for the button icon.
///
/// `theme` supplies colors for the button.
///
/// `app` is the weak app entity updated by button clicks.
///
/// # Returns
///
/// A GPUI element for one icon-only split button.
fn render_panel_split_button(
  panel_id: PanelId,
  axis: PanelSplitAxis,
  icon_path: &'static str,
  theme: UIThemes,
  app: WeakEntity<ChitinApp>,
) -> gpui::Div {
  div()
    .flex()
    .items_center()
    .justify_center()
    .size(px(30.0))
    .cursor_pointer()
    .text_color(theme.text.secondary)
    .hover(move |style| {
      style
        .bg(theme.background.hover)
        .text_color(theme.text.primary)
    })
    .on_mouse_up(MouseButton::Left, move |_, _, cx| {
      let _ = app.update(cx, |app, cx| {
        if app.split_document_panel(panel_id, axis) {
          cx.notify();
        }
      });
    })
    .child(
      svg()
        .path(icon_path)
        .size(PANEL_ACTION_ICON_SIZE)
        .text_color(theme.text.secondary),
    )
}

/// Renders placeholder content for an opened document tab.
///
/// # Parameters
///
/// `document` is the opened file descriptor to display.
///
/// `theme` supplies colors for the document body.
///
/// # Returns
///
/// A GPUI element containing placeholder document content.
fn render_opened_document_body(document: &OpenedProjectDocument, theme: UIThemes) -> AnyElement {
  div()
    .flex()
    .flex_col()
    .flex_1()
    .min_h_0()
    .p_8()
    .gap_3()
    .child(
      div()
        .text_lg()
        .font_weight(FontWeight::SEMIBOLD)
        .child("Placeholder document"),
    )
    .child(
      div()
        .text_sm()
        .text_color(theme.text.secondary)
        .child(document.path.display().to_string()),
    )
    .into_any_element()
}
