//! GPUI rendering for the document panel area.

use chitin_bio::surface::MolecularSurfaceBackend;
use chitin_command::PanelTabCommand;
use chitin_molecule_renderer::{AtomStyle, PolymerStyle, RepresentationLayers, SurfaceStyle};
use chitin_ui::{
  composite::grouped_select::{GroupedSelect, GroupedSelectGroup},
  composite::panel::{PanelId, PanelSplitAxis},
  composite::select_item::IconSelectItem,
};
use gpui_kit::component::theme::ThemeColor;

use gpui::{
  AnyElement, App, AppContext, Context, Entity, FocusHandle, Focusable, FontWeight, InteractiveElement, IntoElement,
  ParentElement, Pixels, RenderOnce, Styled, Subscription, WeakEntity, Window, div, px, svg,
};

use gpui_kit::component::button::{Button, ButtonVariant, ButtonVariants as _};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::{
  IndexPath,
  select::{SelectEvent, SelectItem, SelectState},
};

use crate::{
  app::ChitinApp,
  keybindings::{CloseTab, FocusNextPanelTab, FocusPreviousPanelTab, PANEL_CONTAINER_KEY_CONTEXT},
};

use super::state::{DocumentPanelContent, OpenedProjectDocument};
use gpui_kit::component::dock::DockArea;

/// Asset path for the horizontal split panel action.
const SPLIT_HORIZONTAL_ICON_PATH: &str = "icons/panel-split-horizontal.svg";
/// Asset path for the vertical split panel action.
const SPLIT_VERTICAL_ICON_PATH: &str = "icons/panel-split-vertical.svg";
/// Asset path for the molecular document options action.
const PANEL_MORE_ICON_PATH: &str = "icons/panel-more.svg";
/// Asset path for the stick representation icon.
const STICK_ICON_PATH: &str = "icons/atom-stick.svg";
/// Asset path for the ball-and-stick representation icon.
const BALL_AND_STICK_ICON_PATH: &str = "icons/atom-ball-and-stick.svg";
/// Asset path for the sphere representation icon.
const SPHERE_ICON_PATH: &str = "icons/atom-sphere.svg";
/// Asset path for the cartoon representation icon.
const CARTOON_ICON_PATH: &str = "icons/atom-cartoon.svg";
/// Asset path for the disabled representation icon.
const REPRESENTATION_NONE_ICON_PATH: &str = "icons/representation-none.svg";
/// Asset path for the solid molecular-surface style icon.
const SURFACE_SOLID_ICON_PATH: &str = "icons/surface-solid.svg";
/// Asset path for the sampled implicit scalar-field backend icon.
const IMPLICIT_SURFACE_ICON_PATH: &str = "icons/surface-implicit.svg";
/// Asset path for the analytical MSMS backend icon.
const MSMS_SURFACE_ICON_PATH: &str = "icons/surface-msms.svg";
/// Size used by tab strip action icons.
const PANEL_ACTION_ICON_SIZE: Pixels = px(16.0);
/// Width of the molecular document options menu.
const DOCUMENT_OPTIONS_MENU_WIDTH: Pixels = px(240.0);

/// Molecular rendering choices displayed by one document options popover.
#[derive(Clone, Copy)]
pub(super) struct MolecularDocumentOptions {
  /// Representation layers selected for the molecular scene.
  pub(super) representation: RepresentationLayers,
  /// Algorithm selected for molecular-surface generation.
  pub(super) surface_backend: MolecularSurfaceBackend,
}

/// Deferred molecular document menu rendered above the clipped tab strip.
#[derive(IntoElement)]
struct DocumentOptionsMenu {
  /// Panel whose active molecular view receives menu changes.
  panel_id: PanelId,
  /// Molecular rendering choices selected when this menu is rendered.
  options: MolecularDocumentOptions,
  /// Semantic colors for the menu surface and rows.
  theme: ThemeColor,
  /// Weak root app entity used by dismissal and selection callbacks.
  app: WeakEntity<ChitinApp>,
  /// Controlled open state shared with document-panel command routing.
  open: bool,
  /// Persistent interaction state used by the semantic menu primitive.
  controls: DocumentOptionsControls,
}

