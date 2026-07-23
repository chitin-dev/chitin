//! Benchmarks for generic tree virtualization row primitives.
//!
//! These benchmarks intentionally measure the pure data path in `chitin-ui`
//! rather than GPUI painting. The goal is to separate the cost of constructing
//! flattened rows from the cost of selecting the viewport rows that a virtual
//! list asks the component to render.

use chitin_ui::components::tree::{TreeItemRow, TreeMessageRow, TreeRow};
use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};

const VIEWPORT_ROW_COUNT: usize = 48;
/// Total visible row counts used to pressure the tree row construction path.
const TREE_SIZES: &[usize] = &[1_000, 10_000, 50_000];

#[derive(Clone, Debug, PartialEq, Eq)]
struct BenchTreePayload {
  id: String,
  label: String,
}

/// Builds flattened rows for one expanded root with many leaf children.
///
/// This approximates a wide directory such as generated output folders or
/// package dependency trees where one expanded node owns many direct children.
///
/// # Parameters
///
/// `visible_leaf_count` is the number of leaf rows appended under the root.
///
/// # Returns
///
/// A flattened tree row vector containing one expanded root and the requested
/// number of leaf rows.
fn flat_rows(visible_leaf_count: usize) -> Vec<TreeRow<BenchTreePayload>> {
  let mut rows = Vec::with_capacity(visible_leaf_count.saturating_add(1));
  rows.push(TreeRow::Item(TreeItemRow {
    data: BenchTreePayload {
      id: "root".to_string(),
      label: "root".to_string(),
    },
    expanded: true,
    depth: 0,
  }));

  rows.extend((0..visible_leaf_count).map(|index| {
    TreeRow::Item(TreeItemRow {
      data: BenchTreePayload {
        id: format!("leaf-{index}"),
        label: format!("leaf-{index}"),
      },
      expanded: false,
      depth: 1,
    })
  }));

  rows
}

/// Builds flattened rows for expanded groups with fixed-size child batches.
///
/// This approximates a more realistic hierarchy where visible rows include
/// both expanded container nodes and leaf rows.
///
/// # Parameters
///
/// `group_count` is the number of expanded group rows under the root.
///
/// `leaves_per_group` is the number of leaf rows appended under each group.
///
/// # Returns
///
/// A flattened tree row vector containing one root, group rows, and grouped
/// leaf rows.
fn grouped_rows(group_count: usize, leaves_per_group: usize) -> Vec<TreeRow<BenchTreePayload>> {
  let mut rows = Vec::with_capacity(
    group_count
      .saturating_mul(leaves_per_group)
      .saturating_add(group_count)
      .saturating_add(1),
  );
  rows.push(TreeRow::Item(TreeItemRow {
    data: BenchTreePayload {
      id: "root".to_string(),
      label: "root".to_string(),
    },
    expanded: true,
    depth: 0,
  }));

  for group_index in 0..group_count {
    rows.push(TreeRow::Item(TreeItemRow {
      data: BenchTreePayload {
        id: format!("group-{group_index}"),
        label: format!("group-{group_index}"),
      },
      expanded: true,
      depth: 1,
    }));

    rows.extend((0..leaves_per_group).map(|leaf_index| {
      TreeRow::Item(TreeItemRow {
        data: BenchTreePayload {
          id: format!("group-{group_index}/leaf-{leaf_index}"),
          label: format!("leaf-{leaf_index}"),
        },
        expanded: false,
        depth: 2,
      })
    }));
  }

  rows
}

/// Adds a non-interactive message row to a tree row collection.
///
/// This keeps pressure coverage for loading, empty, or error rows that share
/// the same virtual scrolling path as normal item rows.
///
/// # Parameters
///
/// `rows` is the mutable row collection receiving the message row.
///
/// `depth` is the visual nesting depth assigned to the message row.
///
/// # Returns
///
/// This function returns `()` and appends one row to `rows`.
fn append_message_row(rows: &mut Vec<TreeRow<BenchTreePayload>>, depth: usize) {
  rows.push(TreeRow::Message(TreeMessageRow {
    label: "Loading...".into(),
    depth,
  }));
}

