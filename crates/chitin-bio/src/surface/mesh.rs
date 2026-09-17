//! Renderer-neutral triangle meshes shared by molecular-surface backends.

/// Indexed triangle mesh in source-space ångström coordinates.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct SurfaceMesh {
  /// Interleaved source position and unit-normal rows.
  pub vertices: Vec<[f32; 6]>,
  /// Triangle-list indices into [`Self::vertices`].
  pub indices: Vec<u32>,
}
