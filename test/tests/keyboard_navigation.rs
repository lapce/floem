//! Keyboard focus navigation tests.
//!
//! These tests verify:
//! - Linear tab traversal (Tab / Shift+Tab)
//! - Initial tab target behavior when no element is focused
//! - Wrap behavior for tab traversal
//! - Directional navigation via Alt+Arrow
//! - Group-priority behavior for traversal

use floem::event::Event;
use floem::peniko::Brush;
use floem::prelude::*;
use floem::style::Background;
use floem::{FocusNavMeta, understory_focus::FocusSymbol};
use floem_test::prelude::*;
use std::{cell::RefCell, rc::Rc};
use ui_events::keyboard::{Code, Key, KeyState, KeyboardEvent, Location, Modifiers, NamedKey};

fn key_event_named(named: NamedKey, code: Code, modifiers: Modifiers) -> Event {
    Event::Key(KeyboardEvent {
        key: Key::Named(named),
        code,
        modifiers,
        location: Location::Standard,
        is_composing: false,
        repeat: false,
        state: KeyState::Down,
    })
}

fn press_tab(harness: &mut HeadlessHarness) {
    harness.dispatch_event(key_event_named(
        NamedKey::Tab,
        Code::Tab,
        Modifiers::default(),
    ));
}

fn press_shift_tab(harness: &mut HeadlessHarness) {
    harness.dispatch_event(key_event_named(NamedKey::Tab, Code::Tab, Modifiers::SHIFT));
}

fn press_alt_arrow(harness: &mut HeadlessHarness, key: NamedKey, code: Code) {
    harness.dispatch_event(key_event_named(key, code, Modifiers::ALT));
}

fn background_is(harness: &HeadlessHarness, id: ViewId, color: floem::peniko::Color) -> bool {
    let style = harness.get_computed_style(id);
    matches!(style.get(Background), Some(Brush::Solid(c)) if c == color)
}

#[test]
fn tab_from_no_focus_starts_at_first() {
    let root = TestRoot::new();
    let a = Empty::new().style(|s| s.size(80.0, 40.0).keyboard_navigable());
    let a_id = a.view_id();
    let b = Empty::new().style(|s| s.size(80.0, 40.0).keyboard_navigable());
    let c = Empty::new().style(|s| s.size(80.0, 40.0).keyboard_navigable());
    let view = Stack::horizontal((a, b, c)).style(|s| s.size(300.0, 40.0));

    let mut harness = HeadlessHarness::new_with_size(root, view, 300.0, 40.0);
    press_tab(&mut harness);

    assert!(
        harness.is_focused(a_id),
        "Tab from no focus should pick first"
    );
}

#[test]
fn shift_tab_from_no_focus_starts_at_last() {
    let root = TestRoot::new();
    let a = Empty::new().style(|s| s.size(80.0, 40.0).keyboard_navigable());
    let b = Empty::new().style(|s| s.size(80.0, 40.0).keyboard_navigable());
    let c = Empty::new().style(|s| s.size(80.0, 40.0).keyboard_navigable());
    let c_id = c.view_id();
    let view = Stack::horizontal((a, b, c)).style(|s| s.size(300.0, 40.0));

    let mut harness = HeadlessHarness::new_with_size(root, view, 300.0, 40.0);
    press_shift_tab(&mut harness);

    assert!(
        harness.is_focused(c_id),
        "Shift+Tab from no focus should pick last"
    );
}

#[test]
fn tab_moves_forward_and_shift_tab_moves_backward() {
    let root = TestRoot::new();
    let a = Empty::new().style(|s| s.size(80.0, 40.0).keyboard_navigable());
    let a_id = a.view_id();
    let b = Empty::new().style(|s| s.size(80.0, 40.0).keyboard_navigable());
    let b_id = b.view_id();
    let c = Empty::new().style(|s| s.size(80.0, 40.0).keyboard_navigable());
    let c_id = c.view_id();
    let view = Stack::horizontal((a, b, c)).style(|s| s.size(300.0, 40.0));

    let mut harness = HeadlessHarness::new_with_size(root, view, 300.0, 40.0);

    harness.click(40.0, 20.0);
    assert!(harness.is_focused(a_id), "a should be focused after click");

    press_tab(&mut harness);
    assert!(harness.is_focused(b_id), "Tab should move a -> b");

    press_tab(&mut harness);
    assert!(harness.is_focused(c_id), "Tab should move b -> c");

    press_shift_tab(&mut harness);
    assert!(harness.is_focused(b_id), "Shift+Tab should move c -> b");
}

