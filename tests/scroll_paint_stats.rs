use floem::headless::{HeadlessHarness, TestRoot};
use floem::prelude::*;
use floem::views::{Decorators, Label, ScrollExt, Stack};

fn create_scroll_label_list(n: usize) -> impl IntoView {
    let rows: Vec<_> = (0..n)
        .map(|i| {
            Label::new(format!(
                "Row {i:05} | benchmark label content for retained scroll repaint"
            ))
            .style(|s| {
                s.width_full()
                    .height(24.0)
                    .padding_horiz(6.0)
                    .padding_vert(2.0)
            })
            .into_any()
        })
        .collect();

    Stack::from_iter(rows)
        .style(|s| s.flex_col().width_full())
        .scroll()
        .style(|s| s.size(320.0, 240.0))
}

#[test]
fn print_scroll_paint_stats_after_rebuild() {
    let root = TestRoot::new();
    let view = create_scroll_label_list(1_000);
    let mut harness = HeadlessHarness::new_with_size(root, view, 320.0, 240.0);

    harness.rebuild();
    harness.paint();
    println!("initial after rebuild: {:?}", harness.last_paint_stats());

    harness.scroll_down(160.0, 120.0, 48.0);
    harness.paint();
    println!(
        "after first scroll after rebuild: {:?}",
        harness.last_paint_stats()
    );
}
