//! End-to-end integration test: a non-floem host drives the style
//! engine through every public surface a real consumer would touch.
//!
//! This test is the "proof of reusability" — if it compiles and
//! passes using only `floem_style::*` (plus generic support crates
//! like `peniko`, `taffy`, `understory_box_tree`), the engine is
//! usable by `floem-native` or any other downstream host that brings
//! its own view tree.
//!
//! Walks the full host-integration flow:
//! 1. Allocate engine nodes in a [`StyleTree`], wire parent/child edges.
//! 2. Push direct styles (plain props, classes, selectors).
//! 3. Run [`StyleTree::compute_style`] through a sink that implements
//!    both [`StyleSink`] (engine → host callbacks) and
//!    [`PropExtractorCx`] (what the `prop_extractor!` macro reads).
//! 4. Read resolved [`Style`] back from the tree and verify the
//!    cascade did the right thing (direct values, inheritance,
//!    class-context propagation, selector gates).
//! 5. Run the engine prop extractors ([`LayoutProps`],
//!    [`TransformProps`], [`ViewStyleProps`]) against the computed
//!    style via both `read_explicit` and the `PropExtractorCx`-based
//!    `read_style` path; exercise `apply_to_taffy_style` and
//!    `affine` helpers.
//! 6. Verify sink side-effects (`request_paint`, animation hook) fire.
//!
//! Nothing in this file touches floem. If moved behind a
//! `floem-native` feature flag tomorrow, it would still pass.

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use floem_style::builtin_props::{Background, FontSize};
use floem_style::responsive::ScreenSizeBp;
use floem_style::selectors::{StyleSelector, StyleSelectors};
use floem_style::unit::{Angle, Pt};
use floem_style::{
    CursorStyle, ElementId, LayoutProps, PropExtractorCx, Style, StyleSink, StyleTree,
    TransformProps, ViewStyleProps, recalc::StyleReason,
};
use peniko::color::palette::css;
use peniko::kurbo;
use understory_box_tree::{LocalNode, Tree};

// ─────────────────────────────────────────────────────────────────────────
// MockHost — one struct that plays both roles the engine asks of a host:
// - StyleSink: engine calls it for every cascade-emitted side-effect.
// - PropExtractorCx: the trait `prop_extractor!`-generated convenience
//   methods dispatch through.
// ─────────────────────────────────────────────────────────────────────────

#[derive(Default)]
struct MockHost {
    frame_start: Option<Instant>,
    default_inherited: Style,
    default_classes: Style,

    // Recordings tests assert on.
    paints: Vec<ElementId>,
    transition_requests: Vec<ElementId>,

    // Per-element sink state the cascade reads.
    hovered: std::collections::HashSet<ElementId>,

    // State PropExtractorCx reads through.
    current_element: Option<ElementId>,
    current_direct: Style,
}

impl MockHost {
    fn new() -> Self {
        Self {
            frame_start: Some(Instant::now()),
            ..Self::default()
        }
    }
}

