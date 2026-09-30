//! Benchmarks for Kit's tree projection data and viewport-sized row batches.
//!
//! GPUI Kit owns painting and virtualization. These benchmarks measure the
//! application-side item projection and cloning only a visible batch.

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use gpui_kit::component::tree::{TreeEntry, TreeItem};

const VIEWPORT_ROW_COUNT: usize = 48;
const TREE_SIZES: &[usize] = &[1_000, 10_000, 50_000];

/// Builds a flat projection resembling one expanded filesystem directory.
///
/// # Parameters
///
/// * `count` is the number of file entries under the visible root.
///
/// # Returns
///
/// Kit entries with stable identities and explicit display depths; no redundant
/// Chitin tree state or row types are involved.
fn entries(count: usize) -> Vec<TreeEntry> {
  let mut entries = Vec::with_capacity(count.saturating_add(1));
  entries.push(TreeEntry::new(TreeItem::new("root", "Project"), 0));
  entries.extend(
    (0..count).map(|index| TreeEntry::new(TreeItem::new(format!("entry:{index}"), format!("file-{index}")), 1)),
  );
  entries
}

/// Measures full projection construction separately from viewport batch copies.
///
/// # Parameters
///
/// * `criterion` records timings for each configured project size.
///
/// # Returns
///
/// Nothing; results are emitted by Criterion.
fn tree_virtualization(criterion: &mut Criterion) {
  let mut projection = criterion.benchmark_group("kit_tree_projection");
  for &count in TREE_SIZES {
    projection.throughput(Throughput::Elements(count as u64));
    projection.bench_with_input(BenchmarkId::new("entries", count), &count, |bench, count| {
      bench.iter(|| black_box(entries(*count)));
    });
  }
  projection.finish();

  let mut viewport = criterion.benchmark_group("kit_tree_viewport_batch");
  for &count in TREE_SIZES {
    let rows = entries(count);
    viewport.throughput(Throughput::Elements(VIEWPORT_ROW_COUNT as u64));
    viewport.bench_with_input(BenchmarkId::new("middle", count), &rows, |bench, rows| {
      let start = rows.len() / 2;
      bench.iter(|| black_box(rows[start..start + VIEWPORT_ROW_COUNT].to_vec()));
    });
  }
  viewport.finish();
}

criterion_group!(benches, tree_virtualization);
criterion_main!(benches);