/// Persistent semantic controls used by molecular document panels.
#[derive(Clone)]
pub(crate) struct DocumentOptionsControls {
  /// Single-selection state for atom-layer styles.
  atom: Entity<SelectState<Vec<IconSelectItem>>>,
  /// Single-selection state for polymer-layer styles.
  polymer: Entity<SelectState<Vec<IconSelectItem>>>,
  /// Single-selection state for surface-layer styles.
  surface: Entity<SelectState<Vec<IconSelectItem>>>,
  /// Single-selection state for molecular-surface generation algorithms.
  surface_backend: Entity<SelectState<Vec<IconSelectItem>>>,
}

/// Declarative data used to build one representation-layer selector.
struct SelectOptionSpec {
  /// Stable identifier consumed by the representation event handler.
  id: &'static str,
  /// Human-readable label displayed by the selector.
  label: &'static str,
  /// Asset-relative icon path displayed beside the option.
  icon: Option<&'static str>,
}

impl SelectOptionSpec {
  /// Creates one representation selector option description.
  const fn new(id: &'static str, label: &'static str, icon: &'static str) -> Self {
    Self {
      id,
      label,
      icon: Some(icon),
    }
  }
}

/// Creates a select state with a valid initial representation choice.
fn new_representation_select(
  window: &mut Window,
  cx: &mut Context<ChitinApp>,
  specs: impl IntoIterator<Item = SelectOptionSpec>,
  selected_id: &str,
) -> Entity<SelectState<Vec<IconSelectItem>>> {
  let options = specs
    .into_iter()
    .map(|spec| {
      let option = IconSelectItem::new(spec.id, spec.label);
      match spec.icon {
        Some(icon) => option.icon(icon),
        None => option,
      }
    })
    .collect::<Vec<_>>();
  let selected = options
    .iter()
    .position(|option| option.value().as_ref() == selected_id)
    .map(IndexPath::new);
  cx.new(|cx| SelectState::new(options, selected, window, cx))
}

impl DocumentOptionsControls {
  /// Creates the persistent trigger and representation menu state.
  pub(crate) fn new(window: &mut Window, cx: &mut Context<ChitinApp>) -> Self {
    let atom = new_representation_select(
      window,
      cx,
      [
        SelectOptionSpec::new("none", "None", REPRESENTATION_NONE_ICON_PATH),
        SelectOptionSpec::new("stick", "Stick", STICK_ICON_PATH),
        SelectOptionSpec::new("ball-and-stick", "Ball and stick", BALL_AND_STICK_ICON_PATH),
        SelectOptionSpec::new("sphere", "Space filling", SPHERE_ICON_PATH),
      ],
      "stick",
    );
    let polymer = new_representation_select(
      window,
      cx,
      [
        SelectOptionSpec::new("none", "None", REPRESENTATION_NONE_ICON_PATH),
        SelectOptionSpec::new("cartoon", "Cartoon", CARTOON_ICON_PATH),
      ],
      "none",
    );
    let surface = new_representation_select(
      window,
      cx,
      [
        SelectOptionSpec::new("none", "None", REPRESENTATION_NONE_ICON_PATH),
        SelectOptionSpec::new("solid", "Solid", SURFACE_SOLID_ICON_PATH),
      ],
      "none",
    );
    let surface_backend = new_representation_select(
      window,
      cx,
      [
        SelectOptionSpec::new(
          "implicit-scalar-field",
          "Implicit scalar field",
          IMPLICIT_SURFACE_ICON_PATH,
        ),
        SelectOptionSpec::new("msms", "MSMS", MSMS_SURFACE_ICON_PATH),
      ],
      "implicit-scalar-field",
    );
    Self {
      atom,
      polymer,
      surface,
      surface_backend,
    }
  }

