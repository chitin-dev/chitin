//! Entry-local molecular editors submitting typed requests through the command bus.

use std::{cell::Cell, rc::Rc};

use crate::app::ChitinApp;
use chitin_bio::surface::MolecularSurfaceBackend;
use chitin_command::{
  RenderAtomStyle, RenderCommand, RenderLayer, RenderPolymerStyle, RenderSurfaceBackend, RenderSurfaceStyle,
};
use chitin_molecule_renderer::{AtomStyle, RepresentationLayers};
use chitin_ui::widgets::select_item::IconSelectItem;
use gpui::{App, AppContext, Context, Entity, IntoElement, ParentElement, Styled, Subscription, Window, div};
use gpui_kit::component::{
  Icon, IndexPath, Sizable as _,
  button::{Button, ButtonVariants as _},
  collapsible::Collapsible,
  select::{Select, SelectEvent, SelectItem, SelectState},
  theme::ThemeColor,
};

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
/// Persistent representation and appearance controls for one entry's inline editors.
#[derive(Clone)]
pub(crate) struct MolecularSettingsControls {
  /// Shared color and opacity controls, including coalesced slider edits.
  appearance: super::appearance::AppearanceControls,
  /// Active entry identity and rendering view guard for delayed confirmations.
  target_entry: Rc<Cell<Option<(u64, gpui::EntityId)>>>,
  /// Surface calculation controls are secondary to the layer's appearance.
  advanced: Rc<Cell<bool>>,
  app: gpui::WeakEntity<ChitinApp>,
  /// Parent focus scope keeps an inline editor mounted while its controls are focused.
  focus: gpui::FocusHandle,
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

impl MolecularSettingsControls {
  /// Creates persistent selectors and appearance controls for one entry.
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
      target_entry: Rc::new(Cell::new(None)),
      advanced: Rc::new(Cell::new(false)),
      app: cx.weak_entity(),
      focus: cx.focus_handle(),
      appearance: super::appearance::AppearanceControls::new(window, cx),
      atom,
      polymer,
      surface,
      surface_backend,
    }
  }

  /// Returns whether the Surface backend editor contributes to row height.
  pub(crate) fn advanced_open(&self) -> bool {
    self.advanced.get()
  }

  /// Returns the inline editor's enclosing focus scope.
  pub(crate) fn focus_handle(&self) -> gpui::FocusHandle {
    self.focus.clone()
  }

  /// Opens the uniform-color picker after its inline editor is expanded.
  pub(crate) fn open_color(&self, layer: RenderLayer, window: &mut Window, cx: &mut App) {
    self.appearance.open_color(layer, window, cx);
  }

  /// Binds controls to the active entry and rejects events from obsolete views.
  pub(crate) fn bind_entry(&self, target: Option<(u64, gpui::EntityId)>) {
    if self.target_entry.get() != target {
      self.appearance.clear_pending();
    }
    self.target_entry.set(target);
  }

  /// Renders properties for one inspected entry-wide layer, never a topology scope.
  ///
  /// Backend controls belong only to Surface. Disabled layers keep their style
  /// selector available so they can be enabled, while appearance editors are disabled.
  pub(crate) fn render_layer(
    &self,
    layer: RenderLayer,
    representation: RepresentationLayers,
    backend: Option<MolecularSurfaceBackend>,
    theme: ThemeColor,
    window: &mut Window,
    cx: &mut App,
  ) -> impl IntoElement {
    sync_representation_selectors(self, representation, backend, window, cx);
    let (index, style, enabled) = match layer {
      RenderLayer::Atom => (0, &self.atom, representation.atom_style().is_some()),
      RenderLayer::Polymer => (1, &self.polymer, representation.polymer_style().is_some()),
      RenderLayer::Surface => (2, &self.surface, representation.surface_style().is_some()),
    };
    let mut content = div()
      .flex()
      .flex_col()
      .w_full()
      .min_w_0()
      .max_w(super::appearance::EDITOR_MAX_WIDTH)
      .gap_1()
      .child(super::appearance::property_field(
        "Style",
        Select::new(style).small().w_full(),
        theme,
      ))
      .child(
        self
          .appearance
          .render_layer(index, representation.appearances()[index], enabled, theme, window, cx),
      );
    if layer == RenderLayer::Surface && backend.is_some() {
      let advanced = self.advanced.clone();
      let app = self.app.clone();
      content = content.child(
        Collapsible::new()
          .open(advanced.get())
          .child(
            Button::new(("entry-surface-advanced", self.surface_backend.entity_id()))
              .ghost()
              .small()
              .w_full()
              .justify_start()
              .child(
                div()
                  .flex()
                  .w_full()
                  .items_center()
                  .gap_1()
                  .text_xs()
                  .child(Icon::default().path(if advanced.get() {
                    "icons/tree-expand.svg"
                  } else {
                    "icons/tree-collapse.svg"
                  }))
                  .child("Advanced"),
              )
              .on_click(move |_, _, cx| {
                advanced.set(!advanced.get());
                let _ = app.update(cx, |_, cx| cx.notify());
              }),
          )
          .content(div().flex().flex_col().gap_1().child(super::appearance::property_field(
            "Backend",
            Select::new(&self.surface_backend).small().w_full(),
            theme,
          ))),
      );
    }
    content
  }

  /// Routes selector and appearance events through the shared command/undo bus.
  ///
  /// Confirmation is deferred by Kit, so each event verifies the bound rendering
  /// entity before applying scene-specific settings. Subscriptions live with the app.
  pub(crate) fn subscribe(&self, window: &mut Window, cx: &mut Context<ChitinApp>) {
    self.appearance.subscribe(self.target_entry.clone(), window, cx);
    let target_entry = Rc::clone(&self.target_entry);
    let subscription: Subscription = cx.subscribe_in(&self.atom, window, move |app, _, event, _, cx| {
      let SelectEvent::Confirm(selected_id) = event;
      let Some((panel_id, revision)) = target_entry.get() else {
        return;
      };
      if app.document_panels.rendering_view_id(panel_id) != Some(revision) {
        return;
      }
      let style = match selected_id.as_deref() {
        Some("none") => RenderAtomStyle::None,
        Some("stick") => RenderAtomStyle::Stick,
        Some("ball-and-stick") => RenderAtomStyle::BallAndStick,
        Some("sphere") => RenderAtomStyle::Sphere,
        _ => return,
      };
      submit_settings_command(app, panel_id, RenderCommand::AtomStyle(style), cx);
      cx.notify();
    });
    subscription.detach();

    let target_entry = Rc::clone(&self.target_entry);
    let subscription: Subscription = cx.subscribe_in(&self.polymer, window, move |app, _, event, _, cx| {
      let SelectEvent::Confirm(selected_id) = event;
      let Some((panel_id, revision)) = target_entry.get() else {
        return;
      };
      if app.document_panels.rendering_view_id(panel_id) != Some(revision) {
        return;
      }
      let style = match selected_id.as_deref() {
        Some("none") => RenderPolymerStyle::None,
        Some("cartoon") => RenderPolymerStyle::Cartoon,
        _ => return,
      };
      submit_settings_command(app, panel_id, RenderCommand::PolymerStyle(style), cx);
      cx.notify();
    });
    subscription.detach();

    let target_entry = Rc::clone(&self.target_entry);
    let subscription: Subscription = cx.subscribe_in(&self.surface, window, move |app, _, event, _, cx| {
      let SelectEvent::Confirm(selected_id) = event;
      let Some((panel_id, revision)) = target_entry.get() else {
        return;
      };
      if app.document_panels.rendering_view_id(panel_id) != Some(revision) {
        return;
      }
      let style = match selected_id.as_deref() {
        Some("none") => RenderSurfaceStyle::None,
        Some("solid") => RenderSurfaceStyle::Solid,
        _ => return,
      };
      submit_settings_command(app, panel_id, RenderCommand::SurfaceStyle(style), cx);
      cx.notify();
    });
    subscription.detach();

    let target_entry = Rc::clone(&self.target_entry);
    let subscription: Subscription = cx.subscribe_in(&self.surface_backend, window, move |app, _, event, _, cx| {
      let SelectEvent::Confirm(selected_id) = event;
      let Some((panel_id, revision)) = target_entry.get() else {
        return;
      };
      if app.document_panels.rendering_view_id(panel_id) != Some(revision) {
        return;
      }
      let backend = match selected_id.as_deref() {
        Some("implicit-scalar-field") => RenderSurfaceBackend::ImplicitScalarField,
        Some("msms") => RenderSurfaceBackend::Msms,
        _ => return,
      };
      submit_settings_command(app, panel_id, RenderCommand::SurfaceBackend(backend), cx);
    });
    subscription.detach();
  }
}

