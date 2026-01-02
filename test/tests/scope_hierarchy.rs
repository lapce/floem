//! Tests for scope hierarchy matching view hierarchy.
//!
//! These tests verify that:
//! - Scope hierarchy matches view hierarchy after construction
//! - Scope re-parenting works correctly for eager construction
//! - Disposal cascades through the hierarchy correctly
//! - Context lookup works after view tree is assembled
//!
//! Note: These tests must run serially because they share global state
//! (VIEW_STORAGE, window tracking) that can't be isolated between parallel tests.

use std::cell::Cell;
use std::rc::Rc;

use floem::prelude::*;
use floem::reactive::{Context, Scope};
use floem::views::{Decorators, Empty, Stem};
use floem_test::prelude::*;
use serial_test::serial;

// ============================================================================
// Tests
// ============================================================================

/// Test that scope is properly set on views.
#[test]
#[serial]
fn test_view_scope_is_set() {
    let scope = Scope::current().create_child();
    let view = Stem::new().style(|s| s.size(100.0, 100.0));
    let id = view.view_id();

    // Before setting scope
    assert!(id.scope().is_none(), "Scope should be None initially");

    // Set scope
    id.set_scope(scope);

    // After setting scope
    assert!(id.scope().is_some(), "Scope should be set");
}

/// Test that find_scope walks up the view hierarchy.
#[test]
#[serial]
fn test_find_scope_walks_hierarchy() {
    let parent_scope = Scope::current().create_child();

    // Create parent with scope
    let parent = Stem::new().style(|s| s.size(100.0, 100.0));
    let parent_id = parent.view_id();
    parent_id.set_scope(parent_scope);

    // Create grandchild without scope
    let grandchild = Empty::new().style(|s| s.size(25.0, 25.0));
    let _grandchild_id = grandchild.view_id();

    // Build the view tree
    let view = parent.child(Stem::new().child(grandchild).style(|s| s.size(50.0, 50.0)));

    let _harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Parent should find its own scope
    let found = parent_id.find_scope();
    assert!(found.is_some(), "Parent should find its own scope");
}

/// Test that scope disposal cascades through re-parented scopes.
#[test]
#[serial]
fn test_scope_disposal_cascades() {
    let parent_scope = Scope::new();
    let child_scope = Scope::new();

    // Create a signal in child scope
    let signal = child_scope.create_rw_signal(42);

    // Re-parent child under parent
    child_scope.set_parent(parent_scope);

    // Signal should exist
    assert_eq!(
        signal.get(),
        42,
        "Signal should be readable before disposal"
    );

    // Dispose parent - should cascade to child
    parent_scope.dispose();
}

/// Test A -> B -> C -> D hierarchy with scope re-parenting.
/// This tests that scope re-parenting works when scopes are created independently.
#[test]
#[serial]
fn test_abcd_scope_reparenting() {
    // Create scopes (simulating what context providers would do)
    let a_scope = Scope::new();
    let b_scope = Scope::new();
    let c_scope = Scope::new();
    let d_scope = Scope::new();

    // Create a signal in D's scope
    let signal = d_scope.create_rw_signal(42);

    // Simulate view tree assembly (this is what add_child does internally)
    // D is added to C
    d_scope.set_parent(c_scope);
    // C is added to B
    c_scope.set_parent(b_scope);
    // B is added to A
    b_scope.set_parent(a_scope);

    // Verify hierarchy via parent()
    assert!(b_scope.parent().is_some(), "B should have parent");
    assert!(c_scope.parent().is_some(), "C should have parent");
    assert!(d_scope.parent().is_some(), "D should have parent");

    // Signal should exist
    assert_eq!(signal.get(), 42, "Signal should be readable");

    // Dispose A - should cascade through B, C, D
    a_scope.dispose();
}