#[test]
fn tab_wraps_from_last_to_first() {
    let root = TestRoot::new();
    let a = Empty::new().style(|s| s.size(80.0, 40.0).keyboard_navigable());
    let a_id = a.view_id();
    let b = Empty::new().style(|s| s.size(80.0, 40.0).keyboard_navigable());
    let c = Empty::new().style(|s| s.size(80.0, 40.0).keyboard_navigable());
    let c_id = c.view_id();
    let view = Stack::horizontal((a, b, c)).style(|s| s.size(300.0, 40.0));

    let mut harness = HeadlessHarness::new_with_size(root, view, 300.0, 40.0);

    harness.click(200.0, 20.0);
    assert!(harness.is_focused(c_id), "c should be focused after click");

    press_tab(&mut harness);
    assert!(harness.is_focused(a_id), "Tab should wrap c -> a");
}

#[test]
fn alt_arrow_moves_directionally() {
    let root = TestRoot::new();
    let left = Empty::new().style(|s| s.size(100.0, 40.0).keyboard_navigable());
    let left_id = left.view_id();
    let mid = Empty::new().style(|s| s.size(100.0, 40.0).keyboard_navigable());
    let mid_id = mid.view_id();
    let right = Empty::new().style(|s| s.size(100.0, 40.0).keyboard_navigable());
    let right_id = right.view_id();
    let view = Stack::horizontal((left, mid, right)).style(|s| s.size(300.0, 40.0));

    let mut harness = HeadlessHarness::new_with_size(root, view, 300.0, 40.0);

    harness.click(50.0, 20.0);
    assert!(
        harness.is_focused(left_id),
        "left should be focused after click"
    );

    press_alt_arrow(&mut harness, NamedKey::ArrowRight, Code::ArrowRight);
    assert!(
        harness.is_focused(mid_id),
        "Alt+Right should move left -> mid"
    );

    press_alt_arrow(&mut harness, NamedKey::ArrowRight, Code::ArrowRight);
    assert!(
        harness.is_focused(right_id),
        "Alt+Right should move mid -> right"
    );

    press_alt_arrow(&mut harness, NamedKey::ArrowLeft, Code::ArrowLeft);
    assert!(
        harness.is_focused(mid_id),
        "Alt+Left should move right -> mid"
    );
}

#[test]
fn arrow_without_alt_does_not_trigger_arrow_navigation() {
    let root = TestRoot::new();
    let left = Empty::new().style(|s| s.size(100.0, 40.0).keyboard_navigable());
    let left_id = left.view_id();
    let right = Empty::new().style(|s| s.size(100.0, 40.0).keyboard_navigable());
    let right_id = right.view_id();
    let view = Stack::horizontal((left, right)).style(|s| s.size(200.0, 40.0));

    let mut harness = HeadlessHarness::new_with_size(root, view, 200.0, 40.0);

    harness.click(50.0, 20.0);
    assert!(
        harness.is_focused(left_id),
        "left should be focused after click"
    );

    harness.dispatch_event(key_event_named(
        NamedKey::ArrowRight,
        Code::ArrowRight,
        Modifiers::default(),
    ));
    assert!(
        harness.is_focused(left_id),
        "ArrowRight without Alt should not move focus"
    );
    assert!(!harness.is_focused(right_id));
}

#[test]
fn shift_tab_wraps_from_first_to_last() {
    let root = TestRoot::new();
    let a = Empty::new().style(|s| s.size(80.0, 40.0).keyboard_navigable());
    let a_id = a.view_id();
    let b = Empty::new().style(|s| s.size(80.0, 40.0).keyboard_navigable());
    let c = Empty::new().style(|s| s.size(80.0, 40.0).keyboard_navigable());
    let c_id = c.view_id();
    let view = Stack::horizontal((a, b, c)).style(|s| s.size(300.0, 40.0));

    let mut harness = HeadlessHarness::new_with_size(root, view, 300.0, 40.0);
    harness.click(40.0, 20.0);
    assert!(harness.is_focused(a_id));

    press_shift_tab(&mut harness);
    assert!(
        harness.is_focused(c_id),
        "Shift+Tab should wrap first -> last"
    );
}

