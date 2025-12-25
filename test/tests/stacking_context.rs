//! Tests for CSS stacking context semantics.
//!
//! These tests verify that floem correctly implements CSS-like stacking context
//! behavior for event dispatch:
//! - Views with z-index create stacking contexts that bound their children
//! - Children inside a stacking context cannot "escape" to compete with siblings
//! - Non-stacking-context parents allow children to escape and compete globally
//! - Transform creates stacking context
//! - DOM order is used as a tiebreaker when z-index values are equal
//! - Event bubbling follows DOM tree, not stacking context tree

use floem::event::EventPropagation;
use floem::headless::HeadlessHarness;
use floem::taffy;
use floem::unit::UnitExt;
use floem::views::{Decorators, Empty, stack};
use std::cell::Cell;
use std::rc::Rc;

#[test]
fn test_z_index_click_ordering() {
    // Test that views with higher z-index receive clicks first
    let clicked_z1 = Rc::new(Cell::new(false));
    let clicked_z10 = Rc::new(Cell::new(false));

    let clicked_z1_clone = clicked_z1.clone();
    let clicked_z10_clone = clicked_z10.clone();

    let view = stack((
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(1))
            .on_click_stop(move |_| {
                clicked_z1_clone.set(true);
            }),
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(10))
            .on_click_stop(move |_| {
                clicked_z10_clone.set(true);
            }),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Click in the center where both views overlap
    harness.click(50.0, 50.0);

    // z-index 10 should have been clicked, z-index 1 should not
    assert!(
        clicked_z10.get(),
        "View with z-index 10 should receive the click"
    );
    assert!(
        !clicked_z1.get(),
        "View with z-index 1 should NOT receive the click (blocked by z-index 10)"
    );
}

#[test]
fn test_stacking_context_child_escapes() {
    // Test CSS stacking context escaping: a child with high z-index inside a
    // non-stacking-context parent should "escape" and receive clicks before
    // siblings with lower z-index.
    //
    // Structure:
    //   Root
    //   ├── Wrapper (no z-index, no stacking context)
    //   │   └── Child (z-index: 10)  <-- should receive click!
    //   └── Sibling (z-index: 5)
    //
    // Child should receive the click because it escapes Wrapper's non-stacking-context
    // and competes directly with Sibling.

    let clicked_child = Rc::new(Cell::new(false));
    let clicked_sibling = Rc::new(Cell::new(false));

    let clicked_child_clone = clicked_child.clone();
    let clicked_sibling_clone = clicked_sibling.clone();

    let view = stack((
        // Wrapper without z-index (no stacking context)
        stack((Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(10))
            .on_click_stop(move |_| {
                clicked_child_clone.set(true);
            }),))
        .style(|s| s.absolute().inset(0.0).size(100.0, 100.0)),
        // Sibling with z-index 5
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5))
            .on_click_stop(move |_| {
                clicked_sibling_clone.set(true);
            }),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    // Child (z=10) should receive click, not Sibling (z=5)
    assert!(
        clicked_child.get(),
        "Child with z-index 10 should receive click (escaped from non-stacking-context parent)"
    );
    assert!(
        !clicked_sibling.get(),
        "Sibling with z-index 5 should NOT receive click"
    );
}

#[test]
fn test_stacking_context_bounds_children() {
    // Test CSS stacking context bounding: a child with high z-index inside a
    // stacking-context parent should be BOUNDED and NOT receive clicks before
    // siblings with higher z-index than the parent.
    //
    // Structure:
    //   Root
    //   ├── Parent (z-index: 1, creates stacking context)
    //   │   └── Child (z-index: 100)  <-- bounded within Parent!
    //   └── Sibling (z-index: 5)  <-- should receive click!
    //
    // Sibling should receive the click because Child is bounded within Parent,
    // and Sibling (z=5) > Parent (z=1).

    let clicked_child = Rc::new(Cell::new(false));
    let clicked_sibling = Rc::new(Cell::new(false));

    let clicked_child_clone = clicked_child.clone();
    let clicked_sibling_clone = clicked_sibling.clone();

    let view = stack((
        // Parent with z-index 1 (creates stacking context)
        stack((Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(100))
            .on_click_stop(move |_| {
                clicked_child_clone.set(true);
            }),))
        .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(1)),
        // Sibling with z-index 5
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5))
            .on_click_stop(move |_| {
                clicked_sibling_clone.set(true);
            }),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    // Sibling (z=5) should receive click because Child is bounded within Parent (z=1)
    assert!(
        clicked_sibling.get(),
        "Sibling with z-index 5 should receive click (Parent z=1 < Sibling z=5)"
    );
    assert!(
        !clicked_child.get(),
        "Child with z-index 100 should NOT receive click (bounded within Parent z=1)"
    );
}

