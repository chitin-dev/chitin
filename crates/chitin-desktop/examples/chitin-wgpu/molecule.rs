//! Molecule scene adapter for the desktop WGPU integration example.

use std::sync::Arc;

use chitin_bio::{
  structure::StructureScene,
  surface::{MolecularSurfaceArtifact, MolecularSurfaceRequest, generate_molecular_surface},
};
use chitin_desktop::wgpu_panel::{WgpuPanelFrame, WgpuPanelScene};
use chitin_molecule_renderer::{
  BallAndStickStyle, MoleculeDebugMode, MoleculeRenderInput, MoleculeRenderer, RepresentationLayers,
};

/// Lazily initializes a reusable molecular renderer for a structure scene.
pub struct ExampleMoleculeScene {
  /// CPU-side renderer-neutral structure data shared by split panel clones.
  scene: Arc<StructureScene>,
  /// GPU resources created after GPUI provides a concrete surface device.
  renderer: Option<MoleculeRenderer>,
  /// Scientific surface geometry computed independently of GPU resources.
  surface: Option<MolecularSurfaceArtifact>,
  /// Shader output selected through `CHITIN_MOLECULE_DEBUG_MODE`.
  debug_mode: MoleculeDebugMode,
  /// Representation layers selected by the example command line.
  representation: RepresentationLayers,
}

impl ExampleMoleculeScene {
  /// Creates a lazy molecule scene from shared renderer-neutral data.
  pub fn new(scene: Arc<StructureScene>, representation: RepresentationLayers) -> Self {
    let debug_mode = molecule_debug_mode_from_env();
    let surface = molecular_surface_for_layers(&scene, representation, None);
    Self {
      scene,
      renderer: None,
      surface,
      debug_mode,
      representation,
    }
  }
}

impl WgpuPanelScene for ExampleMoleculeScene {
  /// Renders one fitted atom-and-explicit-bond frame.
  ///
  /// # Parameters
  ///
  /// * `frame` contains the GPUI surface resources and interactive camera.
  ///
  /// # Returns
  ///
  /// The queue submission index used by the panel to present the frame.
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

  /// Returns the molecule-specific interaction hint.
  fn interaction_hint(&self) -> &'static str {
    "Atom representation | L-drag rotate | Shift-L/M-drag pan | R-drag/wheel zoom"
  }

  fn set_representation_layers(&mut self, representation: RepresentationLayers) -> bool {
    if self.representation == representation {
      return false;
    }
    self.representation = representation;
    self.surface = molecular_surface_for_layers(&self.scene, representation, self.surface.take());
    self.renderer = None;
    true
  }
}

/// Computes the default protein-chain surface only when its layer is enabled.
fn molecular_surface_for_layers(
  scene: &StructureScene,
  representation: RepresentationLayers,
  current: Option<MolecularSurfaceArtifact>,
) -> Option<MolecularSurfaceArtifact> {
  match (representation.surface_style(), current) {
    (Some(_), Some(surface)) => Some(surface),
    (Some(_), None) => Some(generate_molecular_surface(scene, MolecularSurfaceRequest::default())),
    (None, _) => None,
  }
}

/// Reads the optional molecule shader diagnostic mode from the environment.
fn molecule_debug_mode_from_env() -> MoleculeDebugMode {
  let Ok(value) = std::env::var("CHITIN_MOLECULE_DEBUG_MODE") else {
    return MoleculeDebugMode::Final;
  };
  let Some(mode) = MoleculeDebugMode::from_name(&value) else {
    log::warn!(
      "unknown CHITIN_MOLECULE_DEBUG_MODE={value:?}; using final (expected final, normal, key-diffuse, fill-diffuse, specular, depth-cue, or element-color)"
    );
    return MoleculeDebugMode::Final;
  };
  log::info!("molecule shader debug mode: {mode:?}");
  mode
}