#[test]
fn tab_with_single_focusable_stays_on_same_element() {
    let root = TestRoot::new();
    let only = Empty::new().style(|s| s.size(100.0, 40.0).keyboard_navigable());
    let only_id = only.view_id();
    let view = Stack::horizontal((
        only,
        Empty::new().style(|s| s.size(100.0, 40.0).focus_none()),
    ))
    .style(|s| s.size(200.0, 40.0));

    let mut harness = HeadlessHarness::new_with_size(root, view, 200.0, 40.0);
    harness.click(50.0, 20.0);
    assert!(harness.is_focused(only_id));

    press_tab(&mut harness);
    assert!(
        harness.is_focused(only_id),
        "Single candidate should wrap to self"
    );
}

#[test]
fn tab_skips_hidden_elements() {
    let root = TestRoot::new();
    let a = Empty::new().style(|s| s.size(80.0, 40.0).keyboard_navigable());
    let a_id = a.view_id();
    let hidden = Empty::new().style(|s| s.size(80.0, 40.0).keyboard_navigable().hide());
    let hidden_id = hidden.view_id();
    let c = Empty::new().style(|s| s.size(80.0, 40.0).keyboard_navigable());
    let c_id = c.view_id();
    let view = Stack::horizontal((a, hidden, c)).style(|s| s.size(300.0, 40.0));

    let mut harness = HeadlessHarness::new_with_size(root, view, 300.0, 40.0);
    harness.click(40.0, 20.0);
    assert!(harness.is_focused(a_id));

    press_tab(&mut harness);
    assert!(
        harness.is_focused(c_id),
        "Tab should skip hidden keyboard-navigable entries"
    );
    assert!(!harness.is_focused(hidden_id));
}

#[test]
fn tab_skips_disabled_elements() {
    let root = TestRoot::new();
    let a = Empty::new().style(|s| s.size(80.0, 40.0).keyboard_navigable());
    let a_id = a.view_id();
    let disabled =
        Empty::new().style(|s| s.size(80.0, 40.0).keyboard_navigable().set_disabled(true));
    let disabled_id = disabled.view_id();
    let c = Empty::new().style(|s| s.size(80.0, 40.0).keyboard_navigable());
    let c_id = c.view_id();
    let view = Stack::horizontal((a, disabled, c)).style(|s| s.size(300.0, 40.0));

    let mut harness = HeadlessHarness::new_with_size(root, view, 300.0, 40.0);
    harness.click(40.0, 20.0);
    assert!(harness.is_focused(a_id));

    press_tab(&mut harness);
    assert!(
        harness.is_focused(c_id),
        "Tab should skip disabled keyboard-navigable entries"
    );
    assert!(!harness.is_focused(disabled_id));
}

#[test]
fn explicit_focus_order_metadata_roundtrip_on_view_elements() {
    let root = TestRoot::new();
    let left = Empty::new().style(|s| s.size(90.0, 40.0).keyboard_navigable());
    let left_id = left.view_id();
    let middle = Empty::new().style(|s| s.size(90.0, 40.0).keyboard_navigable());
    let middle_id = middle.view_id();
    let right = Empty::new().style(|s| s.size(90.0, 40.0).keyboard_navigable());
    let right_id = right.view_id();
    let view = Stack::horizontal((left, middle, right)).style(|s| s.size(300.0, 40.0));
    let mut harness = HeadlessHarness::new_with_size(root, view, 300.0, 40.0);

    assert!(left_id.set_focus_nav_meta_for_element(
        left_id.get_element_id(),
        FocusNavMeta {
            order: Some(20),
            ..Default::default()
        },
    ));
    assert!(middle_id.set_focus_nav_meta_for_element(
        middle_id.get_element_id(),
        FocusNavMeta {
            order: Some(30),
            ..Default::default()
        },
    ));
    assert!(right_id.set_focus_nav_meta_for_element(
        right_id.get_element_id(),
        FocusNavMeta {
            order: Some(10),
            ..Default::default()
        },
    ));
    harness.process_update_no_paint();

    let left_meta = left_id
        .focus_nav_meta_for_element(left_id.get_element_id())
        .expect("left metadata should exist");
    let right_meta = right_id
        .focus_nav_meta_for_element(right_id.get_element_id())
        .expect("right metadata should exist");
    assert_eq!(left_meta.order, Some(20));
    assert_eq!(right_meta.order, Some(10));
}