  /// Subscribes representation-selector events to document-panel commands.
  ///
  /// The ellipsis trigger is not subscribed here: it owns its own click handler
  /// and reaches the panel state through the app entity.
  ///
  /// # Parameters
  ///
  /// * `window` supplies focus routing for the menu when it opens.
  /// * `cx` owns subscriptions and updates the application state.
  pub(crate) fn subscribe(&self, window: &mut Window, cx: &mut Context<ChitinApp>) {
    let subscription: Subscription = cx.subscribe_in(&self.atom, window, move |app, _, event, _, cx| {
      let SelectEvent::Confirm(selected_id) = event;
      let Some(panel_id) = app.document_panels.options_menu_panel_id else {
        return;
      };
      let representation = app
        .document_panels
        .active_representation_layers(panel_id)
        .unwrap_or_else(RepresentationLayers::empty);
      let representation = match selected_id.as_deref() {
        Some("none") => representation.without_atom(),
        Some("stick") => representation.with_atom(AtomStyle::Stick),
        Some("ball-and-stick") => representation.with_atom(AtomStyle::BallAndStick),
        Some("sphere") => representation.with_atom(AtomStyle::Sphere),
        _ => return,
      };
      app.select_document_representation_layers(panel_id, representation, cx);
      cx.notify();
    });
    subscription.detach();

    let subscription: Subscription = cx.subscribe_in(&self.polymer, window, move |app, _, event, _, cx| {
      let SelectEvent::Confirm(selected_id) = event;
      let Some(panel_id) = app.document_panels.options_menu_panel_id else {
        return;
      };
      let Some(representation) = app.document_panels.active_representation_layers(panel_id) else {
        return;
      };
      let representation = match selected_id.as_deref() {
        Some("none") => representation.without_polymer(),
        Some("cartoon") => representation.with_polymer(PolymerStyle::Cartoon),
        _ => return,
      };
      app.select_document_representation_layers(panel_id, representation, cx);
      cx.notify();
    });
    subscription.detach();

    let subscription: Subscription = cx.subscribe_in(&self.surface, window, move |app, _, event, _, cx| {
      let SelectEvent::Confirm(selected_id) = event;
      let Some(panel_id) = app.document_panels.options_menu_panel_id else {
        return;
      };
      let Some(representation) = app.document_panels.active_representation_layers(panel_id) else {
        return;
      };
      let representation = match selected_id.as_deref() {
        Some("none") => representation.without_surface(),
        Some("solid") => representation.with_surface(SurfaceStyle::Solid),
        _ => return,
      };
      app.select_document_representation_layers(panel_id, representation, cx);
      cx.notify();
    });
    subscription.detach();

    let subscription: Subscription = cx.subscribe_in(&self.surface_backend, window, move |app, _, event, _, cx| {
      let SelectEvent::Confirm(selected_id) = event;
      let Some(panel_id) = app.document_panels.options_menu_panel_id else {
        return;
      };
      let backend = match selected_id.as_deref() {
        Some("implicit-scalar-field") => MolecularSurfaceBackend::ImplicitScalarField,
        Some("msms") => MolecularSurfaceBackend::Msms,
        _ => return,
      };
      if app.select_document_surface_backend(panel_id, backend, cx) {
        app.dismiss_document_options_menu();
        cx.notify();
      }
    });
    subscription.detach();
  }
}

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
  let mut actions = div().flex().items_center().h_full().flex_none().bg(theme.background);
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

/// Composes GPUI Kit's popover with the molecular document controls.
///
/// The framework owns anchoring, dismissal, nested overlays, and focus restoration;
/// the desktop records which panel receives representation changes.
fn render_document_options_button(
  panel_id: PanelId,
  options: MolecularDocumentOptions,
  open: bool,
  theme: ThemeColor,
  app: WeakEntity<ChitinApp>,
  controls: DocumentOptionsControls,
) -> DocumentOptionsMenu {
  DocumentOptionsMenu {
    panel_id,
    options,
    open,
    theme,
    app,
    controls,
  }
}

