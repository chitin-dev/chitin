//! CPU data-path comparisons; these do not measure GPUI painting or GPU rendering.

#[expect(
  dead_code,
  reason = "The benchmark imports private sequence data alongside its layout helpers."
)]
#[path = "../src/features/sequence/model.rs"]
mod model;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use gpui::{Bounds, point, px, size};
use model::{CELL_WIDTH, ResidueSpan, RowGeometry, SequenceLayout, visible_columns, wrap_columns};
use std::hint::black_box;

fn sequence_virtualization(criterion: &mut Criterion) {
  let mut columns = criterion.benchmark_group("sequence_columns");
  for len in [1_000, 10_000, 100_000] {
    columns.bench_with_input(BenchmarkId::new("all_columns_before", len), &len, |bench, len| {
      bench.iter(|| black_box((0..black_box(*len)).collect::<Vec<_>>()));
    });
    columns.bench_with_input(BenchmarkId::new("visible_columns_after", len), &len, |bench, len| {
      bench.iter(|| {
        black_box(visible_columns(black_box(*len), black_box(px(-400.0)), black_box(px(1_000.0))).collect::<Vec<_>>())
      });
    });
  }
  columns.finish();

  let mut selection = criterion.benchmark_group("sequence_box_hit");
  let rectangle = Bounds::new(point(px(200.0), px(10.0)), size(px(400.0), px(10.0)));
  for len in [1_000, 10_000, 100_000] {
    let cells: Vec<_> = (0..len)
      .map(|index| {
        Bounds::new(
          point(px(index as f32 * CELL_WIDTH), px(0.0)),
          size(px(CELL_WIDTH), px(36.0)),
        )
      })
      .collect();
    let row = RowGeometry {
      span: ResidueSpan {
        chain: 0,
        range: 0..len,
      },
      origin: point(px(0.0), px(0.0)),
      clip: Bounds::new(point(px(0.0), px(0.0)), size(px(1_000.0), px(36.0))),
    };
    selection.bench_with_input(
      BenchmarkId::new("scan_all_cells_before", len),
      &cells,
      |bench, cells| {
        bench.iter(|| {
          let rectangle = black_box(rectangle);
          black_box(
            black_box(cells)
              .iter()
              .filter(|cell| cell.intersects(&rectangle))
              .count(),
          )
        });
      },
    );
    selection.bench_with_input(BenchmarkId::new("row_interval_after", len), &row, |bench, row| {
      bench.iter(|| black_box(black_box(row).hit(black_box(rectangle))));
    });
  }
  selection.finish();

  let columns = wrap_columns(px(1_080.0));
  let layout = SequenceLayout::new((0..1_000).map(|chain| (chain, 10_000)), Some(columns));
  criterion.bench_function("sequence_wrapped_row_lookup_10m_residues", |bench| {
    bench.iter(|| black_box(layout.row(black_box(layout.count / 2))));
  });
}

criterion_group!(benches, sequence_virtualization);
criterion_main!(benches);