#[test]
fn tab_from_no_focus_ignores_last_focused_history() {
    let root = TestRoot::new();
    let a = Empty::new().style(|s| s.size(80.0, 40.0).keyboard_navigable());
    let a_id = a.view_id();
    let b = Empty::new().style(|s| s.size(80.0, 40.0).keyboard_navigable());
    let b_id = b.view_id();
    let c = Empty::new().style(|s| s.size(80.0, 40.0).keyboard_navigable());
    let view = Stack::horizontal((a, b, c)).style(|s| s.size(300.0, 40.0));
    let mut harness = HeadlessHarness::new_with_size(root, view, 300.0, 40.0);

    harness.click(120.0, 20.0);
    assert!(harness.is_focused(b_id));
    b_id.clear_focus();
    harness.process_update_no_paint();

    press_tab(&mut harness);
    assert!(
        harness.is_focused(a_id),
        "Tab from no active focus should start at first, not prior focus"
    );
}

#[test]
fn arrow_from_no_focus_uses_last_focused_before_pointer_origin() {
    let root = TestRoot::new();
    let left = Empty::new().style(|s| s.size(100.0, 40.0).keyboard_navigable());
    let left_id = left.view_id();
    let mid = Empty::new().style(|s| s.size(100.0, 40.0).keyboard_navigable());
    let mid_id = mid.view_id();
    let right = Empty::new().style(|s| s.size(100.0, 40.0).keyboard_navigable());
    let right_id = right.view_id();
    let view = Stack::horizontal((left, mid, right)).style(|s| s.size(300.0, 40.0));
    let mut harness = HeadlessHarness::new_with_size(root, view, 300.0, 40.0);

    harness.click(150.0, 20.0);
    assert!(harness.is_focused(mid_id));
    mid_id.clear_focus();
    harness.process_update_no_paint();

    // Move pointer to left element; origin should still prefer last focused (mid).
    harness.pointer_move(20.0, 20.0);
    press_alt_arrow(&mut harness, NamedKey::ArrowRight, Code::ArrowRight);
    assert!(
        harness.is_focused(right_id),
        "Arrow origin should prefer last focused over pointer hit when no active focus"
    );
    assert!(!harness.is_focused(left_id));
}

#[test]
fn arrow_navigation_in_grid_prefers_same_row() {
    let root = TestRoot::new();
    let a = Empty::new().style(|s| s.size(100.0, 40.0).keyboard_navigable());
    let a_id = a.view_id();
    let b = Empty::new().style(|s| s.size(100.0, 40.0).keyboard_navigable());
    let b_id = b.view_id();
    let c = Empty::new().style(|s| s.size(100.0, 40.0).keyboard_navigable());
    let c_id = c.view_id();
    let d = Empty::new().style(|s| s.size(100.0, 40.0).keyboard_navigable());
    let d_id = d.view_id();
    let row1 = Stack::horizontal((a, b)).style(|s| s.size(200.0, 40.0));
    let row2 = Stack::horizontal((c, d)).style(|s| s.size(200.0, 40.0));
    let view = Stack::vertical((row1, row2)).style(|s| s.size(200.0, 100.0));
    let mut harness = HeadlessHarness::new_with_size(root, view, 200.0, 100.0);

    harness.click(50.0, 20.0);
    assert!(harness.is_focused(a_id));

    press_alt_arrow(&mut harness, NamedKey::ArrowRight, Code::ArrowRight);
    assert!(
        harness.is_focused(b_id),
        "Alt+Right from top-left should choose top-right first"
    );

    press_alt_arrow(&mut harness, NamedKey::ArrowDown, Code::ArrowDown);
    assert!(
        harness.is_focused(d_id) || harness.is_focused(c_id),
        "Alt+Down should move into next row"
    );
}

