//! Cascade tests for [`StyleTree`].
//!
//! Exercises `StyleTree::compute_style` end-to-end with a mock host: the
//! cascade resolves classes + selectors + inheritance and walks parent →
//! child through the tree's own edges. If these pass, a non-floem host
//! can drive the style engine by assembling a `CascadeInputs` and running
//! `compute_style`.

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use floem_style::builtin_props::{Background, FontSize, TextColor};
use floem_style::props::StyleClass;
use floem_style::responsive::ScreenSizeBp;
use floem_style::{
    CascadeInputs, NoAnimationBackend, PerNodeInteraction, Style, StyleNodeId, StyleTree,
    recalc::StyleReason, style_class,
};
use peniko::color::palette::css;

// ─────────────────────────────────────────────────────────────────────────
// Minimal host state. Cascade tests only need the per-node interaction
// closure and the theme defaults; everything else is constant.
// ─────────────────────────────────────────────────────────────────────────

#[derive(Default)]
struct MockHost {
    hovered: std::collections::HashSet<StyleNodeId>,
    default_inherited: Style,
    default_classes: Style,
}

impl MockHost {
    fn new_default() -> Self {
        Self::default()
    }
}

/// Build a fresh `CascadeInputs` for this pass. Using a function keeps
/// the closures' lifetimes scoped to one call.
fn cascade<'a>(host: &'a MockHost) -> (Box<dyn Fn(StyleNodeId) -> PerNodeInteraction + 'a>, NoAnimationBackend) {
    let interactions: Box<dyn Fn(StyleNodeId) -> PerNodeInteraction + 'a> =
        Box::new(|node: StyleNodeId| PerNodeInteraction {
            is_hovered: host.hovered.contains(&node),
            ..Default::default()
        });
    (interactions, NoAnimationBackend)
}

