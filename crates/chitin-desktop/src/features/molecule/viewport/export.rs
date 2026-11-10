//! Offscreen PNG capture for one molecular viewport.

use std::{fs::File, io::BufWriter, path::Path, sync::mpsc, time::Duration};

use chitin_wgpu::RenderTargetSize;

use super::{ChitinWgpuDocumentPanel, WgpuPanelFrame};

/// Failure while rendering or writing a protein image.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ExportError {
  /// The panel has no WGPU surface to borrow a device and queue from.
  #[error("the protein viewport is not attached to a WGPU surface")]
  NoSurface,
  /// The native viewport has not received a drawable size yet.
  #[error("the protein viewport has no drawable area")]
  EmptyTarget,
  /// The GPU readback failed or timed out.
  #[error("GPU readback failed: {0}")]
  Readback(String),
  /// The destination could not be created or written.
  #[error("failed to write {path}: {source}")]
  Io {
    /// Destination path.
    path: String,
    /// Underlying filesystem error.
    source: std::io::Error,
  },
  /// PNG encoding failed.
  #[error("failed to encode PNG: {0}")]
  Png(#[from] png::EncodingError),
}

impl ChitinWgpuDocumentPanel {
  /// Renders the current camera view into an offscreen texture and writes a PNG.
  ///
  /// The capture uses the live viewport's physical resolution and clear color,
  /// so it matches what the user currently sees without including the FPS or
  /// selection overlays drawn by GPUI.
  ///
  /// # Parameters
  ///
  /// * `path` is the destination image path; missing parent directories are
  ///   created before writing.
  ///
  /// # Returns
  ///
  /// `Ok(())` after the PNG has been written.
  ///
  /// # Errors
  ///
  /// Returns [`ExportError`] when the surface is unavailable, the GPU readback
  /// fails, or the image cannot be encoded or written.
  pub(crate) fn export_png(&mut self, path: &Path) -> Result<(), ExportError> {
    let Some(surface) = self.surface.as_ref() else {
      return Err(ExportError::NoSurface);
    };
    let (width, height) = surface.size();
    if width == 0 || height == 0 {
      return Err(ExportError::EmptyTarget);
    }
    let format = surface.format();
    let device = surface.device();
    let queue = surface.queue();
    // Prevent a surface resize from reconfiguring the shared queue while the
    // offscreen capture is submitted.
    let _submit_guard = surface.submit_guard();

    let texture = device.create_texture(&wgpu::TextureDescriptor {
      label: Some("chitin_protein_export_target"),
      size: wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
      },
      mip_level_count: 1,
      sample_count: 1,
      dimension: wgpu::TextureDimension::D2,
      format,
      usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
      view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let size = RenderTargetSize::new(width, height);
    let elapsed = self.started_at.elapsed();
    let camera = &self.camera;
    let _render_submission = self.scene.render_frame(WgpuPanelFrame {
      scale_factor: 1.0,
      device,
      queue,
      format,
      view: &view,
      size,
      camera,
      elapsed,
    });

    let unpadded_bytes_per_row = width * 4;
    let padded_bytes_per_row =
      unpadded_bytes_per_row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
      label: Some("chitin_protein_export_readback"),
      size: u64::from(padded_bytes_per_row) * u64::from(height),
      usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
      mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
      label: Some("chitin_protein_export_encoder"),
    });
    encoder.copy_texture_to_buffer(
      wgpu::TexelCopyTextureInfo {
        texture: &texture,
        mip_level: 0,
        origin: wgpu::Origin3d::ZERO,
        aspect: wgpu::TextureAspect::All,
      },
      wgpu::TexelCopyBufferInfo {
        buffer: &readback,
        layout: wgpu::TexelCopyBufferLayout {
          offset: 0,
          bytes_per_row: Some(padded_bytes_per_row),
          rows_per_image: Some(height),
        },
      },
      wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
      },
    );
    let copy_submission = queue.submit(Some(encoder.finish()));
    // The copy is queued behind the render, so waiting on it also waits for
    // the frame that produced the pixels.
    device
      .poll(wgpu::PollType::Wait {
        submission_index: Some(copy_submission),
        timeout: Some(Duration::from_secs(30)),
      })
      .map_err(|error| ExportError::Readback(error.to_string()))?;

    let slice = readback.slice(..);
    let (sender, receiver) = mpsc::sync_channel(1);
    slice.map_async(wgpu::MapMode::Read, move |result| {
      let _ = sender.send(result);
    });
    device
      .poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: Some(Duration::from_secs(30)),
      })
      .map_err(|error| ExportError::Readback(error.to_string()))?;
    receiver
      .recv()
      .map_err(|_| ExportError::Readback("map callback was dropped".into()))?
      .map_err(|error| ExportError::Readback(error.to_string()))?;

    let mapped = slice
      .get_mapped_range()
      .map_err(|error| ExportError::Readback(error.to_string()))?;
    let mut rgba = Vec::with_capacity((width as usize) * (height as usize) * 4);
    for row in 0..height {
      let start = (row as usize) * (padded_bytes_per_row as usize);
      let end = start + (unpadded_bytes_per_row as usize);
      let pixels = &mapped[start..end];
      match format {
        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb => {
          for pixel in pixels.chunks_exact(4) {
            rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
          }
        }
        _ => rgba.extend_from_slice(pixels),
      }
    }
    drop(mapped);
    readback.unmap();
    // GPUI presents WGPU surfaces through a non-sRGB swapchain after sampling
    // sRGB targets into linear space. PNG viewers interpret untagged bytes as
    // sRGB, so writing the encoded bytes directly makes the export look brighter
    // than the live viewport. Decode back to the linear values the compositor
    // presents so the exported image matches the software.
    let decode = matches!(
      format,
      wgpu::TextureFormat::Rgba8UnormSrgb | wgpu::TextureFormat::Bgra8UnormSrgb
    );
    if decode {
      for pixel in rgba.chunks_exact_mut(4) {
        pixel[0] = presentation_channel(pixel[0]);
        pixel[1] = presentation_channel(pixel[1]);
        pixel[2] = presentation_channel(pixel[2]);
      }
    }

    if let Some(parent) = path.parent() {
      std::fs::create_dir_all(parent).map_err(|source| ExportError::Io {
        path: parent.display().to_string(),
        source,
      })?;
    }
    let file = File::create(path).map_err(|source| ExportError::Io {
      path: path.display().to_string(),
      source,
    })?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&rgba)?;
    Ok(())
  }
}

/// Converts one sRGB-encoded channel back to the linear value GPUI presents.
fn presentation_channel(value: u8) -> u8 {
  let encoded = f32::from(value) / 255.0;
  let linear = if encoded <= 0.040_45 {
    encoded / 12.92
  } else {
    ((encoded + 0.055) / 1.055).powf(2.4)
  };
  (linear * 255.0).round().clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn presentation_conversion_should_keep_endpoints_and_darken_midtones() {
    assert_eq!(presentation_channel(0), 0);
    assert_eq!(presentation_channel(255), 255);
    let mid = presentation_channel(226);
    assert!(mid < 200, "sRGB midtones must decode toward the linear range");
    assert!(presentation_channel(64) < 64);
  }
}
