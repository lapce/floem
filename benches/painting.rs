//! Benchmarks for painting performance in Floem.
//!
//! These benchmarks measure the performance of:
//! - Painting flat view trees (varying widths)
//! - Painting deep view trees (varying depths)
//! - Painting with CSS transforms (scale, rotate)
//! - Overlay painting (transform rebuild performance)
//!
//! These benchmarks measure painting performance with pre-computed visual_transform
//! which enables O(1) transform lookup instead of O(depth) accumulation.

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use floem::headless::HeadlessHarness;
use floem::prelude::*;
use floem::style::FlexWrap;
use floem::unit::Pct;
use floem::views::{Container, Decorators, Empty, Stack};

// =============================================================================
// View tree creation helpers
// =============================================================================

/// Create a flat tree with N children at the same level.
fn create_flat_tree(n: usize) -> impl IntoView {
    let children: Vec<_> = (0..n)
        .map(|i| {
            Empty::new()
                .style(move |s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(i as i32))
        })
        .collect();

    Stack::from_iter(children).style(|s| s.size(100.0, 100.0))
}

/// Create a deep tree with depth N.
fn create_deep_tree(depth: usize) -> impl IntoView {
    fn build_nested(remaining: usize) -> Container {
        if remaining == 0 {
            Container::new(Empty::new().style(|s| s.size(100.0, 100.0)))
                .style(|s| s.size(100.0, 100.0))
        } else {
            Container::new(build_nested(remaining - 1)).style(|s| s.padding(1.0).size(100.0, 100.0))
        }
    }
    build_nested(depth)
}

/// Create a deep tree where each level has a CSS transform.
fn create_deep_tree_with_transforms(depth: usize) -> impl IntoView {
    fn build_nested(remaining: usize) -> Container {
        if remaining == 0 {
            Container::new(Empty::new().style(|s| s.size(50.0, 50.0))).style(|s| s.size(60.0, 60.0))
        } else {
            Container::new(build_nested(remaining - 1)).style(move |s| {
                // Alternate between scale and translate
                if remaining % 2 == 0 {
                    s.padding(2.0).size(80.0, 80.0).scale(Pct(95.0))
                } else {
                    s.padding(2.0).size(80.0, 80.0).translate_x(1.0)
                }
            })
        }
    }
    build_nested(depth)
}

/// Create a tree with mixed transforms: some scale, some rotate, some plain.
fn create_mixed_transform_tree(n: usize) -> impl IntoView {
    let children: Vec<_> = (0..n)
        .map(|i| {
            let inner = Empty::new().style(|s| s.size(30.0, 30.0));
            match i % 4 {
                0 => Container::new(inner)
                    .style(|s| s.size(40.0, 40.0).scale(Pct(110.0)))
                    .into_any(),
                1 => Container::new(inner)
                    .style(|s| s.size(40.0, 40.0).rotate(15.0.deg()))
                    .into_any(),
                2 => Container::new(inner)
                    .style(|s| s.size(40.0, 40.0).translate_x(5.0).translate_y(3.0))
                    .into_any(),
                _ => Container::new(inner)
                    .style(|s| s.size(40.0, 40.0))
                    .into_any(),
            }
        })
        .collect();

    Stack::from_iter(children).style(|s| s.size(200.0, 200.0).gap(5.0).flex_wrap(FlexWrap::Wrap))
}

/// Create a wide tree with depth 2.
fn create_wide_tree_depth2(width: usize) -> impl IntoView {
    let children: Vec<_> = (0..width)
        .map(|_| {
            let grandchildren: Vec<_> = (0..width)
                .map(|_| Empty::new().style(|s| s.size(10.0, 10.0)))
                .collect();
            Stack::from_iter(grandchildren).style(|s| s.size_full())
        })
        .collect();

    Stack::from_iter(children).style(|s| s.size(100.0, 100.0))
}

// =============================================================================
// Flat tree painting benchmarks
// =============================================================================