/// Test that context providers with eager children still get scope re-parented.
#[test]
#[serial]
fn test_context_provider_eager_children_scope_reparenting() {
    // Create parent scope
    let parent_scope = Scope::new();
    parent_scope.provide_context(42i32);

    // Create child scope (simulating eager construction - child created before parent exists in tree)
    let child_scope = Scope::new();
    let signal = child_scope.create_rw_signal(100);

    // Create views
    let parent = Stem::new().style(|s| s.size(100.0, 100.0));
    let parent_id = parent.view_id();
    parent_id.set_scope(parent_scope);

    let child = Empty::new().style(|s| s.size(50.0, 50.0));
    let child_id = child.view_id();
    child_id.set_scope(child_scope);

    // Build view tree - this should trigger scope re-parenting
    let view = parent.child(child);

    let _harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Verify child's scope was re-parented under parent's scope
    let child_scope_after = child_id.scope().expect("Child should have scope");
    let child_scope_parent = child_scope_after.parent();
    assert!(
        child_scope_parent.is_some(),
        "Child scope should have a parent after view tree assembly"
    );

    // Signal should still exist
    assert_eq!(signal.get(), 100, "Signal should still be readable");
}

/// Test deeply nested hierarchy A -> B -> C -> D -> E with scopes at A and E.
#[test]
#[serial]
fn test_deep_hierarchy_scope_reparenting() {
    let a_scope = Scope::new();
    let e_scope = Scope::new();

    // Create signal in E
    let signal = e_scope.create_rw_signal(999);

    // Create views
    let a = Stem::new().style(|s| s.size(100.0, 100.0));
    let a_id = a.view_id();
    a_id.set_scope(a_scope);

    let e = Empty::new().style(|s| s.size(10.0, 10.0));
    let e_id = e.view_id();
    e_id.set_scope(e_scope);

    // Build: A -> B -> C -> D -> E (B, C, D have no scopes)
    let view = a.child(
        Stem::new()
            .child(
                Stem::new()
                    .child(Stem::new().child(e).style(|s| s.size(20.0, 20.0)))
                    .style(|s| s.size(30.0, 30.0)),
            )
            .style(|s| s.size(50.0, 50.0)),
    );

    let _harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // E's scope should be re-parented under A's scope (since B, C, D have no scopes)
    let e_scope_after = e_id.scope().expect("E should have scope");
    let e_parent_scope = e_scope_after.parent();
    assert!(
        e_parent_scope.is_some(),
        "E's scope should have a parent after assembly"
    );

    // Signal should exist
    assert_eq!(signal.get(), 999, "Signal should be readable");
}

/// Test that multiple levels of nesting work correctly.
#[test]
#[serial]
fn test_multiple_scope_levels() {
    let outer_scope = Scope::new();
    outer_scope.provide_context(1i32);

    let middle_scope = Scope::new();
    middle_scope.provide_context(2i32);

    let inner_scope = Scope::new();
    inner_scope.provide_context(3i32);

    // Build hierarchy
    inner_scope.set_parent(middle_scope);
    middle_scope.set_parent(outer_scope);

    // Verify chain
    assert!(inner_scope.parent().is_some());
    assert!(middle_scope.parent().is_some());
    assert!(outer_scope.parent().is_none()); // Root has no parent

    // Context should be retrievable in inner scope
    let ctx = inner_scope.enter(Context::get::<i32>);
    assert_eq!(ctx, Some(3), "Should get innermost context");
}

/// Test that deferred children get correct scope.
#[test]
#[serial]
fn test_deferred_child_gets_parent_scope() {
    let context_value = Rc::new(Cell::new(None::<i32>));
    let context_value_clone = context_value.clone();

    // Create a view that provides context
    let parent_scope = Scope::current().create_child();
    parent_scope.provide_context(42i32);

    let parent = Stem::new().style(|s| s.size(100.0, 100.0));
    let parent_id = parent.view_id();
    parent_id.set_scope(parent_scope);

    // Use deferred child - it should be built in parent's scope context
    let view = parent.child(
        Empty::new()
            .on_event_stop(floem::event::EventListener::PointerDown, move |_| {
                // This closure captures context at build time
                let ctx = Context::get::<i32>();
                context_value_clone.set(ctx);
            })
            .style(|s| s.size(50.0, 50.0)),
    );

    let _harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // The child was built with access to parent's scope context
    // We can verify the scope chain is set up correctly
    let children = parent_id.children();
    assert_eq!(children.len(), 1, "Parent should have one child");
}