#[test]
fn test_stacking_context_complex_interleaving() {
    // Complex test with multiple escaping children interleaving with siblings
    //
    // Structure:
    //   Root
    //   ├── A (no z-index, no stacking context)
    //   │   ├── A1 (z-index: 3)
    //   │   └── A2 (z-index: 7)  <-- should receive click!
    //   ├── B (z-index: 5)
    //   └── C (z-index: 6)
    //
    // Event order (reverse of paint): A2 (7), C (6), B (5), A1 (3), A (0)
    // A2 should receive the click.

    let clicked_a1 = Rc::new(Cell::new(false));
    let clicked_a2 = Rc::new(Cell::new(false));
    let clicked_b = Rc::new(Cell::new(false));
    let clicked_c = Rc::new(Cell::new(false));

    let clicked_a1_clone = clicked_a1.clone();
    let clicked_a2_clone = clicked_a2.clone();
    let clicked_b_clone = clicked_b.clone();
    let clicked_c_clone = clicked_c.clone();

    let view = stack((
        // A (no stacking context)
        stack((
            Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(3))
                .on_click_stop(move |_| {
                    clicked_a1_clone.set(true);
                }),
            Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(7))
                .on_click_stop(move |_| {
                    clicked_a2_clone.set(true);
                }),
        ))
        .style(|s| s.absolute().inset(0.0).size(100.0, 100.0)),
        // B (z-index: 5)
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5))
            .on_click_stop(move |_| {
                clicked_b_clone.set(true);
            }),
        // C (z-index: 6)
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(6))
            .on_click_stop(move |_| {
                clicked_c_clone.set(true);
            }),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    // A2 (z=7) should receive click - highest z-index among all participants
    assert!(
        clicked_a2.get(),
        "A2 with z-index 7 should receive click (escaped and highest)"
    );
    assert!(!clicked_c.get(), "C should NOT receive click");
    assert!(!clicked_b.get(), "B should NOT receive click");
    assert!(!clicked_a1.get(), "A1 should NOT receive click");
}

#[test]
fn test_stacking_context_negative_z_index() {
    // Test negative z-index: views with negative z-index are painted first
    // and receive events last.
    //
    // Structure:
    //   Root
    //   ├── A (z-index: -1)
    //   ├── B (no z-index, effectively 0)  <-- should receive click!
    //   └── C (z-index: -5)
    //
    // B (z=0) should receive the click because it's highest.

    let clicked_a = Rc::new(Cell::new(false));
    let clicked_b = Rc::new(Cell::new(false));
    let clicked_c = Rc::new(Cell::new(false));

    let clicked_a_clone = clicked_a.clone();
    let clicked_b_clone = clicked_b.clone();
    let clicked_c_clone = clicked_c.clone();

    let view = stack((
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(-1))
            .on_click_stop(move |_| {
                clicked_a_clone.set(true);
            }),
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0))
            .on_click_stop(move |_| {
                clicked_b_clone.set(true);
            }),
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(-5))
            .on_click_stop(move |_| {
                clicked_c_clone.set(true);
            }),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    assert!(
        clicked_b.get(),
        "B (z=0) should receive click - highest z-index"
    );
    assert!(!clicked_a.get(), "A (z=-1) should NOT receive click");
    assert!(!clicked_c.get(), "C (z=-5) should NOT receive click");
}

#[test]
fn test_stacking_context_transform_creates_context() {
    // Test that transform creates a stacking context, bounding children.
    //
    // Structure:
    //   Root
    //   ├── Parent (transform: scale 101%, creates stacking context)
    //   │   └── Child (z-index: 100)  <-- bounded within Parent!
    //   └── Sibling (z-index: 5)  <-- should receive click!
    //
    // Sibling should receive click because Parent has transform (creates context),
    // bounding Child, and Parent itself has z=0 < Sibling z=5.

    let clicked_child = Rc::new(Cell::new(false));
    let clicked_sibling = Rc::new(Cell::new(false));

    let clicked_child_clone = clicked_child.clone();
    let clicked_sibling_clone = clicked_sibling.clone();

    let view = stack((
        // Parent with non-identity transform (creates stacking context even without z-index)
        stack((Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(100))
            .on_click_stop(move |_| {
                clicked_child_clone.set(true);
            }),))
        .style(|s| {
            s.absolute().inset(0.0).size(100.0, 100.0).scale(101.pct()) // Non-identity transform creates stacking context
        }),
        // Sibling with z-index 5
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5))
            .on_click_stop(move |_| {
                clicked_sibling_clone.set(true);
            }),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    // Sibling (z=5) should receive click because Parent (with transform, z=0) bounds Child
    assert!(
        clicked_sibling.get(),
        "Sibling (z=5) should receive click - Parent's transform creates stacking context"
    );
    assert!(
        !clicked_child.get(),
        "Child should NOT receive click - bounded by Parent's transform stacking context"
    );
}