/// Copies the rows that a virtual list would request for one viewport.
///
/// The range access mirrors the production `uniform_list` renderer, where GPUI
/// asks the component to render only the visible item range.
///
/// # Parameters
///
/// `rows` is the flattened row slice to select from.
///
/// `start` is the first requested row index.
///
/// `count` is the maximum number of rows requested for the viewport.
///
/// # Returns
///
/// A cloned vector of rows inside the requested viewport range.
fn collect_viewport_rows(
  rows: &[TreeRow<BenchTreePayload>],
  start: usize,
  count: usize,
) -> Vec<TreeRow<BenchTreePayload>> {
  (start..start.saturating_add(count))
    .filter_map(|index| rows.get(index).cloned())
    .collect()
}

/// Benchmarks constructing flattened row data.
///
/// This measures the remaining O(visible rows) data preparation cost before
/// GPUI receives the rows for virtual rendering.
///
/// # Parameters
///
/// `c` is Criterion's benchmark context.
///
/// # Returns
///
/// This function returns `()` after registering the benchmark group.
fn bench_visible_row_construction(c: &mut Criterion) {
  let mut group = c.benchmark_group("tree_visible_row_construction");

  for &row_count in TREE_SIZES {
    group.throughput(Throughput::Elements(row_count as u64));
    group.bench_with_input(
      BenchmarkId::new("flat_expanded_root", row_count),
      &row_count,
      |b, &row_count| {
        b.iter(|| flat_rows(black_box(row_count)));
      },
    );
  }

  for &row_count in TREE_SIZES {
    group.throughput(Throughput::Elements(row_count as u64));
    group.bench_with_input(
      BenchmarkId::new("expanded_groups", row_count),
      &row_count,
      |b, &row_count| {
        let leaves_per_group = 100;
        let group_count = row_count / leaves_per_group;

        b.iter(|| grouped_rows(black_box(group_count), black_box(leaves_per_group)));
      },
    );
  }

  for &row_count in TREE_SIZES {
    group.throughput(Throughput::Elements(row_count as u64));
    group.bench_with_input(
      BenchmarkId::new("rows_with_message", row_count),
      &row_count,
      |b, &row_count| {
        b.iter(|| {
          let mut rows = flat_rows(black_box(row_count));
          append_message_row(&mut rows, 1);
          rows
        });
      },
    );
  }

  group.finish();
}

/// Benchmarks selecting viewport rows from an already-flattened tree.
///
/// This isolates the virtual-scrolling path that should remain effectively
/// constant as total tree size grows.
///
/// # Parameters
///
/// `c` is Criterion's benchmark context.
///
/// # Returns
///
/// This function returns `()` after registering the benchmark group.
fn bench_virtual_viewport_selection(c: &mut Criterion) {
  let mut group = c.benchmark_group("tree_virtual_viewport_selection");

  for &row_count in TREE_SIZES {
    let rows = flat_rows(row_count);
    let middle_start = rows.len().saturating_sub(VIEWPORT_ROW_COUNT) / 2;
    let end_start = rows.len().saturating_sub(VIEWPORT_ROW_COUNT);

    group.throughput(Throughput::Elements(VIEWPORT_ROW_COUNT as u64));
    group.bench_with_input(
      BenchmarkId::new("first_viewport", row_count),
      &rows,
      |b, rows| {
        b.iter(|| collect_viewport_rows(black_box(rows), 0, VIEWPORT_ROW_COUNT));
      },
    );
    group.bench_with_input(
      BenchmarkId::new("middle_viewport", row_count),
      &rows,
      |b, rows| {
        b.iter(|| collect_viewport_rows(black_box(rows), middle_start, VIEWPORT_ROW_COUNT));
      },
    );
    group.bench_with_input(
      BenchmarkId::new("last_viewport", row_count),
      &rows,
      |b, rows| {
        b.iter(|| collect_viewport_rows(black_box(rows), end_start, VIEWPORT_ROW_COUNT));
      },
    );
  }

  group.finish();
}

criterion_group!(
  benches,
  bench_visible_row_construction,
  bench_virtual_viewport_selection
);
criterion_main!(benches);
