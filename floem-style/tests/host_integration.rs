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
//! 3. Assemble a [`CascadeInputs`] per pass and drive
//!    [`StyleTree::compute_style`].
//! 4. Read resolved [`Style`] back from the tree and verify the
//!    cascade did the right thing (direct values, inheritance,
//!    class-context propagation, selector gates).
//! 5. Run the engine prop extractors ([`LayoutProps`],
//!    [`TransformProps`], [`ViewStyleProps`]) against the computed
//!    style via both `read_explicit` and the `PropExtractorCx`-based
//!    `read_style` path; exercise `apply_to_taffy_style` and
//!    `affine` helpers.
//! 6. Verify the animation backend hook fires and tree-stored
//!    animations tick.
//!
//! Nothing in this file touches floem. If moved behind a
//! `floem-native` feature flag tomorrow, it would still pass.

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use std::cell::Cell;

use floem_style::builtin_props::{Background, FontSize};
use floem_style::responsive::ScreenSizeBp;
use floem_style::unit::{Angle, Pt};
use floem_style::{
    AnimationBackend, CascadeInputs, ElementId, InteractionState, LayoutProps, NoAnimationBackend,
    PerNodeInteraction, PropExtractorCx, Style, StyleNodeId, StyleTree, TransformProps,
    ViewStyleProps, recalc::StyleReason,
};
use peniko::color::palette::css;
use peniko::kurbo;
use understory_box_tree::{LocalNode, Tree};

// ─────────────────────────────────────────────────────────────────────────
// MockHost — bookkeeping a non-floem consumer would keep. The engine
// reads from it through a `CascadeInputs::interactions` closure; extractor
// tests also use it as a `PropExtractorCx`.
// ─────────────────────────────────────────────────────────────────────────

#[derive(Default)]
struct MockHost {
    frame_start: Option<Instant>,
    default_inherited: Style,
    default_classes: Style,

    // Recordings tests assert on.
    transition_requests: Vec<ElementId>,

