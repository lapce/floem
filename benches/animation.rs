//! Benchmarks for animation processing in Floem.
//!
//! These benchmarks measure the performance of:
//! - Style computation for views with animations attached
//! - Animation processing overhead during style passes
//! - Scaling with number of animated views
//!
//! Run with: cargo bench --bench animation

use std::hint::black_box;
use std::time::Duration;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use floem::animate::Animation;
use floem::headless::HeadlessHarness;
use floem::peniko::color::palette;
use floem::prelude::*;
use floem::views::{Decorators, Empty, Stack};

/// Create N views WITHOUT animations (baseline).
fn create_views_no_animation(n: usize) -> impl IntoView {
    let children: Vec<_> = (0..n)
        .map(|_| {
            Empty::new().style(|s| {
                s.size(20.0, 20.0)
                    .background(palette::css::CORAL)
                    .padding(2.0)
            })
        })
        .collect();

    Stack::from_iter(children).style(|s| s.size(400.0, 400.0))
}

/// Create N views WITH animations attached (but in idle/stopped state initially).
fn create_views_with_animation(n: usize) -> impl IntoView {
    let children: Vec<_> = (0..n)
        .map(|_| {
            Empty::new()
                .style(|s| {
                    s.size(20.0, 20.0)
                        .background(palette::css::CORAL)
                        .padding(2.0)
                })
                .animation(|_| {
                    Animation::new()
                        .keyframe(0, |f| f.computed_style())
                        .keyframe(100, |f| f.style(|s| s.background(palette::css::BLUE)))
                        .duration(Duration::from_millis(500))
                })
        })
        .collect();

    Stack::from_iter(children).style(|s| s.size(400.0, 400.0))
}

/// Create N views with repeating animations (always active).
fn create_views_with_repeating_animation(n: usize) -> impl IntoView {
    let children: Vec<_> = (0..n)
        .map(|_| {
            Empty::new()
                .style(|s| {
                    s.size(20.0, 20.0)
                        .background(palette::css::CORAL)
                        .padding(2.0)
                })
                .animation(|_| {
                    Animation::new()
                        .keyframe(0, |f| f.computed_style())
                        .keyframe(100, |f| f.style(|s| s.background(palette::css::BLUE)))
                        .duration(Duration::from_millis(500))
                        .repeat(true)
                })
        })
        .collect();

    Stack::from_iter(children).style(|s| s.size(400.0, 400.0))
}

/// Create N views with complex animations (multiple keyframes, easing).
fn create_views_with_complex_animation(n: usize) -> impl IntoView {
    let children: Vec<_> = (0..n)
        .map(|_| {
            Empty::new()
                .style(|s| {
                    s.size(20.0, 20.0)
                        .background(palette::css::CORAL)
                        .padding(2.0)
                        .border(1.0)
                })
                .animation(|_| {
                    Animation::new()
                        .keyframe(0, |f| f.computed_style())
                        .keyframe(25, |f| {
                            f.style(|s| s.background(palette::css::RED).size(25.0, 25.0))
                                .ease_in()
                        })
                        .keyframe(50, |f| {
                            f.style(|s| s.background(palette::css::GREEN).size(30.0, 30.0))
                                .ease_out()
                        })
                        .keyframe(75, |f| {
                            f.style(|s| s.background(palette::css::YELLOW).size(25.0, 25.0))
                                .ease_in_out()
                        })
                        .keyframe(100, |f| {
                            f.style(|s| s.background(palette::css::BLUE).size(20.0, 20.0))
                        })
                        .duration(Duration::from_secs(2))
                        .repeat(true)
                        .auto_reverse(true)
                })
        })
        .collect();

    Stack::from_iter(children).style(|s| s.size(400.0, 400.0))
}

/// Benchmark: Baseline - views without animations.
///
/// This establishes the baseline cost of style computation without animations.
fn bench_no_animation_baseline(c: &mut Criterion) {
    let mut group = c.benchmark_group("animation_baseline");

    for size in [10, 50, 100].iter() {
        group.bench_with_input(BenchmarkId::new("no_animation", size), size, |b, &size| {
            b.iter(|| {
                let view = create_views_no_animation(size);
                let harness = HeadlessHarness::new_with_size(view, 400.0, 400.0);
                black_box(harness.root_id());
            });
        });
    }

    group.finish();
}

