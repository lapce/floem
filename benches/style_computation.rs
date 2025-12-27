//! Benchmarks for style computation in Floem.
//!
//! These benchmarks measure the performance of:
//! - Style computation for views with simple styles
//! - Style computation for views with identical styles (caching opportunity)
//! - Style resolution with nested views (inheritance)
//! - Style computation with many property types

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use floem::headless::HeadlessHarness;
use floem::prelude::*;
use floem::style::Background;
use floem::views::{Container, Decorators, Empty, Stack};

/// Create N views with identical styles (good candidate for caching).
fn create_identical_styled_views(n: usize) -> impl IntoView {
    let children: Vec<_> = (0..n)
        .map(|_| {
            Empty::new().style(|s| {
                s.size(20.0, 20.0)
                    .background(palette::css::CORAL)
                    .padding(2.0)
                    .margin(1.0)
            })
        })
        .collect();

    Stack::from_iter(children).style(|s| s.size(400.0, 400.0))
}

/// Create N views with different styles (no caching opportunity).
fn create_different_styled_views(n: usize) -> impl IntoView {
    // Pick from a set of distinct colors
    let colors = [
        palette::css::RED,
        palette::css::BLUE,
        palette::css::GREEN,
        palette::css::YELLOW,
        palette::css::PURPLE,
        palette::css::ORANGE,
        palette::css::PINK,
        palette::css::CYAN,
        palette::css::LIME,
        palette::css::TEAL,
    ];

    let children: Vec<_> = (0..n)
        .map(|i| {
            let color = colors[i % colors.len()];
            Empty::new().style(move |s| {
                s.size(20.0, 20.0)
                    .background(color)
                    .padding((i % 10) as f32)
            })
        })
        .collect();

    Stack::from_iter(children).style(|s| s.size(400.0, 400.0))
}

/// Create a deep nested tree (tests inheritance).
fn create_deep_styled_tree(depth: usize) -> Container {
    if depth == 0 {
        Container::new(Empty::new().style(|s| s.size(10.0, 10.0).background(palette::css::GOLD)))
            .style(|s| s.size(20.0, 20.0))
    } else {
        Container::new(create_deep_styled_tree(depth - 1)).style(|s| s.size_full().padding(1.0))
    }
}

/// Create views with complex styles (many properties).
fn create_complex_styled_views(n: usize) -> impl IntoView {
    let children: Vec<_> = (0..n)
        .map(|_| {
            Empty::new().style(|s| {
                s.size(50.0, 50.0)
                    .background(palette::css::LIGHT_BLUE)
                    .padding(5.0)
                    .margin(3.0)
                    .border(1.0)
                    .border_color(palette::css::GRAY)
                    .border_radius(4.0)
            })
        })
        .collect();

    Stack::from_iter(children).style(|s| s.size(500.0, 500.0))
}

fn bench_identical_styles(c: &mut Criterion) {
    let mut group = c.benchmark_group("style_computation_identical");

    for size in [10, 50, 100, 200].iter() {
        group.bench_with_input(
            BenchmarkId::new("create_and_compute", size),
            size,
            |b, &size| {
                b.iter(|| {
                    let view = create_identical_styled_views(size);
                    let harness = HeadlessHarness::new_with_size(view, 400.0, 400.0);
                    black_box(harness.root_id());
                });
            },
        );
    }

    group.finish();
}

fn bench_different_styles(c: &mut Criterion) {
    let mut group = c.benchmark_group("style_computation_different");

    for size in [10, 50, 100, 200].iter() {
        group.bench_with_input(
            BenchmarkId::new("create_and_compute", size),
            size,
            |b, &size| {
                b.iter(|| {
                    let view = create_different_styled_views(size);
                    let harness = HeadlessHarness::new_with_size(view, 400.0, 400.0);
                    black_box(harness.root_id());
                });
            },
        );
    }

    group.finish();
}

fn bench_deep_nesting(c: &mut Criterion) {
    let mut group = c.benchmark_group("style_computation_deep");

    for depth in [5, 10, 20, 30].iter() {
        group.bench_with_input(
            BenchmarkId::new("create_and_compute", depth),
            depth,
            |b, &depth| {
                b.iter(|| {
                    let view = create_deep_styled_tree(depth);
                    let harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);
                    black_box(harness.root_id());
                });
            },
        );
    }

    group.finish();
}

