//! Molecular representation and surface-backend options.

use std::{cell::Cell, rc::Rc};

use crate::{app::ChitinApp, workbench::documents::layout::PanelId};
use chitin_bio::surface::MolecularSurfaceBackend;
use chitin_command::{RenderAtomStyle, RenderCommand, RenderPolymerStyle, RenderSurfaceBackend, RenderSurfaceStyle};
use chitin_molecule_renderer::{AtomStyle, RepresentationLayers};
use chitin_ui::widgets::{
  grouped_select::{GroupedSelect, GroupedSelectGroup},
  select_item::IconSelectItem,
};
use gpui::{
  App, AppContext, Context, Entity, Focusable, IntoElement, ParentElement, Pixels, RenderOnce, Styled, Subscription,
  WeakEntity, Window, px, svg,
};
use gpui_kit::component::{
  IndexPath,
  button::{Button, ButtonVariant, ButtonVariants as _},
  popover::Popover,
  select::{SelectEvent, SelectItem, SelectState},
  theme::ThemeColor,
};

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
pub(crate) struct MolecularDocumentOptions {
  /// Representation layers selected for the molecular scene.
  pub(crate) representation: RepresentationLayers,
  /// Algorithm selected for molecular-surface generation.
  pub(crate) surface_backend: MolecularSurfaceBackend,
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
  /// Retains command targeting when dismissal precedes a deferred confirmation.
  target_panel: Rc<Cell<Option<u64>>>,
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
      target_panel: Rc::new(Cell::new(None)),
      atom,
      polymer,
      surface,
      surface_backend,
    }
  }

  /// Binds selectors to the stable rendering tab opening the menu.
  pub(crate) fn bind_panel(&self, panel_id: u64) {
    self.target_panel.set(Some(panel_id));
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
    let target_panel = Rc::clone(&self.target_panel);
    let subscription: Subscription = cx.subscribe_in(&self.atom, window, move |app, _, event, _, cx| {
      let SelectEvent::Confirm(selected_id) = event;
      let Some(panel_id) = target_panel.get() else {
        return;
      };
      let style = match selected_id.as_deref() {
        Some("none") => RenderAtomStyle::None,
        Some("stick") => RenderAtomStyle::Stick,
        Some("ball-and-stick") => RenderAtomStyle::BallAndStick,
        Some("sphere") => RenderAtomStyle::Sphere,
        _ => return,
      };
      submit_menu_command(app, panel_id, RenderCommand::AtomStyle(style), cx);
      cx.notify();
    });
    subscription.detach();

    let target_panel = Rc::clone(&self.target_panel);
    let subscription: Subscription = cx.subscribe_in(&self.polymer, window, move |app, _, event, _, cx| {
      let SelectEvent::Confirm(selected_id) = event;
      let Some(panel_id) = target_panel.get() else {
        return;
      };
      let style = match selected_id.as_deref() {
        Some("none") => RenderPolymerStyle::None,
        Some("cartoon") => RenderPolymerStyle::Cartoon,
        _ => return,
      };
      submit_menu_command(app, panel_id, RenderCommand::PolymerStyle(style), cx);
      cx.notify();
    });
    subscription.detach();

    let target_panel = Rc::clone(&self.target_panel);
    let subscription: Subscription = cx.subscribe_in(&self.surface, window, move |app, _, event, _, cx| {
      let SelectEvent::Confirm(selected_id) = event;
      let Some(panel_id) = target_panel.get() else {
        return;
      };
      let style = match selected_id.as_deref() {
        Some("none") => RenderSurfaceStyle::None,
        Some("solid") => RenderSurfaceStyle::Solid,
        _ => return,
      };
      submit_menu_command(app, panel_id, RenderCommand::SurfaceStyle(style), cx);
      cx.notify();
    });
    subscription.detach();

    let target_panel = Rc::clone(&self.target_panel);
    let subscription: Subscription = cx.subscribe_in(&self.surface_backend, window, move |app, _, event, _, cx| {
      let SelectEvent::Confirm(selected_id) = event;
      let Some(panel_id) = target_panel.get() else {
        return;
      };
      let backend = match selected_id.as_deref() {
        Some("implicit-scalar-field") => RenderSurfaceBackend::ImplicitScalarField,
        Some("msms") => RenderSurfaceBackend::Msms,
        _ => return,
      };
      submit_menu_command(app, panel_id, RenderCommand::SurfaceBackend(backend), cx);
    });
    subscription.detach();
  }
}

fn submit_menu_command(app: &mut ChitinApp, target: u64, command: RenderCommand, cx: &mut Context<ChitinApp>) {
  if let Err(error) = app.dispatch_render_command(Some(target), command, cx) {
    log::error!("{error}");
  }
  app.dismiss_document_options_menu();
  cx.notify();
}

