use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use floem::ViewId;
use floem::headless::{HeadlessHarness, TestRoot};
use floem::prelude::*;
use floem::views::{Decorators, Empty, Label, LabelClass, Stack};

const LABEL_COUNT: usize = 100;
const EMPTY_COUNT: usize = 100;

floem::style_class!(pub BenchEmptyClass);

fn create_themed_labels(count: usize) -> (impl IntoView, Vec<ViewId>) {
    let labels: Vec<_> = (0..count)
        .map(|i| {
            Label::new(format!("Label {i}"))
                .style(move |s| s.padding_horiz(6.0).padding_vert(4.0).margin_bottom(1.0))
        })
        .collect();

    let ids = labels.iter().map(|label| label.view_id()).collect();

    let view = Stack::from_iter(labels).style(|s| {
        s.flex_col()
            .size(640.0, 1_200.0)
            .font_family("Arial")
            .color(palette::css::DARK_SLATE_GRAY)
            .line_height(1.25)
            .class(LabelClass, |s| {
                s.font_size(15.0)
                    .line_height(1.35)
                    .font_bold()
                    .color(palette::css::MIDNIGHT_BLUE)
            })
    });

    (view, ids)
}

fn create_themed_empty_views(count: usize) -> (impl IntoView, Vec<ViewId>) {
    let empties: Vec<_> = (0..count)
        .map(|i| {
            Empty::new().class(BenchEmptyClass).style(move |s| {
                s.size(24.0 + (i % 3) as f32, 18.0 + (i % 5) as f32)
                    .padding(3.0)
                    .border(1.0)
                    .border_color(palette::css::DARK_GRAY)
                    .background(palette::css::LIGHT_BLUE)
            })
        })
        .collect();

    let ids = empties.iter().map(|view| view.view_id()).collect();

    let view = Stack::from_iter(empties).style(|s| {
        s.flex_col()
            .size(640.0, 1_200.0)
            .class(BenchEmptyClass, |s| {
                s.width(36.0)
                    .height(24.0)
                    .padding_horiz(5.0)
                    .padding_vert(2.0)
                    .border(2.0)
                    .border_color(palette::css::MIDNIGHT_BLUE)
                    .background(palette::css::LIGHT_CORAL)
            })
    });

    (view, ids)
}

fn bench_view_creation(c: &mut Criterion) {
    let mut label_group = c.benchmark_group("label_creation");

    label_group.bench_function("build_100_labels_with_local_style_and_theme_classes", |b| {
        b.iter(|| {
            let root = TestRoot::new();
            let (view, ids) = create_themed_labels(LABEL_COUNT);
            black_box(root.id());
            black_box(ids);
            black_box(view);
        });
    });

    label_group.bench_function("mount_100_labels_with_local_style_and_theme_classes", |b| {
        b.iter(|| {
            let root = TestRoot::new();
            let (view, _ids) = create_themed_labels(LABEL_COUNT);
            let harness = HeadlessHarness::new_with_size(root, view, 640.0, 1_200.0);

            black_box(harness.root_id());
        });
    });

    label_group.finish();

    let mut empty_group = c.benchmark_group("empty_creation");

    empty_group.bench_function("build_100_empty_with_local_style_and_theme_class", |b| {
        b.iter(|| {
            let root = TestRoot::new();
            let (view, ids) = create_themed_empty_views(EMPTY_COUNT);
            black_box(root.id());
            black_box(ids);
            black_box(view);
        });
    });

    empty_group.bench_function("mount_100_empty_with_local_style_and_theme_class", |b| {
        b.iter(|| {
            let root = TestRoot::new();
            let (view, _ids) = create_themed_empty_views(EMPTY_COUNT);
            let harness = HeadlessHarness::new_with_size(root, view, 640.0, 1_200.0);

            black_box(harness.root_id());
        });
    });

    empty_group.finish();
}

criterion_group!(benches, bench_view_creation);
criterion_main!(benches);
