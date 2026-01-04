//! Benchmarks for event dispatch and propagation in Floem.
//!
//! These benchmarks measure the performance of:
//! - Event dispatch through flat view trees (varying widths)
//! - Event dispatch through deep view trees (varying depths)
//! - Stacking context collection and sorting
//! - Hit testing with overlapping views
//! - Pointer events (click, move, scroll)

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use floem::headless::HeadlessHarness;
use floem::prelude::*;
use floem::views::{Container, Decorators, Empty, Stack};

/// Create a flat tree with N children at the same level.
/// All children are positioned absolutely and overlap.
fn create_flat_tree(n: usize) -> impl IntoView {
    let children: Vec<_> = (0..n)
        .map(|i| {
            Empty::new()
                .style(move |s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(i as i32))
        })
        .collect();

    Stack::from_iter(children).style(|s| s.size(100.0, 100.0))
}

/// Create a deep tree with depth N (each level has one child).
/// Uses a recursive approach that builds nested containers.
fn create_deep_tree(depth: usize) -> impl IntoView {
    // Build from inside out
    fn build_nested(remaining: usize) -> Container {
        if remaining == 0 {
            Container::new(Empty::new().style(|s| s.size(100.0, 100.0)))
                .style(|s| s.size(100.0, 100.0))
        } else {
            Container::new(build_nested(remaining - 1)).style(|s| s.size(100.0, 100.0))
        }
    }

    build_nested(depth)
}

/// Create a tree with mixed stacking contexts.
/// Some views create stacking contexts (have z-index), others don't.
fn create_mixed_stacking_tree(n: usize) -> impl IntoView {
    let children: Vec<_> = (0..n)
        .map(|i| {
            // Every 3rd view creates a stacking context
            if i % 3 == 0 {
                Empty::new()
                    .style(move |s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(i as i32))
            } else {
                Empty::new().style(|s| s.absolute().inset(0.0).size(100.0, 100.0))
            }
        })
        .collect();

    Stack::from_iter(children).style(|s| s.size(100.0, 100.0))
}

/// Create a wide tree where each node has multiple children.
/// Uses a fixed-depth approach with stacks.
fn create_wide_tree_depth2(width: usize) -> impl IntoView {
    // Depth 2: root -> width children -> width*width grandchildren
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

/// Create a wider tree with depth 3.
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

    Stack::from_iter(children).style(|s| s.size(100.0, 100.0))
}

fn bench_flat_tree_dispatch(c: &mut Criterion) {
    let mut group = c.benchmark_group("flat_tree_event_dispatch");

    for size in [10, 50, 100, 500].iter() {
        group.bench_with_input(BenchmarkId::new("pointer_down", size), size, |b, &size| {
            let view = create_flat_tree(size);
            let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

            b.iter(|| {
                harness.pointer_down(black_box(50.0), black_box(50.0));
            });
        });

        group.bench_with_input(BenchmarkId::new("pointer_move", size), size, |b, &size| {
            let view = create_flat_tree(size);
            let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

            b.iter(|| {
                harness.pointer_move(black_box(50.0), black_box(50.0));
            });
        });

        group.bench_with_input(BenchmarkId::new("click", size), size, |b, &size| {
            let view = create_flat_tree(size);
            let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

            b.iter(|| {
                harness.click(black_box(50.0), black_box(50.0));
            });
        });
    }

    group.finish();
}