#[test]
fn test_stacking_context_deeply_nested_escape() {
    // Test deeply nested escaping: a child nested multiple levels deep can still
    // escape if no ancestor creates a stacking context.
    //
    // Structure:
    //   Root
    //   ├── Level1 (no z-index)
    //   │   └── Level2 (no z-index)
    //   │       └── Level3 (no z-index)
    //   │           └── DeepChild (z-index: 10)  <-- should receive click!
    //   └── Sibling (z-index: 5)
    //
    // DeepChild escapes all the way up and competes with Sibling.

    let clicked_deep = Rc::new(Cell::new(false));
    let clicked_sibling = Rc::new(Cell::new(false));

    let clicked_deep_clone = clicked_deep.clone();
    let clicked_sibling_clone = clicked_sibling.clone();

    let view = stack((
        // Level1
        stack((
            // Level2
            stack((
                // Level3
                stack((Empty::new()
                    .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(10))
                    .on_click_stop(move |_| {
                        clicked_deep_clone.set(true);
                    }),))
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0)),
            ))
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0)),
        ))
        .style(|s| s.absolute().inset(0.0).size(100.0, 100.0)),
        // Sibling
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5))
            .on_click_stop(move |_| {
                clicked_sibling_clone.set(true);
            }),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    assert!(
        clicked_deep.get(),
        "DeepChild (z=10) should receive click - escaped through 3 levels"
    );
    assert!(
        !clicked_sibling.get(),
        "Sibling (z=5) should NOT receive click"
    );
}

#[test]
fn test_stacking_context_dom_order_tiebreaker() {
    // Test DOM order as tiebreaker: when multiple views have the same z-index,
    // the later one in DOM order should receive events first (painted last).
    //
    // Structure:
    //   Root
    //   ├── First (z-index: 5)
    //   ├── Second (z-index: 5)
    //   └── Third (z-index: 5)  <-- should receive click! (last in DOM)

    let clicked_first = Rc::new(Cell::new(false));
    let clicked_second = Rc::new(Cell::new(false));
    let clicked_third = Rc::new(Cell::new(false));

    let clicked_first_clone = clicked_first.clone();
    let clicked_second_clone = clicked_second.clone();
    let clicked_third_clone = clicked_third.clone();

    let view = stack((
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5))
            .on_click_stop(move |_| {
                clicked_first_clone.set(true);
            }),
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5))
            .on_click_stop(move |_| {
                clicked_second_clone.set(true);
            }),
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5))
            .on_click_stop(move |_| {
                clicked_third_clone.set(true);
            }),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    assert!(
        clicked_third.get(),
        "Third should receive click - last in DOM order with same z-index"
    );
    assert!(!clicked_second.get(), "Second should NOT receive click");
    assert!(!clicked_first.get(), "First should NOT receive click");
}

#[test]
fn test_stacking_context_mixed_contexts() {
    // Test mixed stacking contexts: some views create context, some don't.
    //
    // Structure:
    //   Root
    //   ├── NoContext (no z-index)
    //   │   └── Escaper (z-index: 8)  <-- escapes!
    //   ├── WithContext (z-index: 3, creates context)
    //   │   └── Bounded (z-index: 100)  <-- bounded by WithContext
    //   └── TopLevel (z-index: 6)
    //
    // Event order: Escaper (8), TopLevel (6), WithContext (3) -> Bounded (100)
    // Escaper should receive click.

    let clicked_escaper = Rc::new(Cell::new(false));
    let clicked_bounded = Rc::new(Cell::new(false));
    let clicked_top = Rc::new(Cell::new(false));

    let clicked_escaper_clone = clicked_escaper.clone();
    let clicked_bounded_clone = clicked_bounded.clone();
    let clicked_top_clone = clicked_top.clone();

    let view = stack((
        // NoContext wrapper
        stack((Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(8))
            .on_click_stop(move |_| {
                clicked_escaper_clone.set(true);
            }),))
        .style(|s| s.absolute().inset(0.0).size(100.0, 100.0)),
        // WithContext (z-index creates stacking context)
        stack((Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(100))
            .on_click_stop(move |_| {
                clicked_bounded_clone.set(true);
            }),))
        .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(3)),
        // TopLevel
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(6))
            .on_click_stop(move |_| {
                clicked_top_clone.set(true);
            }),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    assert!(
        clicked_escaper.get(),
        "Escaper (z=8) should receive click - escaped and highest"
    );
    assert!(
        !clicked_top.get(),
        "TopLevel (z=6) should NOT receive click"
    );
    assert!(
        !clicked_bounded.get(),
        "Bounded (z=100) should NOT receive click - bounded by WithContext (z=3)"
    );
}

