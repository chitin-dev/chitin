//! Benchmarks for generic tree virtualization helpers.
//!
//! These benchmarks intentionally measure the pure data path in `chitin-ui`
//! rather than GPUI painting. The goal is to separate the cost of flattening an
//! expanded tree from the cost of selecting the viewport rows that a virtual
//! list asks the component to render.

use chitin_ui::components::tree::{TreeItem, TreeItemKind, VisibleTreeItem, visible_tree_items};
use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};

const VIEWPORT_ROW_COUNT: usize = 48;
/// Total visible leaf counts used to pressure the tree flattening path.
const TREE_SIZES: &[usize] = &[1_000, 10_000, 50_000];

/// Builds a single expanded root with many leaf children.
///
/// This stresses wide directories such as generated output folders or package
/// dependency trees where one expanded node owns many direct children.
fn flat_tree(visible_leaf_count: usize) -> TreeItem {
  let children = (0..visible_leaf_count).map(|index| {
    TreeItem::new(
      format!("leaf-{index}"),
      format!("leaf-{index}"),
      TreeItemKind::Leaf,
    )
  });

  TreeItem::new("root", "root", TreeItemKind::Node)
    .children(children)
    .expanded(true)
}

/// Builds an expanded two-level tree with fixed-size groups.
///
/// This stresses a more realistic hierarchy where visible rows include both
/// expanded container nodes and leaf rows.
fn grouped_tree(group_count: usize, leaves_per_group: usize) -> TreeItem {
  let groups = (0..group_count).map(|group_index| {
    let leaves = (0..leaves_per_group).map(|leaf_index| {
      TreeItem::new(
        format!("group-{group_index}/leaf-{leaf_index}"),
        format!("leaf-{leaf_index}"),
        TreeItemKind::Leaf,
      )
    });

    TreeItem::new(
      format!("group-{group_index}"),
      format!("group-{group_index}"),
      TreeItemKind::Node,
    )
    .children(leaves)
    .expanded(true)
  });

  TreeItem::new("root", "root", TreeItemKind::Node)
    .children(groups)
    .expanded(true)
}

/// Copies the rows that a virtual list would request for one viewport.
///
/// The range access mirrors the production `uniform_list` renderer, where GPUI
/// asks the component to render only the visible item range.
fn collect_viewport_rows(
  rows: &[VisibleTreeItem],
  start: usize,
  count: usize,
) -> Vec<VisibleTreeItem> {
  (start..start.saturating_add(count))
    .filter_map(|index| rows.get(index).cloned())
    .collect()
}

/// Benchmarks converting expanded hierarchical tree data into visible rows.
///
/// This measures the remaining O(visible rows) data preparation cost before
/// GPUI receives the flattened rows for virtual rendering.
fn bench_visible_tree_flattening(c: &mut Criterion) {
  let mut group = c.benchmark_group("tree_visible_row_flattening");

  for &leaf_count in TREE_SIZES {
    group.throughput(Throughput::Elements(leaf_count as u64));
    group.bench_with_input(
      BenchmarkId::new("flat_expanded_root", leaf_count),
      &leaf_count,
      |b, &leaf_count| {
        let tree = flat_tree(leaf_count);

        b.iter(|| visible_tree_items(black_box(&tree)));
      },
    );
  }

  for &leaf_count in TREE_SIZES {
    group.throughput(Throughput::Elements(leaf_count as u64));
    group.bench_with_input(
      BenchmarkId::new("expanded_groups", leaf_count),
      &leaf_count,
      |b, &leaf_count| {
        let leaves_per_group = 100;
        let group_count = leaf_count / leaves_per_group;
        let tree = grouped_tree(group_count, leaves_per_group);

        b.iter(|| visible_tree_items(black_box(&tree)));
      },
    );
  }

  group.finish();
}

/// Benchmarks selecting viewport rows from an already-flattened tree.
///
/// This isolates the virtual-scrolling path that should remain effectively
/// constant as total tree size grows.
fn bench_virtual_viewport_selection(c: &mut Criterion) {
  let mut group = c.benchmark_group("tree_virtual_viewport_selection");

  for &leaf_count in TREE_SIZES {
    let tree = flat_tree(leaf_count);
    let rows = visible_tree_items(&tree);
    let middle_start = rows.len().saturating_sub(VIEWPORT_ROW_COUNT) / 2;
    let end_start = rows.len().saturating_sub(VIEWPORT_ROW_COUNT);

    group.throughput(Throughput::Elements(VIEWPORT_ROW_COUNT as u64));
    group.bench_with_input(
      BenchmarkId::new("first_viewport", leaf_count),
      &rows,
      |b, rows| {
        b.iter(|| collect_viewport_rows(black_box(rows), 0, VIEWPORT_ROW_COUNT));
      },
    );
    group.bench_with_input(
      BenchmarkId::new("middle_viewport", leaf_count),
      &rows,
      |b, rows| {
        b.iter(|| collect_viewport_rows(black_box(rows), middle_start, VIEWPORT_ROW_COUNT));
      },
    );
    group.bench_with_input(
      BenchmarkId::new("last_viewport", leaf_count),
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
  bench_visible_tree_flattening,
  bench_virtual_viewport_selection
);
criterion_main!(benches);
