//! End-to-end demonstration that a non-floem host can drive the style engine.
//!
//! This test implements the [`StyleSink`] trait on a plain bookkeeping struct
//! (no view tree, no window handle, no reactive runtime) and exercises the
//! sink + cascade + cache together. If this compiles and passes, a second
//! host like `floem-native` can reuse `floem_style` through the same public
//! surface floem itself uses.
//!
//! Coverage intentionally stops short of `StyleCx` — that type still reads
//! host-specific `ViewId` state (see `floem::style::cx`) and is outside this
//! crate. The pieces exercised here are the ones a new host would drive
//! directly: [`Style`], [`resolve_nested_maps`], [`StyleCache`], and the
//! [`StyleSink`] trait.

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use floem_style::builtin_props::{Background, Disabled, TextColor};
use floem_style::responsive::ScreenSizeBp;
use floem_style::selectors::{StyleSelector, StyleSelectors};
use floem_style::{
    CacheHit, CursorStyle, ElementId, InheritedInteractionCx, InteractionState, Style, StyleCache,
    StyleCacheKey, StyleSink, recalc::StyleReason, resolve_nested_maps,
};
use peniko::color::palette::css;
use understory_box_tree::{LocalNode, Tree};

// ─────────────────────────────────────────────────────────────────────────
// Minimal mock host.
// ─────────────────────────────────────────────────────────────────────────

/// Recorded calls, so tests can assert on behavior rather than side effects.
#[derive(Default, Debug)]
struct Calls {
    marked_dirty: Vec<ElementId>,
    scheduled: Vec<ElementId>,
    paints: Vec<ElementId>,
    cursor_sets: Vec<(ElementId, CursorStyle)>,
    cursor_clears: Vec<ElementId>,
    captured_styles: Vec<ElementId>,
    needs_layout: bool,
    needs_cursor_resolution: bool,
}

struct MockHost {
    frame_start: Instant,
    default_inherited: Style,
    default_classes: Style,
    cache: StyleCache,
    calls: Calls,
    hovered: std::collections::HashSet<ElementId>,
    cursors: std::collections::HashMap<ElementId, CursorStyle>,
}

impl MockHost {
    fn new() -> Self {
        Self {
            frame_start: Instant::now(),
            default_inherited: Style::new(),
            default_classes: Style::new(),
            cache: StyleCache::new(),
            calls: Calls::default(),
            hovered: Default::default(),
            cursors: Default::default(),
        }
    }
}

impl StyleSink for MockHost {
    fn frame_start(&self) -> Instant {
        self.frame_start
    }
    fn screen_size_bp(&self) -> ScreenSizeBp {
        ScreenSizeBp::Md
    }
    fn keyboard_navigation(&self) -> bool {
        false
    }
    fn root_size_width(&self) -> f64 {
        1024.0
    }
    fn is_dark_mode(&self) -> bool {
        false
    }

    fn default_theme_classes(&self) -> &Style {
        &self.default_classes
    }
    fn default_theme_inherited(&self) -> &Style {
        &self.default_inherited
    }

    fn style_cache_mut(&mut self) -> &mut StyleCache {
        &mut self.cache
    }

    fn is_hovered(&self, id: ElementId) -> bool {
        self.hovered.contains(&id)
    }
    fn is_focused(&self, _id: ElementId) -> bool {
        false
    }
    fn is_focus_within(&self, _id: ElementId) -> bool {
        false
    }
    fn is_active(&self, _id: ElementId) -> bool {
        false
    }
    fn is_file_hover(&self, _id: ElementId) -> bool {
        false
    }

    fn mark_style_dirty_with(&mut self, id: ElementId, _reason: StyleReason) {
        self.calls.marked_dirty.push(id);
    }
    fn schedule_style(&mut self, id: ElementId, _reason: StyleReason) {
        self.calls.scheduled.push(id);
    }
    fn schedule_style_with_target(&mut self, target: ElementId, _reason: StyleReason) {
        self.calls.scheduled.push(target);
    }
    fn mark_descendants_with_selector_dirty(
        &mut self,
        _ancestor: ElementId,
        _selector: StyleSelector,
    ) {
    }
    fn mark_descendants_with_responsive_selector_dirty(&mut self, _ancestor: ElementId) {}
    fn update_selector_interest(&mut self, _id: ElementId, _selectors: Option<StyleSelectors>) {}

    fn register_fixed_element(&mut self, _id: ElementId) {}
    fn unregister_fixed_element(&mut self, _id: ElementId) {}
    fn invalidate_focus_nav_cache(&mut self) {}
    fn request_paint(&mut self, id: ElementId) {
        self.calls.paints.push(id);
    }
    fn mark_needs_cursor_resolution(&mut self) {
        self.calls.needs_cursor_resolution = true;
    }
    fn mark_needs_layout(&mut self) {
        self.calls.needs_layout = true;
    }

    fn set_cursor(&mut self, id: ElementId, cursor: CursorStyle) -> Option<CursorStyle> {
        self.calls.cursor_sets.push((id, cursor));
        self.cursors.insert(id, cursor)
    }
    fn clear_cursor(&mut self, id: ElementId) -> Option<CursorStyle> {
        self.calls.cursor_clears.push(id);
        self.cursors.remove(&id)
    }