#[test]
fn test_stacking_context_partial_overlap() {
    // Test partial overlap: click coordinates matter for hit testing.
    //
    // Structure:
    //   Root (200x100)
    //   ├── Left (0-100, z-index: 5)
    //   └── Right (100-200, z-index: 10)
    //
    // Click at (50, 50) should hit Left.
    // Click at (150, 50) should hit Right.

    let clicked_left = Rc::new(Cell::new(false));
    let clicked_right = Rc::new(Cell::new(false));

    let clicked_left_clone = clicked_left.clone();
    let clicked_right_clone = clicked_right.clone();

    let view = stack((
        Empty::new()
            .style(|s| {
                s.absolute()
                    .inset_left(0.0)
                    .inset_top(0.0)
                    .size(100.0, 100.0)
                    .z_index(5)
            })
            .on_click_stop(move |_| {
                clicked_left_clone.set(true);
            }),
        Empty::new()
            .style(|s| {
                s.absolute()
                    .inset_left(100.0)
                    .inset_top(0.0)
                    .size(100.0, 100.0)
                    .z_index(10)
            })
            .on_click_stop(move |_| {
                clicked_right_clone.set(true);
            }),
    ))
    .style(|s| s.size(200.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 200.0, 100.0);

    // Click left side
    harness.click(50.0, 50.0);

    assert!(clicked_left.get(), "Left should receive click at (50, 50)");
    assert!(
        !clicked_right.get(),
        "Right should NOT receive click at (50, 50)"
    );

    // Reset
    clicked_left.set(false);
    clicked_right.set(false);

    // Click right side
    harness.click(150.0, 50.0);

    assert!(
        clicked_right.get(),
        "Right should receive click at (150, 50)"
    );
    assert!(
        !clicked_left.get(),
        "Left should NOT receive click at (150, 50)"
    );
}

#[test]
fn test_stacking_context_pointer_events_none() {
    // Test that views with pointer_events_none are skipped in event dispatch.
    //
    // Structure:
    //   Root
    //   ├── Top (z-index: 10, pointer_events_none)  <-- skipped!
    //   └── Bottom (z-index: 5)  <-- should receive click!

    let clicked_top = Rc::new(Cell::new(false));
    let clicked_bottom = Rc::new(Cell::new(false));

    let clicked_top_clone = clicked_top.clone();
    let clicked_bottom_clone = clicked_bottom.clone();

    let view = stack((
        Empty::new()
            .style(|s| {
                s.absolute()
                    .inset(0.0)
                    .size(100.0, 100.0)
                    .z_index(10)
                    .pointer_events_none()
            })
            .on_click_stop(move |_| {
                clicked_top_clone.set(true);
            }),
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5))
            .on_click_stop(move |_| {
                clicked_bottom_clone.set(true);
            }),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    assert!(
        !clicked_top.get(),
        "Top (pointer_events_none) should NOT receive click"
    );
    assert!(
        clicked_bottom.get(),
        "Bottom should receive click when Top has pointer_events_none"
    );
}

#[test]
fn test_stacking_context_hidden_view() {
    // Test that hidden views are skipped in event dispatch.
    //
    // Structure:
    //   Root
    //   ├── Hidden (z-index: 10, display: none)  <-- skipped!
    //   └── Visible (z-index: 5)  <-- should receive click!

    let clicked_hidden = Rc::new(Cell::new(false));
    let clicked_visible = Rc::new(Cell::new(false));

    let clicked_hidden_clone = clicked_hidden.clone();
    let clicked_visible_clone = clicked_visible.clone();

    let view = stack((
        Empty::new()
            .style(|s| {
                s.absolute()
                    .inset(0.0)
                    .size(100.0, 100.0)
                    .z_index(10)
                    .display(taffy::Display::None)
            })
            .on_click_stop(move |_| {
                clicked_hidden_clone.set(true);
            }),
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5))
            .on_click_stop(move |_| {
                clicked_visible_clone.set(true);
            }),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    assert!(
        !clicked_hidden.get(),
        "Hidden view should NOT receive click"
    );
    assert!(
        clicked_visible.get(),
        "Visible view should receive click when other is hidden"
    );
}

#[test]
fn test_stacking_context_hidden_parent_hides_children() {
    // Test that children of hidden views don't receive events.
    //
    // Structure:
    //   Root
    //   ├── HiddenParent (z-index: 10, display: none)
    //   │   └── Child (z-index: 100)  <-- should NOT receive click (parent hidden)
    //   └── Visible (z-index: 5)  <-- should receive click!

    let clicked_child = Rc::new(Cell::new(false));
    let clicked_visible = Rc::new(Cell::new(false));

    let clicked_child_clone = clicked_child.clone();
    let clicked_visible_clone = clicked_visible.clone();

    let view = stack((
        stack((Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(100))
            .on_click_stop(move |_| {
                clicked_child_clone.set(true);
            }),))
        .style(|s| {
            s.absolute()
                .inset(0.0)
                .size(100.0, 100.0)
                .z_index(10)
                .display(taffy::Display::None)
        }),
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5))
            .on_click_stop(move |_| {
                clicked_visible_clone.set(true);
            }),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    assert!(
        !clicked_child.get(),
        "Child of hidden parent should NOT receive click"
    );
    assert!(
        clicked_visible.get(),
        "Visible sibling should receive click"
    );
}

#[test]
fn test_stacking_context_hidden_in_escaped_context() {
    // Test hidden view that would otherwise escape to parent stacking context.
    //
    // Structure:
    //   Root
    //   ├── Wrapper (no z-index, no stacking context)
    //   │   ├── Hidden (z-index: 10, display: none)  <-- would escape, but hidden
    //   │   └── Visible (z-index: 5)  <-- escapes, should receive click!
    //   └── Sibling (z-index: 7)
    //
    // Hidden (z=10) would beat Sibling (z=7), but it's hidden.
    // Sibling (z=7) should receive click (beats Visible z=5).

    let clicked_hidden = Rc::new(Cell::new(false));
    let clicked_visible = Rc::new(Cell::new(false));
    let clicked_sibling = Rc::new(Cell::new(false));

    let h_clone = clicked_hidden.clone();
    let v_clone = clicked_visible.clone();
    let s_clone = clicked_sibling.clone();

    let view = stack((
        stack((
            Empty::new()
                .style(|s| {
                    s.absolute()
                        .inset(0.0)
                        .size(100.0, 100.0)
                        .z_index(10)
                        .display(taffy::Display::None)
                })
                .on_click_stop(move |_| {
                    h_clone.set(true);
                }),
            Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5))
                .on_click_stop(move |_| {
                    v_clone.set(true);
                }),
        ))
        .style(|s| s.absolute().inset(0.0).size(100.0, 100.0)),
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(7))
            .on_click_stop(move |_| {
                s_clone.set(true);
            }),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    assert!(!clicked_hidden.get(), "Hidden should NOT receive click");
    assert!(
        !clicked_visible.get(),
        "Visible (z=5) should NOT receive click (Sibling z=7 wins)"
    );
    assert!(
        clicked_sibling.get(),
        "Sibling (z=7) should receive click (Hidden is skipped)"
    );
}

