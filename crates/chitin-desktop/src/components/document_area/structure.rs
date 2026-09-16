//! Structure-file loading and molecular WGPU scene integration.

use std::{cell::Cell, path::Path, rc::Rc, sync::Arc};

use chitin_bio::{
  structure::{MmcifParser, PdbParser, StructureScene},
  surface::{MolecularSurfaceArtifact, MolecularSurfaceRequest, generate_molecular_surface},
};
use chitin_molecule_renderer::{
  AtomStyle, BallAndStickStyle, MoleculeDebugMode, MoleculeRenderInput, MoleculeRenderer, PolymerStyle,
  RepresentationLayers,
};
use chitin_ui::composite::toast::{Toast, ToastVariant, ToastViewport};
use gpui::{App, AppContext, AsyncApp, Entity, Window};

use crate::components::{
  document_area::state::WgpuDocumentView,
  wgpu_panel::{ChitinWgpuDocumentPanel, WgpuPanelFrame, WgpuPanelScene},
};

/// Default representation used when GPUI opens a PDB or mmCIF document.
const DEFAULT_STRUCTURE_REPRESENTATION: RepresentationLayers =
  RepresentationLayers::atom(AtomStyle::Stick).with_polymer(PolymerStyle::Cartoon);

/// Molecular scene backed by one parsed structure and a selectable representation.
pub(crate) struct StructureMoleculeScene {
  /// Renderer-neutral structure data shared by split-panel clones.
  scene: Arc<StructureScene>,
  /// GPU resources created lazily after a surface device is available.
  renderer: Option<MoleculeRenderer>,
  /// Scientific surface geometry computed independently of GPU resources.
  surface: Option<MolecularSurfaceArtifact>,
  /// Shader output selected through the optional debug environment variable.
  debug_mode: MoleculeDebugMode,
  /// Representation layers currently rendered by this scene.
  representation: RepresentationLayers,
}

impl StructureMoleculeScene {
  /// Creates a lazy molecular scene with the selected representation layers.
  pub(crate) fn new(scene: Arc<StructureScene>, representation: RepresentationLayers) -> Self {
    Self {
      scene,
      renderer: None,
      surface: None,
      debug_mode: MoleculeDebugMode::Final,
      representation,
    }
  }
}

impl WgpuPanelScene for StructureMoleculeScene {
  /// Renders one fitted molecular frame into the GPUI-owned surface.
  fn render_frame(&mut self, frame: WgpuPanelFrame<'_>) -> wgpu::SubmissionIndex {
    let renderer = self.renderer.get_or_insert_with(|| {
      MoleculeRenderer::new_with_layers(
        Arc::new(frame.device.clone()),
        Arc::new(frame.queue.clone()),
        frame.size,
        frame.format,
        MoleculeRenderInput {
          scene: &self.scene,
          surface: self.surface.as_ref(),
        },
        self.representation,
        &BallAndStickStyle::default(),
      )
    });
    renderer.resize_if_needed(frame.size);
    renderer.set_debug_mode(self.debug_mode);
    renderer.render(
      frame.view,
      frame.camera.view_matrix(),
      frame.camera.projection_matrix(renderer.aspect()),
    )
  }