fn submit_settings_command(app: &mut ChitinApp, target: u64, command: RenderCommand, cx: &mut Context<ChitinApp>) {
  let error = app
    .dispatch_render_command(Some(target), command, cx)
    .err()
    .map(|error| error.to_string());
  app.entries_sidebar.set_error(error);
  cx.notify();
}

/// Synchronizes representation and surface-backend selectors with the active view.
///
/// # Parameters
///
/// * `controls` contains the persistent selector states shared by the Entries inspector.
/// * `representation` and `surface_backend` supply the current renderer settings.
/// * `window` and `cx` update selector state without emitting confirmation events.
///
/// # Returns
///
/// Nothing. Only differing selections are updated, preventing a render-time
/// synchronization from dispatching another renderer command.
fn sync_representation_selectors(
  controls: &MolecularSettingsControls,
  representation: RepresentationLayers,
  surface_backend: Option<MolecularSurfaceBackend>,
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
    Some(MolecularSurfaceBackend::ImplicitScalarField) => "implicit-scalar-field",
    Some(MolecularSurfaceBackend::Msms) => "msms",
    None => "implicit-scalar-field",
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
  fn switching_entries_should_isolate_settings_and_reject_stale_views(cx: &mut gpui::TestAppContext) {
    cx.update(gpui_kit::init);
    fn view(cx: &mut App) -> WgpuDocumentView {
      WgpuDocumentView::with_representation_layers(
        cx.new(|_| SurfaceProbe),
        RepresentationLayers::atom(AtomStyle::Stick),
        |_, _| {},
      )
    }
    let (app, cx) = cx.add_window_view(|_, cx| {
      ChitinApp::new_with_wgpu_document_panel(
        None,
        cx.focus_handle(),
        "settings probe",
        view(cx),
        WgpuDocumentViewFactory::new(|_, cx| view(cx)),
      )
    });
    let controls = cx.update(|window, cx| {
      app.update(cx, |app, cx| {
        assert!(app.split_document_panel(
          app.document_panels.focused_panel_id,
          crate::workbench::documents::layout::PanelSplitAxis::Horizontal,
          window,
          cx,
        ));
        let controls = MolecularSettingsControls::new(window, cx);
        controls.subscribe(window, cx);
        controls
      })
    });
    for (target, style) in [(1, "sphere"), (2, "ball-and-stick")] {
      cx.update(|_, cx| {
        app.update(cx, |app, cx| {
          let revision = app
            .document_panels
            .rendering_view_id(target)
            .unwrap_or_else(|| panic!("view"));
          controls.bind_entry(Some((target, revision)));
          controls
            .atom
            .update(cx, |_, cx| cx.emit(SelectEvent::Confirm(Some(style.into()))));
        });
      });
      cx.run_until_parked();
    }
    // A replaced view or an unmounted inspector must not accept old confirmations.
    for bound in [true, false] {
      cx.update(|_, cx| {
        app.update(cx, |app, cx| {
          let foreign_view = app
            .document_panels
            .rendering_view_id(2)
            .unwrap_or_else(|| panic!("view"));
          controls.bind_entry(bound.then_some((1, foreign_view)));
          controls
            .atom
            .update(cx, |_, cx| cx.emit(SelectEvent::Confirm(Some("none".into()))));
        });
      });
      cx.run_until_parked();
    }
    cx.update(|_, cx| {
      let app = app.read(cx);
      for (target, expected) in [(1, AtomStyle::Sphere), (2, AtomStyle::BallAndStick)] {
        let (_, layers, _) = app
          .document_panels
          .rendering_settings(target)
          .unwrap_or_else(|| panic!("settings"));
        assert_eq!(layers.atom_style(), Some(expected));
      }
    });
  }

  #[gpui::test]
  fn backend_confirmation_targets_the_bound_entry(cx: &mut gpui::TestAppContext) {
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
    let _controls = cx.update(|window, cx| {
      app.update(cx, |app, cx| {
        let controls = MolecularSettingsControls::new(window, cx);
        controls.subscribe(window, cx);
        let revision = app
          .document_panels
          .rendering_view_id(1)
          .unwrap_or_else(|| panic!("rendering view"));
        controls.bind_entry(Some((1, revision)));
        controls.surface_backend.update(cx, |_, cx| {
          cx.emit(SelectEvent::Confirm(Some("msms".into())));
        });
        controls
      })
    });
    cx.run_until_parked();
    assert_eq!(selected_backend.get(), MolecularSurfaceBackend::Msms);
  }

  #[gpui::test]
  fn inspector_and_shell_share_targeted_render_updates_and_preserve_layers(cx: &mut gpui::TestAppContext) {
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
    let _controls = cx.update(|window, cx| {
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
        let controls = MolecularSettingsControls::new(window, cx);
        controls.subscribe(window, cx);
        let revision = app
          .document_panels
          .rendering_view_id(1)
          .unwrap_or_else(|| panic!("rendering view"));
        controls.bind_entry(Some((1, revision)));
        controls
          .atom
          .update(cx, |_, cx| cx.emit(SelectEvent::Confirm(Some("sphere".into()))));
        controls
      })
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
