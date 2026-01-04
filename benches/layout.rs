//! Benchmarks for layout performance in Floem.
//!
//! These benchmarks measure the performance of:
//! - Layout computation for different tree structures
//! - Transform computation (visual_transform) during layout
//! - Deep nesting with accumulated transforms
//! - Window origin computation
//!
//! These benchmarks measure layout performance with visual_transform as the
//! single source of truth for view positioning.

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use floem::ViewId;
use floem::headless::HeadlessHarness;
use floem::prelude::*;
use floem::unit::Pct;
use floem::views::{Container, Decorators, Empty, Stack};

// =============================================================================
// View tree creation helpers
// =============================================================================

/// Create a flat tree with N children at the same level.
fn create_flat_tree(n: usize) -> impl IntoView {
    let children: Vec<_> = (0..n)
        .map(|_| Empty::new().style(|s| s.size(20.0, 20.0)))
        .collect();

    Stack::from_iter(children).style(|s| s.size(200.0, 200.0).gap(5.0))
}

/// Create a deep tree with depth N.
fn create_deep_tree(depth: usize) -> impl IntoView {
    fn build_nested(remaining: usize) -> Container {
        if remaining == 0 {
            Container::new(Empty::new().style(|s| s.size(50.0, 50.0))).style(|s| s.size(60.0, 60.0))
        } else {
            Container::new(build_nested(remaining - 1)).style(|s| s.padding(2.0).size_full())
        }
    }
    build_nested(depth)
}

/// Create a deep tree where each level has a CSS transform.
fn create_deep_tree_with_transforms(depth: usize) -> impl IntoView {
    fn build_nested(remaining: usize) -> Container {
        if remaining == 0 {
            Container::new(Empty::new().style(|s| s.size(30.0, 30.0))).style(|s| s.size(40.0, 40.0))
        } else {
            Container::new(build_nested(remaining - 1)).style(move |s| {
                // Alternate between different transform types
                match remaining % 3 {
                    0 => s.padding(2.0).size_full().scale(Pct(98.0)),
                    1 => s.padding(2.0).size_full().translate_x(1.0),
                    _ => s.padding(2.0).size_full().rotate(1.0.deg()),
                }
            })
        }
    }
    build_nested(depth)
}

/// Create a wide tree with depth 2.
fn create_wide_tree_depth2(width: usize) -> impl IntoView {
    let children: Vec<_> = (0..width)
        .map(|_| {
            let grandchildren: Vec<_> = (0..width)
                .map(|_| Empty::new().style(|s| s.size(10.0, 10.0)))
                .collect();
            Stack::from_iter(grandchildren).style(|s| s.size_full().gap(1.0))
        })
        .collect();

    Stack::from_iter(children).style(|s| s.size(200.0, 200.0).gap(2.0))
}

/// Create a wide tree with depth 3.
fn create_wide_tree_depth3(width: usize) -> impl IntoView {
    let children: Vec<_> = (0..width)
        .map(|_| {
            let grandchildren: Vec<_> = (0..width)
                .map(|_| {
                    let great_grandchildren: Vec<_> = (0..width)
                        .map(|_| Empty::new().style(|s| s.size(5.0, 5.0)))
                        .collect();
                    Stack::from_iter(great_grandchildren).style(|s| s.size_full())
                })
                .collect();
            Stack::from_iter(grandchildren).style(|s| s.size_full())
        })
        .collect();

    Stack::from_iter(children).style(|s| s.size(200.0, 200.0))
}

// =============================================================================
// Full layout benchmarks
// =============================================================================

fn bench_layout_flat_tree(c: &mut Criterion) {
    let mut group = c.benchmark_group("layout_flat_tree");

    for n in [10, 50, 100, 200].iter() {
        group.bench_with_input(BenchmarkId::new("n", n), n, |b, &n| {
            let view = create_flat_tree(n);
            let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);

            b.iter(|| {
                // Force layout recomputation
                harness.set_size(200.0, 200.0);
                black_box(harness.rebuild());
            });
        });
    }

    group.finish();
}

fn bench_layout_deep_tree(c: &mut Criterion) {
    let mut group = c.benchmark_group("layout_deep_tree");

    for depth in [10, 25, 50, 100].iter() {
        group.bench_with_input(BenchmarkId::new("depth", depth), depth, |b, &depth| {
            let view = create_deep_tree(depth);
            let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);

            b.iter(|| {
                harness.set_size(200.0, 200.0);
                black_box(harness.rebuild());
            });
        });
    }

    group.finish();
}

// =============================================================================
// Transform computation benchmarks
// =============================================================================

fn bench_layout_transform_computation(c: &mut Criterion) {
    let mut group = c.benchmark_group("layout_transform_computation");

    // Deep tree with transforms at each level
    // This tests the visual_transform computation path
    for depth in [5, 10, 20, 40].iter() {
        group.bench_with_input(
            BenchmarkId::new("deep_transforms", depth),
            depth,
            |b, &depth| {
                let view = create_deep_tree_with_transforms(depth);
                let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);

                b.iter(|| {
                    harness.set_size(200.0, 200.0);
                    black_box(harness.rebuild());
                });
            },
        );
    }

    // Compare deep tree with and without transforms
    for depth in [20, 50].iter() {
        // Without transforms
        group.bench_with_input(
            BenchmarkId::new("no_transforms", depth),
            depth,
            |b, &depth| {
                let view = create_deep_tree(depth);
                let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);

                b.iter(|| {
                    harness.set_size(200.0, 200.0);
                    black_box(harness.rebuild());
                });
            },
        );

        // With transforms
        group.bench_with_input(
            BenchmarkId::new("with_transforms", depth),
            depth,
            |b, &depth| {
                let view = create_deep_tree_with_transforms(depth);
                let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);

                b.iter(|| {
                    harness.set_size(200.0, 200.0);
                    black_box(harness.rebuild());
                });
            },
        );
    }

    group.finish();
}