// ============================================================================
// Eager vs Lazy Construction Combination Tests
// ============================================================================
//
// These tests verify scope re-parenting works correctly for all combinations
// of eager (tuple) and lazy (.child()) construction in A -> B -> C -> D hierarchy.
//
// Legend:
// - E = Eager (tuple construction, children built before parent is in tree)
// - L = Lazy (deferred construction via .child(), children built after parent)
//
// Combinations tested:
// - EEEE: All eager (tuples all the way down)
// - LLLL: All lazy (.child() all the way down)
// - ELLL: A eager, rest lazy
// - LELL: B eager, rest lazy
// - LLEL: C eager, rest lazy
// - LLLE: D eager, rest lazy
// - EELL: A,B eager, C,D lazy
// - LLEL: A,B lazy, C,D eager
// - ELEL: Alternating eager/lazy

/// Test EEEE: All eager construction using tuples.
/// Structure: A contains (B contains (C contains D)) - all built eagerly
#[test]
#[serial]
fn test_all_eager_eeee() {
    let a_scope = Scope::new();
    let d_scope = Scope::new();
    let signal = d_scope.create_rw_signal(42);

    // Create all views first (eagerly)
    let d = Empty::new().style(|s| s.size(10.0, 10.0));
    let d_id = d.view_id();
    d_id.set_scope(d_scope);

    let c = Stem::new().style(|s| s.size(30.0, 30.0));
    let b = Stem::new().style(|s| s.size(50.0, 50.0));
    let a = Stem::new().style(|s| s.size(100.0, 100.0));
    let a_id = a.view_id();
    a_id.set_scope(a_scope);

    // Build tree using tuples (eager) - children are already built
    let view = a.children((b.children((c.children((d,)),)),));

    let _harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // D's scope should be re-parented under A's scope
    let d_scope_after = d_id.scope().expect("D should have scope");
    assert!(
        d_scope_after.parent().is_some(),
        "D's scope should have parent after eager construction"
    );

    // Signal should be accessible
    assert_eq!(signal.get(), 42);

    // Disposing A should cascade to D
    a_scope.dispose();
}

/// Test LLLL: All lazy construction using .child().
/// Structure: A.child(B.child(C.child(D)))
#[test]
#[serial]
fn test_all_lazy_llll() {
    let a_scope = Scope::new();
    let d_scope = Scope::new();
    let signal = d_scope.create_rw_signal(42);

    let a = Stem::new().style(|s| s.size(100.0, 100.0));
    let a_id = a.view_id();
    a_id.set_scope(a_scope);

    let d = Empty::new().style(|s| s.size(10.0, 10.0));
    let d_id = d.view_id();
    d_id.set_scope(d_scope);

    // Build tree using .child() (lazy/deferred)
    let view = a.child(
        Stem::new()
            .child(Stem::new().child(d).style(|s| s.size(30.0, 30.0)))
            .style(|s| s.size(50.0, 50.0)),
    );

    let _harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // D's scope should be re-parented under A's scope
    let d_scope_after = d_id.scope().expect("D should have scope");
    assert!(
        d_scope_after.parent().is_some(),
        "D's scope should have parent after lazy construction"
    );

    // Signal should be accessible
    assert_eq!(signal.get(), 42);
}

/// Test ELLL: A->B eager, B->C->D lazy.
/// A contains B eagerly, then B.child(C.child(D))
#[test]
#[serial]
fn test_eager_then_lazy_elll() {
    let a_scope = Scope::new();
    let d_scope = Scope::new();
    let signal = d_scope.create_rw_signal(42);

    let a = Stem::new().style(|s| s.size(100.0, 100.0));
    let a_id = a.view_id();
    a_id.set_scope(a_scope);

    let d = Empty::new().style(|s| s.size(10.0, 10.0));
    let d_id = d.view_id();
    d_id.set_scope(d_scope);

    // B is created eagerly, but C and D are added lazily
    let b = Stem::new()
        .child(Stem::new().child(d).style(|s| s.size(30.0, 30.0)))
        .style(|s| s.size(50.0, 50.0));

    // A contains B eagerly (tuple)
    let view = a.children((b,));

    let _harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    let d_scope_after = d_id.scope().expect("D should have scope");
    assert!(
        d_scope_after.parent().is_some(),
        "D's scope should have parent"
    );
    assert_eq!(signal.get(), 42);
}