    fn inspector_capture_style(&mut self, id: ElementId, _computed_style: &Style) {
        self.calls.captured_styles.push(id);
    }
}

/// Mint an [`ElementId`] by pushing a stub node into a real box tree.
fn fresh_element(tree: &mut Tree, owning_view_raw: u64) -> ElementId {
    let node = tree.push_child(None, LocalNode::default());
    ElementId(node, owning_view_raw, true)
}

// ─────────────────────────────────────────────────────────────────────────
// Tests.
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn mock_host_can_implement_sink_and_record_calls() {
    let mut tree = Tree::new();
    let elem = fresh_element(&mut tree, 1);
    let mut host = MockHost::new();

    host.mark_style_dirty_with(elem, StyleReason::inherited());
    host.schedule_style(elem, StyleReason::animation());
    host.request_paint(elem);
    host.mark_needs_layout();
    host.mark_needs_cursor_resolution();
    host.set_cursor(elem, CursorStyle::Pointer);

    assert_eq!(host.calls.marked_dirty, vec![elem]);
    assert_eq!(host.calls.scheduled, vec![elem]);
    assert_eq!(host.calls.paints, vec![elem]);
    assert!(host.calls.needs_layout);
    assert!(host.calls.needs_cursor_resolution);
    assert_eq!(host.calls.cursor_sets, vec![(elem, CursorStyle::Pointer)]);

    let prev = host.clear_cursor(elem);
    assert_eq!(prev, Some(CursorStyle::Pointer));
    assert_eq!(host.calls.cursor_clears, vec![elem]);
}

#[test]
fn cascade_runs_with_interaction_state_derived_from_sink() {
    let mut tree = Tree::new();
    let elem = fresh_element(&mut tree, 7);
    let mut host = MockHost::new();
    host.hovered.insert(elem);

    let style = Style::new()
        .background(css::RED)
        .hover(|s| s.background(css::BLUE));

    // Derive InteractionState from sink trait methods, as a host would when
    // building the cascade's input.
    let mut state = InteractionState {
        is_hovered: host.is_hovered(elem),
        is_dark_mode: host.is_dark_mode(),
        window_width: host.root_size_width(),
        using_keyboard_navigation: host.keyboard_navigation(),
        ..Default::default()
    };

    let (resolved, selectors) = resolve_nested_maps(
        style,
        &mut state,
        host.screen_size_bp(),
        &[],
        host.default_theme_inherited(),
        host.default_theme_classes(),
    );
    assert_eq!(resolved.get(Background), Some(css::BLUE.into()));
    assert!(selectors.has(StyleSelector::Hover));
}

#[test]
fn style_cache_is_reachable_through_sink() {
    let mut tree = Tree::new();
    let elem = fresh_element(&mut tree, 42);
    let mut host = MockHost::new();
    let parent = Style::new();
    let style = Style::new().background(css::RED);
    let state = InteractionState::default();

    let key = StyleCacheKey::new(&style, &state, host.screen_size_bp(), &[], &Style::new());

    // Miss the first time.
    assert!(host.style_cache_mut().get(&key, &parent).is_none());

    // Insert via the sink handle and confirm the next lookup hits.
    host.style_cache_mut().insert(
        key.clone(),
        &style,
        None,
        InheritedInteractionCx::default(),
        &parent,
    );
    let hit: CacheHit = host.style_cache_mut().get(&key, &parent).unwrap();
    assert_eq!(hit.combined_style.get(Background), Some(css::RED.into()));

    // Cache stats reflect the hit + miss.
    let stats = host.cache.stats();
    assert_eq!(stats.hits, 1);
    assert_eq!(stats.misses, 1);
    assert_eq!(stats.insertions, 1);

    let _ = elem; // keep the tree node alive for Miri-like scrutiny
}

#[test]
fn inspector_capture_routes_through_default_impl() {
    let mut tree = Tree::new();
    let elem = fresh_element(&mut tree, 99);
    let mut host = MockHost::new();
    let style = Style::new()
        .set(TextColor, Some(css::RED))
        .set(Disabled, true);

    // This method has a default `{}` impl on the trait; MockHost overrides it
    // to record. Either behavior is fine for floem-native hosts that don't
    // run an inspector.
    host.inspector_capture_style(elem, &style);
    assert_eq!(host.calls.captured_styles, vec![elem]);
}

#[test]
fn trait_object_dispatch_works() {
    // Exercises `&mut dyn StyleSink` — the shape `StyleCx` stores internally.
    fn touch_sink(sink: &mut dyn StyleSink, id: ElementId) {
        sink.mark_needs_layout();
        sink.request_paint(id);
    }

    let mut tree = Tree::new();
    let elem = fresh_element(&mut tree, 3);
    let mut host = MockHost::new();
    touch_sink(&mut host, elem);

    assert!(host.calls.needs_layout);
    assert_eq!(host.calls.paints, vec![elem]);
}
