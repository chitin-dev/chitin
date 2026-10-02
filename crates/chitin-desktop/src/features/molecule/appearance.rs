//! Appearance controls submit the same targeted requests as the built-in shell.

use crate::app::ChitinApp;
use chitin_command::{RenderColorScheme, RenderCommand, RenderLayer, RenderOpacity, RenderRgb};
use chitin_molecule_renderer::{
  RepresentationLayers,
  appearance::{ColorScheme, LayerAppearance},
};
use chitin_ui::widgets::select_item::IconSelectItem;
use gpui::{
  App, AppContext, Context, Entity, IntoElement, ParentElement, Styled, Window, div, prelude::FluentBuilder, px,
};
use gpui_kit::component::{
  IndexPath,
  color_picker::{ColorPickerEvent, ColorPickerState, ColorSelect},
  select::{Select, SelectEvent, SelectState},
  slider::{Slider, SliderEvent, SliderState, SliderValue},
  theme::ThemeColor,
};
use std::{cell::Cell, rc::Rc};

#[derive(Clone)]
pub(super) struct AppearanceControls {
  layers: [LayerControls; 3],
}

#[derive(Clone)]
struct LayerControls {
  scheme: Entity<SelectState<Vec<IconSelectItem>>>,
  color: Entity<ColorPickerState>,
  opacity: Entity<SliderState>,
  pending_opacity: Rc<Cell<Option<(u64, RenderCommand)>>>,
}

impl LayerControls {
  fn sync_opacity(&self, appearance: LayerAppearance, window: &mut Window, cx: &mut App) {
    // A coalesced command may not have reached the model yet. Do not roll the
    // thumb back to last frame's value while that command is waiting.
    if self.pending_opacity.get().is_none()
      && (self.opacity.read(cx).value().end() - appearance.opacity()).abs() > f32::EPSILON
    {
      self
        .opacity
        .update(cx, |state, cx| state.set_value(appearance.opacity(), window, cx));
    }
  }
}

fn scheme_id(scheme: ColorScheme) -> &'static str {
  match scheme {
    ColorScheme::Uniform => "uniform",
    ColorScheme::Element => "element",
    ColorScheme::Chain => "chain",
    ColorScheme::ChainElement => "chain-element",
  }
}

fn dispatch(app: &mut ChitinApp, target: u64, command: RenderCommand, cx: &mut Context<ChitinApp>) {
  if let Err(error) = app.dispatch_render_command(Some(target), command, cx) {
    log::error!("{error}");
  }
  cx.notify();
}

