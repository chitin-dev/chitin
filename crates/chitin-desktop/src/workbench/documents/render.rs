//! Document area, tab actions, and generic file presentation.

use super::{
  layout::{PanelId, PanelSplitAxis},
  state::{DocumentPanelContent, OpenedProjectDocument},
};
use crate::{
  app::ChitinApp,
  features::molecule::options::{DocumentOptionsControls, MolecularDocumentOptions, render_document_options_button},
  keybindings::{CloseTab, FocusNextPanelTab, FocusPreviousPanelTab, PANEL_CONTAINER_KEY_CONTEXT},
};
use chitin_command::PanelTabCommand;
use gpui::{
  AnyElement, Context, Entity, FocusHandle, FontWeight, InteractiveElement, IntoElement, ParentElement, Pixels, Styled,
  WeakEntity, div, px, svg,
};
use gpui_kit::component::{
  button::{Button, ButtonVariant, ButtonVariants as _},
  dock::DockArea,
  theme::ThemeColor,
};

const SPLIT_HORIZONTAL_ICON_PATH: &str = "icons/panel-split-horizontal.svg";
const SPLIT_VERTICAL_ICON_PATH: &str = "icons/panel-split-vertical.svg";
const PANEL_ACTION_ICON_SIZE: Pixels = px(16.0);

/// Composes the Kit docking view with desktop document commands.
///
/// # Parameters
///
/// * `area` is the persistent Kit docking mount.
/// * `focus_handle` marks the document command scope, including child views.
/// * `cx` dispatches previous/next/close actions through typed commands.
///
/// # Returns
///
/// A full-size document region; Kit owns its tabs and split interactions.
pub fn render_document_area(
  area: Entity<DockArea>,
  focus_handle: &FocusHandle,
  cx: &mut Context<ChitinApp>,
) -> impl IntoElement {
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
    .child(area)
}

/// Renders split controls at the right end of one document panel tab strip.
///
/// # Parameters
///
/// * `panel_id` identifies the panel controlled by the split buttons.
/// * `theme` supplies colors for the action buttons.
/// * `app` is the weak app entity updated by button clicks.
/// * `controls` contains the persistent trigger state and menu state.
///
/// # Returns
///
/// A GPUI element containing horizontal and vertical split buttons.
pub(super) fn render_panel_tab_strip_actions(
  panel_id: PanelId,
  theme: ThemeColor,
  app: WeakEntity<ChitinApp>,
  document_options: Option<MolecularDocumentOptions>,
  options_menu_open: bool,
  controls: DocumentOptionsControls,
) -> gpui::Div {
  let mut actions = div().flex().items_center().h_full().flex_none().bg(theme.muted);
  if let Some(document_options) = document_options {
    actions = actions.child(render_document_options_button(
      panel_id,
      document_options,
      options_menu_open,
      theme,
      app.clone(),
      controls,
    ));
  }
  actions
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
/// * `panel_id` identifies the panel that should be split when clicked.
/// * `axis` controls the orientation of the created split.
/// * `icon_path` is the desktop asset path for the button icon.
/// * `theme` supplies colors for the button.
/// * `app` is the weak app entity updated by button clicks.
///
/// # Returns
///
/// A GPUI element for one icon-only split button.
fn render_panel_split_button(
  panel_id: PanelId,
  axis: PanelSplitAxis,
  icon_path: &'static str,
  theme: ThemeColor,
  app: WeakEntity<ChitinApp>,
) -> Button {
  Button::new(format!("document-split-{}-{axis:?}", panel_id.value()))
    .with_variant(ButtonVariant::Ghost)
    .size(px(30.0))
    .px(px(0.0))
    .on_click(move |_, window, cx| {
      let _ = app.update(cx, |app, cx| {
        if app.split_document_panel(panel_id, axis, window, cx) {
          cx.notify();
        }
      });
    })
    .child(
      svg()
        .path(icon_path)
        .size(PANEL_ACTION_ICON_SIZE)
        .text_color(theme.muted_foreground),
    )
}

/// Renders the body for one document-panel tab payload.
///
/// # Parameters
///
/// * `content` is the tab payload selected by the panel container.
/// * `theme` supplies colors for placeholder project-document content.
///
/// # Returns
///
/// A GPUI element for either a project document placeholder or WGPU viewport.
pub(super) fn render_document_panel_content(content: &DocumentPanelContent, theme: ThemeColor) -> AnyElement {
  match content {
    DocumentPanelContent::ProjectDocument(document) => render_opened_document_body(document, theme),
    DocumentPanelContent::WgpuInteractive { view, .. } => view.clone().into_any_element(),
  }
}

/// Renders placeholder content for an opened document tab.
///
/// # Parameters
///
/// * `document` is the opened file descriptor to display.
/// * `theme` supplies colors for the document body.
///
/// # Returns
///
/// A GPUI element containing placeholder document content.
fn render_opened_document_body(document: &OpenedProjectDocument, theme: ThemeColor) -> AnyElement {
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
        .text_color(theme.muted_foreground)
        .child(document.path.display().to_string()),
    )
    .into_any_element()
}