fn bench_complex_styles(c: &mut Criterion) {
    let mut group = c.benchmark_group("style_computation_complex");

    for size in [10, 50, 100].iter() {
        group.bench_with_input(
            BenchmarkId::new("create_and_compute", size),
            size,
            |b, &size| {
                b.iter(|| {
                    let view = create_complex_styled_views(size);
                    let harness = HeadlessHarness::new_with_size(view, 500.0, 500.0);
                    black_box(harness.root_id());
                });
            },
        );
    }

    group.finish();
}

fn bench_get_computed_style(c: &mut Criterion) {
    let mut group = c.benchmark_group("get_computed_style");

    // Setup: create views once
    let views: Vec<_> = (0..100)
        .map(|_| {
            Empty::new().style(|s| {
                s.size(20.0, 20.0)
                    .background(palette::css::CORAL)
                    .padding(2.0)
            })
        })
        .collect();

    let ids: Vec<_> = views.iter().map(|v| v.view_id()).collect();
    let container = Stack::from_iter(views).style(|s| s.size(400.0, 400.0));
    let harness = HeadlessHarness::new_with_size(container, 400.0, 400.0);

    group.bench_function("get_style_100_views", |b| {
        b.iter(|| {
            for id in &ids {
                let style = harness.get_computed_style(*id);
                black_box(style.get(Background));
            }
        });
    });

    group.finish();
}

/// Benchmark re-styling existing views (the use case where cache helps).
///
/// This simulates what happens when interaction state changes (hover, focus, etc.)
/// and views need to be re-styled. With caching, identical style inputs should
/// hit the cache and avoid redundant computation.
fn bench_restyle_views(c: &mut Criterion) {
    let mut group = c.benchmark_group("restyle_views");

    // Test with 50 views
    let size = 50;

    // Setup: create views once with identical styles
    let views: Vec<_> = (0..size)
        .map(|_| {
            Empty::new().style(|s| {
                s.size(20.0, 20.0)
                    .background(palette::css::CORAL)
                    .padding(2.0)
                    .margin(1.0)
            })
        })
        .collect();

    let ids: Vec<_> = views.iter().map(|v| v.view_id()).collect();
    let container = Stack::from_iter(views).style(|s| s.size(400.0, 400.0));
    let mut harness = HeadlessHarness::new_with_size(container, 400.0, 400.0);

    group.bench_function("request_and_recompute_50", |b| {
        b.iter(|| {
            // Request style recomputation for all views
            for id in &ids {
                id.request_style();
            }
            // Process the style recomputation
            harness.recompute_styles();
            black_box(harness.root_id());
        });
    });

    group.finish();
}

/// Benchmark re-styling views with hover styles (selector resolution).
///
/// This tests the full selector resolution path where views have conditional
/// styles that depend on interaction state.
fn bench_restyle_with_selectors(c: &mut Criterion) {
    let mut group = c.benchmark_group("restyle_with_selectors");

    // Test with 50 views
    let size = 50;

    // Setup: create views with hover styles
    let views: Vec<_> = (0..size)
        .map(|_| {
            Empty::new().style(|s| {
                s.size(20.0, 20.0)
                    .background(palette::css::CORAL)
                    .hover(|s| s.background(palette::css::LIGHT_CORAL))
                    .active(|s| s.background(palette::css::DARK_RED))
            })
        })
        .collect();

    let ids: Vec<_> = views.iter().map(|v| v.view_id()).collect();
    let container = Stack::from_iter(views).style(|s| s.size(400.0, 400.0));
    let mut harness = HeadlessHarness::new_with_size(container, 400.0, 400.0);

    group.bench_function("with_hover_active_50", |b| {
        b.iter(|| {
            // Request style recomputation for all views
            for id in &ids {
                id.request_style();
            }
            // Process the style recomputation
            harness.recompute_styles();
            black_box(harness.root_id());
        });
    });

    group.finish();
}