#[test]
fn test_stacking_context_hidden_does_not_bubble() {
    // Test that events don't bubble through hidden ancestors.
    //
    // Structure:
    //   Root
    //   └── HiddenParent (display: none, with on_click)
    //       └── Child (z-index: 5, with on_click)
    //
    // Neither should receive the click (parent is hidden).

    let clicked_parent = Rc::new(Cell::new(false));
    let clicked_child = Rc::new(Cell::new(false));

    let p_clone = clicked_parent.clone();
    let c_clone = clicked_child.clone();

    let view = stack((Empty::new()
        .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5))
        .on_click(move |_| {
            c_clone.set(true);
            EventPropagation::Continue
        }),))
    .style(|s| s.size(100.0, 100.0).display(taffy::Display::None))
    .on_click(move |_| {
        p_clone.set(true);
        EventPropagation::Continue
    });

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    assert!(
        !clicked_child.get(),
        "Child of hidden parent should NOT receive click"
    );
    assert!(
        !clicked_parent.get(),
        "Hidden parent should NOT receive click"
    );
}

#[test]
fn test_stacking_context_nested_contexts() {
    // Test nested stacking contexts: a stacking context inside another stacking context.
    //
    // Structure:
    //   Root
    //   ├── Outer (z-index: 5, creates context)
    //   │   └── Inner (z-index: 3, creates context)
    //   │       └── DeepChild (z-index: 100)  <-- bounded by Inner, which is bounded by Outer
    //   └── Sibling (z-index: 6)  <-- should receive click!
    //
    // Sibling (z=6) > Outer (z=5), so Sibling wins.

    let clicked_deep = Rc::new(Cell::new(false));
    let clicked_sibling = Rc::new(Cell::new(false));

    let clicked_deep_clone = clicked_deep.clone();
    let clicked_sibling_clone = clicked_sibling.clone();

    let view = stack((
        // Outer stacking context
        stack((
            // Inner stacking context
            stack((Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(100))
                .on_click_stop(move |_| {
                    clicked_deep_clone.set(true);
                }),))
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(3)),
        ))
        .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5)),
        // Sibling
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(6))
            .on_click_stop(move |_| {
                clicked_sibling_clone.set(true);
            }),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    assert!(
        clicked_sibling.get(),
        "Sibling (z=6) should receive click - higher than Outer (z=5)"
    );
    assert!(
        !clicked_deep.get(),
        "DeepChild should NOT receive click - doubly bounded by nested contexts"
    );
}

#[test]
fn test_stacking_context_sibling_isolation() {
    // Test that sibling stacking contexts are isolated from each other.
    //
    // Structure:
    //   Root
    //   ├── ContextA (z-index: 5, creates context)
    //   │   └── ChildA (z-index: 100)  <-- bounded by ContextA
    //   └── ContextB (z-index: 10, creates context)  <-- should receive click!
    //       └── ChildB (z-index: 1)  <-- bounded by ContextB
    //
    // ContextB (z=10) > ContextA (z=5), so ContextB's subtree gets events first.
    // Within ContextB, ChildB (z=1) is the only option.

    let clicked_child_a = Rc::new(Cell::new(false));
    let clicked_child_b = Rc::new(Cell::new(false));

    let clicked_child_a_clone = clicked_child_a.clone();
    let clicked_child_b_clone = clicked_child_b.clone();

    let view = stack((
        // ContextA (z=5)
        stack((Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(100))
            .on_click_stop(move |_| {
                clicked_child_a_clone.set(true);
            }),))
        .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5)),
        // ContextB (z=10)
        stack((Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(1))
            .on_click_stop(move |_| {
                clicked_child_b_clone.set(true);
            }),))
        .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(10)),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    assert!(
        clicked_child_b.get(),
        "ChildB should receive click - ContextB (z=10) > ContextA (z=5)"
    );
    assert!(
        !clicked_child_a.get(),
        "ChildA (z=100) should NOT receive click - bounded by ContextA (z=5)"
    );
}