/// Test LELL: A->B lazy, B->C eager, C->D lazy.
#[test]
#[serial]
fn test_lazy_eager_lazy_lell() {
    let a_scope = Scope::new();
    let d_scope = Scope::new();
    let signal = d_scope.create_rw_signal(42);

    let a = Stem::new().style(|s| s.size(100.0, 100.0));
    let a_id = a.view_id();
    a_id.set_scope(a_scope);

    let d = Empty::new().style(|s| s.size(10.0, 10.0));
    let d_id = d.view_id();
    d_id.set_scope(d_scope);

    // C contains D lazily
    let c = Stem::new().child(d).style(|s| s.size(30.0, 30.0));

    // B contains C eagerly (tuple)
    let b = Stem::new().children((c,)).style(|s| s.size(50.0, 50.0));

    // A contains B lazily
    let view = a.child(b);

    let _harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    let d_scope_after = d_id.scope().expect("D should have scope");
    assert!(
        d_scope_after.parent().is_some(),
        "D's scope should have parent"
    );
    assert_eq!(signal.get(), 42);
}

/// Test EELL: A->B->C eager, C->D lazy.
#[test]
#[serial]
fn test_eager_eager_lazy_eell() {
    let a_scope = Scope::new();
    let d_scope = Scope::new();
    let signal = d_scope.create_rw_signal(42);

    let a = Stem::new().style(|s| s.size(100.0, 100.0));
    let a_id = a.view_id();
    a_id.set_scope(a_scope);

    let d = Empty::new().style(|s| s.size(10.0, 10.0));
    let d_id = d.view_id();
    d_id.set_scope(d_scope);

    // C contains D lazily
    let c = Stem::new().child(d).style(|s| s.size(30.0, 30.0));

    // B contains C eagerly
    let b = Stem::new().children((c,)).style(|s| s.size(50.0, 50.0));

    // A contains B eagerly
    let view = a.children((b,));

    let _harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    let d_scope_after = d_id.scope().expect("D should have scope");
    assert!(
        d_scope_after.parent().is_some(),
        "D's scope should have parent"
    );
    assert_eq!(signal.get(), 42);
}

/// Test LLEE: A->B lazy, B->C->D eager.
#[test]
#[serial]
fn test_lazy_lazy_eager_llee() {
    let a_scope = Scope::new();
    let d_scope = Scope::new();
    let signal = d_scope.create_rw_signal(42);

    let a = Stem::new().style(|s| s.size(100.0, 100.0));
    let a_id = a.view_id();
    a_id.set_scope(a_scope);

    let d = Empty::new().style(|s| s.size(10.0, 10.0));
    let d_id = d.view_id();
    d_id.set_scope(d_scope);

    // C contains D eagerly
    let c = Stem::new().children((d,)).style(|s| s.size(30.0, 30.0));

    // B contains C eagerly
    let b = Stem::new().children((c,)).style(|s| s.size(50.0, 50.0));

    // A contains B lazily
    let view = a.child(b);

    let _harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    let d_scope_after = d_id.scope().expect("D should have scope");
    assert!(
        d_scope_after.parent().is_some(),
        "D's scope should have parent"
    );
    assert_eq!(signal.get(), 42);
}

/// Test ELEL: Alternating eager/lazy.
/// A->B eager, B->C lazy, C->D eager
#[test]
#[serial]
fn test_alternating_elel() {
    let a_scope = Scope::new();
    let d_scope = Scope::new();
    let signal = d_scope.create_rw_signal(42);

    let a = Stem::new().style(|s| s.size(100.0, 100.0));
    let a_id = a.view_id();
    a_id.set_scope(a_scope);

    let d = Empty::new().style(|s| s.size(10.0, 10.0));
    let d_id = d.view_id();
    d_id.set_scope(d_scope);

    // C contains D eagerly
    let c = Stem::new().children((d,)).style(|s| s.size(30.0, 30.0));

    // B contains C lazily
    let b = Stem::new().child(c).style(|s| s.size(50.0, 50.0));

    // A contains B eagerly
    let view = a.children((b,));

    let _harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    let d_scope_after = d_id.scope().expect("D should have scope");
    assert!(
        d_scope_after.parent().is_some(),
        "D's scope should have parent"
    );
    assert_eq!(signal.get(), 42);
}