fn bench_paint_flat_tree(c: &mut Criterion) {
    let mut group = c.benchmark_group("paint_flat_tree");

    for n in [10, 50, 100, 200].iter() {
        group.bench_with_input(BenchmarkId::new("n", n), n, |b, &n| {
            let view = create_flat_tree(n);
            let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

            b.iter(|| {
                black_box(harness.paint());
            });
        });
    }

    group.finish();
}

// =============================================================================
// Deep tree painting benchmarks
// =============================================================================

fn bench_paint_deep_tree(c: &mut Criterion) {
    let mut group = c.benchmark_group("paint_deep_tree");

    for depth in [10, 25, 50, 100].iter() {
        group.bench_with_input(BenchmarkId::new("depth", depth), depth, |b, &depth| {
            let view = create_deep_tree(depth);
            let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

            b.iter(|| {
                black_box(harness.paint());
            });
        });
    }

    group.finish();
}

// =============================================================================
// Transform painting benchmarks
// =============================================================================

fn bench_paint_with_transforms(c: &mut Criterion) {
    let mut group = c.benchmark_group("paint_with_transforms");

    // Deep tree with transforms at each level
    for depth in [5, 10, 20, 40].iter() {
        group.bench_with_input(
            BenchmarkId::new("deep_transforms", depth),
            depth,
            |b, &depth| {
                let view = create_deep_tree_with_transforms(depth);
                let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);

                b.iter(|| {
                    black_box(harness.paint());
                });
            },
        );
    }

    // Mixed transform tree (flat with different transform types)
    for n in [10, 25, 50].iter() {
        group.bench_with_input(BenchmarkId::new("mixed_transforms", n), n, |b, &n| {
            let view = create_mixed_transform_tree(n);
            let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);

            b.iter(|| {
                black_box(harness.paint());
            });
        });
    }

    group.finish();
}

// =============================================================================
// Wide tree painting benchmarks (many siblings)
// =============================================================================

fn bench_paint_wide_tree(c: &mut Criterion) {
    let mut group = c.benchmark_group("paint_wide_tree");

    for width in [5, 10, 15, 20].iter() {
        let total_nodes = width + width * width;
        let label = format!("w{}_n{}", width, total_nodes);

        group.bench_with_input(BenchmarkId::new("d2", &label), width, |b, &width| {
            let view = create_wide_tree_depth2(width);
            let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

            b.iter(|| {
                black_box(harness.paint());
            });
        });
    }

    group.finish();
}

// =============================================================================
// Paint order tracking overhead benchmark
// =============================================================================

fn bench_paint_order_tracking(c: &mut Criterion) {
    let mut group = c.benchmark_group("paint_order_tracking");

    // Compare paint with and without order tracking
    for n in [50, 100].iter() {
        // Without tracking
        group.bench_with_input(BenchmarkId::new("no_tracking", n), n, |b, &n| {
            let view = create_flat_tree(n);
            let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

            b.iter(|| {
                black_box(harness.paint());
            });
        });

        // With tracking
        group.bench_with_input(BenchmarkId::new("with_tracking", n), n, |b, &n| {
            let view = create_flat_tree(n);
            let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

            b.iter(|| {
                black_box(harness.paint_and_get_order());
            });
        });
    }

    group.finish();
}

// =============================================================================
// Repaint (incremental) benchmarks
// =============================================================================

fn bench_repaint_after_change(c: &mut Criterion) {
    let mut group = c.benchmark_group("repaint");

    // Measure repaint after a style change triggers layout
    for depth in [10, 25, 50].iter() {
        group.bench_with_input(
            BenchmarkId::new("after_layout_change", depth),
            depth,
            |b, &depth| {
                let view = create_deep_tree(depth);
                let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

                b.iter(|| {
                    // Simulate a frame: process updates and paint
                    harness.rebuild();
                    black_box(harness.paint());
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_paint_flat_tree,
    bench_paint_deep_tree,
    bench_paint_with_transforms,
    bench_paint_wide_tree,
    bench_paint_order_tracking,
    bench_repaint_after_change,
);

criterion_main!(benches);