#[test]
fn group_metadata_roundtrip_on_element() {
    const GROUP_A: FocusSymbol = FocusSymbol(501);
    let root = TestRoot::new();
    let a = Empty::new().style(|s| s.size(80.0, 40.0).keyboard_navigable());
    let a_id = a.view_id();
    let view = Stack::horizontal((a,)).style(|s| s.size(80.0, 40.0));
    let mut harness = HeadlessHarness::new_with_size(root, view, 80.0, 40.0);
    harness.rebuild();

    let element = a_id.get_element_id();
    assert!(a_id.set_focus_group_for_element(element, Some(GROUP_A)));
    let meta = a_id
        .focus_nav_meta_for_element(element)
        .expect("element should have nav metadata");
    assert_eq!(meta.group, Some(GROUP_A));
}

#[test]
fn tab_group_preference_moves_within_group_first() {
    const GROUP_A: FocusSymbol = FocusSymbol(600);
    const GROUP_B: FocusSymbol = FocusSymbol(601);

    let root = TestRoot::new();
    let a1 = Empty::new().style(|s| s.size(70.0, 40.0).keyboard_navigable());
    let a1_id = a1.view_id();
    let b1 = Empty::new().style(|s| s.size(70.0, 40.0).keyboard_navigable());
    let b1_id = b1.view_id();
    let a2 = Empty::new().style(|s| s.size(70.0, 40.0).keyboard_navigable());
    let a2_id = a2.view_id();
    let b2 = Empty::new().style(|s| s.size(70.0, 40.0).keyboard_navigable());
    let b2_id = b2.view_id();
    let view = Stack::horizontal((a1, b1, a2, b2)).style(|s| s.size(280.0, 40.0));
    let mut harness = HeadlessHarness::new_with_size(root, view, 280.0, 40.0);

    assert!(a1_id.set_focus_group_for_element(a1_id.get_element_id(), Some(GROUP_A)));
    assert!(a2_id.set_focus_group_for_element(a2_id.get_element_id(), Some(GROUP_A)));
    assert!(b1_id.set_focus_group_for_element(b1_id.get_element_id(), Some(GROUP_B)));
    assert!(b2_id.set_focus_group_for_element(b2_id.get_element_id(), Some(GROUP_B)));
    harness.process_update_no_paint();

    harness.click(35.0, 20.0);
    assert!(harness.is_focused(a1_id));
    press_tab(&mut harness);
    assert!(
        harness.is_focused(a2_id),
        "Tab should prefer same-group candidate before global traversal"
    );
}

#[test]
fn arrow_group_preference_moves_within_group_first() {
    const GROUP_A: FocusSymbol = FocusSymbol(700);
    const GROUP_B: FocusSymbol = FocusSymbol(701);

    let root = TestRoot::new();
    let a1 = Empty::new().style(|s| s.size(70.0, 40.0).keyboard_navigable());
    let a1_id = a1.view_id();
    let b1 = Empty::new().style(|s| s.size(70.0, 40.0).keyboard_navigable());
    let b1_id = b1.view_id();
    let a2 = Empty::new().style(|s| s.size(70.0, 40.0).keyboard_navigable());
    let a2_id = a2.view_id();
    let b2 = Empty::new().style(|s| s.size(70.0, 40.0).keyboard_navigable());
    let b2_id = b2.view_id();
    let view = Stack::horizontal((a1, b1, a2, b2)).style(|s| s.size(280.0, 40.0));
    let mut harness = HeadlessHarness::new_with_size(root, view, 280.0, 40.0);

    assert!(a1_id.set_focus_group_for_element(a1_id.get_element_id(), Some(GROUP_A)));
    assert!(a2_id.set_focus_group_for_element(a2_id.get_element_id(), Some(GROUP_A)));
    assert!(b1_id.set_focus_group_for_element(b1_id.get_element_id(), Some(GROUP_B)));
    assert!(b2_id.set_focus_group_for_element(b2_id.get_element_id(), Some(GROUP_B)));
    harness.process_update_no_paint();

    harness.click(35.0, 20.0);
    assert!(harness.is_focused(a1_id));
    press_alt_arrow(&mut harness, NamedKey::ArrowRight, Code::ArrowRight);
    assert!(
        harness.is_focused(a2_id),
        "Alt+Right should prefer same-group candidate first"
    );
}

