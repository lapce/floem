use floem::peniko::Brush;
use floem::prelude::*;
use floem::style::{Background, NthChild};
use floem::views::list;
use floem_test::prelude::*;

#[test]
fn test_first_child_selector_applies() {
    let root = TestRoot::new();
    let child1 = Empty::new().style(|s| {
        s.size(20.0, 20.0)
            .background(palette::css::GRAY)
            .first_child(|s| s.background(palette::css::RED))
    });
    let child2 = Empty::new().style(|s| {
        s.size(20.0, 20.0)
            .background(palette::css::GRAY)
            .first_child(|s| s.background(palette::css::RED))
    });
    let child3 = Empty::new().style(|s| {
        s.size(20.0, 20.0)
            .background(palette::css::GRAY)
            .first_child(|s| s.background(palette::css::RED))
    });
    let id1 = child1.view_id();
    let id2 = child2.view_id();
    let id3 = child3.view_id();

    let view = Stack::new((child1, child2, child3));
    let harness = HeadlessHarness::new_with_size(root, view, 100.0, 30.0);

    let s1 = harness.get_computed_style(id1);
    let s2 = harness.get_computed_style(id2);
    let s3 = harness.get_computed_style(id3);
    assert!(matches!(s1.get(Background), Some(Brush::Solid(c)) if c == palette::css::RED));
    assert!(matches!(s2.get(Background), Some(Brush::Solid(c)) if c == palette::css::GRAY));
    assert!(matches!(s3.get(Background), Some(Brush::Solid(c)) if c == palette::css::GRAY));
}

#[test]
fn test_last_child_selector_applies() {
    let root = TestRoot::new();
    let child1 = Empty::new().style(|s| {
        s.size(20.0, 20.0)
            .background(palette::css::GRAY)
            .last_child(|s| s.background(palette::css::BLUE))
    });
    let child2 = Empty::new().style(|s| {
        s.size(20.0, 20.0)
            .background(palette::css::GRAY)
            .last_child(|s| s.background(palette::css::BLUE))
    });
    let child3 = Empty::new().style(|s| {
        s.size(20.0, 20.0)
            .background(palette::css::GRAY)
            .last_child(|s| s.background(palette::css::BLUE))
    });
    let id1 = child1.view_id();
    let id2 = child2.view_id();
    let id3 = child3.view_id();

    let view = Stack::new((child1, child2, child3));
    let harness = HeadlessHarness::new_with_size(root, view, 100.0, 30.0);

    let s1 = harness.get_computed_style(id1);
    let s2 = harness.get_computed_style(id2);
    let s3 = harness.get_computed_style(id3);
    assert!(matches!(s1.get(Background), Some(Brush::Solid(c)) if c == palette::css::GRAY));
    assert!(matches!(s2.get(Background), Some(Brush::Solid(c)) if c == palette::css::GRAY));
    assert!(matches!(s3.get(Background), Some(Brush::Solid(c)) if c == palette::css::BLUE));
}

#[test]
fn test_nth_child_selector_applies() {
    let root = TestRoot::new();
    let child1 = Empty::new().style(|s| {
        s.size(20.0, 20.0)
            .background(palette::css::GRAY)
            .nth_child(NthChild::odd(), |s| s.background(palette::css::GREEN))
    });
    let child2 = Empty::new().style(|s| {
        s.size(20.0, 20.0)
            .background(palette::css::GRAY)
            .nth_child(NthChild::odd(), |s| s.background(palette::css::GREEN))
    });
    let child3 = Empty::new().style(|s| {
        s.size(20.0, 20.0)
            .background(palette::css::GRAY)
            .nth_child(NthChild::odd(), |s| s.background(palette::css::GREEN))
    });
    let id1 = child1.view_id();
    let id2 = child2.view_id();
    let id3 = child3.view_id();

    let view = Stack::new((child1, child2, child3));
    let harness = HeadlessHarness::new_with_size(root, view, 100.0, 30.0);

    let s1 = harness.get_computed_style(id1);
    let s2 = harness.get_computed_style(id2);
    let s3 = harness.get_computed_style(id3);
    assert!(matches!(s1.get(Background), Some(Brush::Solid(c)) if c == palette::css::GREEN));
    assert!(matches!(s2.get(Background), Some(Brush::Solid(c)) if c == palette::css::GRAY));
    assert!(matches!(s3.get(Background), Some(Brush::Solid(c)) if c == palette::css::GREEN));
}

#[test]
fn test_even_selector_applies_in_list_items() {
    let root = TestRoot::new();
    let list_view = list((0..4).map(|_| {
        Empty::new().style(|s| {
            s.size(20.0, 20.0)
                .background(palette::css::GRAY)
                .even(|s| s.background(palette::css::GREEN))
        })
    }));
    let list_id = list_view.view_id();

    let harness = HeadlessHarness::new_with_size(root, list_view, 120.0, 120.0);

    let item_ids = list_id.children();
    assert_eq!(item_ids.len(), 4);

    // list() wraps each item: List -> Item wrapper -> user view (ListItemClass).
    let row_ids: Vec<_> = item_ids
        .iter()
        .map(|item_id| item_id.children()[0])
        .collect();
    let parent_ids: Vec<_> = row_ids
        .iter()
        .map(|row_id| row_id.parent().expect("row should have Item parent"))
        .collect();
    assert_eq!(parent_ids, item_ids);

    let s1 = harness.get_computed_style(row_ids[0]);
    let s2 = harness.get_computed_style(row_ids[1]);
    let s3 = harness.get_computed_style(row_ids[2]);
    let s4 = harness.get_computed_style(row_ids[3]);

    assert!(!matches!(s1.get(Background), Some(Brush::Solid(c)) if c == palette::css::GREEN));
    assert!(matches!(s2.get(Background), Some(Brush::Solid(c)) if c == palette::css::GREEN));
    assert!(!matches!(s3.get(Background), Some(Brush::Solid(c)) if c == palette::css::GREEN));
    assert!(matches!(s4.get(Background), Some(Brush::Solid(c)) if c == palette::css::GREEN));
}