/// Benchmark: Views with animations attached.
///
/// Measures the overhead of having animation infrastructure attached to views,
/// even before animations are actively running.
fn bench_with_animation_attached(c: &mut Criterion) {
    let mut group = c.benchmark_group("animation_attached");

    for size in [10, 50, 100].iter() {
        group.bench_with_input(
            BenchmarkId::new("with_animation", size),
            size,
            |b, &size| {
                b.iter(|| {
                    let view = create_views_with_animation(size);
                    let harness = HeadlessHarness::new_with_size(view, 400.0, 400.0);
                    black_box(harness.root_id());
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: Rebuild with active repeating animations.
///
/// Measures the per-frame cost of processing active animations during style computation.
fn bench_active_animation_rebuild(c: &mut Criterion) {
    let mut group = c.benchmark_group("animation_active_rebuild");

    for size in [10, 50, 100].iter() {
        group.bench_with_input(
            BenchmarkId::new("repeating_animation", size),
            size,
            |b, &size| {
                let view = create_views_with_repeating_animation(size);
                let mut harness = HeadlessHarness::new_with_size(view, 400.0, 400.0);

                b.iter(|| {
                    harness.rebuild();
                    black_box(harness.root_id());
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: Rebuild with complex multi-keyframe animations.
///
/// Measures the cost of processing animations with multiple keyframes and easing functions.
fn bench_complex_animation_rebuild(c: &mut Criterion) {
    let mut group = c.benchmark_group("animation_complex_rebuild");

    for size in [10, 50, 100].iter() {
        group.bench_with_input(
            BenchmarkId::new("complex_animation", size),
            size,
            |b, &size| {
                let view = create_views_with_complex_animation(size);
                let mut harness = HeadlessHarness::new_with_size(view, 400.0, 400.0);

                b.iter(|| {
                    harness.rebuild();
                    black_box(harness.root_id());
                });
            },
        );
    }

    group.finish();
}

/// Create mixed views - some with animations, some without.
fn create_mixed_views(total: usize, animated_percent: usize) -> impl IntoView {
    use floem::view::AnyView;

    let animated_count = total * animated_percent / 100;

    let children: Vec<AnyView> = (0..total)
        .map(|i| {
            if i < animated_count {
                // Animated view
                Empty::new()
                    .style(|s| {
                        s.size(20.0, 20.0)
                            .background(palette::css::CORAL)
                            .padding(2.0)
                    })
                    .animation(|_| {
                        Animation::new()
                            .keyframe(0, |f| f.computed_style())
                            .keyframe(100, |f| f.style(|s| s.background(palette::css::BLUE)))
                            .duration(Duration::from_millis(500))
                            .repeat(true)
                    })
                    .into_any()
            } else {
                // Static view
                Empty::new()
                    .style(|s| {
                        s.size(20.0, 20.0)
                            .background(palette::css::CORAL)
                            .padding(2.0)
                    })
                    .into_any()
            }
        })
        .collect();

    Stack::from_iter(children).style(|s| s.size(400.0, 400.0))
}

/// Benchmark: Mixed animated and non-animated views.
///
/// Simulates a realistic scenario where only some views have animations.
fn bench_mixed_animation(c: &mut Criterion) {
    let mut group = c.benchmark_group("animation_mixed");

    // 10% of views animated
    for total in [50, 100, 200].iter() {
        group.bench_with_input(
            BenchmarkId::new("10_percent_animated", total),
            total,
            |b, &total| {
                let view = create_mixed_views(total, 10);
                let mut harness = HeadlessHarness::new_with_size(view, 400.0, 400.0);

                b.iter(|| {
                    harness.rebuild();
                    black_box(harness.root_id());
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: Multiple animation passes (simulating sustained animation).
///
/// Measures the cost of running many consecutive animation frames.
fn bench_sustained_animation(c: &mut Criterion) {
    let mut group = c.benchmark_group("animation_sustained");

    group.bench_function("50_views_10_frames", |b| {
        let view = create_views_with_repeating_animation(50);
        let mut harness = HeadlessHarness::new_with_size(view, 400.0, 400.0);

        b.iter(|| {
            for _ in 0..10 {
                harness.rebuild();
            }
            black_box(harness.root_id());
        });
    });

    group.bench_function("100_views_10_frames", |b| {
        let view = create_views_with_repeating_animation(100);
        let mut harness = HeadlessHarness::new_with_size(view, 400.0, 400.0);

        b.iter(|| {
            for _ in 0..10 {
                harness.rebuild();
            }
            black_box(harness.root_id());
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_no_animation_baseline,
    bench_with_animation_attached,
    bench_active_animation_rebuild,
    bench_complex_animation_rebuild,
    bench_mixed_animation,
    bench_sustained_animation,
);
criterion_main!(benches);