fn bench_deep_tree_dispatch(c: &mut Criterion) {
    let mut group = c.benchmark_group("deep_tree_event_dispatch");

    for depth in [5, 10, 20, 50].iter() {
        group.bench_with_input(
            BenchmarkId::new("pointer_down", depth),
            depth,
            |b, &depth| {
                let view = create_deep_tree(depth);
                let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

                b.iter(|| {
                    harness.pointer_down(black_box(50.0), black_box(50.0));
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("pointer_move", depth),
            depth,
            |b, &depth| {
                let view = create_deep_tree(depth);
                let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

                b.iter(|| {
                    harness.pointer_move(black_box(50.0), black_box(50.0));
                });
            },
        );
    }

    group.finish();
}

fn bench_mixed_stacking_dispatch(c: &mut Criterion) {
    let mut group = c.benchmark_group("mixed_stacking_event_dispatch");

    for size in [10, 50, 100].iter() {
        group.bench_with_input(BenchmarkId::new("pointer_down", size), size, |b, &size| {
            let view = create_mixed_stacking_tree(size);
            let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

            b.iter(|| {
                harness.pointer_down(black_box(50.0), black_box(50.0));
            });
        });
    }

    group.finish();
}

fn bench_wide_tree_dispatch(c: &mut Criterion) {
    let mut group = c.benchmark_group("wide_tree_event_dispatch");

    // Test different widths with depth 2
    for width in [3, 5, 10, 20].iter() {
        let total_nodes = width + width * width; // root + children + grandchildren
        let label = format!("w{}_d2_n{}", width, total_nodes);

        group.bench_with_input(
            BenchmarkId::new("pointer_down", &label),
            width,
            |b, &width| {
                let view = create_wide_tree_depth2(width);
                let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

                b.iter(|| {
                    harness.pointer_down(black_box(5.0), black_box(5.0));
                });
            },
        );
    }

    // Test a few widths with depth 3
    for width in [3, 5].iter() {
        let total_nodes = width + width * width + width * width * width;
        let label = format!("w{}_d3_n{}", width, total_nodes);

        group.bench_with_input(
            BenchmarkId::new("pointer_down", &label),
            width,
            |b, &width| {
                let view = create_wide_tree_depth3(width);
                let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

                b.iter(|| {
                    harness.pointer_down(black_box(5.0), black_box(5.0));
                });
            },
        );
    }

    group.finish();
}

fn bench_hit_testing(c: &mut Criterion) {
    let mut group = c.benchmark_group("hit_testing");

    for size in [10, 50, 100].iter() {
        group.bench_with_input(BenchmarkId::new("flat_tree", size), size, |b, &size| {
            let view = create_flat_tree(size);
            let harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

            b.iter(|| {
                harness.view_at(black_box(50.0), black_box(50.0));
            });
        });
    }

    for depth in [5, 10, 20].iter() {
        group.bench_with_input(BenchmarkId::new("deep_tree", depth), depth, |b, &depth| {
            let view = create_deep_tree(depth);
            let harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

            b.iter(|| {
                harness.view_at(black_box(50.0), black_box(50.0));
            });
        });
    }

    // Hit test with misses (point outside views)
    group.bench_function("miss", |b| {
        let view = create_flat_tree(50);
        let harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

        b.iter(|| {
            harness.view_at(black_box(150.0), black_box(150.0));
        });
    });

    group.finish();
}

fn bench_scroll_events(c: &mut Criterion) {
    let mut group = c.benchmark_group("scroll_events");

    for size in [10, 50, 100].iter() {
        group.bench_with_input(BenchmarkId::new("flat_tree", size), size, |b, &size| {
            let view = create_flat_tree(size);
            let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

            b.iter(|| {
                harness.scroll(
                    black_box(50.0),
                    black_box(50.0),
                    black_box(0.0),
                    black_box(-10.0),
                );
            });
        });
    }

    group.finish();
}

fn bench_event_sequence(c: &mut Criterion) {
    let mut group = c.benchmark_group("event_sequence");

    // Benchmark a typical interaction: move -> down -> up (click pattern)
    for size in [10, 50, 100].iter() {
        group.bench_with_input(
            BenchmarkId::new("move_click_sequence", size),
            size,
            |b, &size| {
                let view = create_flat_tree(size);
                let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

                b.iter(|| {
                    harness.pointer_move(black_box(50.0), black_box(50.0));
                    harness.pointer_down(black_box(50.0), black_box(50.0));
                    harness.pointer_up(black_box(50.0), black_box(50.0));
                });
            },
        );
    }

    // Benchmark drag-like movement (multiple pointer_move events)
    group.bench_function("drag_sequence_10_moves", |b| {
        let view = create_flat_tree(50);
        let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

        b.iter(|| {
            harness.pointer_down(black_box(10.0), black_box(50.0));
            for i in 0..10 {
                harness.pointer_move(black_box(10.0 + i as f64 * 8.0), black_box(50.0));
            }
            harness.pointer_up(black_box(90.0), black_box(50.0));
        });
    });

    group.finish();
}

/// Benchmarks specifically designed to measure hit test result caching effectiveness.
/// These simulate real-world event patterns where multiple events occur at the same location.
fn bench_cache_effectiveness(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache_effectiveness");

    // Benchmark: 20 consecutive events at the SAME location
    // This is the ideal case for a result cache - should see maximum benefit
    for size in [50, 100].iter() {
        group.bench_with_input(
            BenchmarkId::new("same_location_20_events", size),
            size,
            |b, &size| {
                let view = create_flat_tree(size);
                let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

                b.iter(|| {
                    for _ in 0..20 {
                        harness.pointer_move(black_box(50.0), black_box(50.0));
                    }
                });
            },
        );
    }

    // Benchmark: Alternating between 2 locations
    // A 2-entry cache should handle this well without thrashing
    for size in [50, 100].iter() {
        group.bench_with_input(
            BenchmarkId::new("alternating_2_locations", size),
            size,
            |b, &size| {
                let view = create_flat_tree(size);
                let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

                b.iter(|| {
                    for i in 0..20 {
                        let x = if i % 2 == 0 { 25.0 } else { 75.0 };
                        harness.pointer_move(black_box(x), black_box(50.0));
                    }
                });
            },
        );
    }

    // Benchmark: Alternating between 3 locations
    // A 2-entry cache will thrash here - this shows the limitation
    for size in [50, 100].iter() {
        group.bench_with_input(
            BenchmarkId::new("alternating_3_locations", size),
            size,
            |b, &size| {
                let view = create_flat_tree(size);
                let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

                b.iter(|| {
                    for i in 0..20 {
                        let x = match i % 3 {
                            0 => 20.0,
                            1 => 50.0,
                            _ => 80.0,
                        };
                        harness.pointer_move(black_box(x), black_box(50.0));
                    }
                });
            },
        );
    }

    // Benchmark: Simulated mouse hover with small movements
    // Real mouse movement often stays in a small area
    group.bench_function("hover_jitter_small_area", |b| {
        let view = create_flat_tree(100);
        let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

        b.iter(|| {
            // Simulate small jittery movements around a point
            for i in 0..20 {
                let offset = (i % 5) as f64 * 0.5; // 0, 0.5, 1.0, 1.5, 2.0 pixel offsets
                harness.pointer_move(black_box(50.0 + offset), black_box(50.0));
            }
        });
    });

    // Benchmark: Hit testing only (no event dispatch overhead)
    // Direct comparison for cache effectiveness on hit_test alone
    for size in [50, 100].iter() {
        group.bench_with_input(
            BenchmarkId::new("hit_test_same_location_20x", size),
            size,
            |b, &size| {
                let view = create_flat_tree(size);
                let harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

                b.iter(|| {
                    for _ in 0..20 {
                        black_box(harness.view_at(black_box(50.0), black_box(50.0)));
                    }
                });
            },
        );
    }

    group.finish();
}

/// Benchmarks specifically for the "no listeners" early exit optimization.
/// These trees have NO event listeners, so we can measure the overhead of
/// dispatching through paths that don't need processing.
fn bench_no_listeners_path(c: &mut Criterion) {
    let mut group = c.benchmark_group("no_listeners_path");

    // Flat tree with no listeners - tests sibling traversal
    for size in [10, 50, 100, 200].iter() {
        group.bench_with_input(
            BenchmarkId::new("flat_no_listeners", size),
            size,
            |b, &size| {
                let view = create_flat_tree(size);
                let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

                b.iter(|| {
                    harness.pointer_down(black_box(50.0), black_box(50.0));
                    harness.pointer_up(black_box(50.0), black_box(50.0));
                });
            },
        );
    }

    // Deep tree with no listeners - tests path building and traversal
    for depth in [10, 20, 50, 100].iter() {
        group.bench_with_input(
            BenchmarkId::new("deep_no_listeners", depth),
            depth,
            |b, &depth| {
                let view = create_deep_tree(depth);
                let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

                b.iter(|| {
                    harness.pointer_down(black_box(50.0), black_box(50.0));
                    harness.pointer_up(black_box(50.0), black_box(50.0));
                });
            },
        );
    }

    // Wide tree depth 3 with no listeners
    for width in [3, 5, 7].iter() {
        let total_nodes = width + width * width + width * width * width;
        let label = format!("w{}_d3_n{}", width, total_nodes);

        group.bench_with_input(
            BenchmarkId::new("wide_no_listeners", &label),
            width,
            |b, &width| {
                let view = create_wide_tree_depth3(width);
                let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

                b.iter(|| {
                    harness.pointer_down(black_box(5.0), black_box(5.0));
                    harness.pointer_up(black_box(5.0), black_box(5.0));
                });
            },
        );
    }

    group.finish();
}

/// Benchmarks for the active view dispatch path.
/// This path is taken when a view sets itself as "active" (e.g., slider during drag).
/// Events are then dispatched via dispatch_to_active_view instead of path-based dispatch.
///
/// Uses real Slider widget which calls cx.update_active() on pointer down.
fn bench_active_view_dispatch(c: &mut Criterion) {
    use floem::views::slider;

    let mut group = c.benchmark_group("active_view_dispatch");

    // Create a slider nested at various depths to test dispatch_to_active_view
    fn create_slider_tree(depth: usize) -> impl IntoView {
        fn build_nested(remaining: usize) -> Container {
            if remaining == 0 {
                // Innermost: the slider (which sets itself as active on pointer down)
                Container::new(
                    slider::Slider::new(|| 50.0.pct()).style(|s| s.width(80.0).height(20.0)),
                )
                .style(|s| s.size(100.0, 100.0))
            } else {
                Container::new(build_nested(remaining - 1))
                    .style(|s| s.padding(5.0).size(100.0, 100.0))
            }
        }
        build_nested(depth)
    }

    // Benchmark pointer_up after pointer_down (uses dispatch_to_active_view)
    // The slider sets itself as active on pointer_down, so pointer_up goes through
    // dispatch_to_active_view with the coordinate transformation we're testing.
    for depth in [0, 1, 5, 10].iter() {
        group.bench_with_input(
            BenchmarkId::new("slider_click", depth),
            depth,
            |b, &depth| {
                let view = create_slider_tree(depth);
                let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);

                // Calculate slider position based on nesting depth
                // Each level adds 5px padding
                let offset = depth as f64 * 5.0 + 10.0; // slider is ~10px from container edge

                b.iter(|| {
                    harness.pointer_down(black_box(offset + 40.0), black_box(offset + 10.0));
                    harness.pointer_up(black_box(offset + 40.0), black_box(offset + 10.0));
                });
            },
        );
    }

    // Benchmark drag sequence with slider (multiple moves via dispatch_to_active_view)
    for depth in [0, 1, 5].iter() {
        group.bench_with_input(
            BenchmarkId::new("slider_drag", depth),
            depth,
            |b, &depth| {
                let view = create_slider_tree(depth);
                let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);

                let offset = depth as f64 * 5.0 + 10.0;
                let y = offset + 10.0;

                b.iter(|| {
                    harness.pointer_down(black_box(offset + 10.0), black_box(y));
                    // All these moves go through dispatch_to_active_view
                    for i in 0..10 {
                        harness.pointer_move(black_box(offset + 10.0 + i as f64 * 6.0), black_box(y));
                    }
                    harness.pointer_up(black_box(offset + 70.0), black_box(y));
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_flat_tree_dispatch,
    bench_deep_tree_dispatch,
    bench_mixed_stacking_dispatch,
    bench_wide_tree_dispatch,
    bench_hit_testing,
    bench_scroll_events,
    bench_event_sequence,
    bench_cache_effectiveness,
    bench_no_listeners_path,
    bench_active_view_dispatch,
);

criterion_main!(benches);
