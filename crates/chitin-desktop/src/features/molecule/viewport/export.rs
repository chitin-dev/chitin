//! Offscreen PNG capture for one molecular viewport.

use std::{
  fs::{File, OpenOptions},
  io::{BufWriter, ErrorKind},
  path::{Path, PathBuf},
  sync::mpsc,
  time::Duration,
};

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
  /// The caller requested that an existing file not be replaced.
  #[error("refusing to overwrite existing image {0}")]
  AlreadyExists(String),
  /// PNG encoding failed.
  #[error("failed to encode PNG: {0}")]
  Png(#[from] png::EncodingError),
}

impl ChitinWgpuDocumentPanel {
  /// Starts an offscreen capture of the current camera view.
  ///
  /// This half runs on the UI thread and only performs GPU submission: it
  /// creates the target texture, renders the scene, and queues the readback.
  /// Polling, mapping, PNG encoding, and file I/O happen in
  /// [`PendingExport::finish`] on a background worker.
  ///
  /// # Parameters
  ///
  /// * `path` is the final image path. The target directory is not created
  ///   until the background write runs.
  /// * `create_new` rejects an existing destination instead of overwriting it.
  ///
  /// # Returns
  ///
  /// A pending capture that can be finished off the UI thread.
  ///
  /// # Errors
  ///
  /// Returns [`ExportError`] when the surface is unavailable, the target is
  /// empty, or GPU submission fails.
  pub(crate) fn begin_export(&mut self, path: &Path, create_new: bool) -> Result<PendingExport, ExportError> {
    let Some(surface) = self.surface.as_ref() else {
      return Err(ExportError::NoSurface);
    };
    let (width, height) = surface.size();
    if width == 0 || height == 0 {
      return Err(ExportError::EmptyTarget);
    }
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let device = surface.device();
    let queue = surface.queue();
    // Prevent a surface resize from reconfiguring the shared queue while the
    // offscreen capture is submitted. The guard is released before returning.
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

    let (readback, submission) = submit_texture_readback(device, queue, &texture, width, height)?;
    Ok(PendingExport {
      device: device.clone(),
      path: path.to_path_buf(),
      width,
      height,
      format,
      readback,
      submission,
      create_new,
    })
  }
}

/// Submitted GPU capture whose readback and file write can run off the UI thread.
pub(crate) struct PendingExport {
  /// Device used by the submitted copy.
  device: wgpu::Device,
  /// Final image path.
  path: PathBuf,
  /// Capture width in physical pixels.
  width: u32,
  /// Capture height in physical pixels.
  height: u32,
  /// Format of the source render target.
  format: wgpu::TextureFormat,
  /// Buffer receiving the texture copy.
  readback: wgpu::Buffer,
  /// Queue submission that must complete before mapping.
  submission: wgpu::SubmissionIndex,
  /// Reject an existing destination instead of overwriting it.
  create_new: bool,
}

impl PendingExport {
  /// Waits for the GPU copy, reads it back, and writes the PNG.
  ///
  /// This blocks the calling worker until the GPU finishes, so callers must not
  /// invoke it on the GPUI event thread.
  ///
  /// # Returns
  ///
  /// The written image path.
  ///
  /// # Errors
  ///
  /// Returns [`ExportError`] when GPU readback fails or the image cannot be
  /// encoded or written.
  pub(crate) fn finish(self) -> Result<PathBuf, ExportError> {
    let mut rgba = read_buffer_rgba(
      &self.device,
      &self.readback,
      self.format,
      self.width,
      self.height,
      Some(self.submission),
    )?;
    if self.format.is_srgb() {
      decode_presentation_rgb(&mut rgba);
    }
    if let Some(parent) = self.path.parent() {
      std::fs::create_dir_all(parent).map_err(|source| ExportError::Io {
        path: parent.display().to_string(),
        source,
      })?;
    }
    let file = open_export_file(&self.path, self.create_new)?;
    encode_png(BufWriter::new(file), self.width, self.height, &rgba)?;
    Ok(self.path)
  }
}

/// Opens the destination, optionally refusing to replace an existing file.
fn open_export_file(path: &Path, create_new: bool) -> Result<File, ExportError> {
  let mut options = OpenOptions::new();
  options.write(true);
  if create_new {
    options.create_new(true);
  } else {
    options.create(true).truncate(true);
  }
  options.open(path).map_err(|source| {
    if create_new && source.kind() == ErrorKind::AlreadyExists {
      ExportError::AlreadyExists(path.display().to_string())
    } else {
      ExportError::Io {
        path: path.display().to_string(),
        source,
      }
    }
  })
}