// =============================================================================
// Wide tree layout benchmarks
// =============================================================================

fn bench_layout_wide_tree(c: &mut Criterion) {
    let mut group = c.benchmark_group("layout_wide_tree");

    // Depth 2
    for width in [5, 10, 15, 20].iter() {
        let total_nodes = width + width * width;
        let label = format!("d2_w{}_n{}", width, total_nodes);

        group.bench_with_input(BenchmarkId::new("depth2", &label), width, |b, &width| {
            let view = create_wide_tree_depth2(width);
            let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);

            b.iter(|| {
                harness.set_size(200.0, 200.0);
                black_box(harness.rebuild());
            });
        });
    }

    // Depth 3
    for width in [3, 5, 7].iter() {
        let total_nodes = width + width * width + width * width * width;
        let label = format!("d3_w{}_n{}", width, total_nodes);

        group.bench_with_input(BenchmarkId::new("depth3", &label), width, |b, &width| {
            let view = create_wide_tree_depth3(width);
            let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);

            b.iter(|| {
                harness.set_size(200.0, 200.0);
                black_box(harness.rebuild());
            });
        });
    }

    group.finish();
}

// =============================================================================
// Incremental layout benchmarks
// =============================================================================

fn bench_incremental_layout(c: &mut Criterion) {
    let mut group = c.benchmark_group("incremental_layout");

    // Measure layout after a size change (should trigger re-layout)
    for depth in [10, 25, 50].iter() {
        group.bench_with_input(BenchmarkId::new("resize", depth), depth, |b, &depth| {
            let view = create_deep_tree(depth);
            let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
            harness.rebuild();

            let mut size = 200.0;
            b.iter(|| {
                // Alternate size to force layout
                size = if size == 200.0 { 201.0 } else { 200.0 };
                harness.set_size(size, size);
                black_box(harness.rebuild());
            });
        });
    }

    group.finish();
}

// =============================================================================
// Visual origin access benchmarks
// =============================================================================

fn bench_visual_origin_access(c: &mut Criterion) {
    let mut group = c.benchmark_group("visual_origin_access");

    // Measure the cost of accessing visual_origin for deeply nested views
    for depth in [10, 25, 50, 100].iter() {
        group.bench_with_input(BenchmarkId::new("get_origin", depth), depth, |b, &depth| {
            // Build the tree and get the deepest view's ID
            fn get_deepest_id(depth: usize) -> (impl IntoView, ViewId) {
                fn build_nested(remaining: usize, deepest_id: &mut Option<ViewId>) -> Container {
                    if remaining == 0 {
                        let inner = Empty::new().style(|s| s.size(50.0, 50.0));
                        *deepest_id = Some(inner.view_id());
                        Container::new(inner).style(|s| s.size(60.0, 60.0))
                    } else {
                        Container::new(build_nested(remaining - 1, deepest_id))
                            .style(|s| s.padding(2.0).size_full())
                    }
                }
                let mut deepest_id = None;
                let view = build_nested(depth, &mut deepest_id);
                (view, deepest_id.unwrap())
            }

            let (view, deepest_id) = get_deepest_id(depth);
            let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
            harness.rebuild();

            b.iter(|| {
                black_box(deepest_id.get_visual_origin());
            });
        });
    }

    // Measure the cost of accessing visual_transform
    for depth in [10, 25, 50, 100].iter() {
        group.bench_with_input(
            BenchmarkId::new("get_transform", depth),
            depth,
            |b, &depth| {
                fn get_deepest_id(depth: usize) -> (impl IntoView, ViewId) {
                    fn build_nested(
                        remaining: usize,
                        deepest_id: &mut Option<ViewId>,
                    ) -> Container {
                        if remaining == 0 {
                            let inner = Empty::new().style(|s| s.size(50.0, 50.0));
                            *deepest_id = Some(inner.view_id());
                            Container::new(inner).style(|s| s.size(60.0, 60.0))
                        } else {
                            Container::new(build_nested(remaining - 1, deepest_id))
                                .style(|s| s.padding(2.0).size_full())
                        }
                    }
                    let mut deepest_id = None;
                    let view = build_nested(depth, &mut deepest_id);
                    (view, deepest_id.unwrap())
                }

                let (view, deepest_id) = get_deepest_id(depth);
                let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);
                harness.rebuild();

                b.iter(|| {
                    black_box(deepest_id.get_visual_transform());
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_layout_flat_tree,
    bench_layout_deep_tree,
    bench_layout_transform_computation,
    bench_layout_wide_tree,
    bench_incremental_layout,
    bench_visual_origin_access,
);

criterion_main!(benches);