#[test]
fn test_stacking_context_event_bubbling() {
    // Test event bubbling with stacking context: when a child with z-index
    // handles an event and returns Continue, the event should bubble up
    // to its DOM parent (even if the parent has lower z-index).
    //
    // Structure:
    //   Root
    //   └── Parent (no z-index, with on_click returning Continue)
    //       └── Child (z-index: 5, with on_click returning Continue)
    //
    // Both should receive the click due to bubbling.

    let clicked_parent = Rc::new(Cell::new(false));
    let clicked_child = Rc::new(Cell::new(false));

    let clicked_parent_clone = clicked_parent.clone();
    let clicked_child_clone = clicked_child.clone();

    let view = stack((Empty::new()
        .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5))
        .on_click(move |_| {
            clicked_child_clone.set(true);
            EventPropagation::Continue
        }),))
    .style(|s| s.size(100.0, 100.0))
    .on_click(move |_| {
        clicked_parent_clone.set(true);
        EventPropagation::Continue
    });

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    assert!(clicked_child.get(), "Child should receive click first");
    assert!(
        clicked_parent.get(),
        "Parent should receive click via bubbling"
    );
}

#[test]
fn test_stacking_context_bubbling_stops_on_stop() {
    // Test that bubbling stops when a handler returns Stop.
    //
    // Structure:
    //   Root
    //   └── Parent (no z-index, with on_click returning Continue)
    //       └── Child (z-index: 5, with on_click_stop)
    //
    // Only Child should receive the click (bubbling stops).

    let clicked_parent = Rc::new(Cell::new(false));
    let clicked_child = Rc::new(Cell::new(false));

    let clicked_parent_clone = clicked_parent.clone();
    let clicked_child_clone = clicked_child.clone();

    let view = stack((Empty::new()
        .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5))
        .on_click_stop(move |_| {
            clicked_child_clone.set(true);
        }),))
    .style(|s| s.size(100.0, 100.0))
    .on_click_stop(move |_| {
        clicked_parent_clone.set(true);
    });

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    assert!(clicked_child.get(), "Child should receive click");
    assert!(
        !clicked_parent.get(),
        "Parent should NOT receive click (bubbling stopped)"
    );
}

#[test]
fn test_stacking_context_deep_bubbling() {
    // Test event bubbling through multiple ancestor levels (no stacking contexts).
    //
    // Structure:
    //   Root
    //   └── GrandParent (no z-index, with on_click returning Continue)
    //       └── Parent (no z-index, with on_click returning Continue)
    //           └── Child (z-index: 5, with on_click returning Continue)
    //
    // All three should receive the click due to bubbling.

    let clicked_grandparent = Rc::new(Cell::new(false));
    let clicked_parent = Rc::new(Cell::new(false));
    let clicked_child = Rc::new(Cell::new(false));

    let gp_clone = clicked_grandparent.clone();
    let p_clone = clicked_parent.clone();
    let c_clone = clicked_child.clone();

    let view = stack((stack((Empty::new()
        .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5))
        .on_click(move |_| {
            c_clone.set(true);
            EventPropagation::Continue
        }),))
    .style(|s| s.absolute().inset(0.0).size(100.0, 100.0))
    .on_click(move |_| {
        p_clone.set(true);
        EventPropagation::Continue
    }),))
    .style(|s| s.size(100.0, 100.0))
    .on_click(move |_| {
        gp_clone.set(true);
        EventPropagation::Continue
    });

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    assert!(clicked_child.get(), "Child should receive click first");
    assert!(
        clicked_parent.get(),
        "Parent should receive click via bubbling"
    );
    assert!(
        clicked_grandparent.get(),
        "GrandParent should receive click via bubbling"
    );
}

#[test]
fn test_stacking_context_bubbling_across_stacking_contexts() {
    // Test event bubbling through nested stacking contexts (like web browser).
    //
    // In web: events bubble through DOM ancestors regardless of stacking contexts.
    // Each ancestor with z-index creates its own stacking context, but bubbling
    // still follows the DOM tree.
    //
    // Structure:
    //   Root
    //   └── GrandParent (z-index: 1, creates stacking context)
    //       └── Parent (z-index: 2, creates stacking context)
    //           └── Child (z-index: 3, with on_click returning Continue)
    //
    // Event goes to Child, then bubbles to Parent, then GrandParent.

    let clicked_grandparent = Rc::new(Cell::new(false));
    let clicked_parent = Rc::new(Cell::new(false));
    let clicked_child = Rc::new(Cell::new(false));

    let gp_clone = clicked_grandparent.clone();
    let p_clone = clicked_parent.clone();
    let c_clone = clicked_child.clone();

    let view = stack((stack((Empty::new()
        .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(3))
        .on_click(move |_| {
            c_clone.set(true);
            EventPropagation::Continue
        }),))
    .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(2))
    .on_click(move |_| {
        p_clone.set(true);
        EventPropagation::Continue
    }),))
    .style(|s| s.size(100.0, 100.0).z_index(1))
    .on_click(move |_| {
        gp_clone.set(true);
        EventPropagation::Continue
    });

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    assert!(clicked_child.get(), "Child should receive click first");
    assert!(
        clicked_parent.get(),
        "Parent should receive click via bubbling (matches web)"
    );
    assert!(
        clicked_grandparent.get(),
        "GrandParent should receive click via bubbling (matches web)"
    );
}

