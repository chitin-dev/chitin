//! Structure-file loading and molecular WGPU scene integration.

use std::{path::Path, sync::Arc};

use chitin_bio::structure::{MmcifParser, PdbParser, StructureScene};
use chitin_wgpu::{AtomRepresentation, BallAndStickStyle, MoleculeDebugMode, MoleculeRenderer};
use gpui::{App, AppContext, Window};

use crate::components::{
  document_area::state::WgpuDocumentView,
  wgpu_panel::{ChitinWgpuDocumentPanel, WgpuPanelFrame, WgpuPanelScene},
};

/// Molecular scene backed by one parsed structure and a selectable representation.
pub(crate) struct StructureMoleculeScene {
  /// Renderer-neutral structure data shared by split-panel clones.
  scene: Arc<StructureScene>,
  /// GPU resources created lazily after a surface device is available.
  renderer: Option<MoleculeRenderer>,
  /// Shader output selected through the optional debug environment variable.
  debug_mode: MoleculeDebugMode,
  /// Atom representation currently rendered by this scene.
  representation: AtomRepresentation,
}

impl StructureMoleculeScene {
  /// Creates a lazy molecular scene with the selected atom representation.
  pub(crate) fn new(scene: Arc<StructureScene>, representation: AtomRepresentation) -> Self {
    Self {
      scene,
      renderer: None,
      debug_mode: MoleculeDebugMode::Final,
      representation,
    }
  }
}

impl WgpuPanelScene for StructureMoleculeScene {
  /// Renders one fitted molecular frame into the GPUI-owned surface.
  fn render_frame(&mut self, frame: WgpuPanelFrame<'_>) -> wgpu::SubmissionIndex {
    let renderer = self.renderer.get_or_insert_with(|| {
      MoleculeRenderer::new_with_representation(
        Arc::new(frame.device.clone()),
        Arc::new(frame.queue.clone()),
        frame.size,
        frame.format,
        &self.scene,
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
    match self.representation {
      AtomRepresentation::Stick => {
        "Atom representation: Stick | L-drag rotate | Shift-L/M-drag pan | R-drag/wheel zoom"
      }
      AtomRepresentation::BallAndStick => {
        "Atom representation: Ball and stick | L-drag rotate | Shift-L/M-drag pan | R-drag/wheel zoom"
      }
      AtomRepresentation::Sphere => {
        "Atom representation: Sphere | L-drag rotate | Shift-L/M-drag pan | R-drag/wheel zoom"
      }
    }
  }

  fn set_atom_representation(&mut self, representation: AtomRepresentation) -> bool {
    if self.representation == representation {
      return false;
    }
    self.representation = representation;
    self.renderer = None;
    true
  }
}

/// Creates a WGPU document view for a local PDB or mmCIF file.
pub fn build_structure_view(path: &Path, window: &mut Window, cx: &mut App) -> WgpuDocumentView {
  let surface = window.create_wgpu_surface(960, 540, wgpu::TextureFormat::Rgba8UnormSrgb);
  match load_structure_scene(path) {
    Ok(scene) => {
      let panel = cx.new(|_| {
        ChitinWgpuDocumentPanel::new_with_scene(
          surface,
          StructureMoleculeScene::new(Arc::new(scene), AtomRepresentation::Stick),
        )
      });
      let controlled_panel = panel.clone();
      WgpuDocumentView::with_atom_representation(panel, AtomRepresentation::Stick, move |representation, cx| {
        controlled_panel.update(cx, |panel, cx| {
          if panel.set_atom_representation(representation) {
            cx.notify();
          }
        });
      })
    }
    Err(error) => {
      log::error!("failed to load structure '{}': {error}", path.display());
      WgpuDocumentView::new(cx.new(|_| ChitinWgpuDocumentPanel::new(surface)))
    }
  }
}

/// Loads a local PDB or mmCIF file and extracts its first renderable model.
fn load_structure_scene(path: &Path) -> Result<StructureScene, String> {
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
  StructureScene::from_first_model(&structure).map_err(|error| error.to_string())
}