impl RenderOnce for DocumentOptionsMenu {
  fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
    let panel_id = self.panel_id;
    let app = self.app;
    let focus = self.controls.atom.read(cx).focus_handle(cx);
    let controls = self.controls;
    let options = self.options;
    let theme = self.theme;
    Popover::new(("document-options-menu", panel_id.value()))
      .anchor(gpui_kit::base::Anchor::TopRight)
      .open(self.open)
      .track_focus(&focus)
      .trigger(
        Button::new(format!("document-options-{}", panel_id.value()))
          .with_variant(ButtonVariant::Ghost)
          .w(px(30.0))
          .h(px(30.0))
          .px(px(0.0))
          .child(
            svg()
              .path(PANEL_MORE_ICON_PATH)
              .size(PANEL_ACTION_ICON_SIZE)
              .text_color(theme.muted_foreground),
          ),
      )
      .on_open_change(move |open, _, cx| {
        let _ = app.update(cx, |app, cx| {
          if *open {
            if app.document_panels.options_menu_panel_id != Some(panel_id) {
              app.toggle_document_options_menu(panel_id);
            }
          } else if app.document_panels.options_menu_panel_id == Some(panel_id) {
            app.dismiss_document_options_menu();
          }
          cx.notify();
        });
      })
      .content(move |_, window, cx| {
        sync_document_option_selectors(&controls, options.representation, options.surface_backend, window, cx);
        GroupedSelect::new()
          .theme(theme)
          .width(DOCUMENT_OPTIONS_MENU_WIDTH)
          .group(GroupedSelectGroup::new("Atom style", controls.atom.clone()))
          .group(GroupedSelectGroup::new("Polymer style", controls.polymer.clone()))
          .group(GroupedSelectGroup::new("Surface style", controls.surface.clone()))
          .group(GroupedSelectGroup::new(
            "Surface backend",
            controls.surface_backend.clone(),
          ))
      })
  }
}

/// Synchronizes representation and surface-backend selectors with the active view.
///
/// # Parameters
///
/// * `controls` contains the Kit selector states owned by this document panel.
/// * `representation` and `surface_backend` supply the current renderer settings.
/// * `window` and `cx` update selector state without emitting confirmation events.
///
/// # Returns
///
/// Nothing. Only differing selections are updated, preventing a render-time
/// synchronization from dispatching another renderer command.
fn sync_document_option_selectors(
  controls: &DocumentOptionsControls,
  representation: RepresentationLayers,
  surface_backend: MolecularSurfaceBackend,
  window: &mut Window,
  cx: &mut App,
) {
  let atom_id = match representation.atom_style() {
    Some(AtomStyle::Stick) => "stick",
    Some(AtomStyle::BallAndStick) => "ball-and-stick",
    Some(AtomStyle::Sphere) => "sphere",
    None => "none",
  };
  let polymer_id = if representation.polymer_style().is_some() {
    "cartoon"
  } else {
    "none"
  };
  let surface_id = if representation.surface_style().is_some() {
    "solid"
  } else {
    "none"
  };
  let surface_backend_id = match surface_backend {
    MolecularSurfaceBackend::ImplicitScalarField => "implicit-scalar-field",
    MolecularSurfaceBackend::Msms => "msms",
  };
  for (state, id) in [
    (&controls.atom, atom_id),
    (&controls.polymer, polymer_id),
    (&controls.surface, surface_id),
    (&controls.surface_backend, surface_backend_id),
  ] {
    state.update(cx, |state, cx| {
      if state.selected_value().map(|value| value.as_ref()) != Some(id) {
        state.set_selected_value(&id.into(), window, cx);
      }
    });
  }
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
