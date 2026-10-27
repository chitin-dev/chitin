//! Measures complete offscreen GPU frames, including submission and completion.
//! This is not presentation FPS: window-system and compositor latency are absent.

use std::{error::Error, hint::black_box, sync::Arc, time::Duration};

use chitin_bio::structure::{PdbParser, StructureScene};
use chitin_molecule_renderer::{MoleculeRenderer, ViewerCamera};
use chitin_wgpu::RenderTargetSize;
use criterion::{Criterion, criterion_group, criterion_main};

/// Produces a deterministic geometry workload without downloaded PDB fixtures.
fn fixture() -> String {
  let mut pdb = String::new();
  for index in 0..512 {
    let serial = index + 1;
    let x = (index % 8) as f32 * 1.5;
    let y = ((index / 8) % 8) as f32 * 1.5;
    let z = (index / 64) as f32 * 1.5;
    pdb.push_str(&format!(
      "ATOM  {serial:5}  C   GLY A{serial:4}    {x:8.3}{y:8.3}{z:8.3}  1.00 10.00           C  \n"
    ));
  }
  pdb.push_str("END\n");
  pdb
}

struct OffscreenFrame {
  renderer: MoleculeRenderer,
  device: Arc<wgpu::Device>,
  view: wgpu::TextureView,
  camera: ViewerCamera,
}

/// Creates a native adapter and real render target; missing GPU support is an
/// error rather than silently dropping a benchmark from the performance gate.
async fn offscreen_frame(pdb: &[u8]) -> Result<OffscreenFrame, Box<dyn Error>> {
  let parsed = PdbParser::new().parse_bytes(pdb)?;
  let model_id = parsed.structure.models().first().ok_or("fixture has no model")?.id;
  let scene = StructureScene::from_model(&parsed.structure, model_id)?;
  let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
  let adapter = instance
    .request_adapter(&wgpu::RequestAdapterOptions {
      force_fallback_adapter: std::env::var_os("CHITIN_BENCH_SOFTWARE_GPU").is_some(),
      ..Default::default()
    })
    .await?;
  eprintln!("Offscreen benchmark adapter: {:?}", adapter.get_info());
  let (device, queue) = adapter.request_device(&Default::default()).await?;
  let device = Arc::new(device);
  let queue = Arc::new(queue);
  let size = RenderTargetSize::new(960, 540);
  let format = wgpu::TextureFormat::Rgba8UnormSrgb;
  let texture = device.create_texture(&wgpu::TextureDescriptor {
    label: Some("release_size_benchmark_target"),
    size: wgpu::Extent3d {
      width: 960,
      height: 540,
      depth_or_array_layers: 1,
    },
    mip_level_count: 1,
    sample_count: 1,
    dimension: wgpu::TextureDimension::D2,
    format,
    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
    view_formats: &[],
  });
  Ok(OffscreenFrame {
    renderer: MoleculeRenderer::new(device.clone(), queue, size, format, &scene),
    device,
    view: texture.create_view(&Default::default()),
    camera: ViewerCamera::default(),
  })
}

/// Compares parsing and complete rendering under the selected Cargo profile.
fn native_frame(criterion: &mut Criterion) {
  let pdb = fixture();
  criterion.bench_function("native_structure_parse_512_atoms", |bench| {
    bench.iter(|| {
      black_box(
        PdbParser::new()
          .parse_bytes(black_box(pdb.as_bytes()))
          .unwrap_or_else(|error| panic!("benchmark fixture must parse: {error}")),
      )
    });
  });
  let mut frame = pollster::block_on(offscreen_frame(pdb.as_bytes()))
    .unwrap_or_else(|error| panic!("offscreen benchmark must have a usable native adapter: {error}"));
  criterion.bench_function("native_molecule_frame_512_atoms", |bench| {
    bench.iter(|| {
      let submission = frame.renderer.render(
        black_box(&frame.view),
        frame.camera.view_matrix(),
        frame.camera.projection_matrix(frame.renderer.aspect()),
      );
      // Waiting avoids measuring only command encoding while GPU work queues up.
      frame
        .device
        .poll(wgpu::PollType::Wait {
          submission_index: Some(submission),
          timeout: Some(Duration::from_secs(30)),
        })
        .unwrap_or_else(|error| panic!("offscreen frame must finish: {error}"));
    });
  });
}

criterion_group!(benches, native_frame);
criterion_main!(benches);