impl StyleSink for MockHost {
    fn frame_start(&self) -> Instant {
        self.frame_start.unwrap()
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
    fn mark_style_dirty_with(&mut self, _id: ElementId, _reason: StyleReason) {}
    fn register_fixed_element(&mut self, _id: ElementId) {}
    fn unregister_fixed_element(&mut self, _id: ElementId) {}
    fn invalidate_focus_nav_cache(&mut self) {}
    fn mark_needs_cursor_resolution(&mut self) {}
    fn mark_needs_layout(&mut self) {}
    fn set_cursor(&mut self, _id: ElementId, _cursor: CursorStyle) -> Option<CursorStyle> {
        None
    }
    fn clear_cursor(&mut self, _id: ElementId) -> Option<CursorStyle> {
        None
    }
}

impl PropExtractorCx for MockHost {
    fn now(&self) -> Instant {
        self.frame_start.unwrap()
    }
    fn direct_style(&self) -> &Style {
        &self.current_direct
    }
    fn current_element(&self) -> ElementId {
        self.current_element.expect("current_element not set")
    }
    fn request_transition_for(&mut self, target: ElementId) {
        self.transition_requests.push(target);
    }
}

fn fresh_element(tree: &mut Tree, owning: u64) -> ElementId {
    let node = tree.push_child(None, LocalNode::default());
    ElementId(node, owning, true)
}

// ─────────────────────────────────────────────────────────────────────────
// Tests.
// ─────────────────────────────────────────────────────────────────────────

/// Build a small tree, run the cascade, verify direct-style flows to
/// computed_style and inherited props propagate to descendants.
#[test]
fn tree_cascade_produces_computed_style_and_inheritance() {
    let mut box_tree = Tree::new();
    let mut tree = StyleTree::new();
    let mut host = MockHost::new();

    let root = tree.new_node(fresh_element(&mut box_tree, 1));
    let child = tree.new_node(fresh_element(&mut box_tree, 2));
    tree.set_parent(child, Some(root));

    tree.set_direct_style(
        root,
        Style::new()
            .width(Pt(200.0))
            .height(Pt(120.0))
            .padding_left(Pt(8.0))
            .font_size(14.0)
            .background(css::RED),
    );
    tree.set_direct_style(child, Style::new().width(Pt(50.0)).height(Pt(30.0)));

    tree.compute_style(root, &mut host);

    // Root has its direct values resolved.
    let root_computed = tree.computed_style(root).unwrap();
    assert_eq!(root_computed.get(FontSize), 14.0);
    assert_eq!(root_computed.get(Background), Some(css::RED.into()));

    // Child inherits font-size from root (inherited prop, propagates).
    let child_computed = tree.computed_style(child).unwrap();
    assert_eq!(child_computed.get(FontSize), 14.0);
    // But NOT background (non-inherited).
    assert_eq!(child_computed.get(Background), None);
}

/// Run `LayoutProps::read_explicit` against a computed style and verify
/// `apply_to_taffy_style` produces a taffy style hosts can feed to their
/// layout solver.
#[test]
fn layout_extractor_fills_taffy_style() {
    let mut box_tree = Tree::new();
    let mut tree = StyleTree::new();
    let mut host = MockHost::new();

    let n = tree.new_node(fresh_element(&mut box_tree, 1));
    tree.set_direct_style(
        n,
        Style::new()
            .width(Pt(200.0))
            .height(Pt(120.0))
            .padding_left(Pt(8.0))
            .font_size(14.0),
    );
    tree.compute_style(n, &mut host);

    let computed = tree.computed_style(n).unwrap();

    // Drive the extractor through `read_explicit` (no context needed).
    let mut layout = LayoutProps::default();
    let mut transitioning = false;
    layout.read_explicit(computed, &host.now(), &mut transitioning);

    assert_eq!(layout.font_size(), 14.0);

    // apply_to_taffy_style fills in width/height/padding and friends.
    // Confirm it pushes a non-default padding onto the taffy style.
    let default_taffy: taffy::style::Style = taffy::style::Style::default();
    let mut taffy: taffy::style::Style = taffy::style::Style::default();
    layout.apply_to_taffy_style(&mut taffy);
    assert_ne!(
        taffy.padding.left, default_taffy.padding.left,
        "apply_to_taffy_style should push a non-default padding"
    );

    // `border()` helper aggregates per-side values into a Border.
    let _border = layout.border();
}

/// Run `TransformProps` through the `PropExtractorCx`-based `read_style`
/// entry point (the one the convenience methods call) and verify the
/// extracted values feed `affine()` correctly.
#[test]
fn transform_extractor_through_prop_extractor_cx() {
    let mut box_tree = Tree::new();
    let mut tree = StyleTree::new();
    let mut host = MockHost::new();

    let n = tree.new_node(fresh_element(&mut box_tree, 1));
    let element_id = tree.get(n).unwrap().element_id;
    tree.set_direct_style(
        n,
        Style::new()
            .scale(150.0)
            .rotate(Angle::Deg(90.0))
            .border_top_left_radius(Pt(4.0)),
    );
    tree.compute_style(n, &mut host);

    let computed = tree.computed_style(n).unwrap().clone();

    // Set up PropExtractorCx state the extractor will read.
    host.current_element = Some(element_id);
    host.current_direct = computed.clone();

    let mut transform = TransformProps::default();
    // `read_style` on the extractor takes `&mut dyn PropExtractorCx`.
    transform.read_style(&mut host as &mut dyn PropExtractorCx, &computed);

    // Rotation extracted — non-zero (we set 90°).
    assert!(transform.rotation().to_radians().abs() > 0.0);

    // `affine()` composes translate/rotate/scale — should differ from
    // identity with these inputs.
    let affine = transform.affine(
        kurbo::Size::new(100.0, 100.0),
        &floem_style::unit::FontSizeCx::new(14.0, 18.0),
    );
    assert_ne!(affine, kurbo::Affine::IDENTITY);

    // border_radius() helper aggregates per-corner values into a BorderRadius.
    let br = transform.border_radius();
    assert!(br.top_left.is_some());
}

/// Run `ViewStyleProps` against a visual style. Exercises the
/// `.background()` / `.border_color()` aggregator helpers.
#[test]
fn view_style_extractor_reads_visual_props() {
    let mut box_tree = Tree::new();
    let mut tree = StyleTree::new();
    let mut host = MockHost::new();

    let n = tree.new_node(fresh_element(&mut box_tree, 1));
    tree.set_direct_style(n, Style::new().background(css::BLUE).outline(2.0));
    tree.compute_style(n, &mut host);

    let computed = tree.computed_style(n).unwrap();
    let mut vs = ViewStyleProps::default();
    vs.read_explicit(computed, &host.now(), &mut false);

    assert_eq!(vs.background(), Some(peniko::Brush::Solid(css::BLUE)));

    // Aggregator helpers don't panic on the extracted state.
    let _radius = vs.border_radius();
    let _colors = vs.border_color();
}

/// Class-context propagation through the tree: parent declares a class;
/// child that applies the class picks up the class's styles without
/// the host doing anything special.
#[test]
fn class_context_propagates_through_tree() {
    use floem_style::props::StyleClass;
    use floem_style::style_class;

    style_class!(pub Callout);

    let mut box_tree = Tree::new();
    let mut tree = StyleTree::new();
    let mut host = MockHost::new();

    let root = tree.new_node(fresh_element(&mut box_tree, 1));
    let child = tree.new_node(fresh_element(&mut box_tree, 2));
    tree.set_parent(child, Some(root));

    tree.set_direct_style(
        root,
        Style::new().class(Callout, |s| s.background(css::BLUE)),
    );
    tree.set_classes(child, &[Callout::class_ref()]);

    tree.compute_style(root, &mut host);

    assert_eq!(
        tree.computed_style(child).unwrap().get(Background),
        Some(css::BLUE.into()),
        "descendant applying a class defined on its ancestor should resolve to the class style"
    );
}

/// Interaction-driven cascade: flipping `hovered` on the sink between
/// two `compute_style` passes must re-resolve the `:hover` branch.
#[test]
fn hover_selector_switches_between_passes() {
    let mut box_tree = Tree::new();
    let mut tree = StyleTree::new();
    let mut host = MockHost::new();

    let n = tree.new_node(fresh_element(&mut box_tree, 1));
    let element_id = tree.get(n).unwrap().element_id;
    tree.set_direct_style(
        n,
        Style::new()
            .background(css::RED)
            .hover(|s| s.background(css::GREEN)),
    );

    tree.compute_style(n, &mut host);
    assert_eq!(
        tree.computed_style(n).unwrap().get(Background),
        Some(css::RED.into())
    );

    host.hovered.insert(element_id);
    tree.mark_dirty(n, StyleReason::style_pass());
    tree.compute_style(n, &mut host);
    assert_eq!(
        tree.computed_style(n).unwrap().get(Background),
        Some(css::GREEN.into())
    );
}

/// `StyleSink::apply_animations` is the host's hook for injecting
/// per-view animations into the cascade. Override it, confirm the tree
/// invokes it every time it recomputes a node.
#[test]
fn sink_apply_animations_hook_is_invoked() {
    struct AnimHost {
        inner: MockHost,
        anim_calls: usize,
    }
    impl AnimHost {
        fn new() -> Self {
            Self {
                inner: MockHost::new(),
                anim_calls: 0,
            }
        }
    }
    // Forward the bulk of StyleSink to the inner mock; override apply_animations.
    impl StyleSink for AnimHost {
        fn frame_start(&self) -> Instant {
            self.inner.frame_start()
        }
        fn screen_size_bp(&self) -> ScreenSizeBp {
            self.inner.screen_size_bp()
        }
        fn keyboard_navigation(&self) -> bool {
            self.inner.keyboard_navigation()
        }
        fn root_size_width(&self) -> f64 {
            self.inner.root_size_width()
        }
        fn is_dark_mode(&self) -> bool {
            self.inner.is_dark_mode()
        }
        fn default_theme_classes(&self) -> &Style {
            self.inner.default_theme_classes()
        }
        fn default_theme_inherited(&self) -> &Style {
            self.inner.default_theme_inherited()
        }
        fn is_hovered(&self, id: ElementId) -> bool {
            self.inner.is_hovered(id)
        }
        fn is_focused(&self, id: ElementId) -> bool {
            self.inner.is_focused(id)
        }
        fn is_focus_within(&self, id: ElementId) -> bool {
            self.inner.is_focus_within(id)
        }
        fn is_active(&self, id: ElementId) -> bool {
            self.inner.is_active(id)
        }
        fn is_file_hover(&self, id: ElementId) -> bool {
            self.inner.is_file_hover(id)
        }
        fn mark_style_dirty_with(&mut self, id: ElementId, reason: StyleReason) {
            self.inner.mark_style_dirty_with(id, reason)
        }
        fn register_fixed_element(&mut self, id: ElementId) {
            self.inner.register_fixed_element(id)
        }
        fn unregister_fixed_element(&mut self, id: ElementId) {
            self.inner.unregister_fixed_element(id)
        }
        fn invalidate_focus_nav_cache(&mut self) {
            self.inner.invalidate_focus_nav_cache()
        }
        fn mark_needs_cursor_resolution(&mut self) {
            self.inner.mark_needs_cursor_resolution()
        }
        fn mark_needs_layout(&mut self) {
            self.inner.mark_needs_layout()
        }
        fn set_cursor(&mut self, id: ElementId, cursor: CursorStyle) -> Option<CursorStyle> {
            self.inner.set_cursor(id, cursor)
        }
        fn clear_cursor(&mut self, id: ElementId) -> Option<CursorStyle> {
            self.inner.clear_cursor(id)
        }
        fn apply_animations(
            &mut self,
            _id: ElementId,
            _combined: &mut Style,
            _interact: &mut floem_style::InteractionState,
        ) -> bool {
            self.anim_calls += 1;
            false
        }
    }

    let mut box_tree = Tree::new();
    let mut tree = StyleTree::new();
    let mut host = AnimHost::new();

    let n = tree.new_node(fresh_element(&mut box_tree, 1));
    tree.set_direct_style(n, Style::new().background(css::RED));
    tree.compute_style(n, &mut host);

    assert_eq!(
        host.anim_calls, 1,
        "compute_style should invoke sink.apply_animations once per dirty node"
    );
}

/// Animations pushed directly onto a `StyleTree` node are ticked by the
/// cascade without any host-side registry. The generated events land in
/// the node's event buffer for the host to drain. This is the path
/// standalone consumers (floem-native, tests) use — no reactive
/// wrapper, no per-view `Stack<Animation>`, just the engine.
#[test]
fn tree_stored_animations_tick_during_cascade() {
    use floem_style::animation::{Animation, KeyFrame};
    use std::time::Duration;

    let mut box_tree = Tree::new();
    let mut tree = StyleTree::new();
    let mut host = MockHost::new();

    let element_id = fresh_element(&mut box_tree, 7);
    let n = tree.new_node(element_id);
    tree.set_direct_style(n, Style::new());

    // Push a simple two-keyframe animation onto the tree node. The
    // host never touches it — the cascade handles the whole lifecycle.
    let anim = Animation::new()
        .duration(Duration::from_millis(500))
        .keyframe(0, |f: KeyFrame| {
            f.ease_linear().style(|s| s.background(css::RED))
        })
        .keyframe(100, |f: KeyFrame| {
            f.ease_linear().style(|s| s.background(css::BLUE))
        });
    let slot = tree.push_animation(n, anim);
    assert_eq!(slot, 0);
    assert_eq!(tree.animations(n).len(), 1);

    // First compute_style: animation is Idle; the tick transitions it
    // to PassInProgress and fires a `started` event.
    tree.compute_style(n, &mut host);
    let events = tree.take_animation_events(n);
    assert_eq!(events.len(), 1, "started event should have fired");
    assert!(events[0].1.started);
    assert_eq!(events[0].0, slot, "event should reference the pushed slot");

    // Cascade should also schedule the node for another frame while the
    // animation is active.
    let scheduled: Vec<_> = tree.take_scheduled().collect();
    assert!(
        scheduled.iter().any(|(id, _)| *id == element_id),
        "active tree-animation should have scheduled its element"
    );

    // After the tick the animation is actually advancing: it's no
    // longer Idle and `can_advance` remains true.
    let anim = &tree.animations(n)[slot];
    assert!(!anim.is_idle());
    assert!(anim.is_in_progress());
    assert!(anim.can_advance());
}