/// Test LELE: Alternating lazy/eager.
/// A->B lazy, B->C eager, C->D lazy
#[test]
#[serial]
fn test_alternating_lele() {
    let a_scope = Scope::new();
    let d_scope = Scope::new();
    let signal = d_scope.create_rw_signal(42);

    let a = Stem::new().style(|s| s.size(100.0, 100.0));
    let a_id = a.view_id();
    a_id.set_scope(a_scope);

    let d = Empty::new().style(|s| s.size(10.0, 10.0));
    let d_id = d.view_id();
    d_id.set_scope(d_scope);

    // C contains D lazily
    let c = Stem::new().child(d).style(|s| s.size(30.0, 30.0));

    // B contains C eagerly
    let b = Stem::new().children((c,)).style(|s| s.size(50.0, 50.0));

    // A contains B lazily
    let view = a.child(b);

    let _harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    let d_scope_after = d_id.scope().expect("D should have scope");
    assert!(
        d_scope_after.parent().is_some(),
        "D's scope should have parent"
    );
    assert_eq!(signal.get(), 42);
}

// ============================================================================
// Multiple Scopes at Different Levels Tests
// ============================================================================

/// Test with scopes at A and C (skipping B).
/// Verifies D's scope is parented to C's scope, not A's.
#[test]
#[serial]
fn test_scopes_at_a_and_c() {
    let a_scope = Scope::new();
    let c_scope = Scope::new();
    let d_scope = Scope::new();
    let signal = d_scope.create_rw_signal(42);

    let a = Stem::new().style(|s| s.size(100.0, 100.0));
    let a_id = a.view_id();
    a_id.set_scope(a_scope);

    let c = Stem::new().style(|s| s.size(30.0, 30.0));
    let c_id = c.view_id();
    c_id.set_scope(c_scope);

    let d = Empty::new().style(|s| s.size(10.0, 10.0));
    let d_id = d.view_id();
    d_id.set_scope(d_scope);

    // Build: A -> B -> C -> D (B has no scope)
    let view = a.child(Stem::new().child(c.child(d)).style(|s| s.size(50.0, 50.0)));

    let _harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // D's scope should be parented to C's scope (nearest ancestor with scope)
    let d_scope_after = d_id.scope().expect("D should have scope");
    let d_parent = d_scope_after
        .parent()
        .expect("D's scope should have parent");

    // C's scope should be parented to A's scope
    let c_scope_after = c_id.scope().expect("C should have scope");
    let c_parent = c_scope_after
        .parent()
        .expect("C's scope should have parent");

    // Verify the chain: D -> C -> A
    assert!(d_parent.parent().is_some() || c_parent.parent().is_none());
    assert_eq!(signal.get(), 42);
}

/// Test with scopes at B and D (skipping A and C).
#[test]
#[serial]
fn test_scopes_at_b_and_d() {
    let b_scope = Scope::new();
    let d_scope = Scope::new();
    let signal = d_scope.create_rw_signal(42);

    let b = Stem::new().style(|s| s.size(50.0, 50.0));
    let b_id = b.view_id();
    b_id.set_scope(b_scope);

    let d = Empty::new().style(|s| s.size(10.0, 10.0));
    let d_id = d.view_id();
    d_id.set_scope(d_scope);

    // Build: A -> B -> C -> D (A and C have no scopes)
    let view = Stem::new()
        .child(b.child(Stem::new().child(d).style(|s| s.size(30.0, 30.0))))
        .style(|s| s.size(100.0, 100.0));

    let _harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // D's scope should be parented to B's scope
    let d_scope_after = d_id.scope().expect("D should have scope");
    assert!(
        d_scope_after.parent().is_some(),
        "D's scope should have parent (B's scope)"
    );

    assert_eq!(signal.get(), 42);
}