#[test]
fn test_stacking_context_multiple_escaped_children() {
    // Test multiple escaped children competing: highest z-index wins.
    //
    // Structure:
    //   Root
    //   └── Wrapper (no stacking context)
    //       ├── Child1 (z-index: 3)
    //       ├── Child2 (z-index: 7)
    //       ├── Child3 (z-index: 5)
    //       └── Child4 (z-index: 7)  <-- should receive click! (same z as Child2, but later in DOM)
    //
    // All escape, Child4 wins (z=7, last in DOM order).

    let clicked = [
        Rc::new(Cell::new(false)),
        Rc::new(Cell::new(false)),
        Rc::new(Cell::new(false)),
        Rc::new(Cell::new(false)),
    ];

    let c0 = clicked[0].clone();
    let c1 = clicked[1].clone();
    let c2 = clicked[2].clone();
    let c3 = clicked[3].clone();

    let view = stack((stack((
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(3))
            .on_click_stop(move |_| c0.set(true)),
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(7))
            .on_click_stop(move |_| c1.set(true)),
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5))
            .on_click_stop(move |_| c2.set(true)),
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(7))
            .on_click_stop(move |_| c3.set(true)),
    ))
    .style(|s| s.absolute().inset(0.0).size(100.0, 100.0)),))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    assert!(!clicked[0].get(), "Child1 (z=3) should NOT receive click");
    assert!(
        !clicked[1].get(),
        "Child2 (z=7) should NOT receive click - Child4 is later in DOM"
    );
    assert!(!clicked[2].get(), "Child3 (z=5) should NOT receive click");
    assert!(
        clicked[3].get(),
        "Child4 (z=7, last in DOM) should receive click"
    );
}

#[test]
fn test_stacking_context_explicit_z_index_zero() {
    // Test explicit z-index: 0 creates stacking context (bounds children).
    //
    // Structure:
    //   Root
    //   ├── Parent (z-index: 0, creates stacking context!)
    //   │   └── Child (z-index: 100)  <-- bounded by Parent!
    //   └── Sibling (z-index: 1)  <-- should receive click!
    //
    // Parent has explicit z-index: 0 which creates stacking context.
    // Sibling (z=1) > Parent (z=0), so Sibling wins.

    let clicked_child = Rc::new(Cell::new(false));
    let clicked_sibling = Rc::new(Cell::new(false));

    let clicked_child_clone = clicked_child.clone();
    let clicked_sibling_clone = clicked_sibling.clone();

    let view = stack((
        // Parent with explicit z-index: 0
        stack((Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(100))
            .on_click_stop(move |_| {
                clicked_child_clone.set(true);
            }),))
        .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(0)),
        // Sibling
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(1))
            .on_click_stop(move |_| {
                clicked_sibling_clone.set(true);
            }),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    assert!(
        clicked_sibling.get(),
        "Sibling (z=1) should receive click - higher than Parent (z=0)"
    );
    assert!(
        !clicked_child.get(),
        "Child should NOT receive click - bounded by Parent's explicit z-index: 0"
    );
}

// ========== Opacity Stacking Context Tests ==========
// CSS spec: opacity < 1 creates a stacking context, bounding children

#[test]
fn test_opacity_creates_stacking_context() {
    // Test that opacity < 1 creates a stacking context, bounding children.
    //
    // Structure:
    //   Root
    //   ├── Parent (opacity: 0.5, creates stacking context!)
    //   │   └── Child (z-index: 100)  <-- bounded by Parent!
    //   └── Sibling (z-index: 5)  <-- should receive click!
    //
    // Parent has opacity: 0.5 which creates stacking context per CSS spec.
    // Sibling (z=5) > Parent (z=0 implicit), so Sibling wins.

    let clicked_child = Rc::new(Cell::new(false));
    let clicked_sibling = Rc::new(Cell::new(false));

    let clicked_child_clone = clicked_child.clone();
    let clicked_sibling_clone = clicked_sibling.clone();

    let view = stack((
        // Parent with opacity < 1 (should create stacking context)
        stack((Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(100))
            .on_click_stop(move |_| {
                clicked_child_clone.set(true);
            }),))
        .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).opacity(0.5)),
        // Sibling with z-index 5
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5))
            .on_click_stop(move |_| {
                clicked_sibling_clone.set(true);
            }),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    // Sibling (z=5) should receive click because Parent (with opacity, z=0) bounds Child
    assert!(
        clicked_sibling.get(),
        "Sibling (z=5) should receive click - Parent's opacity creates stacking context"
    );
    assert!(
        !clicked_child.get(),
        "Child should NOT receive click - bounded by Parent's opacity stacking context"
    );
}

#[test]
fn test_opacity_1_does_not_create_stacking_context() {
    // Test that opacity: 1.0 does NOT create a stacking context.
    // Children should be able to escape.
    //
    // Structure:
    //   Root
    //   ├── Parent (opacity: 1.0, NO stacking context)
    //   │   └── Child (z-index: 10)  <-- escapes! Should receive click!
    //   └── Sibling (z-index: 5)
    //
    // Parent has opacity: 1.0 which does NOT create stacking context.
    // Child (z=10) escapes and wins over Sibling (z=5).

    let clicked_child = Rc::new(Cell::new(false));
    let clicked_sibling = Rc::new(Cell::new(false));

    let clicked_child_clone = clicked_child.clone();
    let clicked_sibling_clone = clicked_sibling.clone();

    let view = stack((
        // Parent with opacity = 1.0 (should NOT create stacking context)
        stack((Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(10))
            .on_click_stop(move |_| {
                clicked_child_clone.set(true);
            }),))
        .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).opacity(1.0)),
        // Sibling with z-index 5
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5))
            .on_click_stop(move |_| {
                clicked_sibling_clone.set(true);
            }),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    // Child (z=10) should receive click because it escapes Parent (opacity: 1.0)
    assert!(
        clicked_child.get(),
        "Child (z=10) should receive click - escaped from Parent with opacity: 1.0"
    );
    assert!(
        !clicked_sibling.get(),
        "Sibling (z=5) should NOT receive click"
    );
}