fn inputs<'a>(
    host: &'a MockHost,
    interactions: &'a dyn Fn(StyleNodeId) -> PerNodeInteraction,
    animations: &'a NoAnimationBackend,
) -> CascadeInputs<'a> {
    CascadeInputs {
        frame_start: Instant::now(),
        screen_size_bp: ScreenSizeBp::Md,
        keyboard_navigation: false,
        root_size_width: 1024.0,
        is_dark_mode: false,
        default_theme_classes: &host.default_classes,
        default_theme_inherited: &host.default_inherited,
        interactions,
        animations,
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Tests.
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn single_node_direct_style_flows_to_computed() {
    let mut tree = StyleTree::new();
    let host = MockHost::new_default();
    let (interactions, anim) = cascade(&host);
    let cx = inputs(&host, &*interactions, &anim);

    let n = tree.new_node();
    tree.set_direct_style(n, Style::new().background(css::RED));
    tree.compute_style(n, &cx);

    let computed = tree.computed_style(n).unwrap();
    assert_eq!(computed.get(Background), Some(css::RED.into()));
}

#[test]
fn hover_selector_respects_host_state() {
    let mut tree = StyleTree::new();
    let mut host = MockHost::new_default();

    let n = tree.new_node();
    tree.set_direct_style(
        n,
        Style::new()
            .background(css::RED)
            .hover(|s| s.background(css::BLUE)),
    );

    // Not hovered → base.
    {
        let (interactions, anim) = cascade(&host);
        let cx = inputs(&host, &*interactions, &anim);
        tree.compute_style(n, &cx);
    }
    assert_eq!(
        tree.computed_style(n).unwrap().get(Background),
        Some(css::RED.into())
    );

    // Hovered → hover branch.
    host.hovered.insert(n);
    tree.mark_dirty(n, StyleReason::style_pass());
    {
        let (interactions, anim) = cascade(&host);
        let cx = inputs(&host, &*interactions, &anim);
        tree.compute_style(n, &cx);
    }
    assert_eq!(
        tree.computed_style(n).unwrap().get(Background),
        Some(css::BLUE.into())
    );
}

#[test]
fn inherited_font_size_flows_parent_to_child() {
    let mut tree = StyleTree::new();
    let host = MockHost::new_default();
    let (interactions, anim) = cascade(&host);
    let cx = inputs(&host, &*interactions, &anim);

    let parent = tree.new_node();
    let child = tree.new_node();
    tree.set_parent(child, Some(parent));

    // Parent sets font-size; child inherits.
    tree.set_direct_style(parent, Style::new().set(FontSize, 22.0));
    tree.compute_style(parent, &cx);

    assert_eq!(tree.computed_style(parent).unwrap().get(FontSize), 22.0);
    assert_eq!(tree.computed_style(child).unwrap().get(FontSize), 22.0);
}

#[test]
fn class_context_from_ancestor_resolves_in_descendant() {
    style_class!(pub Button);

    let mut tree = StyleTree::new();
    let host = MockHost::new_default();
    let (interactions, anim) = cascade(&host);
    let cx = inputs(&host, &*interactions, &anim);

    let grandparent = tree.new_node();
    let parent = tree.new_node();
    let child = tree.new_node();
    tree.set_parent(parent, Some(grandparent));
    tree.set_parent(child, Some(parent));

    // Grandparent defines a class via its direct style.
    tree.set_direct_style(
        grandparent,
        Style::new().class(Button, |s| s.background(css::GREEN)),
    );
    // Child applies the class.
    tree.set_classes(child, &[Button::class_ref()]);

    tree.compute_style(grandparent, &cx);

    assert_eq!(
        tree.computed_style(child).unwrap().get(Background),
        Some(css::GREEN.into()),
        "class defined on grandparent should reach child via class_context"
    );
}

#[test]
fn changing_parent_inherited_marks_child_dirty_on_next_pass() {
    let mut tree = StyleTree::new();
    let host = MockHost::new_default();

    let parent = tree.new_node();
    let child = tree.new_node();
    tree.set_parent(child, Some(parent));

    tree.set_direct_style(parent, Style::new().set(TextColor, Some(css::RED)));
    {
        let (interactions, anim) = cascade(&host);
        let cx = inputs(&host, &*interactions, &anim);
        tree.compute_style(parent, &cx);
    }
    assert_eq!(
        tree.computed_style(child).unwrap().get(TextColor),
        Some(css::RED)
    );

    // Change parent's inherited prop. Child shouldn't still show red.
    tree.set_direct_style(parent, Style::new().set(TextColor, Some(css::BLUE)));
    {
        let (interactions, anim) = cascade(&host);
        let cx = inputs(&host, &*interactions, &anim);
        tree.compute_style(parent, &cx);
    }
    assert_eq!(
        tree.computed_style(child).unwrap().get(TextColor),
        Some(css::BLUE),
        "child's computed style must refresh when parent's inherited changes"
    );
}

#[test]
fn first_child_structural_selector_uses_tree_sibling_order() {
    let mut tree = StyleTree::new();
    let host = MockHost::new_default();
    let (interactions, anim) = cascade(&host);
    let cx = inputs(&host, &*interactions, &anim);

    let parent = tree.new_node();
    let a = tree.new_node();
    let b = tree.new_node();
    tree.set_children(parent, &[a, b]);

    // Each child has a style that changes background if it's the first child.
    let styled = Style::new()
        .background(css::RED)
        .first_child(|s| s.background(css::GREEN));
    tree.set_direct_style(a, styled.clone());
    tree.set_direct_style(b, styled);

    tree.compute_style(parent, &cx);

    assert_eq!(
        tree.computed_style(a).unwrap().get(Background),
        Some(css::GREEN.into()),
        "first child's :first-child branch should apply"
    );
    assert_eq!(
        tree.computed_style(b).unwrap().get(Background),
        Some(css::RED.into()),
        "non-first sibling should not match :first-child"
    );
}

#[test]
fn clean_tree_stays_clean_after_compute() {
    let mut tree = StyleTree::new();
    let host = MockHost::new_default();

    let n = tree.new_node();
    tree.set_direct_style(n, Style::new().background(css::RED));

    {
        let (interactions, anim) = cascade(&host);
        let cx = inputs(&host, &*interactions, &anim);
        tree.compute_style(n, &cx);
    }
    assert!(!tree.is_dirty(n));

    // Second pass with nothing dirty: node stays clean and
    // `take_scheduled` reports nothing new.
    {
        let (interactions, anim) = cascade(&host);
        let cx = inputs(&host, &*interactions, &anim);
        tree.compute_style(n, &cx);
    }
    assert!(!tree.is_dirty(n), "clean node should not re-enter dirty");
    assert!(
        tree.take_scheduled().next().is_none(),
        "no scheduling work when nothing dirty"
    );
}

#[test]
fn cascade_visits_each_dirty_node_and_computes_it() {
    let mut tree = StyleTree::new();
    let host = MockHost::new_default();
    let (interactions, anim) = cascade(&host);
    let cx = inputs(&host, &*interactions, &anim);

    let parent = tree.new_node();
    let c1 = tree.new_node();
    let c2 = tree.new_node();
    tree.set_children(parent, &[c1, c2]);

    tree.compute_style(parent, &cx);

    // All three nodes should be clean and have computed styles populated.
    assert!(!tree.is_dirty(parent));
    assert!(!tree.is_dirty(c1));
    assert!(!tree.is_dirty(c2));
    assert!(tree.computed_style(parent).is_some());
    assert!(tree.computed_style(c1).is_some());
    assert!(tree.computed_style(c2).is_some());
}