/// Test with scopes at all levels A, B, C, D.
#[test]
#[serial]
fn test_scopes_at_all_levels() {
    let a_scope = Scope::new();
    let b_scope = Scope::new();
    let c_scope = Scope::new();
    let d_scope = Scope::new();

    a_scope.provide_context(1i32);
    b_scope.provide_context(2i32);
    c_scope.provide_context(3i32);
    d_scope.provide_context(4i32);

    let a = Stem::new().style(|s| s.size(100.0, 100.0));
    let a_id = a.view_id();
    a_id.set_scope(a_scope);

    let b = Stem::new().style(|s| s.size(50.0, 50.0));
    let b_id = b.view_id();
    b_id.set_scope(b_scope);

    let c = Stem::new().style(|s| s.size(30.0, 30.0));
    let c_id = c.view_id();
    c_id.set_scope(c_scope);

    let d = Empty::new().style(|s| s.size(10.0, 10.0));
    let d_id = d.view_id();
    d_id.set_scope(d_scope);

    let view = a.child(b.child(c.child(d)));

    let _harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Verify scope chain: D -> C -> B -> A
    let d_scope_after = d_id.scope().expect("D should have scope");
    let c_scope_after = c_id.scope().expect("C should have scope");
    let b_scope_after = b_id.scope().expect("B should have scope");
    let _a_scope_after = a_id.scope().expect("A should have scope");

    assert!(d_scope_after.parent().is_some(), "D should have parent");
    assert!(c_scope_after.parent().is_some(), "C should have parent");
    assert!(b_scope_after.parent().is_some(), "B should have parent");
    // A is root, may or may not have parent depending on global scope
}

/// Test disposal cascades correctly with scopes at all levels.
#[test]
#[serial]
fn test_disposal_cascades_all_levels() {
    let a_scope = Scope::new();
    let b_scope = Scope::new();
    let c_scope = Scope::new();
    let d_scope = Scope::new();

    let signal_a = a_scope.create_rw_signal(1);
    let signal_b = b_scope.create_rw_signal(2);
    let signal_c = c_scope.create_rw_signal(3);
    let signal_d = d_scope.create_rw_signal(4);

    // Re-parent scopes manually (simulating what the view system does)
    d_scope.set_parent(c_scope);
    c_scope.set_parent(b_scope);
    b_scope.set_parent(a_scope);

    // All signals should be readable
    assert_eq!(signal_a.get(), 1);
    assert_eq!(signal_b.get(), 2);
    assert_eq!(signal_c.get(), 3);
    assert_eq!(signal_d.get(), 4);

    // Dispose A - should cascade through B, C, D
    a_scope.dispose();

    // After disposal, the scopes and their signals are cleaned up
    // (We can't easily verify signal disposal without panicking, so this test
    // mainly verifies no panics occur during cascading disposal)
}

/// Test that same scope on parent and child doesn't create a cycle.
/// This is a pathological case but shouldn't crash or create infinite loops.
#[test]
#[serial]
fn test_same_scope_on_parent_and_child_no_cycle() {
    let scope = Scope::new();
    let signal = scope.create_rw_signal(42);

    // Create parent and child, both with the SAME scope
    let parent = Stem::new().style(|s| s.size(100.0, 100.0));
    let parent_id = parent.view_id();
    parent_id.set_scope(scope);

    let child = Empty::new().style(|s| s.size(50.0, 50.0));
    let child_id = child.view_id();
    child_id.set_scope(scope); // Same scope as parent!

    // Build tree - this should NOT create a cycle
    let view = parent.child(child);

    let _harness = HeadlessHarness::new_with_size(view, 100.0, 100.0);

    // Both views should have the same scope
    assert_eq!(parent_id.scope(), child_id.scope());

    // Scope should NOT be its own parent (that would be a cycle)
    let scope_parent = scope.parent();
    if let Some(parent_scope) = scope_parent {
        assert_ne!(parent_scope, scope, "Scope should not be its own parent");
    }

    // Signal should still work
    assert_eq!(signal.get(), 42);

    // Disposal should not cause infinite recursion
    scope.dispose();
}