/// Benchmark inherited prop updates (the use case where graduated propagation helps).
///
/// This tests the scenario where an ancestor sets an inherited prop and descendants
/// use it. When the prop updates, views without selectors can use the inherited-only
/// fast path instead of full selector resolution.
fn bench_inherited_prop_updates(c: &mut Criterion) {
    use floem::peniko::Color;
    use floem::prop;
    use floem::style::Style;

    // Define an inherited prop for this benchmark
    prop!(
        pub BenchInheritedColor: Color { inherited } = palette::css::BLACK
    );

    trait BenchColorExt {
        fn with_bench_color(self, f: impl Fn(Self, &Color) -> Self + 'static) -> Self
        where
            Self: Sized;
    }

    impl BenchColorExt for Style {
        fn with_bench_color(self, f: impl Fn(Self, &Color) -> Self + 'static) -> Self {
            self.with_context::<BenchInheritedColor>(f)
        }
    }

    let mut group = c.benchmark_group("inherited_prop_updates");

    // Benchmark: deep hierarchy with inherited prop
    for depth in [5, 10, 20].iter() {
        group.bench_with_input(
            BenchmarkId::new("deep_hierarchy", depth),
            depth,
            |b, &depth| {
                let color_signal = RwSignal::new(palette::css::RED);

                fn create_deep_with_inherited(depth: usize) -> Container {
                    if depth == 0 {
                        Container::new(Empty::new().style(|s| s.size(10.0, 10.0)))
                            .style(|s| s.size(20.0, 20.0))
                    } else {
                        Container::new(create_deep_with_inherited(depth - 1))
                            .style(|s| s.size_full().padding(1.0))
                    }
                }

                let root = Container::new(create_deep_with_inherited(depth)).style(move |s| {
                    s.size(200.0, 200.0)
                        .set(BenchInheritedColor, color_signal.get())
                });

                let mut harness = HeadlessHarness::new_with_size(root, 200.0, 200.0);

                b.iter(|| {
                    // Toggle the inherited prop
                    color_signal.update(|c| {
                        *c = if *c == palette::css::RED {
                            palette::css::BLUE
                        } else {
                            palette::css::RED
                        };
                    });
                    harness.rebuild();
                    black_box(harness.root_id());
                });
            },
        );
    }

    // Benchmark: wide hierarchy (many siblings) with inherited prop
    for width in [10, 50, 100].iter() {
        group.bench_with_input(
            BenchmarkId::new("wide_hierarchy", width),
            width,
            |b, &width| {
                let color_signal = RwSignal::new(palette::css::RED);

                let children: Vec<_> = (0..width)
                    .map(|_| Empty::new().style(|s| s.size(20.0, 20.0)))
                    .collect();

                let root = Stack::from_iter(children).style(move |s| {
                    s.size(400.0, 400.0)
                        .set(BenchInheritedColor, color_signal.get())
                });

                let mut harness = HeadlessHarness::new_with_size(root, 400.0, 400.0);

                b.iter(|| {
                    color_signal.update(|c| {
                        *c = if *c == palette::css::RED {
                            palette::css::BLUE
                        } else {
                            palette::css::RED
                        };
                    });
                    harness.rebuild();
                    black_box(harness.root_id());
                });
            },
        );
    }

    group.finish();
}

/// Benchmark inherited prop updates for views WITH selectors.
///
/// This tests views that have hover/active selectors which cannot use the
/// inherited-only fast path and must do full style resolution.
fn bench_inherited_with_selectors(c: &mut Criterion) {
    use floem::peniko::Color;
    use floem::prop;
    use floem::style::Style;

    prop!(
        pub BenchInheritedColor2: Color { inherited } = palette::css::BLACK
    );

    trait BenchColorExt2 {
        fn with_bench_color2(self, f: impl Fn(Self, &Color) -> Self + 'static) -> Self
        where
            Self: Sized;
    }

    impl BenchColorExt2 for Style {
        fn with_bench_color2(self, f: impl Fn(Self, &Color) -> Self + 'static) -> Self {
            self.with_context::<BenchInheritedColor2>(f)
        }
    }

    let mut group = c.benchmark_group("inherited_with_selectors");

    for width in [10, 50].iter() {
        group.bench_with_input(
            BenchmarkId::new("with_hover_selectors", width),
            width,
            |b, &width| {
                let color_signal = RwSignal::new(palette::css::RED);

                // Views with hover selector - cannot use inherited-only fast path
                let children: Vec<_> = (0..width)
                    .map(|_| {
                        Empty::new().style(|s| {
                            s.size(20.0, 20.0)
                                .with_bench_color2(|s, c| s.background(*c))
                                .hover(|s| s.border(1.0))
                        })
                    })
                    .collect();

                let root = Stack::from_iter(children).style(move |s| {
                    s.size(400.0, 400.0)
                        .set(BenchInheritedColor2, color_signal.get())
                });

                let mut harness = HeadlessHarness::new_with_size(root, 400.0, 400.0);

                b.iter(|| {
                    color_signal.update(|c| {
                        *c = if *c == palette::css::RED {
                            palette::css::BLUE
                        } else {
                            palette::css::RED
                        };
                    });
                    harness.rebuild();
                    black_box(harness.root_id());
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_identical_styles,
    bench_different_styles,
    bench_deep_nesting,
    bench_complex_styles,
    bench_get_computed_style,
    bench_restyle_views,
    bench_restyle_with_selectors,
    bench_inherited_prop_updates,
    bench_inherited_with_selectors,
);
criterion_main!(benches);