impl AppearanceControls {
  pub fn new(window: &mut Window, cx: &mut Context<ChitinApp>) -> Self {
    Self {
      layers: std::array::from_fn(|_| LayerControls {
        scheme: cx.new(|cx| {
          SelectState::new(
            [
              ("uniform", "Uniform"),
              ("element", "Element"),
              ("chain", "Chain"),
              ("chain-element", "Chain + element"),
            ]
            .into_iter()
            .map(|(id, name)| IconSelectItem::new(id, name))
            .collect::<Vec<_>>(),
            Some(IndexPath::new(0)),
            window,
            cx,
          )
        }),
        color: cx.new(|cx| ColorPickerState::new(window, cx)),
        opacity: cx.new(|_| SliderState::new().min(0.0).max(1.0).step(0.01).default_value(1.0)),
        pending_opacity: Rc::new(Cell::new(None)),
      }),
    }
  }
  pub fn subscribe(&self, target: Rc<Cell<Option<u64>>>, window: &mut Window, cx: &mut Context<ChitinApp>) {
    for (controls, layer) in self
      .layers
      .iter()
      .zip([RenderLayer::Atom, RenderLayer::Polymer, RenderLayer::Surface])
    {
      let destination = target.clone();
      cx.subscribe_in(&controls.scheme, window, move |app, _, event, _, cx| {
        let SelectEvent::Confirm(value) = event;
        let Some(target) = destination.get() else {
          return;
        };
        let scheme = match value.as_deref() {
          Some("uniform") => RenderColorScheme::Uniform,
          Some("element") => RenderColorScheme::Element,
          Some("chain") => RenderColorScheme::Chain,
          Some("chain-element") => RenderColorScheme::ChainElement,
          _ => return,
        };
        dispatch(
          app,
          target,
          RenderCommand::Color {
            layer,
            scheme,
            value: None,
          },
          cx,
        );
      })
      .detach();
      let destination = target.clone();
      cx.subscribe_in(&controls.color, window, move |app, _, event, _, cx| {
        let ColorPickerEvent::Change(Some(value)) = event else {
          return;
        };
        let Some(target) = destination.get() else {
          return;
        };
        let rgb = value.to_rgb();
        let value = RenderRgb([rgb.r, rgb.g, rgb.b].map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8));
        dispatch(
          app,
          target,
          RenderCommand::Color {
            layer,
            scheme: RenderColorScheme::Uniform,
            value: Some(value),
          },
          cx,
        );
      })
      .detach();
      let destination = target.clone();
      let pending = controls.pending_opacity.clone();
      cx.subscribe_in(&controls.opacity, window, move |app, _, event, window, cx| {
        let (SliderEvent::Change(SliderValue::Single(value)) | SliderEvent::Release(SliderValue::Single(value))) =
          event
        else {
          return;
        };
        let Some(target) = destination.get() else {
          return;
        };
        let Ok(value) = RenderOpacity::new(*value) else {
          return;
        };
        let command = RenderCommand::Opacity { layer, value };
        if matches!(event, SliderEvent::Release(_)) {
          pending.take();
          dispatch(app, target, command, cx);
          return;
        }
        if pending.replace(Some((target, command))).is_none() {
          let pending = pending.clone();
          cx.on_next_frame(window, move |app, _, cx| {
            if let Some((target, command)) = pending.take() {
              dispatch(app, target, command, cx);
            }
          });
        }
      })
      .detach();
    }
  }
  pub fn render_layer(
    &self,
    index: usize,
    appearance: LayerAppearance,
    enabled: bool,
    theme: ThemeColor,
    window: &mut Window,
    cx: &mut App,
  ) -> impl IntoElement {
    let controls = &self.layers[index];
    let id = scheme_id(appearance.scheme);
    let selected = ["uniform", "element", "chain", "chain-element"]
      .iter()
      .position(|value| *value == id)
      .map(IndexPath::new);
    if controls.scheme.read(cx).selected_index(cx) != selected {
      controls
        .scheme
        .update(cx, |state, cx| state.set_selected_index(selected, window, cx));
    }
    let rgb = ((appearance.color[0] as u32) << 16) | ((appearance.color[1] as u32) << 8) | appearance.color[2] as u32;
    let color: gpui::Hsla = gpui::rgb(rgb).into();
    if controls.color.read(cx).value() != Some(color) {
      controls
        .color
        .update(cx, |state, cx| state.set_value(color, window, cx));
    }
    controls.sync_opacity(appearance, window, cx);
    div()
      .flex()
      .flex_col()
      .gap_2()
      .child(div().text_xs().text_color(theme.muted_foreground).child("Color"))
      .child(Select::new(&controls.scheme).disabled(!enabled).w_full())
      .when(enabled && appearance.scheme == ColorScheme::Uniform, |container| {
        container.child(
          ColorSelect::new(&controls.color)
            .accessibility_label("Uniform color")
            .w_full(),
        )
      })
      .child(
        div()
          .text_xs()
          .text_color(theme.muted_foreground)
          .child(format!("Opacity  {:.0}%", appearance.opacity() * 100.0)),
      )
      .child(Slider::new(&controls.opacity).disabled(!enabled).w_full().h(px(20.0)))
  }
  pub fn render(
    &self,
    representation: RepresentationLayers,
    styles: [&Entity<SelectState<Vec<IconSelectItem>>>; 3],
    backend: &Entity<SelectState<Vec<IconSelectItem>>>,
    theme: ThemeColor,
    window: &mut Window,
    cx: &mut App,
  ) -> impl IntoElement {
    let enabled = [
      representation.atom_style().is_some(),
      representation.polymer_style().is_some(),
      representation.surface_style().is_some(),
    ];
    let mut container = div().flex().flex_col().gap_3().p_2();
    for (index, name) in ["Atom", "Polymer", "Surface"].into_iter().enumerate() {
      container = container.child(
        div()
          .flex()
          .flex_col()
          .gap_2()
          .child(div().text_sm().child(name))
          .child(div().text_xs().text_color(theme.muted_foreground).child("Style"))
          .child(Select::new(styles[index]).w_full())
          .child(self.render_layer(
            index,
            representation.appearances()[index],
            enabled[index],
            theme,
            window,
            cx,
          )),
      );
    }
    container
      .child(
        div()
          .text_xs()
          .text_color(theme.muted_foreground)
          .child("Surface backend"),
      )
      .child(Select::new(backend).w_full())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{
    commands::shell_host::DesktopShellHost,
    workbench::documents::{
      OpenedProjectDocument,
      layout::PanelSplitAxis,
      state::{WgpuDocumentView, WgpuDocumentViewFactory},
    },
  };
  use chitin_builtin_shell::ShellInvocationSource;
  use chitin_molecule_renderer::{AtomStyle, PolymerStyle, SurfaceStyle};

  struct Probe;
  impl gpui::Render for Probe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
      div()
    }
  }
  fn view(cx: &mut App) -> WgpuDocumentView {
    WgpuDocumentView::with_representation_layers(
      cx.new(|_| Probe),
      RepresentationLayers::atom(AtomStyle::Stick)
        .with_polymer(PolymerStyle::Cartoon)
        .with_surface(SurfaceStyle::Solid),
      |_, _| {},
    )
  }

  #[gpui::test]
  fn pending_opacity_should_not_be_overwritten_by_the_previous_model_value(cx: &mut gpui::TestAppContext) {
    cx.update(gpui_kit::init);
    let (_, visual) = cx.add_window_view(|_, _| Probe);
    visual.update(|window, cx| {
      let app = cx.new(|_| ChitinApp::new(None));
      let controls = app.update(cx, |_, cx| AppearanceControls::new(window, cx));
      let layer = &controls.layers[0];
      layer.opacity.update(cx, |state, cx| state.set_value(0.35, window, cx));
      let Ok(value) = RenderOpacity::new(0.35) else {
        panic!("valid opacity should be accepted");
      };
      layer.pending_opacity.set(Some((
        1,
        RenderCommand::Opacity {
          layer: RenderLayer::Atom,
          value,
        },
      )));
      let appearance = LayerAppearance::new(ColorScheme::Element);
      layer.sync_opacity(appearance, window, cx);
      assert_eq!(layer.opacity.read(cx).value(), SliderValue::Single(0.35));
      layer.pending_opacity.take();
      layer.sync_opacity(appearance, window, cx);
      assert_eq!(layer.opacity.read(cx).value(), SliderValue::Single(1.0));
    });
    visual.run_until_parked();
  }

  #[gpui::test]
  fn controls_and_shell_should_share_appearance_on_a_stable_inactive_tab(cx: &mut gpui::TestAppContext) {
    cx.update(gpui_kit::init);
    let (app, visual) = cx.add_window_view(|_, cx| {
      ChitinApp::new_with_wgpu_document_panel(
        None,
        cx.focus_handle(),
        "probe",
        view(cx),
        WgpuDocumentViewFactory::new(|_, cx| view(cx)),
      )
    });
    let controls = visual.update(|window, cx| {
      app.update(cx, |app, cx| {
        let controls = AppearanceControls::new(window, cx);
        controls.subscribe(Rc::new(Cell::new(Some(1))), window, cx);
        app.open_project_document(OpenedProjectDocument::new(std::path::Path::new("/tmp/notes.txt")));
        controls.layers[0]
          .scheme
          .update(cx, |_, cx| cx.emit(SelectEvent::Confirm(Some("chain".into()))));
        // Release commits the final value even if a change was waiting for a frame.
        controls.layers[2]
          .opacity
          .update(cx, |_, cx| cx.emit(SliderEvent::Release(SliderValue::Single(0.35))));
        controls
      })
    });
    visual.run_until_parked();
    visual.update(|window, cx| {
      app.update(cx, |app, cx| {
        let Some((_, layers, _)) = app.document_panels.rendering_settings(1) else {
          panic!("probe should remain open");
        };
        assert_eq!(layers.appearances()[0].scheme, ColorScheme::Chain);
        assert_eq!(layers.appearances()[2].opacity(), 0.35);
        assert_eq!(layers.polymer_style(), Some(PolymerStyle::Cartoon));
        let host = DesktopShellHost::new(chitin_command::CommandExecutionContext::new("."));
        assert!(
          app
            .submit_shell_line_with_host(&host, "panel enter 1", ShellInvocationSource::Interactive, window, cx)
            .is_ok()
        );
        assert!(
          app
            .submit_shell_line_with_host(
              &host,
              "render atom color uniform --value \"#FF0000\"",
              ShellInvocationSource::Interactive,
              window,
              cx
            )
            .is_ok()
        );
        assert!(
          app
            .submit_shell_line_with_host(
              &host,
              "render surface style none",
              ShellInvocationSource::Interactive,
              window,
              cx
            )
            .is_ok()
        );
        assert!(
          app
            .submit_shell_line_with_host(
              &host,
              "render surface opacity 0",
              ShellInvocationSource::Interactive,
              window,
              cx
            )
            .is_ok()
        );
        let Some((_, layers, _)) = app.document_panels.rendering_settings(1) else {
          panic!("probe should remain open");
        };
        assert_eq!(layers.appearances()[0].color, [255, 0, 0]);
        assert_eq!(layers.surface_style(), None);
        assert_eq!(layers.appearances()[2].opacity(), 0.0);
        let root = app.document_panels.focused_panel_id;
        assert!(app.split_document_panel(root, PanelSplitAxis::Horizontal, window, cx));
        let target = app
          .document_panels
          .rendering_panels()
          .into_iter()
          .find(|panel| panel.id() != 1);
        assert!(target.is_some_and(|panel| {
          app
            .document_panels
            .rendering_settings(panel.id())
            .is_some_and(|(_, copied, _)| copied == layers)
        }));
        drop(controls);
      })
    });
  }
}