/// Composes GPUI Kit's popover with the molecular document controls.
///
/// The framework owns anchoring, dismissal, nested overlays, and focus restoration;
/// the desktop records which panel receives representation changes.
pub(crate) fn render_document_options_button(
  panel_id: PanelId,
  options: MolecularDocumentOptions,
  open: bool,
  theme: ThemeColor,
  app: WeakEntity<ChitinApp>,
  controls: DocumentOptionsControls,
) -> impl IntoElement {
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

#[cfg(test)]
mod tests {
  use super::*;
  use crate::workbench::documents::state::{WgpuDocumentView, WgpuDocumentViewFactory};

  struct SurfaceProbe;

  impl gpui::Render for SurfaceProbe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
      gpui::div()
    }
  }

  #[gpui::test]
  fn backend_confirmation_survives_options_menu_dismissal(cx: &mut gpui::TestAppContext) {
    cx.update(gpui_kit::init);
    let selected_backend = Rc::new(Cell::new(MolecularSurfaceBackend::ImplicitScalarField));
    let (app, cx) = cx.add_window_view({
      let selected_backend = Rc::clone(&selected_backend);
      move |_, cx| {
        let view = WgpuDocumentView::with_representation_layers(
          cx.new(|_| SurfaceProbe),
          RepresentationLayers::empty(),
          |_, _| {},
        )
        .with_surface_backend(MolecularSurfaceBackend::ImplicitScalarField, move |backend, _| {
          selected_backend.set(backend)
        });
        ChitinApp::new_with_wgpu_document_panel(
          Some(std::path::PathBuf::from("/tmp")),
          cx.focus_handle(),
          "surface-probe",
          view,
          WgpuDocumentViewFactory::new(|_, cx| WgpuDocumentView::new(cx.new(|_| SurfaceProbe))),
        )
      }
    });
    cx.update(|window, cx| {
      app.update(cx, |app, cx| {
        let controls = DocumentOptionsControls::new(window, cx);
        controls.subscribe(window, cx);
        app.document_options_controls = Some(controls.clone());
        assert!(app.toggle_document_options_menu(app.document_panels.focused_panel_id));
        assert!(app.dismiss_document_options_menu());
        controls.surface_backend.update(cx, |_, cx| {
          cx.emit(SelectEvent::Confirm(Some("msms".into())));
        });
      });
    });
    cx.run_until_parked();
    assert_eq!(selected_backend.get(), MolecularSurfaceBackend::Msms);
  }

  #[gpui::test]
  fn menu_and_shell_share_targeted_render_updates_and_preserve_layers(cx: &mut gpui::TestAppContext) {
    use crate::commands::shell_host::{DesktopShellDispatch, DesktopShellHost};
    use chitin_builtin_shell::ShellInvocationSource;
    use chitin_molecule_renderer::PolymerStyle;

    cx.update(gpui_kit::init);
    let updates = Rc::new(Cell::new(0));
    let (app, cx) = cx.add_window_view({
      let updates = updates.clone();
      move |_, cx| {
        let view = WgpuDocumentView::with_representation_layers(
          cx.new(|_| SurfaceProbe),
          RepresentationLayers::atom(AtomStyle::Stick).with_polymer(PolymerStyle::Cartoon),
          move |_, _| updates.set(updates.get() + 1),
        )
        .with_surface_backend(MolecularSurfaceBackend::ImplicitScalarField, |_, _| {});
        ChitinApp::new_with_wgpu_document_panel(
          None,
          cx.focus_handle(),
          "probe",
          view,
          WgpuDocumentViewFactory::new(|_, cx| WgpuDocumentView::new(cx.new(|_| SurfaceProbe))),
        )
      }
    });
    cx.update(|window, cx| {
      app.update(cx, |app, cx| {
        let host = DesktopShellHost::new(chitin_command::CommandExecutionContext::new("."));
        let source = ShellInvocationSource::Interactive;
        assert!(
          app
            .submit_shell_line_with_host(&host, "render status", source.clone(), window, cx)
            .is_err()
        );
        assert!(
          host
            .session()
            .snapshot()
            .is_ok_and(|snapshot| snapshot.active.is_none())
        );
        assert!(
          app
            .submit_shell_line_with_host(&host, "panel enter 1", source.clone(), window, cx)
            .is_ok()
        );
        let controls = DocumentOptionsControls::new(window, cx);
        controls.subscribe(window, cx);
        app.document_options_controls = Some(controls.clone());
        assert!(app.toggle_document_options_menu(app.document_panels.focused_panel_id));
        controls
          .atom
          .update(cx, |_, cx| cx.emit(SelectEvent::Confirm(Some("sphere".into()))));
      });
    });
    cx.run_until_parked();
    assert_eq!(updates.get(), 1);
    cx.update(|window, cx| {
      app.update(cx, |app, cx| {
        let host = DesktopShellHost::new(chitin_command::CommandExecutionContext::new("."));
        let source = ShellInvocationSource::Interactive;
        assert!(app.submit_shell_line_with_host(&host, "panel enter 1", source.clone(), window, cx).is_ok());
        let result = app.submit_shell_line_with_host(&host, "render status", source.clone(), window, cx);
        assert!(matches!(result, Ok(DesktopShellDispatch::Frontend { output, .. }) if output.contains("Atom style: sphere") && output.contains("Polymer style: cartoon")));
        assert!(app.submit_shell_line_with_host(&host, "render atom style sphere", source.clone(), window, cx).is_ok());
        assert_eq!(updates.get(), 1, "unchanged settings do not rebuild the renderer");
        assert!(app.submit_shell_line_with_host(&host, "render surface backend msms", source.clone(), window, cx).is_ok());
        let result = app.submit_shell_line_with_host(&host, "render status", source, window, cx);
        assert!(matches!(result, Ok(DesktopShellDispatch::Frontend { output, .. }) if output.contains("Surface style: none") && output.contains("Surface backend: msms")));
      });
    });
  }
}