#[test]
fn custom_element_metadata_roundtrip_fields() {
    const GROUP_X: FocusSymbol = FocusSymbol(801);
    const HINT_GRID: FocusSymbol = FocusSymbol(802);

    let root = TestRoot::new();
    let owner = Empty::new().style(|s| s.size(200.0, 60.0).focus_none());
    let owner_id = owner.view_id();
    let mut harness = HeadlessHarness::new_with_size(root, owner, 200.0, 60.0);
    harness.rebuild();

    let element = owner_id.create_child_element_id(0);
    let meta = FocusNavMeta {
        order: Some(17),
        group: Some(GROUP_X),
        policy_hint: Some(HINT_GRID),
        scope_depth: 3,
        autofocus: true,
        enabled: false,
    };
    assert!(owner_id.set_focus_nav_meta_for_element(element, meta));
    let read = owner_id
        .focus_nav_meta_for_element(element)
        .expect("custom element should have nav metadata");
    assert_eq!(read.order, Some(17));
    assert_eq!(read.group, Some(GROUP_X));
    assert_eq!(read.policy_hint, Some(HINT_GRID));
    assert_eq!(read.scope_depth, 3);
    assert!(read.autofocus);
    assert!(!read.enabled);
}

#[test]
fn custom_element_group_and_order_roundtrip() {
    const GROUP: FocusSymbol = FocusSymbol(850);
    let root = TestRoot::new();
    let owner = Empty::new().style(|s| s.size(200.0, 40.0).keyboard_navigable());
    let owner_id = owner.view_id();
    let mut harness = HeadlessHarness::new_with_size(root, owner, 200.0, 40.0);
    harness.rebuild();

    let custom = owner_id.create_child_element_id(0);
    assert!(owner_id.set_focus_nav_meta_for_element(
        custom,
        FocusNavMeta {
            order: Some(99),
            group: Some(GROUP),
            ..Default::default()
        }
    ));
    let read = owner_id
        .focus_nav_meta_for_element(custom)
        .expect("custom element metadata should exist");
    assert_eq!(read.order, Some(99));
    assert_eq!(read.group, Some(GROUP));
}

#[test]
fn tab_focus_style_restyles_old_and_new_views() {
    let root = TestRoot::new();
    let a = Empty::new().style(|s| {
        s.size(100.0, 40.0)
            .keyboard_navigable()
            .background(palette::css::BLUE)
            .focus(|s| s.background(palette::css::YELLOW))
    });
    let a_id = a.view_id();
    let b = Empty::new().style(|s| {
        s.size(100.0, 40.0)
            .keyboard_navigable()
            .background(palette::css::GREEN)
            .focus(|s| s.background(palette::css::ORANGE))
    });
    let b_id = b.view_id();
    let view = Stack::horizontal((a, b)).style(|s| s.size(200.0, 40.0));
    let mut harness = HeadlessHarness::new_with_size(root, view, 200.0, 40.0);

    harness.click(40.0, 20.0);
    assert!(harness.is_focused(a_id));
    assert!(background_is(&harness, a_id, palette::css::YELLOW));
    assert!(background_is(&harness, b_id, palette::css::GREEN));

    press_tab(&mut harness);
    assert!(harness.is_focused(b_id));
    assert!(
        background_is(&harness, a_id, palette::css::BLUE),
        "old focused view should be restyled to non-focus state after Tab"
    );
    assert!(
        background_is(&harness, b_id, palette::css::ORANGE),
        "new focused view should be restyled to focus state after Tab"
    );
}

