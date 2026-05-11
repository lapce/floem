//! End-to-end demonstration that a non-floem host can drive the style engine.
//!
//! A bookkeeping struct (no view tree, no window handle, no reactive
//! runtime) assembles a [`CascadeInputs`] and exercises the cache plus
//! `resolve_nested_maps`. If this compiles and passes, a second host
//! like `floem-native` can reuse `floem_style` through the same public
//! surface floem itself uses.

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use floem_style::builtin_props::Background;
use floem_style::responsive::ScreenSizeBp;
use floem_style::selectors::StyleSelector;
use floem_style::{
    CacheHit, CursorStyle, InheritedInteractionCx, InteractionState, Style, StyleCache,
    StyleCacheKey, StyleNodeId, resolve_nested_maps,
};
use peniko::color::palette::css;

// ─────────────────────────────────────────────────────────────────────────
// Minimal mock host.
// ─────────────────────────────────────────────────────────────────────────

/// Recorded calls, so tests can assert on behavior rather than side effects.
#[derive(Default, Debug)]
struct Calls {
    paints: Vec<StyleNodeId>,
    cursor_sets: Vec<(StyleNodeId, CursorStyle)>,
    cursor_clears: Vec<StyleNodeId>,
    needs_layout: bool,
    needs_cursor_resolution: bool,
}

struct MockHost {
    default_inherited: Style,
    default_classes: Style,
    calls: Calls,
    hovered: std::collections::HashSet<StyleNodeId>,
    cursors: std::collections::HashMap<StyleNodeId, CursorStyle>,
}

impl MockHost {
    fn new() -> Self {
        Self {
            default_inherited: Style::new(),
            default_classes: Style::new(),
            calls: Calls::default(),
            hovered: Default::default(),
            cursors: Default::default(),
        }
    }

    fn request_paint(&mut self, id: StyleNodeId) {
        self.calls.paints.push(id);
    }

    fn mark_needs_cursor_resolution(&mut self) {
        self.calls.needs_cursor_resolution = true;
    }

    fn mark_needs_layout(&mut self) {
        self.calls.needs_layout = true;
    }

    fn set_cursor(&mut self, id: StyleNodeId, cursor: CursorStyle) -> Option<CursorStyle> {
        self.calls.cursor_sets.push((id, cursor));
        self.cursors.insert(id, cursor)
    }

    fn clear_cursor(&mut self, id: StyleNodeId) -> Option<CursorStyle> {
        self.calls.cursor_clears.push(id);
        self.cursors.remove(&id)
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Tests.
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn mock_host_records_inherent_calls() {
    let mut tree = floem_style::StyleTree::new();
    let node = tree.new_node();
    let mut host = MockHost::new();

    host.request_paint(node);
    host.mark_needs_layout();
    host.mark_needs_cursor_resolution();
    host.set_cursor(node, CursorStyle::Pointer);

    assert_eq!(host.calls.paints, vec![node]);
    assert!(host.calls.needs_layout);
    assert!(host.calls.needs_cursor_resolution);
    assert_eq!(host.calls.cursor_sets, vec![(node, CursorStyle::Pointer)]);

    let prev = host.clear_cursor(node);
    assert_eq!(prev, Some(CursorStyle::Pointer));
    assert_eq!(host.calls.cursor_clears, vec![node]);
}

#[test]
fn resolve_nested_maps_picks_hover_branch_for_hovered_node() {
    let mut tree = floem_style::StyleTree::new();
    let node = tree.new_node();
    let mut host = MockHost::new();
    host.hovered.insert(node);

    let style = Style::new()
        .background(css::RED)
        .hover(|s| s.background(css::BLUE));

    // Build an InteractionState the way a host would when composing
    // a `CascadeInputs::interactions` closure.
    let mut state = InteractionState {
        is_hovered: host.hovered.contains(&node),
        ..Default::default()
    };

    let (resolved, selectors) = resolve_nested_maps(
        style,
        &mut state,
        ScreenSizeBp::Md,
        &[],
        &host.default_inherited,
        &host.default_classes,
    );
    assert_eq!(resolved.get(Background), Some(css::BLUE.into()));
    assert!(selectors.has(StyleSelector::Hover));
}

#[test]
fn style_cache_round_trips_values() {
    let parent = Style::new();
    let style = Style::new().background(css::RED);
    let state = InteractionState::default();
    let mut cache = StyleCache::new();

    let key = StyleCacheKey::new(&style, &state, ScreenSizeBp::Md, &[], &Style::new());

    assert!(cache.get(&key, &parent).is_none());

    cache.insert(
        key.clone(),
        &style,
        None,
        InheritedInteractionCx::default(),
        &parent,
    );
    let hit: CacheHit = cache.get(&key, &parent).unwrap();
    assert_eq!(hit.combined_style.get(Background), Some(css::RED.into()));

    let stats = cache.stats();
    assert_eq!(stats.hits, 1);
    assert_eq!(stats.misses, 1);
    assert_eq!(stats.insertions, 1);
}

#[test]
fn interactions_closure_captures_host_state() {
    // Simulates the shape a host would construct for
    // `CascadeInputs::interactions`: a closure that maps
    // `StyleNodeId → PerNodeInteraction` by consulting the host's own
    // per-view state. We exercise it standalone, without driving the
    // full cascade — that's what `style_tree_cascade.rs` covers.
    use floem_style::PerNodeInteraction;

    let mut style_tree = floem_style::StyleTree::new();
    let a = style_tree.new_node();
    let b = style_tree.new_node();
    let mut host = MockHost::new();
    host.hovered.insert(a);

    let _unused_instant: Instant = Instant::now();

    let interactions = |node: StyleNodeId| PerNodeInteraction {
        is_hovered: host.hovered.contains(&node),
        ..Default::default()
    };

    assert!(interactions(a).is_hovered);
    assert!(!interactions(b).is_hovered);
}
