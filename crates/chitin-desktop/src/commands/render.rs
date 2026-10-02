//! Shared presentation command bus for menus, shell sessions, and typed callers.

use chitin_bio::surface::MolecularSurfaceBackend;
use chitin_command::{RenderAtomStyle, RenderCommand, RenderPolymerStyle, RenderSurfaceBackend, RenderSurfaceStyle};
use chitin_command::{RenderColorScheme, RenderLayer};
use chitin_molecule_renderer::appearance::ColorScheme;
use chitin_molecule_renderer::{AtomStyle, PolymerStyle, SurfaceStyle};
use gpui::Context;

use crate::app::ChitinApp;

/// A presentation request could not resolve its explicit target.
#[derive(Debug, thiserror::Error)]
pub enum RenderCommandError {
  #[error("--value is only valid with the uniform coloring scheme")]
  UnexpectedColor,
  #[error("no rendering panel context; use 'panel list' and 'panel enter <ID>' first")]
  MissingContext,
  #[error("rendering panel #{id} is not open; use 'panel list' to see available views")]
  Unavailable { id: u64 },
  #[error("rendering panel #{id} does not support surface backend selection")]
  UnsupportedBackend { id: u64 },
}

impl ChitinApp {
  /// Applies one presentation request to a stable tab without changing UI focus.
  pub(crate) fn dispatch_render_command(
    &mut self,
    target: Option<u64>,
    command: RenderCommand,
    cx: &mut Context<Self>,
  ) -> Result<String, RenderCommandError> {
    let id = target.ok_or(RenderCommandError::MissingContext)?;
    let (title, mut layers, backend) = self
      .document_panels
      .rendering_settings(id)
      .ok_or(RenderCommandError::Unavailable { id })?;
    match command {
      RenderCommand::Status => {}
      RenderCommand::Color { layer, scheme, value } => {
        if value.is_some() && scheme != RenderColorScheme::Uniform {
          return Err(RenderCommandError::UnexpectedColor);
        }
        let index = layer_index(layer);
        let mut appearance = layers.appearances();
        appearance[index].scheme = match scheme {
          RenderColorScheme::Uniform => ColorScheme::Uniform,
          RenderColorScheme::Element => ColorScheme::Element,
          RenderColorScheme::Chain => ColorScheme::Chain,
          RenderColorScheme::ChainElement => ColorScheme::ChainElement,
        };
        if let Some(value) = value {
          appearance[index] = appearance[index].with_color(value.0);
        }
        layers = layers.with_appearances(appearance);
      }
      RenderCommand::Opacity { layer, value } => {
        let index = layer_index(layer);
        let mut appearance = layers.appearances();
        if let Some(updated) = appearance[index].with_opacity(value.value()) {
          appearance[index] = updated;
        }
        layers = layers.with_appearances(appearance);
      }
      RenderCommand::AtomStyle(style) => {
        layers = match style {
          RenderAtomStyle::None => layers.without_atom(),
          RenderAtomStyle::Stick => layers.with_atom(AtomStyle::Stick),
          RenderAtomStyle::BallAndStick => layers.with_atom(AtomStyle::BallAndStick),
          RenderAtomStyle::Sphere => layers.with_atom(AtomStyle::Sphere),
        };
      }
      RenderCommand::PolymerStyle(style) => {
        layers = match style {
          RenderPolymerStyle::None => layers.without_polymer(),
          RenderPolymerStyle::Cartoon => layers.with_polymer(PolymerStyle::Cartoon),
        };
      }
      RenderCommand::SurfaceStyle(style) => {
        layers = match style {
          RenderSurfaceStyle::None => layers.without_surface(),
          RenderSurfaceStyle::Solid => layers.with_surface(SurfaceStyle::Solid),
        };
      }
      RenderCommand::SurfaceBackend(value) => {
        backend.ok_or(RenderCommandError::UnsupportedBackend { id })?;
        let value = match value {
          RenderSurfaceBackend::ImplicitScalarField => MolecularSurfaceBackend::ImplicitScalarField,
          RenderSurfaceBackend::Msms => MolecularSurfaceBackend::Msms,
        };
        if let Some(on_change) = self.document_panels.select_surface_backend(id, value) {
          on_change(value, cx);
          cx.notify();
        }
      }
    }
    if let Some(on_change) = self.document_panels.select_representation_layers(id, layers) {
      on_change(layers, cx);
      cx.notify();
    }
    let (_, layers, backend) = self
      .document_panels
      .rendering_settings(id)
      .ok_or(RenderCommandError::Unavailable { id })?;
    let atom = match layers.atom_style() {
      None => "none",
      Some(AtomStyle::Stick) => "stick",
      Some(AtomStyle::BallAndStick) => "ball-and-stick",
      Some(AtomStyle::Sphere) => "sphere",
    };
    let polymer = match layers.polymer_style() {
      None => "none",
      Some(PolymerStyle::Cartoon) => "cartoon",
    };
    let surface = match layers.surface_style() {
      None => "none",
      Some(SurfaceStyle::Solid) => "solid",
    };
    let backend = match backend {
      None => "unavailable",
      Some(MolecularSurfaceBackend::ImplicitScalarField) => "implicit-scalar-field",
      Some(MolecularSurfaceBackend::Msms) => "msms",
    };
    let mut output = format!(
      "Panel #{id}: {}\nAtom style: {atom}\nPolymer style: {polymer}\nSurface style: {surface}\nSurface backend: {backend}",
      super::terminal_label(&title)
    );
    for (name, appearance) in ["Atom", "Polymer", "Surface"].into_iter().zip(layers.appearances()) {
      let scheme = match appearance.scheme {
        ColorScheme::Uniform => "uniform",
        ColorScheme::Element => "element",
        ColorScheme::Chain => "chain",
        ColorScheme::ChainElement => "chain-element",
      };
      let [r, g, b] = appearance.color;
      output.push_str(&format!(
        "\n{name} color: {scheme} (#{r:02X}{g:02X}{b:02X}) | opacity: {:.4}",
        appearance.opacity()
      ));
    }
    Ok(output)
  }
}

fn layer_index(layer: RenderLayer) -> usize {
  match layer {
    RenderLayer::Atom => 0,
    RenderLayer::Polymer => 1,
    RenderLayer::Surface => 2,
  }
}