/// Queues a texture-to-buffer copy and returns the buffer with its submission.
fn submit_texture_readback(
  device: &wgpu::Device,
  queue: &wgpu::Queue,
  texture: &wgpu::Texture,
  width: u32,
  height: u32,
) -> Result<(wgpu::Buffer, wgpu::SubmissionIndex), ExportError> {
  let (readback, padded_bytes_per_row) = create_readback_buffer(device, width, height);
  let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
    label: Some("chitin_protein_export_encoder"),
  });
  encoder.copy_texture_to_buffer(
    wgpu::TexelCopyTextureInfo {
      texture,
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
  let submission = queue.submit(Some(encoder.finish()));
  Ok((readback, submission))
}

/// Creates the mappable readback buffer and returns its aligned row stride.
fn create_readback_buffer(device: &wgpu::Device, width: u32, height: u32) -> (wgpu::Buffer, u32) {
  let unpadded_bytes_per_row = width * 4;
  let padded_bytes_per_row =
    unpadded_bytes_per_row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
  let readback = device.create_buffer(&wgpu::BufferDescriptor {
    label: Some("chitin_protein_export_readback"),
    size: u64::from(padded_bytes_per_row) * u64::from(height),
    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
    mapped_at_creation: false,
  });
  (readback, padded_bytes_per_row)
}

/// Waits for a submitted copy and returns tightly packed RGBA bytes.
fn read_buffer_rgba(
  device: &wgpu::Device,
  readback: &wgpu::Buffer,
  format: wgpu::TextureFormat,
  width: u32,
  height: u32,
  submission: Option<wgpu::SubmissionIndex>,
) -> Result<Vec<u8>, ExportError> {
  device
    .poll(wgpu::PollType::Wait {
      submission_index: submission.clone(),
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
      submission_index: submission,
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
  let unpadded_bytes_per_row = width * 4;
  let padded_bytes_per_row =
    unpadded_bytes_per_row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
  let mut rgba = Vec::with_capacity((width as usize) * (height as usize) * 4);
  for row in 0..height {
    let start = (row as usize) * (padded_bytes_per_row as usize);
    let end = start + (unpadded_bytes_per_row as usize);
    let pixels = &mapped[start..end];
    match format {
      wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb => {
        for pixel in pixels.as_chunks::<4>().0 {
          rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
        }
      }
      _ => rgba.extend_from_slice(pixels),
    }
  }
  drop(mapped);
  readback.unmap();
  Ok(rgba)
}

/// Writes tightly packed RGBA bytes as an 8-bit PNG.
fn encode_png(mut writer: impl std::io::Write, width: u32, height: u32, rgba: &[u8]) -> Result<(), ExportError> {
  let mut encoder = png::Encoder::new(&mut writer, width, height);
  encoder.set_color(png::ColorType::Rgba);
  encoder.set_depth(png::BitDepth::Eight);
  let mut png = encoder.write_header()?;
  png.write_image_data(rgba)?;
  Ok(())
}

/// Matches the values GPUI currently presents for a WGPU surface.
///
/// The production viewport uses an sRGB surface that GPUI samples into linear
/// space and writes to a non-sRGB swapchain. Exporting the raw sRGB bytes looks
/// brighter than that viewport, so decode the RGB channels before writing PNG.
fn decode_presentation_rgb(rgba: &mut [u8]) {
  for pixel in rgba.as_chunks_mut::<4>().0 {
    pixel[0] = srgb_to_linear_channel(pixel[0]);
    pixel[1] = srgb_to_linear_channel(pixel[1]);
    pixel[2] = srgb_to_linear_channel(pixel[2]);
  }
}

/// Converts one sRGB-encoded channel to the linear value shown by GPUI.
fn srgb_to_linear_channel(value: u8) -> u8 {
  let encoded = f32::from(value) / 255.0;
  let linear = if encoded <= 0.040_45 {
    encoded / 12.92
  } else {
    ((encoded + 0.055) / 1.055).powf(2.4)
  };
  (linear * 255.0).round().clamp(0.0, 255.0) as u8
}

#[cfg(test)]
fn read_texture_rgba(
  device: &wgpu::Device,
  queue: &wgpu::Queue,
  texture: &wgpu::Texture,
  format: wgpu::TextureFormat,
  width: u32,
  height: u32,
) -> Result<Vec<u8>, ExportError> {
  let (readback, padded_bytes_per_row) = create_readback_buffer(device, width, height);
  let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
    label: Some("chitin_export_test_encoder"),
  });
  encoder.copy_texture_to_buffer(
    wgpu::TexelCopyTextureInfo {
      texture,
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
  let submission = queue.submit(Some(encoder.finish()));
  read_buffer_rgba(device, &readback, format, width, height, Some(submission))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn png_write_should_preserve_exact_rgba_values() {
    let pixels = [
      0, 0, 0, 255, // black
      255, 255, 255, 255, // white
      128, 128, 128, 255, // mid gray
      255, 0, 0, 255, // red
      0, 255, 0, 255, // green
      0, 0, 255, 255, // blue
    ];
    let mut bytes = Vec::new();
    if let Err(error) = encode_png(&mut bytes, 6, 1, &pixels) {
      panic!("PNG encoding should succeed: {error}");
    }
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder
      .read_info()
      .unwrap_or_else(|error| panic!("PNG header should decode: {error}"));
    let mut output = vec![0; reader.output_buffer_size()];
    let info = reader
      .next_frame(&mut output)
      .unwrap_or_else(|error| panic!("PNG frame should decode: {error}"));
    assert_eq!(info.width, 6);
    assert_eq!(info.height, 1);
    assert_eq!(&output[..pixels.len()], &pixels);
  }

  #[test]
  fn presentation_decode_should_keep_endpoints_and_darken_midtones() {
    let mut pixels = [0, 0, 0, 255, 255, 255, 255, 255, 226, 226, 226, 255];
    decode_presentation_rgb(&mut pixels);
    assert_eq!(&pixels[0..4], &[0, 0, 0, 255]);
    assert_eq!(&pixels[4..8], &[255, 255, 255, 255]);
    assert!(pixels[8] < 200, "sRGB midtones should decode toward linear");
    assert!(srgb_to_linear_channel(64) < 64);
  }

  #[test]
  fn create_new_exports_should_not_replace_an_existing_file() {
    let directory = std::env::temp_dir().join(format!(
      "chitin-export-overwrite-{}-{}",
      std::process::id(),
      std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos())
    ));
    if let Err(error) = std::fs::create_dir_all(&directory) {
      panic!("test directory should be creatable: {error}");
    }
    let path = directory.join("image.png");
    let first = open_export_file(&path, true);
    assert!(first.is_ok());
    drop(first);
    assert!(matches!(
      open_export_file(&path, true),
      Err(ExportError::AlreadyExists(_))
    ));
    assert!(open_export_file(&path, false).is_ok());
    let _ = std::fs::remove_dir_all(&directory);
  }

  /// Manual GPU check; ignored so CI does not require an adapter.
  ///
  /// Run with:
  /// `cargo test -p chitin-desktop --lib gpu_srgb_readback -- --ignored`
  #[test]
  #[ignore = "requires a GPU adapter; run manually"]
  fn gpu_srgb_readback_should_encode_linear_clear_colors_once() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
      backends: wgpu::Backends::all(),
      ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
      .unwrap_or_else(|error| panic!("test requires a GPU adapter: {error}"));
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default()))
      .unwrap_or_else(|error| panic!("test requires a GPU device: {error}"));
    let cases = [
      ([0.0, 0.0, 0.0], [0, 0, 0]),
      ([1.0, 1.0, 1.0], [255, 255, 255]),
      ([0.5, 0.5, 0.5], [188, 188, 188]),
      ([1.0, 0.0, 0.0], [255, 0, 0]),
      ([0.0, 1.0, 0.0], [0, 255, 0]),
      ([0.0, 0.0, 1.0], [0, 0, 255]),
    ];
    for (linear, expected) in cases {
      let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("chitin_export_color_test"),
        size: wgpu::Extent3d {
          width: 2,
          height: 2,
          depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
      });
      let view = texture.create_view(&Default::default());
      let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("chitin_export_color_test_encoder"),
      });
      {
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
          label: Some("chitin_export_color_test_pass"),
          color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &view,
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations {
              load: wgpu::LoadOp::Clear(wgpu::Color {
                r: linear[0],
                g: linear[1],
                b: linear[2],
                a: 1.0,
              }),
              store: wgpu::StoreOp::Store,
            },
          })],
          depth_stencil_attachment: None,
          timestamp_writes: None,
          occlusion_query_set: None,
          multiview_mask: None,
        });
      }
      queue.submit(Some(encoder.finish()));
      let rgba = read_texture_rgba(&device, &queue, &texture, wgpu::TextureFormat::Rgba8UnormSrgb, 2, 2)
        .unwrap_or_else(|error| panic!("readback should succeed: {error}"));
      for pixel in rgba.as_chunks::<4>().0 {
        for (channel, expected) in pixel[..3].iter().zip(expected) {
          assert!(
            i32::from(*channel).abs_diff(expected) <= 1,
            "linear {linear:?} should encode to {expected:?}, got {pixel:?}"
          );
        }
      }
    }
  }
}
