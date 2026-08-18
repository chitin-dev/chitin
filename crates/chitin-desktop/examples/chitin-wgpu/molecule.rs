//! Molecule scene adapter for the desktop WGPU integration example.

use std::sync::Arc;

use chitin_bio::structure::StructureScene;
use chitin_desktop::wgpu_panel::{WgpuPanelFrame, WgpuPanelScene};
use chitin_wgpu::MoleculeRenderer;

/// Lazily initializes a reusable molecular renderer for a structure scene.
pub struct ExampleMoleculeScene {
  /// CPU-side renderer-neutral structure data shared by split panel clones.
  scene: Arc<StructureScene>,
  /// GPU resources created after GPUI provides a concrete surface device.
  renderer: Option<MoleculeRenderer>,
}

impl ExampleMoleculeScene {
  /// Creates a lazy molecule scene from shared renderer-neutral data.
  pub fn new(scene: Arc<StructureScene>) -> Self {
    Self { scene, renderer: None }
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
      MoleculeRenderer::new(
        Arc::new(frame.device.clone()),
        Arc::new(frame.queue.clone()),
        frame.size,
        frame.format,
        &self.scene,
      )
    });

    renderer.resize_if_needed(frame.size);
    renderer.render(frame.view, frame.camera.view_projection(renderer.aspect()))
  }

  /// Returns the molecule-specific interaction hint.
  fn interaction_hint(&self) -> &'static str {
    "Atoms + explicit bonds | L-drag rotate | Shift-L/M-drag pan | R-drag/wheel zoom"
  }
}