  /// Returns the interaction hint displayed over a molecular viewport.
  fn interaction_hint(&self) -> &'static str {
    match (self.representation.atom_style(), self.representation.polymer_style()) {
      (Some(AtomStyle::Stick), None) => "Atom style: Stick | L-drag rotate | Shift-L/M-drag pan | R-drag/wheel zoom",
      (Some(AtomStyle::BallAndStick), None) => {
        "Atom style: Ball and stick | L-drag rotate | Shift-L/M-drag pan | R-drag/wheel zoom"
      }
      (Some(AtomStyle::Sphere), None) => "Atom style: Sphere | L-drag rotate | Shift-L/M-drag pan | R-drag/wheel zoom",
      (Some(AtomStyle::Stick), Some(PolymerStyle::Cartoon)) => {
        "Molecule representations: Stick + Cartoon | L-drag rotate | Shift-L/M-drag pan | R-drag/wheel zoom"
      }
      (_, Some(PolymerStyle::Cartoon)) => {
        "Molecule representation includes Cartoon | L-drag rotate | Shift-L/M-drag pan | R-drag/wheel zoom"
      }
      (None, None) => "No molecule representation | L-drag rotate | Shift-L/M-drag pan | R-drag/wheel zoom",
    }
  }

  fn set_representation_layers(&mut self, representation: RepresentationLayers) -> bool {
    if self.representation == representation {
      return false;
    }
    self.representation = representation;
    if let Some(renderer) = self.renderer.as_mut() {
      renderer.set_representation_layers(
        MoleculeRenderInput {
          scene: &self.scene,
          surface: self.surface.as_ref(),
        },
        representation,
        &BallAndStickStyle::default(),
      );
    }
    true
  }

  fn set_molecular_surface(&mut self, surface: MolecularSurfaceArtifact) -> bool {
    let is_visible = self.representation.surface_style().is_some();
    if is_visible && let Some(renderer) = self.renderer.as_mut() {
      renderer.set_surface_artifact(&self.scene, &surface, &BallAndStickStyle::default());
    }
    self.surface = Some(surface);
    is_visible
  }
}

/// Creates a WGPU document view from an already parsed, renderer-neutral scene.
///
/// This function must run on the GPUI event thread because surface and entity
/// creation require `Window` and `App`. File reading and parsing should happen
/// before this function is called.
///
/// # Parameters
///
/// * `scene` is the parsed structure scene shared with the renderer and surface task.
/// * `toast_viewport` receives surface lifecycle notifications without re-entering root app state.
/// * `window` creates the WGPU surface and identifies the window updated on completion.
/// * `cx` creates the panel entity and schedules background surface generation.
///
/// # Returns
///
/// A molecular document view whose surface representation is generated lazily.
pub(crate) fn build_structure_view_from_scene(
  scene: Arc<StructureScene>,
  toast_viewport: Entity<ToastViewport>,
  window: &mut Window,
  cx: &mut App,
) -> WgpuDocumentView {
  let surface = window.create_wgpu_surface(960, 540, wgpu::TextureFormat::Rgba8UnormSrgb);
  let calculation_scene = Arc::clone(&scene);
  let panel = cx.new(|_| {
    ChitinWgpuDocumentPanel::new_with_scene(
      surface,
      StructureMoleculeScene::new(scene, DEFAULT_STRUCTURE_REPRESENTATION),
    )
  });
  let controlled_panel = panel.clone();
  let surface_calculation_started = Rc::new(Cell::new(false));
  let window_handle = window.window_handle();
  WgpuDocumentView::with_representation_layers(panel, DEFAULT_STRUCTURE_REPRESENTATION, move |representation, cx| {
    controlled_panel.update(cx, |panel, cx| {
      if panel.set_representation_layers(representation) {
        cx.notify();
      }
    });
    if representation.surface_style().is_none() || surface_calculation_started.replace(true) {
      return;
    }

    let info_toast = toast_viewport.update(cx, |viewport, cx| {
      viewport.push(
        Toast::new("Generating molecular surface")
          .description("Computing the solvent-excluded surface in the background.")
          .variant(ToastVariant::Info)
          .duration(None),
        cx,
      )
    });
    let scene = Arc::clone(&calculation_scene);
    let panel = controlled_panel.downgrade();
    let toast_viewport = toast_viewport.clone();
    let calculation_started = Rc::clone(&surface_calculation_started);
    cx.spawn(async move |async_cx: &mut AsyncApp| {
      let result = async_cx
        .background_executor()
        .spawn(async move {
          let surface = generate_molecular_surface(&scene, MolecularSurfaceRequest::default());
          if molecular_surface_has_geometry(&surface) {
            Ok(surface)
          } else {
            Err("The calculation produced no renderable surface geometry.".to_string())
          }
        })
        .await;
      let _ = async_cx.update_window(window_handle, |_, _, cx| {
        toast_viewport.update(cx, |viewport, cx| {
          viewport.dismiss(info_toast, cx);
          match &result {
            Ok(_) => {
              viewport.push(
                Toast::new("Molecular surface ready")
                  .description("Surface generation completed successfully.")
                  .variant(ToastVariant::Success),
                cx,
              );
            }
            Err(error) => {
              calculation_started.set(false);
              log::error!("molecular surface generation failed: {error}");
              viewport.push(
                Toast::new("Molecular surface generation failed")
                  .description(error.clone())
                  .variant(ToastVariant::Error),
                cx,
              );
            }
          }
        });
        if let Ok(surface) = result {
          let _ = panel.update(cx, |panel, cx| {
            if panel.set_molecular_surface(surface) {
              cx.notify();
            }
          });
        }
      });
    })
    .detach();
  })
}