#[test]
fn tab_focus_visible_restyles_old_and_new_views() {
    let root = TestRoot::new();
    let a = Empty::new().style(|s| {
        s.size(100.0, 40.0)
            .keyboard_navigable()
            .background(palette::css::BLUE)
            .focus_visible(|s| s.background(palette::css::YELLOW))
    });
    let a_id = a.view_id();
    let b = Empty::new().style(|s| {
        s.size(100.0, 40.0)
            .keyboard_navigable()
            .background(palette::css::GREEN)
            .focus_visible(|s| s.background(palette::css::ORANGE))
    });
    let b_id = b.view_id();
    let view = Stack::horizontal((a, b)).style(|s| s.size(200.0, 40.0));
    let mut harness = HeadlessHarness::new_with_size(root, view, 200.0, 40.0);

    press_tab(&mut harness);
    assert!(harness.is_focused(a_id));
    assert!(
        background_is(&harness, a_id, palette::css::YELLOW),
        "Tab should apply focus-visible style to first focused view"
    );
    assert!(background_is(&harness, b_id, palette::css::GREEN));

    press_tab(&mut harness);
    assert!(harness.is_focused(b_id));
    assert!(
        background_is(&harness, a_id, palette::css::BLUE),
        "old focus-visible view should be restyled after Tab focus moves"
    );
    assert!(
        background_is(&harness, b_id, palette::css::ORANGE),
        "new focus-visible view should be restyled after Tab focus moves"
    );
}

#[test]
fn alt_arrow_focus_visible_restyles_old_and_new_views() {
    let root = TestRoot::new();
    let left = Empty::new().style(|s| {
        s.size(100.0, 40.0)
            .keyboard_navigable()
            .background(palette::css::BLUE)
            .focus_visible(|s| s.background(palette::css::YELLOW))
    });
    let left_id = left.view_id();
    let right = Empty::new().style(|s| {
        s.size(100.0, 40.0)
            .keyboard_navigable()
            .background(palette::css::GREEN)
            .focus_visible(|s| s.background(palette::css::ORANGE))
    });
    let right_id = right.view_id();
    let view = Stack::horizontal((left, right)).style(|s| s.size(200.0, 40.0));
    let mut harness = HeadlessHarness::new_with_size(root, view, 200.0, 40.0);

    press_tab(&mut harness);
    assert!(harness.is_focused(left_id));
    assert!(background_is(&harness, left_id, palette::css::YELLOW));

    press_alt_arrow(&mut harness, NamedKey::ArrowRight, Code::ArrowRight);
    assert!(harness.is_focused(right_id));
    assert!(
        background_is(&harness, left_id, palette::css::BLUE),
        "old focused view should be restyled after Alt+Arrow focus move"
    );
    assert!(
        background_is(&harness, right_id, palette::css::ORANGE),
        "new focused view should be restyled after Alt+Arrow focus move"
    );
}

#[test]
fn tab_order_is_depth_first_across_nested_hierarchy() {
    let root = TestRoot::new();

    let a1 = Empty::new().style(|s| s.size(70.0, 40.0).keyboard_navigable());
    let a1_id = a1.view_id();
    let a2 = Empty::new().style(|s| s.size(70.0, 40.0).keyboard_navigable());
    let a2_id = a2.view_id();
    let b1 = Empty::new().style(|s| s.size(70.0, 40.0).keyboard_navigable());
    let b1_id = b1.view_id();
    let b2 = Empty::new().style(|s| s.size(70.0, 40.0).keyboard_navigable());
    let b2_id = b2.view_id();

    let group_a = Stack::horizontal((a1, a2)).style(|s| s.size(140.0, 40.0));
    let group_b = Stack::horizontal((b1, b2)).style(|s| s.size(140.0, 40.0));
    let view = Stack::vertical((group_a, group_b)).style(|s| s.size(140.0, 80.0));

    let mut harness = HeadlessHarness::new_with_size(root, view, 140.0, 80.0);

    press_tab(&mut harness);
    assert!(harness.is_focused(a1_id), "first tab target should be A1");
    press_tab(&mut harness);
    assert!(harness.is_focused(a2_id), "second tab target should be A2");
    press_tab(&mut harness);
    assert!(harness.is_focused(b1_id), "third tab target should be B1");
    press_tab(&mut harness);
    assert!(harness.is_focused(b2_id), "fourth tab target should be B2");

    press_shift_tab(&mut harness);
    assert!(harness.is_focused(b1_id), "shift-tab should move B2 -> B1");
    press_shift_tab(&mut harness);
    assert!(harness.is_focused(a2_id), "shift-tab should move B1 -> A2");
}

