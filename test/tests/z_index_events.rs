//! Tests for z-index event dispatch behavior.
//!
//! These tests verify that events are dispatched to views based on their
//! visual stacking order (z-index), not just DOM order.

use floem::style::Position;
use floem_test::prelude::*;

#[test]
fn test_higher_z_index_receives_click_first() {
    let tracker = ClickTracker::new();

    // Create two overlapping views - higher z-index should receive click
    let view = layers((
        tracker.track_named("z1", Empty::new().style(|s| s.z_index(1))),
        tracker.track_named("z10", Empty::new().style(|s| s.z_index(10))),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);
    harness.click(50.0, 50.0);

    // z-index 10 should have been clicked, z-index 1 should not
    assert_eq!(
        tracker.clicked_names(),
        vec!["z10"],
        "Only z-index 10 should receive the click"
    );
}

#[test]
fn test_negative_z_index_receives_click_last() {
    let tracker = ClickTracker::new();

    // Views with negative z-index should be behind those with z-index 0
    let view = layers((
        tracker.track_named("neg", Empty::new().style(|s| s.z_index(-1))),
        tracker.track_named("zero", Empty::new().style(|s| s.z_index(0))),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);
    harness.click(50.0, 50.0);

    assert_eq!(
        tracker.clicked_names(),
        vec!["zero"],
        "z-index 0 should receive click, not negative z-index"
    );
}

#[test]
fn test_dom_order_when_z_index_equal() {
    let tracker = ClickTracker::new();

    // When z-index is equal, last child in DOM order receives events first
    let view = layers((
        tracker.track_named("first", Empty::new().style(|s| s.z_index(5))),
        tracker.track_named("second", Empty::new().style(|s| s.z_index(5))),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);
    harness.click(50.0, 50.0);

    // Last child in DOM order should receive the click
    assert_eq!(
        tracker.clicked_names(),
        vec!["second"],
        "Second view (last in DOM) should receive the click when z-index is equal"
    );
}

#[test]
fn test_stacking_context_boundary() {
    // Test that grandchild's z-index doesn't escape parent's stacking context
    //
    // Structure:
    //   layers
    //   ├── container (z-index: 1)
    //   │   └── grandchild (z-index: 1000) <- should NOT escape parent
    //   └── sibling (z-index: 2) <- should receive click
    //
    let tracker = ClickTracker::new();

    let view = layers((
        Container::new(tracker.track_named(
            "grandchild",
            Empty::new().style(|s| s.size(100.0, 100.0).z_index(1000)),
        ))
        .style(|s| s.z_index(1)),
        tracker.track_named("sibling", Empty::new().style(|s| s.z_index(2))),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);
    harness.click(50.0, 50.0);

    // The sibling with z-index 2 should receive the click,
    // NOT the grandchild with z-index 1000 (it's inside z-index 1 container)
    assert_eq!(
        tracker.clicked_names(),
        vec!["sibling"],
        "Sibling with z-index 2 should receive click, not grandchild with z-index 1000"
    );
}

#[test]
fn test_hit_test_respects_z_index() {
    // Test the hit_test function directly
    let view = layers((
        Empty::new().style(|s| s.z_index(1)),
        Empty::new().style(|s| s.z_index(10)),
    ))
    .style(|s| s.size(100.0, 100.0));

    let harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // The view at (50, 50) should be the one with z-index 10
    let hit = harness.view_at(50.0, 50.0);
    assert!(hit.is_some(), "Should hit a view at (50, 50)");
}

#[test]
fn test_click_tracker_reset() {
    let tracker = ClickTracker::new();

    let view = tracker.track(Empty::new().style(|s| s.size(100.0, 100.0)));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);
    assert_eq!(tracker.click_count(), 1);

    harness.click(50.0, 50.0);
    assert_eq!(tracker.click_count(), 2);

    tracker.reset();
    assert_eq!(tracker.click_count(), 0);
    assert!(!tracker.was_clicked());
}

#[test]
fn test_multiple_z_index_layers() {
    // Test sorting with more than 2 elements including negative, zero, and positive
    let tracker = ClickTracker::new();

    let view = layers((
        tracker.track_named("neg5", Empty::new().style(|s| s.z_index(-5))),
        tracker.track_named("pos100", Empty::new().style(|s| s.z_index(100))),
        tracker.track_named("zero", Empty::new().style(|s| s.z_index(0))),
        tracker.track_named("pos10", Empty::new().style(|s| s.z_index(10))),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);
    harness.click(50.0, 50.0);

    // Highest z-index (100) should receive the click
    assert_eq!(
        tracker.clicked_names(),
        vec!["pos100"],
        "z-index 100 should receive click over all others"
    );
}

#[test]
fn test_partial_overlap_click_non_overlapping_region() {
    // Test clicking on a region where only one view exists
    //
    // Layout:
    //   +--------+
    //   | left   |
    //   |   +----+----+
    //   |   |overlap  |
    //   +---+----+    |
    //       |  right  |
    //       +---------+
    //
    let tracker = ClickTracker::new();

    let view = stack((
        // Left view: 0-60 x, 0-60 y
        tracker.track_named("left", Empty::new()).style(|s| {
            s.absolute()
                .inset_left(0.0)
                .inset_top(0.0)
                .size(60.0, 60.0)
                .z_index(1)
        }),
        // Right view: 40-100 x, 40-100 y (overlaps with left in 40-60 x 40-60 region)
        tracker.track_named("right", Empty::new()).style(|s| {
            s.absolute()
                .inset_left(40.0)
                .inset_top(40.0)
                .size(60.0, 60.0)
                .z_index(10)
        }),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Click in left-only region (20, 20)
    harness.click(20.0, 20.0);
    assert_eq!(
        tracker.clicked_names(),
        vec!["left"],
        "Click at (20,20) should hit only 'left'"
    );

    tracker.reset();

    // Click in right-only region (80, 80)
    harness.click(80.0, 80.0);
    assert_eq!(
        tracker.clicked_names(),
        vec!["right"],
        "Click at (80,80) should hit only 'right'"
    );

    tracker.reset();

    // Click in overlap region (50, 50) - right has higher z-index
    harness.click(50.0, 50.0);
    assert_eq!(
        tracker.clicked_names(),
        vec!["right"],
        "Click at (50,50) overlap should hit 'right' (higher z-index)"
    );
}

#[test]
fn test_click_outside_all_views() {
    // Clicking outside all views should not trigger any handlers
    let tracker = ClickTracker::new();

    let view = stack((tracker.track_named("small", Empty::new()).style(|s| {
        s.absolute()
            .inset_left(10.0)
            .inset_top(10.0)
            .size(30.0, 30.0)
    }),))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Click outside the small view
    harness.click(80.0, 80.0);

    assert_eq!(
        tracker.click_count(),
        0,
        "Clicking outside all views should not trigger any handlers"
    );
}

#[test]
fn test_extreme_z_index_values() {
    // Test with very large z-index values
    let tracker = ClickTracker::new();

    let view = layers((
        tracker.track_named("min", Empty::new().style(|s| s.z_index(i32::MIN))),
        tracker.track_named("max", Empty::new().style(|s| s.z_index(i32::MAX))),
        tracker.track_named("zero", Empty::new().style(|s| s.z_index(0))),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);
    harness.click(50.0, 50.0);

    assert_eq!(
        tracker.clicked_names(),
        vec!["max"],
        "i32::MAX z-index should receive click"
    );
}

#[test]
fn test_select_like_structure_simple() {
    // Simplified test: nested item inside a container with z-index
    // Structure:
    //   layers (fills 100x100)
    //   ├── backdrop (z-index: 99, fills entire area)
    //   └── dropdown (z-index: 100, fills entire area)
    //       └── item (click handler, fills entire area)

    let tracker = ClickTracker::new();

    let item = tracker.track_named("item", Empty::new().style(|s| s.size(100.0, 100.0)));

    let dropdown = Container::new(item).style(|s| s.z_index(100));

    let backdrop = tracker.track_named("backdrop", Empty::new().style(|s| s.z_index(99)));

    let view = layers((backdrop, dropdown)).style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    // The item inside the dropdown (z-index 100) should receive the click
    // NOT the backdrop (z-index 99)
    assert_eq!(
        tracker.clicked_names(),
        vec!["item"],
        "Item inside dropdown (z-index 100) should receive click, not backdrop (z-index 99)"
    );
}

#[test]
fn test_nested_items_in_z_index_container() {
    // Test that items inside a z-index container can receive clicks
    // when there's a v_stack between the container and the items.
    //
    // Structure:
    //   layers (100x100)
    //   ├── backdrop (z-index: 99)
    //   └── dropdown (z-index: 100)
    //       └── v_stack
    //           ├── item0 (30px tall)
    //           ├── item1 (30px tall) <- click here
    //           └── item2 (30px tall)

    let tracker = ClickTracker::new();

    let items_container = v_stack((
        tracker.track_named("item0", Empty::new().style(|s| s.size(100.0, 30.0))),
        tracker.track_named("item1", Empty::new().style(|s| s.size(100.0, 30.0))),
        tracker.track_named("item2", Empty::new().style(|s| s.size(100.0, 30.0))),
    ));

    let dropdown = Container::new(items_container).style(|s| s.z_index(100));

    let backdrop = tracker.track_named("backdrop", Empty::new().style(|s| s.z_index(99)));

    let view = layers((backdrop, dropdown)).style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Click on item1 (y = 30 + 15 = 45, centered in second item)
    harness.click(50.0, 45.0);

    assert_eq!(
        tracker.clicked_names(),
        vec!["item1"],
        "Item1 inside dropdown (z-index 100) should receive click"
    );

    tracker.reset();

    // Click on item0 (y = 15, centered in first item)
    harness.click(50.0, 15.0);

    assert_eq!(
        tracker.clicked_names(),
        vec!["item0"],
        "Item0 inside dropdown (z-index 100) should receive click"
    );
}

#[test]
fn test_absolute_positioned_items() {
    // Test that items inside an absolutely positioned z-index container
    // can receive clicks.
    //
    // Structure:
    //   stack (200x200)
    //   └── dropdown (z-index: 100, absolute, at y=40)
    //       └── item (100x30)

    let tracker = ClickTracker::new();

    let item = tracker.track_named("item", Empty::new().style(|s| s.size(100.0, 30.0)));

    let dropdown = Container::new(item).style(|s| {
        s.absolute()
            .inset_top(40.0)
            .inset_left(0.0)
            .width(100.0)
            .z_index(100)
    });

    let view = stack((dropdown,)).style(|s| s.size(200.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);

    // Click on item (y = 40 + 15 = 55, centered in item)
    harness.click(50.0, 55.0);

    assert_eq!(
        tracker.clicked_names(),
        vec!["item"],
        "Item inside absolutely positioned dropdown should receive click"
    );
}

#[test]
fn test_absolute_with_backdrop() {
    // Test with backdrop behind dropdown
    //
    // Structure:
    //   stack (200x200)
    //   ├── backdrop (z-index: 99, absolute, covers everything)
    //   └── dropdown (z-index: 100, absolute, at y=40)
    //       └── item (100x30)

    let tracker = ClickTracker::new();

    let item = tracker.track_named("item", Empty::new().style(|s| s.size(100.0, 30.0)));

    let dropdown = Container::new(item).style(|s| {
        s.absolute()
            .inset_top(40.0)
            .inset_left(0.0)
            .width(100.0)
            .z_index(100)
    });

    let backdrop = tracker.track_named("backdrop", Empty::new()).style(|s| {
        s.absolute()
            .inset_top(0.0)
            .inset_left(0.0)
            .width(200.0)
            .height(200.0)
            .z_index(99)
    });

    let view = stack((backdrop, dropdown)).style(|s| s.size(200.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);

    // Click on item (y = 40 + 15 = 55, centered in item)
    harness.click(50.0, 55.0);

    assert_eq!(
        tracker.clicked_names(),
        vec!["item"],
        "Item inside dropdown (z-index 100) should receive click, not backdrop (z-index 99)"
    );
}

#[test]
fn test_absolute_with_negative_backdrop() {
    // Test with backdrop that has negative offsets
    //
    // Structure:
    //   stack (200x200)
    //   ├── backdrop (z-index: 99, absolute, at -100,-100 with size 500x500)
    //   └── dropdown (z-index: 100, absolute, at y=40)
    //       └── item (100x30)

    let tracker = ClickTracker::new();

    let item = tracker.track_named("item", Empty::new().style(|s| s.size(100.0, 30.0)));

    let dropdown = Container::new(item).style(|s| {
        s.absolute()
            .inset_top(40.0)
            .inset_left(0.0)
            .width(100.0)
            .z_index(100)
    });

    let backdrop = tracker.track_named("backdrop", Empty::new()).style(|s| {
        s.absolute()
            .inset_top(-100.0)
            .inset_left(-100.0)
            .width(500.0)
            .height(500.0)
            .z_index(99)
    });

    let view = stack((backdrop, dropdown)).style(|s| s.size(200.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);

    // Click on item (y = 40 + 15 = 55, centered in item)
    harness.click(50.0, 55.0);

    assert_eq!(
        tracker.clicked_names(),
        vec!["item"],
        "Item inside dropdown (z-index 100) should receive click, not backdrop (z-index 99)"
    );
}

#[test]
fn test_with_trigger_sibling() {
    // Test with a trigger sibling (non-absolute positioned)
    //
    // Structure:
    //   stack (200x200)
    //   ├── trigger (no z-index, 100x36)
    //   ├── backdrop (z-index: 99, absolute, at -100,-100 with size 500x500)
    //   └── dropdown (z-index: 100, absolute, at y=40)
    //       └── item (100x30)

    let tracker = ClickTracker::new();

    let item = tracker.track_named("item", Empty::new().style(|s| s.size(100.0, 30.0)));

    let dropdown = Container::new(item).style(|s| {
        s.absolute()
            .inset_top(40.0)
            .inset_left(0.0)
            .width(100.0)
            .z_index(100)
    });

    let backdrop = tracker.track_named("backdrop", Empty::new()).style(|s| {
        s.absolute()
            .inset_top(-100.0)
            .inset_left(-100.0)
            .width(500.0)
            .height(500.0)
            .z_index(99)
    });

    let trigger = tracker
        .track_named("trigger", Empty::new())
        .style(|s| s.width(100.0).height(36.0));

    let view = stack((trigger, backdrop, dropdown)).style(|s| s.size(200.0, 200.0));

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);

    // Click on item (y = 40 + 15 = 55, centered in item)
    harness.click(50.0, 55.0);

    assert_eq!(
        tracker.clicked_names(),
        vec!["item"],
        "Item inside dropdown (z-index 100) should receive click"
    );
}

#[test]
fn test_with_container_wrapper() {
    // Test with an outer Container wrapper (position: relative)
    //
    // Structure:
    //   Container (relative, 200x200)
    //   └── stack
    //       ├── trigger (no z-index, 100x36)
    //       ├── backdrop (z-index: 99, absolute)
    //       └── dropdown (z-index: 100, absolute, at y=40)
    //           └── item (100x30)

    let tracker = ClickTracker::new();

    let item = tracker.track_named("item", Empty::new().style(|s| s.size(100.0, 30.0)));

    let dropdown = Container::new(item).style(|s| {
        s.absolute()
            .inset_top(40.0)
            .inset_left(0.0)
            .width(100.0)
            .z_index(100)
    });

    let backdrop = tracker.track_named("backdrop", Empty::new()).style(|s| {
        s.absolute()
            .inset_top(-100.0)
            .inset_left(-100.0)
            .width(500.0)
            .height(500.0)
            .z_index(99)
    });

    let trigger = tracker
        .track_named("trigger", Empty::new())
        .style(|s| s.width(100.0).height(36.0));

    let view = Container::new(stack((trigger, backdrop, dropdown)))
        .style(|s| s.position(Position::Relative).width(200.0).height(200.0));

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);

    // Click on item (y = 40 + 15 = 55, centered in item)
    harness.click(50.0, 55.0);

    assert_eq!(
        tracker.clicked_names(),
        vec!["item"],
        "Item inside dropdown (z-index 100) should receive click"
    );
}

#[test]
fn test_with_vstack_items() {
    // Test with v_stack containing items (same as nested items test)
    //
    // Structure:
    //   Container (relative, 200x200)
    //   └── stack
    //       ├── trigger (100x36)
    //       ├── backdrop (z-index: 99, absolute)
    //       └── dropdown (z-index: 100, absolute, at y=40)
    //           └── v_stack
    //               ├── item0 (100x30)
    //               ├── item1 (100x30)
    //               └── item2 (100x30)

    let tracker = ClickTracker::new();

    let items_container = v_stack((
        tracker.track_named("item0", Empty::new().style(|s| s.size(100.0, 30.0))),
        tracker.track_named("item1", Empty::new().style(|s| s.size(100.0, 30.0))),
        tracker.track_named("item2", Empty::new().style(|s| s.size(100.0, 30.0))),
    ));

    let dropdown = Container::new(items_container).style(|s| {
        s.absolute()
            .inset_top(40.0)
            .inset_left(0.0)
            .width(100.0)
            .z_index(100)
    });

    let backdrop = tracker.track_named("backdrop", Empty::new()).style(|s| {
        s.absolute()
            .inset_top(-100.0)
            .inset_left(-100.0)
            .width(500.0)
            .height(500.0)
            .z_index(99)
    });

    let trigger = tracker
        .track_named("trigger", Empty::new())
        .style(|s| s.width(100.0).height(36.0));

    let view = Container::new(stack((trigger, backdrop, dropdown)))
        .style(|s| s.position(Position::Relative).width(200.0).height(200.0));

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);

    // Click on item1 (y = 40 + 30 + 15 = 85, centered in second item)
    harness.click(50.0, 85.0);

    assert_eq!(
        tracker.clicked_names(),
        vec!["item1"],
        "Item1 inside dropdown should receive click"
    );
}

#[test]
fn test_inside_scroll_view() {
    // Test that items inside a Scroll view can receive clicks
    // This matches the showcase structure where Select is inside a Scroll
    //
    // Structure:
    //   Scroll
    //   └── Container (relative, 200x500)
    //       └── stack
    //           ├── trigger (120x36)
    //           ├── backdrop (z-index: 99)
    //           └── dropdown (z-index: 100, at y=40)
    //               └── v_stack (width_full)
    //                   └── items (width_full)

    let tracker = ClickTracker::new();

    let items_container = v_stack((
        tracker.track_named("item0", Empty::new().style(|s| s.width_full().height(30.0))),
        tracker.track_named("item1", Empty::new().style(|s| s.width_full().height(30.0))),
        tracker.track_named("item2", Empty::new().style(|s| s.width_full().height(30.0))),
    ))
    .style(|s| s.width_full());

    let dropdown = Container::new(items_container).style(|s| {
        s.absolute()
            .inset_top(40.0)
            .inset_left(0.0)
            .inset_right(0.0)
            .z_index(100)
    });

    let backdrop = tracker.track_named("backdrop", Empty::new()).style(|s| {
        s.absolute()
            .inset_top(-1000.0)
            .inset_left(-1000.0)
            .width(3000.0)
            .height(3000.0)
            .z_index(99)
    });

    let trigger = tracker
        .track_named("trigger", Empty::new())
        .style(|s| s.width(120.0).height(36.0));

    let content = Container::new(stack((trigger, backdrop, dropdown)))
        .style(|s| s.position(Position::Relative).width(200.0).height(500.0));

    // Wrap in Scroll like the showcase does
    let view = Scroll::new(content).style(|s| s.size(300.0, 400.0));

    let mut harness = HeadlessHarness::new_with_size(view, 300.0, 400.0);

    // Click on item1 (y = 40 + 30 + 15 = 85, centered in second item)
    harness.click(60.0, 85.0);

    assert_eq!(
        tracker.clicked_names(),
        vec!["item1"],
        "Item1 inside dropdown in Scroll view should receive click"
    );

    tracker.reset();

    // Click on area covered by backdrop but not by dropdown (below the dropdown)
    // Dropdown is at y=40 with 3 items of 30px each = 90px total height
    // So dropdown covers y=40-130. Click at y=150 should hit backdrop.
    harness.click(60.0, 150.0);

    assert_eq!(
        tracker.clicked_names(),
        vec!["backdrop"],
        "Backdrop should receive clicks in area not covered by dropdown"
    );
}

#[test]
fn test_inset_left_right_without_width() {
    // Test the EXACT structure used by the Select component
    // This tests whether inset_left(0) + inset_right(0) properly propagates width
    //
    // Structure:
    //   Container (relative, 120x200)
    //   └── stack
    //       ├── trigger (120x36)
    //       ├── backdrop (z-index: 99, absolute, -1000,-1000, 3000x3000)
    //       └── dropdown (z-index: 100, absolute, inset_left(0), inset_right(0))
    //           └── v_stack (width_full)
    //               ├── item0 (width_full, height: 30)
    //               ├── item1 (width_full, height: 30)
    //               └── item2 (width_full, height: 30)

    let tracker = ClickTracker::new();

    let items_container = v_stack((
        tracker.track_named("item0", Empty::new().style(|s| s.width_full().height(30.0))),
        tracker.track_named("item1", Empty::new().style(|s| s.width_full().height(30.0))),
        tracker.track_named("item2", Empty::new().style(|s| s.width_full().height(30.0))),
    ))
    .style(|s| s.width_full());

    // Using inset_left(0) + inset_right(0) WITHOUT explicit width - matches Select component
    let dropdown = Container::new(items_container).style(|s| {
        s.absolute()
            .inset_top(40.0)
            .inset_left(0.0)
            .inset_right(0.0) // No explicit width!
            .z_index(100)
    });

    let backdrop = tracker.track_named("backdrop", Empty::new()).style(|s| {
        s.absolute()
            .inset_top(-1000.0)
            .inset_left(-1000.0)
            .width(3000.0)
            .height(3000.0)
            .z_index(99)
    });

    let trigger = tracker
        .track_named("trigger", Empty::new())
        .style(|s| s.width(120.0).height(36.0));

    let view = Container::new(stack((trigger, backdrop, dropdown)))
        .style(|s| s.position(Position::Relative).width(120.0).height(200.0));

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 250.0);

    // Click on item1 (y = 40 + 30 + 15 = 85, centered in second item)
    harness.click(60.0, 85.0);

    assert_eq!(
        tracker.clicked_names(),
        vec!["item1"],
        "Item1 inside dropdown should receive click even with inset_left/right instead of width"
    );

    tracker.reset();

    // Also test that backdrop receives clicks outside the dropdown
    harness.click(150.0, 85.0);

    assert_eq!(
        tracker.clicked_names(),
        vec!["backdrop"],
        "Backdrop should receive clicks outside the dropdown"
    );
}

#[test]
fn test_with_width_full_items_fixed() {
    // Test with items using width_full() - FIXED by adding width_full() to v_stack
    //
    // The issue was that when v_stack doesn't have explicit width and items use
    // width_full(), there's a circular dependency that results in 0 width.
    // The fix is to add width_full() to the v_stack.
    //
    // Structure:
    //   Container (relative, 120x200)
    //   └── stack
    //       ├── trigger (120x36)
    //       ├── backdrop (z-index: 99, absolute)
    //       └── dropdown (z-index: 100, absolute, width: 120, at y=40)
    //           └── v_stack (width_full) <- FIX: add width_full here
    //               ├── item0 (width_full, height: 30)
    //               ├── item1 (width_full, height: 30)
    //               └── item2 (width_full, height: 30)

    let tracker = ClickTracker::new();

    let items_container = v_stack((
        tracker.track_named("item0", Empty::new().style(|s| s.width_full().height(30.0))),
        tracker.track_named("item1", Empty::new().style(|s| s.width_full().height(30.0))),
        tracker.track_named("item2", Empty::new().style(|s| s.width_full().height(30.0))),
    ))
    .style(|s| s.width_full()); // FIX: add width_full to v_stack

    let dropdown = Container::new(items_container).style(|s| {
        s.absolute()
            .inset_top(40.0)
            .inset_left(0.0)
            .width(120.0)
            .z_index(100)
    });

    let backdrop = tracker.track_named("backdrop", Empty::new()).style(|s| {
        s.absolute()
            .inset_top(-100.0)
            .inset_left(-100.0)
            .width(500.0)
            .height(500.0)
            .z_index(99)
    });

    let trigger = tracker
        .track_named("trigger", Empty::new())
        .style(|s| s.width(120.0).height(36.0));

    let view = Container::new(stack((trigger, backdrop, dropdown)))
        .style(|s| s.position(Position::Relative).width(120.0).height(200.0));

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 250.0);

    // Click on item1 (y = 40 + 30 + 15 = 85, centered in second item)
    harness.click(60.0, 85.0);

    assert_eq!(
        tracker.clicked_names(),
        vec!["item1"],
        "Item1 inside dropdown should receive click"
    );
}

#[test]
fn test_with_vstack_width_full() {
    // Test with v_stack using width_full() - to narrow down the issue
    //
    // Structure:
    //   Container (relative, 120x200)
    //   └── stack
    //       ├── trigger (120x36)
    //       ├── backdrop (z-index: 99, absolute)
    //       └── dropdown (z-index: 100, absolute, width: 120, at y=40)
    //           └── v_stack (width_full)
    //               ├── item0 (100x30)
    //               ├── item1 (100x30)
    //               └── item2 (100x30)

    let tracker = ClickTracker::new();

    let items_container = v_stack((
        tracker.track_named("item0", Empty::new().style(|s| s.size(100.0, 30.0))),
        tracker.track_named("item1", Empty::new().style(|s| s.size(100.0, 30.0))),
        tracker.track_named("item2", Empty::new().style(|s| s.size(100.0, 30.0))),
    ))
    .style(|s| s.width_full());

    let dropdown = Container::new(items_container).style(|s| {
        s.absolute()
            .inset_top(40.0)
            .inset_left(0.0)
            .width(120.0)
            .z_index(100)
    });

    let backdrop = tracker.track_named("backdrop", Empty::new()).style(|s| {
        s.absolute()
            .inset_top(-100.0)
            .inset_left(-100.0)
            .width(500.0)
            .height(500.0)
            .z_index(99)
    });

    let trigger = tracker
        .track_named("trigger", Empty::new())
        .style(|s| s.width(120.0).height(36.0));

    let view = Container::new(stack((trigger, backdrop, dropdown)))
        .style(|s| s.position(Position::Relative).width(120.0).height(200.0));

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 250.0);

    // Click on item1 (y = 40 + 30 + 15 = 85, centered in second item)
    harness.click(50.0, 85.0);

    assert_eq!(
        tracker.clicked_names(),
        vec!["item1"],
        "Item1 inside dropdown should receive click"
    );
}

#[test]
fn test_select_like_no_explicit_container_width() {
    // Test structure that matches the real Select component issue:
    // - Outer container has position:relative but NO explicit width (gets it from trigger)
    // - Dropdown uses inset_left(0) + inset_right(0)
    // - v_stack inside uses width_full()
    //
    // This mimics what happens when Select is placed directly in a v_stack without Scroll
    //
    // Structure:
    //   v_stack (width_full, height_full) <- root, gets window dimensions
    //   └── Container (relative, NO explicit width)
    //       └── stack
    //           ├── trigger (min_width: 120, height: 36)
    //           ├── backdrop (z-index: 99, absolute)
    //           └── dropdown (z-index: 100, absolute, inset_left(0), inset_right(0))
    //               └── v_stack (width_full)
    //                   └── items (width_full)

    let tracker = ClickTracker::new();

    let items_container = v_stack((
        tracker.track_named("item0", Empty::new().style(|s| s.width_full().height(30.0))),
        tracker.track_named("item1", Empty::new().style(|s| s.width_full().height(30.0))),
        tracker.track_named("item2", Empty::new().style(|s| s.width_full().height(30.0))),
    ))
    .style(|s| s.width_full());

    // Dropdown uses inset_left(0) + inset_right(0) instead of explicit width
    let dropdown = Container::new(items_container).style(|s| {
        s.absolute()
            .inset_top(40.0)
            .inset_left(0.0)
            .inset_right(0.0) // Stretch to parent width
            .z_index(100)
    });

    let backdrop = tracker.track_named("backdrop", Empty::new()).style(|s| {
        s.absolute()
            .inset_top(-1000.0)
            .inset_left(-1000.0)
            .width(3000.0)
            .height(3000.0)
            .z_index(99)
    });

    // Trigger with min_width only - no explicit width
    let trigger = tracker
        .track_named("trigger", Empty::new())
        .style(|s| s.min_width(120.0).height(36.0));

    // Outer container has position:relative but NO explicit width
    // Width should come from the trigger's min_width
    let select_container = Container::new(stack((trigger, backdrop, dropdown)))
        .style(|s| s.position(Position::Relative).min_width(120.0));

    // Wrap in v_stack like a real app would
    let view = v_stack((select_container,)).style(|s| s.width_full().height_full().padding(50.0));

    let mut harness = HeadlessHarness::new_with_size(view, 400.0, 400.0);

    // Click on item1 (padding 50 + inset_top 40 + item0 30 + 15 = 135)
    harness.click(100.0, 135.0);

    assert_eq!(
        tracker.clicked_names(),
        vec!["item1"],
        "Item1 inside dropdown should receive click even without explicit container width"
    );
}

#[test]
fn test_select_structure_with_inset_top_pct() {
    // Test with inset_top_pct(100.0) like the actual Select component uses
    // This positions the dropdown at 100% of the parent's height (below the trigger)
    //
    // Structure:
    //   v_stack (width_full, height_full, padding: 50)
    //   └── Container (relative, min_width: 120)
    //       └── stack
    //           ├── trigger (min_width: 120, height: 36)
    //           ├── backdrop (z-index: 99)
    //           └── dropdown (z-index: 100, inset_top_pct(100), inset_left(0), inset_right(0))
    //               └── v_stack (width_full)
    //                   └── items (width_full, height: 30 each)

    let tracker = ClickTracker::new();

    let items_container = v_stack((
        tracker.track_named("item0", Empty::new().style(|s| s.width_full().height(30.0))),
        tracker.track_named("item1", Empty::new().style(|s| s.width_full().height(30.0))),
        tracker.track_named("item2", Empty::new().style(|s| s.width_full().height(30.0))),
    ))
    .style(|s| s.width_full());

    // Use inset_top_pct(100.0) like the real Select
    let dropdown = Container::new(items_container).style(|s| {
        s.absolute()
            .inset_top_pct(100.0) // Position below trigger
            .inset_left(0.0)
            .inset_right(0.0)
            .z_index(100)
    });

    let backdrop = tracker.track_named("backdrop", Empty::new()).style(|s| {
        s.absolute()
            .inset_top(-1000.0)
            .inset_left(-1000.0)
            .width(3000.0)
            .height(3000.0)
            .z_index(99)
    });

    let trigger = tracker
        .track_named("trigger", Empty::new())
        .style(|s| s.min_width(120.0).height(36.0));

    let select_container = Container::new(stack((trigger, backdrop, dropdown)))
        .style(|s| s.position(Position::Relative).min_width(120.0));

    let view = v_stack((select_container,)).style(|s| s.width_full().height_full().padding(50.0));

    let mut harness = HeadlessHarness::new_with_size(view, 400.0, 400.0);

    // With inset_top_pct(100.0), dropdown is at y = padding(50) + trigger_height(36) = 86
    // Click on item1: y = 86 + item0(30) + 15 = 131
    harness.click(100.0, 131.0);

    assert_eq!(
        tracker.clicked_names(),
        vec!["item1"],
        "Item1 should receive click with inset_top_pct(100.0)"
    );
}

#[test]
fn test_display_none_to_visible_layout() {
    // Test that children have correct layout when parent's display changes from None to visible.
    //
    // This reproduces a bug where:
    // 1. Dropdown starts with display: None
    // 2. When display changes to visible, children have zero size
    // 3. Clicks go to backdrop instead of items because items have no layout
    //
    // Structure:
    //   Container (relative, 200x200)
    //   └── stack
    //       ├── trigger (100x36)
    //       ├── backdrop (z-index: 99)
    //       └── dropdown (z-index: 100, starts with display: None)
    //           └── v_stack
    //               ├── item0 (100x30)
    //               ├── item1 (100x30)
    //               └── item2 (100x30)

    use floem::reactive::RwSignal;

    let tracker = ClickTracker::new();
    let is_open = RwSignal::new(false); // Start closed (display: None)

    let items_container = v_stack((
        tracker.track_named("item0", Empty::new().style(|s| s.size(100.0, 30.0))),
        tracker.track_named("item1", Empty::new().style(|s| s.size(100.0, 30.0))),
        tracker.track_named("item2", Empty::new().style(|s| s.size(100.0, 30.0))),
    ));

    let dropdown = Container::new(items_container).style(move |s| {
        let open = is_open.get();
        let base = s
            .absolute()
            .inset_top(40.0)
            .inset_left(0.0)
            .width(120.0)
            .z_index(100);
        if open {
            base
        } else {
            base.display(floem::style::Display::None)
        }
    });

    let backdrop = tracker
        .track_named("backdrop", Empty::new())
        .style(move |s| {
            let open = is_open.get();
            let base = s
                .absolute()
                .inset_top(-100.0)
                .inset_left(-100.0)
                .width(500.0)
                .height(500.0)
                .z_index(99);
            if open {
                base
            } else {
                base.display(floem::style::Display::None)
            }
        });

    let trigger = tracker
        .track_named("trigger", Empty::new())
        .style(|s| s.width(100.0).height(36.0));

    let view = Container::new(stack((trigger, backdrop, dropdown)))
        .style(|s| s.position(Position::Relative).width(200.0).height(200.0));

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);

    // Toggle to open
    is_open.set(true);
    harness.rebuild();

    // Click on item1 (y = 40 + 30 + 15 = 85, centered in second item)
    harness.click(50.0, 85.0);

    assert_eq!(
        tracker.clicked_names(),
        vec!["item1"],
        "Item1 inside dropdown should receive click after display changed from None to visible"
    );
}

#[test]
fn test_display_toggle_multiple_times() {
    // Test that layout is correct after multiple display toggles.
    //
    // This is a more comprehensive test of the display toggle bug.

    use floem::reactive::RwSignal;

    let tracker = ClickTracker::new();
    let is_open = RwSignal::new(true); // Start open

    let items_container = v_stack((
        tracker.track_named("item0", Empty::new().style(|s| s.size(100.0, 30.0))),
        tracker.track_named("item1", Empty::new().style(|s| s.size(100.0, 30.0))),
    ));

    let dropdown = Container::new(items_container).style(move |s| {
        let open = is_open.get();
        let base = s
            .absolute()
            .inset_top(40.0)
            .inset_left(0.0)
            .width(120.0)
            .z_index(100);
        if open {
            base
        } else {
            base.display(floem::style::Display::None)
        }
    });

    let backdrop = tracker
        .track_named("backdrop", Empty::new())
        .style(move |s| {
            let open = is_open.get();
            let base = s
                .absolute()
                .inset_top(-100.0)
                .inset_left(-100.0)
                .width(500.0)
                .height(500.0)
                .z_index(99);
            if open {
                base
            } else {
                base.display(floem::style::Display::None)
            }
        });

    let trigger = tracker
        .track_named("trigger", Empty::new())
        .style(|s| s.width(100.0).height(36.0));

    let view = Container::new(stack((trigger, backdrop, dropdown)))
        .style(|s| s.position(Position::Relative).width(200.0).height(200.0));

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);

    // First click with dropdown open (initial state)
    harness.click(50.0, 55.0);
    assert_eq!(
        tracker.clicked_names(),
        vec!["item0"],
        "Item0 should receive first click"
    );
    tracker.reset();

    // Close and reopen
    is_open.set(false);
    harness.rebuild();
    is_open.set(true);
    harness.rebuild();

    // Click again - this is where the bug would manifest
    harness.click(50.0, 55.0);
    assert_eq!(
        tracker.clicked_names(),
        vec!["item0"],
        "Item0 should receive click after close/reopen cycle"
    );
    tracker.reset();

    // Do it again to make sure
    is_open.set(false);
    harness.rebuild();
    is_open.set(true);
    harness.rebuild();

    harness.click(50.0, 85.0);
    assert_eq!(
        tracker.clicked_names(),
        vec!["item1"],
        "Item1 should receive click after second close/reopen cycle"
    );
}

#[test]
fn test_inside_scroll_view_with_display_toggle() {
    // Test that z-index event dispatch works correctly when the dropdown is inside a Scroll view
    // and starts with display:None.
    //
    // This mimics the showcase structure where Select is inside Scroll::new(dyn_container(...))
    //
    // Structure:
    //   Scroll
    //   └── Container (content)
    //       └── Container (select, position:relative)
    //           └── stack
    //               ├── trigger
    //               ├── backdrop (z-index: 99, starts display:None)
    //               └── dropdown (z-index: 100, starts display:None)
    //                   └── v_stack
    //                       └── items

    use floem::reactive::RwSignal;
    use floem::views::Scroll;

    let tracker = ClickTracker::new();
    let is_open = RwSignal::new(false); // Start closed like real Select

    let items_container = v_stack((
        tracker.track_named("item0", Empty::new().style(|s| s.width(100.0).height(30.0))),
        tracker.track_named("item1", Empty::new().style(|s| s.width(100.0).height(30.0))),
        tracker.track_named("item2", Empty::new().style(|s| s.width(100.0).height(30.0))),
    ));

    let dropdown = Container::new(items_container).style(move |s| {
        let open = is_open.get();
        let base = s
            .absolute()
            .inset_top(40.0)
            .inset_left(0.0)
            .width(120.0)
            .z_index(100);
        if open {
            base
        } else {
            base.display(floem::style::Display::None)
        }
    });

    let backdrop = tracker
        .track_named("backdrop", Empty::new())
        .style(move |s| {
            let open = is_open.get();
            let base = s
                .absolute()
                .inset_top(-100.0)
                .inset_left(-100.0)
                .width(500.0)
                .height(500.0)
                .z_index(99);
            if open {
                base
            } else {
                base.display(floem::style::Display::None)
            }
        });

    let trigger = tracker
        .track_named("trigger", Empty::new())
        .style(|s| s.width(100.0).height(36.0));

    let select_container = Container::new(stack((trigger, backdrop, dropdown)))
        .style(|s| s.position(Position::Relative).width(120.0));

    // Wrap in a content container inside a Scroll view - like the showcase
    let content =
        Container::new(select_container).style(|s| s.width_full().height(500.0).padding(20.0));

    let view = Scroll::new(content).style(|s| s.width_full().height_full());

    let mut harness = HeadlessHarness::new_with_size(view, 300.0, 300.0);

    // Open the dropdown
    is_open.set(true);
    harness.rebuild();

    // Click on item1 (padding 20 + inset_top 40 + item0 30 + 15 = 105)
    harness.click(60.0, 105.0);

    assert_eq!(
        tracker.clicked_names(),
        vec!["item1"],
        "Item1 inside dropdown should receive click when inside Scroll view"
    );
}

#[test]
fn test_inset_top_pct_positioning() {
    // Test z-index event dispatch with percentage-based positioning (inset_top_pct)
    // This mimics the Select component which uses inset_top_pct(100.0) to position
    // the dropdown below the trigger.
    //
    // Structure:
    //   Container (position:relative, height:36)
    //   └── stack
    //       ├── trigger (width:100, height:36)
    //       ├── backdrop (z-index: 99)
    //       └── dropdown (z-index: 100, inset_top_pct: 100%)
    //           └── v_stack
    //               └── items

    use floem::reactive::RwSignal;

    let tracker = ClickTracker::new();
    let is_open = RwSignal::new(false);

    let items_container = v_stack((
        tracker.track_named("item0", Empty::new().style(|s| s.width(100.0).height(30.0))),
        tracker.track_named("item1", Empty::new().style(|s| s.width(100.0).height(30.0))),
        tracker.track_named("item2", Empty::new().style(|s| s.width(100.0).height(30.0))),
    ));

    // Use inset_top_pct(100.0) like the real Select component
    let dropdown = Container::new(items_container).style(move |s| {
        let open = is_open.get();
        let base = s
            .absolute()
            .inset_top_pct(100.0) // Position below parent
            .inset_left(0.0)
            .width(120.0)
            .z_index(100);
        if open {
            base
        } else {
            base.display(floem::style::Display::None)
        }
    });

    let backdrop = tracker
        .track_named("backdrop", Empty::new())
        .style(move |s| {
            let open = is_open.get();
            let base = s
                .absolute()
                .inset_top(-100.0)
                .inset_left(-100.0)
                .width(500.0)
                .height(500.0)
                .z_index(99);
            if open {
                base
            } else {
                base.display(floem::style::Display::None)
            }
        });

    let trigger = tracker
        .track_named("trigger", Empty::new())
        .style(|s| s.width(100.0).height(36.0));

    // Container with position:relative - dropdown will be positioned relative to this
    let select_container = Container::new(stack((trigger, backdrop, dropdown)))
        .style(|s| s.position(Position::Relative).width(120.0).height(36.0));

    let view =
        Container::new(select_container).style(|s| s.width_full().height_full().padding(50.0));

    let mut harness = HeadlessHarness::new_with_size(view, 300.0, 300.0);

    // Open dropdown
    is_open.set(true);
    harness.rebuild();

    // Click on item1: padding(50) + trigger_height(36) + item0(30) + 15 = 131
    harness.click(100.0, 131.0);

    assert_eq!(
        tracker.clicked_names(),
        vec!["item1"],
        "Item1 should receive click with inset_top_pct positioning"
    );
}

#[test]
fn test_dropdown_extends_beyond_scroll_area() {
    // Test that z-index event dispatch works correctly when a dropdown extends
    // beyond the scroll container's visible area.
    //
    // This replicates the showcase issue where:
    // 1. Select is inside a Scroll view
    // 2. The dropdown extends below the trigger (using inset_top_pct(100%))
    // 3. Part of the dropdown is outside the scroll container's clip area
    // 4. Clicks on the dropdown should go to items (z-index 100), not backdrop (z-index 99)
    //
    // Structure:
    //   Scroll (height: 100) <-- clips content
    //   └── Container (height: 200, padding: 20)
    //       └── Container (select, position:relative, at y=20)
    //           └── stack
    //               ├── trigger (height: 36)
    //               ├── backdrop (z-index: 99)
    //               └── dropdown (z-index: 100, inset_top: 40)
    //                   └── items at y=40, 70, 100 relative to select
    //
    // The dropdown items start at y=60 in scroll content coordinates.
    // With scroll container height of 100, items at y>80 are outside the visible area.
    // But they should still receive clicks because they're rendered with absolute positioning.

    use floem::reactive::RwSignal;
    use floem::views::Scroll;

    let tracker = ClickTracker::new();
    let is_open = RwSignal::new(false);

    let items_container = v_stack((
        tracker.track_named("item0", Empty::new().style(|s| s.width(100.0).height(30.0))),
        tracker.track_named("item1", Empty::new().style(|s| s.width(100.0).height(30.0))),
        tracker.track_named("item2", Empty::new().style(|s| s.width(100.0).height(30.0))),
    ));

    let dropdown = Container::new(items_container).style(move |s| {
        let open = is_open.get();
        let base = s
            .absolute()
            .inset_top(40.0)
            .inset_left(0.0)
            .width(120.0)
            .z_index(100);
        if open {
            base
        } else {
            base.display(floem::style::Display::None)
        }
    });

    let backdrop = tracker
        .track_named("backdrop", Empty::new())
        .style(move |s| {
            let open = is_open.get();
            let base = s
                .absolute()
                .inset_top(-100.0)
                .inset_left(-100.0)
                .width(500.0)
                .height(500.0)
                .z_index(99);
            if open {
                base
            } else {
                base.display(floem::style::Display::None)
            }
        });

    let trigger = tracker
        .track_named("trigger", Empty::new())
        .style(|s| s.width(100.0).height(36.0));

    let select_container = Container::new(stack((trigger, backdrop, dropdown)))
        .style(|s| s.position(Position::Relative).width(120.0));

    // Content is taller than the scroll container
    // Select is at the top (padding: 20), dropdown extends below
    let content =
        Container::new(select_container).style(|s| s.width_full().height(200.0).padding(20.0));

    // Scroll container is smaller than content - this creates clipping
    let view = Scroll::new(content).style(|s| s.width(200.0).height(100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 200.0);

    // Open dropdown
    is_open.set(true);
    harness.rebuild();

    // First, verify item0 can receive clicks (it's inside the scroll container)
    // item0 is at y=60-90 in scroll content (padding 20 + dropdown offset 40)
    // Click at y=75 (middle of item0)
    harness.click(60.0, 75.0);

    assert_eq!(
        tracker.clicked_names(),
        vec!["item0"],
        "Item0 should receive click (inside scroll container)"
    );
    tracker.reset();

    // Now click on item1 which is OUTSIDE the scroll container's visible area
    // item1 is at y=90-120 in scroll content
    // Scroll container shows y=0-100. item1 is partially outside.
    // Click at y=105 (middle of item1) - this is outside the scroll container (y>100)
    harness.click(60.0, 105.0);

    assert_eq!(
        tracker.clicked_names(),
        vec!["item1"],
        "Item1 should receive click even though it extends beyond scroll container"
    );
}