    // Per-element sink state the cascade reads.
    hovered: std::collections::HashSet<StyleNodeId>,

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

/// Build a `CascadeInputs` for the current host state. Captures `host`
/// immutably for the interactions closure and pairs with a
/// `NoAnimationBackend`; scoping everything inside one call avoids
/// tangling the closure's lifetime with the tree.
fn with_inputs<R>(
    host: &MockHost,
    f: impl FnOnce(CascadeInputs<'_>) -> R,
) -> R {
    let anim = NoAnimationBackend;
    let interactions = |node: StyleNodeId| PerNodeInteraction {
        is_hovered: host.hovered.contains(&node),
        ..Default::default()
    };
    f(CascadeInputs {
        frame_start: host.frame_start.unwrap_or_else(Instant::now),
        screen_size_bp: ScreenSizeBp::Md,
        keyboard_navigation: false,
        root_size_width: 1024.0,
        is_dark_mode: false,
        default_theme_classes: &host.default_classes,
        default_theme_inherited: &host.default_inherited,
        interactions: &interactions,
        animations: &anim,
    })
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
    let mut tree = StyleTree::new();
    let host = MockHost::new();

    let root = tree.new_node();
    let child = tree.new_node();
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

    with_inputs(&host, |cx| tree.compute_style(root, &cx));

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
    let mut tree = StyleTree::new();
    let host = MockHost::new();

    let n = tree.new_node();
    tree.set_direct_style(
        n,
        Style::new()
            .width(Pt(200.0))
            .height(Pt(120.0))
            .padding_left(Pt(8.0))
            .font_size(14.0),
    );
    with_inputs(&host, |cx| tree.compute_style(n, &cx));

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

    let n = tree.new_node();
    // PropExtractorCx still uses `ElementId` for transition-target
    // identity — it's floem's and other hosts' own notion of
    // which sub-element a transition targets, separate from engine
    // node identity. Mint a host-side element_id for the cx state.
    let element_id = fresh_element(&mut box_tree, 1);
    tree.set_direct_style(
        n,
        Style::new()
            .scale(150.0)
            .rotate(Angle::Deg(90.0))
            .border_top_left_radius(Pt(4.0)),
    );
    with_inputs(&host, |cx| tree.compute_style(n, &cx));

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
    let mut tree = StyleTree::new();
    let host = MockHost::new();

    let n = tree.new_node();
    tree.set_direct_style(n, Style::new().background(css::BLUE).outline(2.0));
    with_inputs(&host, |cx| tree.compute_style(n, &cx));

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

    let mut tree = StyleTree::new();
    let host = MockHost::new();

    let root = tree.new_node();
    let child = tree.new_node();
    tree.set_parent(child, Some(root));

    tree.set_direct_style(
        root,
        Style::new().class(Callout, |s| s.background(css::BLUE)),
    );
    tree.set_classes(child, &[Callout::class_ref()]);

    with_inputs(&host, |cx| tree.compute_style(root, &cx));

    assert_eq!(
        tree.computed_style(child).unwrap().get(Background),
        Some(css::BLUE.into()),
        "descendant applying a class defined on its ancestor should resolve to the class style"
    );
}

/// Interaction-driven cascade: flipping `hovered` on the host between
/// two `compute_style` passes must re-resolve the `:hover` branch.
#[test]
fn hover_selector_switches_between_passes() {
    let mut tree = StyleTree::new();
    let mut host = MockHost::new();

    let n = tree.new_node();
    tree.set_direct_style(
        n,
        Style::new()
            .background(css::RED)
            .hover(|s| s.background(css::GREEN)),
    );

    with_inputs(&host, |cx| tree.compute_style(n, &cx));
    assert_eq!(
        tree.computed_style(n).unwrap().get(Background),
        Some(css::RED.into())
    );

    host.hovered.insert(n);
    tree.mark_dirty(n, StyleReason::style_pass());
    with_inputs(&host, |cx| tree.compute_style(n, &cx));
    assert_eq!(
        tree.computed_style(n).unwrap().get(Background),
        Some(css::GREEN.into())
    );
}

/// `AnimationBackend::apply` is the host's hook for injecting per-view
/// animations into the cascade. Override it, confirm the tree invokes
/// it every time it recomputes a node.
#[test]
fn animation_backend_hook_is_invoked() {
    struct AnimBackend {
        anim_calls: Cell<usize>,
    }
    impl AnimationBackend for AnimBackend {
        fn apply(
            &self,
            _node: StyleNodeId,
            _combined: &mut Style,
            _interact: &mut InteractionState,
        ) -> bool {
            self.anim_calls.set(self.anim_calls.get() + 1);
            false
        }
    }

    let backend = AnimBackend {
        anim_calls: Cell::new(0),
    };
    let host = MockHost::new();

    let mut tree = StyleTree::new();
    let n = tree.new_node();
    tree.set_direct_style(n, Style::new().background(css::RED));

    let interactions = |_: StyleNodeId| PerNodeInteraction::default();
    let inputs = CascadeInputs {
        frame_start: Instant::now(),
        screen_size_bp: ScreenSizeBp::Md,
        keyboard_navigation: false,
        root_size_width: 1024.0,
        is_dark_mode: false,
        default_theme_classes: &host.default_classes,
        default_theme_inherited: &host.default_inherited,
        interactions: &interactions,
        animations: &backend,
    };
    tree.compute_style(n, &inputs);

    assert_eq!(
        backend.anim_calls.get(),
        1,
        "compute_style should invoke AnimationBackend::apply once per dirty node"
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

    let mut tree = StyleTree::new();
    let host = MockHost::new();

    let n = tree.new_node();
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
    with_inputs(&host, |cx| tree.compute_style(n, &cx));
    let events = tree.take_animation_events(n);
    assert_eq!(events.len(), 1, "started event should have fired");
    assert!(events[0].1.started);
    assert_eq!(events[0].0, slot, "event should reference the pushed slot");

    // Cascade should also schedule the node for another frame while the
    // animation is active.
    let scheduled: Vec<_> = tree.take_scheduled().collect();
    assert!(
        scheduled.iter().any(|(id, _)| *id == n),
        "active tree-animation should have scheduled its node"
    );

    // After the tick the animation is actually advancing: it's no
    // longer Idle and `can_advance` remains true.
    let anim = &tree.animations(n)[slot];
    assert!(!anim.is_idle());
    assert!(anim.is_in_progress());
    assert!(anim.can_advance());
}