#[test]
fn test_opacity_near_zero_creates_stacking_context() {
    // Test that very low opacity (near 0) creates stacking context.
    //
    // Structure:
    //   Root
    //   ├── Parent (opacity: 0.01)
    //   │   └── Child (z-index: 100)  <-- bounded!
    //   └── Sibling (z-index: 5)  <-- should receive click!

    let clicked_child = Rc::new(Cell::new(false));
    let clicked_sibling = Rc::new(Cell::new(false));

    let clicked_child_clone = clicked_child.clone();
    let clicked_sibling_clone = clicked_sibling.clone();

    let view = stack((
        stack((Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(100))
            .on_click_stop(move |_| {
                clicked_child_clone.set(true);
            }),))
        .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).opacity(0.01)),
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5))
            .on_click_stop(move |_| {
                clicked_sibling_clone.set(true);
            }),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    assert!(
        clicked_sibling.get(),
        "Sibling should receive click - Parent's near-zero opacity creates stacking context"
    );
    assert!(
        !clicked_child.get(),
        "Child should NOT receive click - bounded by opacity stacking context"
    );
}

#[test]
fn test_opacity_with_z_index_combination() {
    // Test that opacity combined with z-index works correctly.
    // The z-index determines the stacking order at the parent level.
    //
    // Structure:
    //   Root
    //   ├── ParentA (z-index: 10, opacity: 0.5)
    //   │   └── ChildA (z-index: 100)  <-- bounded by ParentA
    //   └── ParentB (z-index: 5)
    //       └── ChildB (z-index: 1)
    //
    // ParentA (z=10) > ParentB (z=5), so ChildA receives click.
    // ChildA is bounded within ParentA due to opacity.

    let clicked_child_a = Rc::new(Cell::new(false));
    let clicked_child_b = Rc::new(Cell::new(false));

    let clicked_a_clone = clicked_child_a.clone();
    let clicked_b_clone = clicked_child_b.clone();

    let view = stack((
        // ParentA with z-index: 10 and opacity: 0.5
        stack((Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(100))
            .on_click_stop(move |_| {
                clicked_a_clone.set(true);
            }),))
        .style(|s| {
            s.absolute()
                .inset(0.0)
                .size(100.0, 100.0)
                .z_index(10)
                .opacity(0.5)
        }),
        // ParentB with z-index: 5
        stack((Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(1))
            .on_click_stop(move |_| {
                clicked_b_clone.set(true);
            }),))
        .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5)),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    // ChildA should receive click because ParentA (z=10) > ParentB (z=5)
    assert!(
        clicked_child_a.get(),
        "ChildA should receive click - ParentA (z=10) wins"
    );
    assert!(!clicked_child_b.get(), "ChildB should NOT receive click");
}

#[test]
fn test_opacity_deeply_nested() {
    // Test opacity stacking context with deep nesting.
    //
    // Structure:
    //   Root
    //   ├── Level1 (no stacking context)
    //   │   └── Level2 (opacity: 0.8, creates stacking context)
    //   │       └── DeepChild (z-index: 100)  <-- bounded by Level2!
    //   └── Sibling (z-index: 5)  <-- should receive click!
    //
    // Level2's opacity bounds DeepChild. Level2 has implicit z=0.

    let clicked_deep = Rc::new(Cell::new(false));
    let clicked_sibling = Rc::new(Cell::new(false));

    let clicked_deep_clone = clicked_deep.clone();
    let clicked_sibling_clone = clicked_sibling.clone();

    let view = stack((
        // Level1 (no stacking context)
        stack((
            // Level2 (opacity creates stacking context)
            stack((Empty::new()
                .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(100))
                .on_click_stop(move |_| {
                    clicked_deep_clone.set(true);
                }),))
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).opacity(0.8)),
        ))
        .style(|s| s.absolute().inset(0.0).size(100.0, 100.0)),
        // Sibling
        Empty::new()
            .style(|s| s.absolute().inset(0.0).size(100.0, 100.0).z_index(5))
            .on_click_stop(move |_| {
                clicked_sibling_clone.set(true);
            }),
    ))
    .style(|s| s.size(100.0, 100.0));

    let mut harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    harness.click(50.0, 50.0);

    assert!(
        clicked_sibling.get(),
        "Sibling (z=5) should receive click - DeepChild bounded by Level2's opacity"
    );
    assert!(
        !clicked_deep.get(),
        "DeepChild should NOT receive click - bounded by opacity stacking context"
    );
}