/// Reports whether a generated artifact contains triangles the renderer can draw.
fn molecular_surface_has_geometry(surface: &MolecularSurfaceArtifact) -> bool {
  surface.domains.iter().any(|domain| !domain.mesh.indices.is_empty())
}

/// Loads a local PDB or mmCIF file and extracts its first renderable model.
pub(crate) fn load_structure_scene(path: &Path) -> Result<Arc<StructureScene>, String> {
  let bytes = std::fs::read(path).map_err(|error| format!("cannot read '{}': {error}", path.display()))?;
  let extension = path
    .extension()
    .and_then(|extension| extension.to_str())
    .map(str::to_ascii_lowercase);
  let structure = match extension.as_deref() {
    Some("pdb") | Some("ent") => PdbParser::new()
      .parse_bytes(&bytes)
      .map(|parsed| parsed.structure)
      .map_err(|error| error.to_string())?,
    Some("cif") | Some("mmcif") => MmcifParser::new()
      .parse_bytes(&bytes)
      .map(|parsed| parsed.structure)
      .map_err(|error| error.to_string())?,
    _ => return Err("expected a .pdb, .ent, .cif, or .mmcif file".to_string()),
  };
  StructureScene::from_first_model(&structure)
    .map(Arc::new)
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
  use super::*;
  use chitin_molecule_renderer::SurfaceStyle;

  fn single_atom_scene() -> Arc<StructureScene> {
    let parsed = PdbParser::new()
      .parse_bytes(b"ATOM      1  C   GLY A   1       0.000   0.000   0.000  1.00 10.00           C  \nEND\n")
      .unwrap_or_else(|error| panic!("surface fixture should parse: {error}"));
    StructureScene::from_first_model(&parsed.structure)
      .map(Arc::new)
      .unwrap_or_else(|error| panic!("surface fixture should produce a scene: {error}"))
  }

  #[test]
  fn enabling_surface_should_defer_calculation() {
    let mut scene = StructureMoleculeScene::new(single_atom_scene(), DEFAULT_STRUCTURE_REPRESENTATION);
    let representation = DEFAULT_STRUCTURE_REPRESENTATION.with_surface(SurfaceStyle::Solid);

    assert!(scene.set_representation_layers(representation));
    assert!(scene.surface.is_none());
    assert_eq!(scene.representation, representation);
  }

  #[test]
  fn completed_surface_should_remain_cached_when_hidden() {
    let structure = single_atom_scene();
    let surface = generate_molecular_surface(&structure, MolecularSurfaceRequest::default());
    let mut scene = StructureMoleculeScene::new(structure, DEFAULT_STRUCTURE_REPRESENTATION);

    assert!(!scene.set_molecular_surface(surface));
    assert!(scene.surface.is_some());
  }

  #[test]
  fn surface_without_domains_should_not_be_renderable() {
    let surface = MolecularSurfaceArtifact {
      request: MolecularSurfaceRequest::default(),
      domains: Vec::new(),
    };

    assert!(!molecular_surface_has_geometry(&surface));
  }
}
