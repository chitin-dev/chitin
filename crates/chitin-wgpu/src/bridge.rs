//! Small WGPU helpers shared by UI adapters.

use std::sync::Arc;

/// Physical size of a WGPU render target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderTargetSize {
  /// Width in physical pixels.
  pub width: u32,
  /// Height in physical pixels.
  pub height: u32,
}

impl RenderTargetSize {
  /// Creates render target size metadata.
  ///
  /// # Parameters
  ///
  /// `width` and `height` are physical pixel dimensions.
  ///
  /// # Returns
  ///
  /// A [`RenderTargetSize`] value for resource sizing and aspect calculations.
  pub fn new(width: u32, height: u32) -> Self {
    Self { width, height }
  }

  /// Returns the width divided by height.
  ///
  /// # Parameters
  ///
  /// This function takes no parameters.
  ///
  /// # Returns
  ///
  /// A non-panicking aspect ratio with a non-zero height guard.
  pub fn aspect(&self) -> f32 {
    self.width as f32 / self.height.max(1) as f32
  }
}

/// Reusable depth texture for renderers that target a single surface.
pub struct DepthTarget {
  /// WGPU depth view used by render passes.
  view: wgpu::TextureView,
  /// Physical size used to allocate the current view.
  size: RenderTargetSize,
  /// Texture format used by the current view.
  format: wgpu::TextureFormat,
}

impl DepthTarget {
  /// Creates a depth target for one render target size.
  ///
  /// # Parameters
  ///
  /// `device` creates the backing depth texture.
  ///
  /// `size` is the physical pixel size of the render target.
  ///
  /// # Returns
  ///
  /// A depth target using `Depth32Float`.
  pub fn new(device: &wgpu::Device, size: RenderTargetSize) -> Self {
    Self::with_format(device, size, wgpu::TextureFormat::Depth32Float)
  }

  /// Creates a depth target with a caller-selected format.
  ///
  /// # Parameters
  ///
  /// `device` creates the backing depth texture.
  ///
  /// `size` is the physical pixel size of the render target.
  ///
  /// `format` is the WGPU texture format for the depth target.
  ///
  /// # Returns
  ///
  /// A depth target matching `size` and `format`.
  pub fn with_format(device: &wgpu::Device, size: RenderTargetSize, format: wgpu::TextureFormat) -> Self {
    Self {
      view: create_depth_view(device, size, format),
      size,
      format,
    }
  }

  /// Recreates the backing texture when the target size changes.
  ///
  /// # Parameters
  ///
  /// `device` creates a replacement texture when needed.
  ///
  /// `size` is the latest physical pixel size.
  ///
  /// # Returns
  ///
  /// This function returns `()` after refreshing resources when needed.
  pub fn resize_if_needed(&mut self, device: &wgpu::Device, size: RenderTargetSize) {
    if self.size == size {
      return;
    }

    self.size = size;
    self.view = create_depth_view(device, size, self.format);
  }

  /// Returns the current depth texture view.
  ///
  /// # Parameters
  ///
  /// This function takes no parameters.
  ///
  /// # Returns
  ///
  /// A WGPU texture view suitable for a depth attachment.
  pub fn view(&self) -> &wgpu::TextureView {
    &self.view
  }
}

/// Renderer that clears a color target and maintains a matching depth target.
pub struct ClearRenderer {
  /// Shared WGPU device used for encoder and texture creation.
  device: Arc<wgpu::Device>,
  /// Shared WGPU queue used for command submission.
  queue: Arc<wgpu::Queue>,
  /// Size-dependent depth target.
  depth: DepthTarget,
  /// Clear color applied to the render target.
  clear_color: wgpu::Color,
}

impl ClearRenderer {
  /// Creates a clear renderer for a UI-owned WGPU target.
  ///
  /// # Parameters
  ///
  /// `device` creates WGPU resources and command encoders.
  ///
  /// `queue` submits command buffers.
  ///
  /// `size` is the initial render target size in physical pixels.
  ///
  /// `clear_color` is the color used for the color attachment clear operation.
  ///
  /// # Returns
  ///
  /// A renderer that can clear matching WGPU texture views.
  pub fn new(
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    size: RenderTargetSize,
    clear_color: wgpu::Color,
  ) -> Self {
    let depth = DepthTarget::new(&device, size);

    Self {
      device,
      queue,
      depth,
      clear_color,
    }
  }

  /// Recreates size-dependent resources when the target size changes.
  ///
  /// # Parameters
  ///
  /// `size` is the latest render target size in physical pixels.
  ///
  /// # Returns
  ///
  /// This function returns `()` after refreshing depth resources when needed.
  pub fn resize_if_needed(&mut self, size: RenderTargetSize) {
    self.depth.resize_if_needed(&self.device, size);
  }

  /// Clears one WGPU texture view.
  ///
  /// # Parameters
  ///
  /// `view` is the color render target.
  ///
  /// # Returns
  ///
  /// The queue submission index for synchronized presentation by the caller.
  pub fn render(&mut self, view: &wgpu::TextureView) -> wgpu::SubmissionIndex {
    let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
      label: Some("chitin_wgpu_clear_encoder"),
    });
    {
      let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("chitin_wgpu_clear_pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
          view,
          resolve_target: None,
          depth_slice: None,
          ops: wgpu::Operations {
            load: wgpu::LoadOp::Clear(self.clear_color),
            store: wgpu::StoreOp::Store,
          },
        })],
        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
          view: self.depth.view(),
          depth_ops: Some(wgpu::Operations {
            load: wgpu::LoadOp::Clear(1.0),
            store: wgpu::StoreOp::Discard,
          }),
          stencil_ops: None,
        }),
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
      });
    }

    self.queue.submit(std::iter::once(encoder.finish()))
  }
}

/// Creates a depth texture view for a render target.
///
/// # Parameters
///
/// `device` creates the texture.
///
/// `size` is the physical pixel size of the texture.
///
/// `format` is the WGPU texture format for the depth target.
///
/// # Returns
///
/// A texture view suitable for a depth attachment.
fn create_depth_view(device: &wgpu::Device, size: RenderTargetSize, format: wgpu::TextureFormat) -> wgpu::TextureView {
  let texture = device.create_texture(&wgpu::TextureDescriptor {
    label: Some("chitin_wgpu_depth"),
    size: wgpu::Extent3d {
      width: size.width,
      height: size.height,
      depth_or_array_layers: 1,
    },
    mip_level_count: 1,
    sample_count: 1,
    dimension: wgpu::TextureDimension::D2,
    format,
    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
    view_formats: &[],
  });

  texture.create_view(&wgpu::TextureViewDescriptor::default())
}
