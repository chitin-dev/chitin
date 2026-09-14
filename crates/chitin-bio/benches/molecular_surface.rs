//! End-to-end benchmarks for CPU molecular-surface generation.
//!
//! Scene construction and PDB parsing happen outside the timed region. The
//! measured operation starts with surface atom grouping and ends with the
//! complete renderer-neutral mesh artifact.

use std::time::Duration;

use chitin_bio::structure::{
  MolecularSurfaceRequest, PdbParser, SesParameters, StructureScene, StructureSceneOptions, SurfacePartition,
  generate_molecular_surface,
};
use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};

#[cfg(feature = "surface-profiling")]
use chitin_bio::structure::profile_molecular_surface;

/// Atom counts used to expose scaling without making the baseline impractical.
const SINGLE_CHAIN_ATOM_COUNTS: &[usize] = &[64, 512, 2_048];
/// Number of atoms in the partition-strategy comparison.
const ASSEMBLY_ATOM_COUNT: usize = 1_024;
/// Number of independently positioned chains in the assembly fixture.
const ASSEMBLY_CHAIN_COUNT: usize = 4;
/// Atom count used to expose grid-resolution scaling.
const RESOLUTION_ATOM_COUNT: usize = 512;
/// Grid spacings covering preview, default, and fine surface quality.
const GRID_SPACINGS: &[f32] = &[0.75, 0.50, 0.35];
/// Approximate separation between neighboring atoms in one synthetic domain.
const ATOM_SPACING: f32 = 1.8;

/// Builds a deterministic compact atom lattice split across separated chains.
fn synthetic_scene(atom_count: usize, chain_count: usize) -> StructureScene {
  assert!(chain_count > 0, "benchmark fixtures need at least one chain");
  assert!(chain_count <= 26, "PDB benchmark fixtures use one-letter chain IDs");

  let atoms_per_chain = atom_count.div_ceil(chain_count);
  let side = (atoms_per_chain as f32).cbrt().ceil() as usize;
  let chain_stride = side as f32 * ATOM_SPACING + 8.0;
  let mut pdb = String::with_capacity(atom_count.saturating_mul(82));

  for atom_index in 0..atom_count {
    let chain_index = atom_index.saturating_mul(chain_count) / atom_count;
    let local_index = atom_index - chain_index * atoms_per_chain;
    let x = (local_index % side) as f32 * ATOM_SPACING + chain_index as f32 * chain_stride;
    let y = ((local_index / side) % side) as f32 * ATOM_SPACING;
    let z = (local_index / (side * side)) as f32 * ATOM_SPACING;
    let chain = char::from(b'A' + chain_index as u8);
    let serial = atom_index + 1;
    let residue = local_index + 1;
    pdb.push_str(&format!(
      "ATOM  {serial:5}  C   GLY {chain}{residue:4}    {x:8.3}{y:8.3}{z:8.3}  1.00 10.00           C  \n"
    ));
  }
  pdb.push_str("END\n");

  let parsed = PdbParser::new()
    .parse_bytes(pdb.as_bytes())
    .unwrap_or_else(|error| panic!("synthetic surface fixture should parse: {error}"));
  let model_id = parsed
    .structure
    .models()
    .first()
    .map(|model| model.id)
    .unwrap_or_else(|| panic!("synthetic surface fixture should contain one model"));
  let scene = StructureScene::from_model_with_options(
    &parsed.structure,
    model_id,
    StructureSceneOptions {
      infer_missing_bonds: false,
      ..StructureSceneOptions::default()
    },
  )
  .unwrap_or_else(|error| panic!("synthetic surface fixture should produce a scene: {error}"));
  assert_eq!(scene.atoms.len(), atom_count, "surface fixture lost parsed atoms");
  scene
}

/// Measures default per-chain SES generation as atom count increases.
fn bench_single_chain_scaling(c: &mut Criterion) {
  let mut group = c.benchmark_group("molecular_surface/single_chain");
  configure_slow_group(&mut group);

  for &atom_count in SINGLE_CHAIN_ATOM_COUNTS {
    let scene = synthetic_scene(atom_count, 1);
    #[cfg(feature = "surface-profiling")]
    if atom_count == 512 {
      let profile = profile_molecular_surface(&scene, MolecularSurfaceRequest::default());
      eprintln!("512-atom surface profile: {:#?}", profile.timings);
    }
    group.throughput(Throughput::Elements(atom_count as u64));
    group.bench_with_input(BenchmarkId::from_parameter(atom_count), &scene, |b, scene| {
      b.iter(|| {
        black_box(generate_molecular_surface(
          black_box(scene),
          black_box(MolecularSurfaceRequest::default()),
        ))
      });
    });
  }

  group.finish();
}

/// Measures the cubic cost sensitivity of scalar-grid resolution.
fn bench_grid_resolution(c: &mut Criterion) {
  let mut group = c.benchmark_group("molecular_surface/grid_resolution");
  configure_slow_group(&mut group);
  group.throughput(Throughput::Elements(RESOLUTION_ATOM_COUNT as u64));
  let scene = synthetic_scene(RESOLUTION_ATOM_COUNT, 1);

  for &grid_spacing in GRID_SPACINGS {
    let request = MolecularSurfaceRequest {
      ses: SesParameters::new(1.4, grid_spacing)
        .unwrap_or_else(|error| panic!("benchmark grid spacing should be valid: {error}")),
      ..MolecularSurfaceRequest::default()
    };
    group.bench_with_input(
      BenchmarkId::new("angstrom", format!("{grid_spacing:.2}")),
      &request,
      |b, request| {
        b.iter(|| black_box(generate_molecular_surface(black_box(&scene), black_box(*request))));
      },
    );
  }

  group.finish();
}

/// Compares per-chain and unified calculation domains for one assembly.
fn bench_partition_strategy(c: &mut Criterion) {
  let mut group = c.benchmark_group("molecular_surface/four_chain_assembly");
  configure_slow_group(&mut group);
  group.throughput(Throughput::Elements(ASSEMBLY_ATOM_COUNT as u64));
  let scene = synthetic_scene(ASSEMBLY_ATOM_COUNT, ASSEMBLY_CHAIN_COUNT);

  for partition in [SurfacePartition::ByChain, SurfacePartition::Unified] {
    let request = MolecularSurfaceRequest {
      partition,
      ..MolecularSurfaceRequest::default()
    };
    group.bench_with_input(
      BenchmarkId::new(format!("{partition:?}"), ASSEMBLY_ATOM_COUNT),
      &request,
      |b, request| {
        b.iter(|| black_box(generate_molecular_surface(black_box(&scene), black_box(*request))));
      },
    );
  }

  group.finish();
}

/// Keeps expensive baseline runs short while preserving Criterion statistics.
fn configure_slow_group<M: criterion::measurement::Measurement>(group: &mut criterion::BenchmarkGroup<'_, M>) {
  group
    .sample_size(10)
    .warm_up_time(Duration::from_millis(500))
    .measurement_time(Duration::from_secs(2));
}

criterion_group!(
  benches,
  bench_single_chain_scaling,
  bench_partition_strategy,
  bench_grid_resolution
);
criterion_main!(benches);
