//! Cascade tests for [`StyleTree`].
//!
//! Exercises `StyleTree::compute_style` end-to-end with a mock sink: the
//! cascade resolves classes + selectors + inheritance and walks parent →
//! child through the tree's own edges. If these pass, a non-floem host can
//! drive the style engine by pushing state into a `StyleTree` and running
//! `compute_style`.

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use floem_style::builtin_props::{Background, FontSize, TextColor};
use floem_style::props::StyleClass;
use floem_style::responsive::ScreenSizeBp;
use floem_style::selectors::{StyleSelector, StyleSelectors};
use floem_style::{
    CursorStyle, ElementId, Style, StyleSink, StyleTree, recalc::StyleReason,
    style_class,
};
use peniko::color::palette::css;
use understory_box_tree::{LocalNode, Tree};

// ─────────────────────────────────────────────────────────────────────────
// Minimal MockHost. Duplicated from `mock_sink.rs` (integration tests don't
// share modules); keep it small.
// ─────────────────────────────────────────────────────────────────────────

#[derive(Default)]
struct MockHost {
    hovered: std::collections::HashSet<ElementId>,
    captured: Vec<(ElementId, Style)>,
    default_inherited: Style,
    default_classes: Style,
}

impl StyleSink for MockHost {
    fn frame_start(&self) -> Instant {
        Instant::now()
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
    fn schedule_style(&mut self, _id: ElementId, _reason: StyleReason) {}
    fn schedule_style_with_target(&mut self, _target: ElementId, _reason: StyleReason) {}
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
    fn request_paint(&mut self, _id: ElementId) {}
    fn mark_needs_cursor_resolution(&mut self) {}
    fn mark_needs_layout(&mut self) {}
    fn set_cursor(&mut self, _id: ElementId, _cursor: CursorStyle) -> Option<CursorStyle> {
        None
    }
    fn clear_cursor(&mut self, _id: ElementId) -> Option<CursorStyle> {
        None
    }
    fn inspector_capture_style(&mut self, id: ElementId, computed_style: &Style) {
        self.captured.push((id, computed_style.clone()));
    }
}

fn fresh_element(tree: &mut Tree, owning: u64) -> ElementId {
    let node = tree.push_child(None, LocalNode::default());
    ElementId(node, owning, true)
}

// ─────────────────────────────────────────────────────────────────────────
// Tests.
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn single_node_direct_style_flows_to_computed() {
    let mut box_tree = Tree::new();
    let mut tree = StyleTree::new();
    let mut host = MockHost::new_default();

    let n = tree.new_node(fresh_element(&mut box_tree, 1));
    tree.set_direct_style(n, Style::new().background(css::RED));
    tree.compute_style(n, &mut host);

    let computed = tree.computed_style(n).unwrap();
    assert_eq!(computed.get(Background), Some(css::RED.into()));
}

#[test]
fn hover_selector_respects_sink_state() {
    let mut box_tree = Tree::new();
    let mut tree = StyleTree::new();
    let mut host = MockHost::new_default();

    let elem = fresh_element(&mut box_tree, 1);
    let n = tree.new_node(elem);
    tree.set_direct_style(
        n,
        Style::new()
            .background(css::RED)
            .hover(|s| s.background(css::BLUE)),
    );

    // Not hovered → base.
    tree.compute_style(n, &mut host);
    assert_eq!(
        tree.computed_style(n).unwrap().get(Background),
        Some(css::RED.into())
    );

    // Hovered → hover branch.
    host.hovered.insert(elem);
    tree.mark_dirty(n, StyleReason::style_pass());
    tree.compute_style(n, &mut host);
    assert_eq!(
        tree.computed_style(n).unwrap().get(Background),
        Some(css::BLUE.into())
    );
}

#[test]
fn inherited_font_size_flows_parent_to_child() {
    let mut box_tree = Tree::new();
    let mut tree = StyleTree::new();
    let mut host = MockHost::new_default();

    let parent = tree.new_node(fresh_element(&mut box_tree, 1));
    let child = tree.new_node(fresh_element(&mut box_tree, 2));
    tree.set_parent(child, Some(parent));

    // Parent sets font-size; child inherits.
    tree.set_direct_style(parent, Style::new().set(FontSize, 22.0));
    tree.compute_style(parent, &mut host);

    assert_eq!(tree.computed_style(parent).unwrap().get(FontSize), 22.0);
    assert_eq!(tree.computed_style(child).unwrap().get(FontSize), 22.0);
}

#[test]
fn class_context_from_ancestor_resolves_in_descendant() {
    style_class!(pub Button);

    let mut box_tree = Tree::new();
    let mut tree = StyleTree::new();
    let mut host = MockHost::new_default();

    let grandparent = tree.new_node(fresh_element(&mut box_tree, 1));
    let parent = tree.new_node(fresh_element(&mut box_tree, 2));
    let child = tree.new_node(fresh_element(&mut box_tree, 3));
    tree.set_parent(parent, Some(grandparent));
    tree.set_parent(child, Some(parent));

    // Grandparent defines a class via its direct style.
    tree.set_direct_style(
        grandparent,
        Style::new().class(Button, |s| s.background(css::GREEN)),
    );
    // Child applies the class.
    tree.set_classes(child, &[Button::class_ref()]);

    tree.compute_style(grandparent, &mut host);

    assert_eq!(
        tree.computed_style(child).unwrap().get(Background),
        Some(css::GREEN.into()),
        "class defined on grandparent should reach child via class_context"
    );
}

#[test]
fn changing_parent_inherited_marks_child_dirty_on_next_pass() {
    let mut box_tree = Tree::new();
    let mut tree = StyleTree::new();
    let mut host = MockHost::new_default();

    let parent = tree.new_node(fresh_element(&mut box_tree, 1));
    let child = tree.new_node(fresh_element(&mut box_tree, 2));
    tree.set_parent(child, Some(parent));

    tree.set_direct_style(parent, Style::new().set(TextColor, Some(css::RED)));
    tree.compute_style(parent, &mut host);
    assert_eq!(
        tree.computed_style(child).unwrap().get(TextColor),
        Some(css::RED)
    );

    // Change parent's inherited prop. Child shouldn't still show red.
    tree.set_direct_style(parent, Style::new().set(TextColor, Some(css::BLUE)));
    tree.compute_style(parent, &mut host);
    assert_eq!(
        tree.computed_style(child).unwrap().get(TextColor),
        Some(css::BLUE),
        "child's computed style must refresh when parent's inherited changes"
    );
}

#[test]
fn first_child_structural_selector_uses_tree_sibling_order() {
    let mut box_tree = Tree::new();
    let mut tree = StyleTree::new();
    let mut host = MockHost::new_default();

    let parent = tree.new_node(fresh_element(&mut box_tree, 1));
    let a = tree.new_node(fresh_element(&mut box_tree, 2));
    let b = tree.new_node(fresh_element(&mut box_tree, 3));
    tree.set_children(parent, &[a, b]);

    // Each child has a style that changes background if it's the first child.
    let styled = Style::new()
        .background(css::RED)
        .first_child(|s| s.background(css::GREEN));
    tree.set_direct_style(a, styled.clone());
    tree.set_direct_style(b, styled);

    tree.compute_style(parent, &mut host);

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
    let mut box_tree = Tree::new();
    let mut tree = StyleTree::new();
    let mut host = MockHost::new_default();

    let n = tree.new_node(fresh_element(&mut box_tree, 1));
    tree.set_direct_style(n, Style::new().background(css::RED));

    tree.compute_style(n, &mut host);
    assert!(!tree.is_dirty(n));

    // Second pass with nothing dirty: inspector_capture_style should not
    // fire again for `n`.
    let captures_before = host.captured.len();
    tree.compute_style(n, &mut host);
    assert_eq!(
        host.captured.len(),
        captures_before,
        "clean nodes should not be recomputed"
    );
}

#[test]
fn inspector_capture_fires_once_per_computed_node() {
    let mut box_tree = Tree::new();
    let mut tree = StyleTree::new();
    let mut host = MockHost::new_default();

    let parent = tree.new_node(fresh_element(&mut box_tree, 1));
    let c1 = tree.new_node(fresh_element(&mut box_tree, 2));
    let c2 = tree.new_node(fresh_element(&mut box_tree, 3));
    tree.set_children(parent, &[c1, c2]);

    tree.compute_style(parent, &mut host);
    assert_eq!(host.captured.len(), 3);
    // Order of capture is parent-first, then children in order.
    assert_eq!(host.captured[0].0, tree.get(parent).unwrap().element_id);
    assert_eq!(host.captured[1].0, tree.get(c1).unwrap().element_id);
    assert_eq!(host.captured[2].0, tree.get(c2).unwrap().element_id);
}

// Test helper.
impl MockHost {
    fn new_default() -> Self {
        Self::default()
    }
}