#[test]
fn tab_from_pointer_only_focus_moves_to_following_keyboard_sibling() {
    let root = TestRoot::new();

    let a = Empty::new().style(|s| s.size(70.0, 40.0).keyboard_navigable());
    let a_id = a.view_id();
    let pointer_only = Empty::new().style(|s| s.size(70.0, 40.0).focusable());
    let pointer_only_id = pointer_only.view_id();
    let c = Empty::new().style(|s| s.size(70.0, 40.0).keyboard_navigable());
    let c_id = c.view_id();
    let view = Stack::horizontal((a, pointer_only, c)).style(|s| s.size(210.0, 40.0));

    let mut harness = HeadlessHarness::new_with_size(root, view, 210.0, 40.0);

    harness.click(105.0, 20.0);
    assert!(harness.is_focused(pointer_only_id));

    press_tab(&mut harness);
    assert!(
        harness.is_focused(c_id),
        "Tab from pointer-only focused element should move to following keyboard sibling"
    );
    assert!(!harness.is_focused(a_id));

    press_shift_tab(&mut harness);
    assert!(
        harness.is_focused(a_id),
        "Shift+Tab from keyboard sibling should move back to previous keyboard sibling"
    );
}

#[test]
fn tab_order_after_tab_page_switch_stays_forward() {
    let root = TestRoot::new();
    let active_tab = RwSignal::new(0usize);

    let labels_ids: Rc<RefCell<Vec<ViewId>>> = Rc::new(RefCell::new(Vec::new()));
    let buttons_ids: Rc<RefCell<Vec<ViewId>>> = Rc::new(RefCell::new(Vec::new()));
    let labels_ids_for_build = Rc::clone(&labels_ids);
    let buttons_ids_for_build = Rc::clone(&buttons_ids);

    let tabs = tab(
        move || Some(active_tab.get()),
        move || vec!["Label", "Button"],
        |it| *it,
        move |it| match it {
            "Label" => {
                let l1 = Empty::new().style(|s| s.size(100.0, 36.0).keyboard_navigable());
                let l2 = Empty::new().style(|s| s.size(100.0, 36.0).keyboard_navigable());
                let ids = vec![l1.view_id(), l2.view_id()];
                if labels_ids_for_build.borrow().is_empty() {
                    *labels_ids_for_build.borrow_mut() = ids;
                }
                Stack::vertical((l1, l2))
                    .style(|s| s.size(120.0, 80.0))
                    .into_any()
            }
            "Button" => {
                let b1 = Empty::new().style(|s| s.size(100.0, 36.0).keyboard_navigable());
                let b2 = Empty::new().style(|s| s.size(100.0, 36.0).keyboard_navigable());
                let b3 = Empty::new().style(|s| s.size(100.0, 36.0).keyboard_navigable());
                let ids = vec![b1.view_id(), b2.view_id(), b3.view_id()];
                if buttons_ids_for_build.borrow().is_empty() {
                    *buttons_ids_for_build.borrow_mut() = ids;
                }
                Stack::vertical((b1, b2, b3))
                    .style(|s| s.size(120.0, 120.0))
                    .into_any()
            }
            _ => Empty::new().into_any(),
        },
    )
    .style(|s| s.size(150.0, 160.0));

    let mut harness = HeadlessHarness::new_with_size(root, tabs, 150.0, 160.0);
    let label_ids = labels_ids.borrow().clone();
    let button_ids = buttons_ids.borrow().clone();
    assert_eq!(label_ids.len(), 2);
    assert_eq!(button_ids.len(), 3);

    // Focus inside the Label page first.
    press_tab(&mut harness);
    assert!(harness.is_focused(label_ids[0]));

    // Switch to Button page while focus is still on Label page content.
    active_tab.set(1);
    harness.process_update_no_paint();

    press_tab(&mut harness);
    assert!(
        harness.is_focused(button_ids[0]),
        "After switching tabs, first Tab into page should land on first button"
    );
    press_tab(&mut harness);
    assert!(
        harness.is_focused(button_ids[1]),
        "Button page tab order should continue forward"
    );
    press_tab(&mut harness);
    assert!(
        harness.is_focused(button_ids[2]),
        "Button page tab order should continue forward"
    );
}
