use crate::{
    ViewId,
    action::add_update_message,
    animate::Animation,
    context::{
        CleanupListeners, EventCallback, EventCallbackConfig, EventListenerVec, LayoutChanged,
        MenuCallback, VisualChanged,
    },
    event::listener::{self, EventListenerKey},
    message::UpdateMessage,
    prop_extractor,
    style::{
        Background, BorderBottomColor, BorderBottomLeftRadius, BorderBottomRightRadius,
        BorderLeftColor, BorderRightColor, BorderTopColor, BorderTopLeftRadius,
        BorderTopRightRadius, BoxShadowProp, CursorStyle, InheritedInteractionCx, Outline,
        OutlineColor, Style, StyleClassRef, StyleStorage, recalc::StyleReason,
    },
    view::LayoutTree,
};
use floem_reactive::Scope;
use imbl::HashSet;
use peniko::kurbo::{Affine, Vec2};
use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use std::{cell::RefCell, marker::PhantomData, rc::Rc};
use taffy::tree::NodeId;

/// A stack of view attributes. Each entry is associated with a view decorator call.
#[derive(Debug)]
pub struct Stack<T> {
    pub stack: SmallVec<[T; 3]>,
}

impl<T> Default for Stack<T> {
    fn default() -> Self {
        Stack {
            stack: SmallVec::new(),
        }
    }
}

#[derive(Debug)]
pub struct StackOffset<T> {
    offset: usize,
    phantom: PhantomData<T>,
}

impl<T> Clone for StackOffset<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for StackOffset<T> {}

impl<T> Stack<T> {
    pub fn next_offset(&mut self) -> StackOffset<T> {
        StackOffset {
            offset: self.stack.len(),
            phantom: PhantomData,
        }
    }
    pub fn push(&mut self, value: T) {
        self.stack.push(value);
    }
    pub fn set(&mut self, offset: StackOffset<T>, value: T) {
        self.stack[offset.offset] = value;
    }

    pub fn update(&mut self, offset: StackOffset<T>, update: impl Fn(&mut T) + 'static) {
        update(&mut self.stack[offset.offset]);
    }

    pub fn get(&mut self, offset: StackOffset<T>) -> &T {
        &self.stack[offset.offset]
    }
}

prop_extractor! {
    pub(crate) ViewStyleProps {
        pub border_top_left_radius: BorderTopLeftRadius,
        pub border_top_right_radius: BorderTopRightRadius,
        pub border_bottom_left_radius: BorderBottomLeftRadius,
        pub border_bottom_right_radius: BorderBottomRightRadius,
        pub border_progress: crate::style::BorderProgress,

        pub outline: Outline,
        pub outline_color: OutlineColor,
        pub outline_progress: crate::style::OutlineProgress,
        pub border_left_color: BorderLeftColor,
        pub border_top_color: BorderTopColor,
        pub border_right_color: BorderRightColor,
        pub border_bottom_color: BorderBottomColor,
        pub background: Background,
        pub shadow: BoxShadowProp,
    }
}

pub use floem_style::{Visibility, VisibilityPhase};

impl ViewStyleProps {
    pub fn border_radius(&self) -> crate::style::BorderRadius {
        crate::style::BorderRadius {
            top_left: Some(self.border_top_left_radius()),
            top_right: Some(self.border_top_right_radius()),
            bottom_left: Some(self.border_bottom_left_radius()),
            bottom_right: Some(self.border_bottom_right_radius()),
        }
    }

    pub fn border_color(&self) -> crate::style::BorderColor {
        crate::style::BorderColor {
            left: self.border_left_color(),
            top: self.border_top_color(),
            right: self.border_right_color(),
            bottom: self.border_bottom_color(),
        }
    }
}

/// Cached prefix-sum style stack with dirty tracking.
pub struct StyleStack {
    /// The raw per-decorator styles (same role as Stack<Style>).
    pub stack: Stack<Style>,
    /// Prefix-sum cache: `cache[i]` = stack[0..=i] all applied together.
    /// len() always equals stack.len() after a full recompute, but may be
    /// shorter when the stack is dirty (new pushes haven't been cached yet).
    cache: SmallVec<[Style; 3]>,
    /// Index of the first entry that needs recomputation.
    /// `dirty_from == stack.len()` means fully clean.
    dirty_from: usize,
    /// Cached content hash of the top-of-stack style.
    /// Computed during `style()` recomputation, avoids O(N) `content_hash()` per lookup.
    cached_content_hash: u64,
}

impl Default for StyleStack {
    fn default() -> Self {
        StyleStack {
            stack: Stack::default(),
            cache: SmallVec::new(),
            dirty_from: 0,
            cached_content_hash: 0,
        }
    }
}

impl StyleStack {
    /// Reserve a slot and return its offset (mirrors Stack::next_offset).
    pub fn next_offset(&mut self) -> StackOffset<Style> {
        let offset = self.stack.next_offset();
        self.mark_dirty(offset.offset);
        offset
    }

    pub fn push(&mut self, style: Style) {
        self.stack.push(style);
    }

    pub fn set(&mut self, offset: StackOffset<Style>, value: Style) {
        self.stack.set(offset, value);
        self.mark_dirty(offset.offset);
    }

    fn mark_dirty(&mut self, idx: usize) {
        if idx < self.dirty_from {
            self.dirty_from = idx;
            self.cache.truncate(idx);
        }
    }

    /// Recompute dirty entries and return the fully-combined style.
    pub fn style(&mut self) -> Style {
        let len = self.stack.stack.len();

        if len == 0 {
            self.cache.clear();
            self.dirty_from = 0;
            self.cached_content_hash = 0;
            return Style::new();
        }

        if self.dirty_from >= len {
            return self.cache[len - 1].clone();
        }

        let start = self.dirty_from;
        self.cache.resize_with(len, Style::new);

        for i in start..len {
            self.cache[i] = if i == 0 {
                self.stack.stack[0].clone()
            } else {
                let mut combined = self.cache[i - 1].clone();
                combined.apply_mut(&self.stack.stack[i]);
                combined
            };
        }

        self.dirty_from = len;
        // Cache the content hash while we have the computed style
        self.cached_content_hash = self.cache[len - 1].content_hash();
        self.cache[len - 1].clone()
    }

    /// Ensure the style stack cache is up-to-date (recompute if dirty)
    /// without returning a clone.
    pub fn ensure_clean(&mut self) {
        if self.dirty_from < self.stack.stack.len() {
            let _ = self.style(); // recompute and discard the clone
        }
    }

    /// The content hash of the top-of-stack style. Must call `ensure_clean()` first.
    /// O(1) field read — the hash is computed during `style()` recomputation.
    pub fn content_hash(&self) -> u64 {
        self.cached_content_hash
    }

    /// Whether the top-of-stack style is cacheable (no structural selectors, no context values).
    pub fn is_cacheable(&self) -> bool {
        self.cache.last().is_some_and(|s| {
            !s.map.is_empty() && !s.has_structural_selectors() && !s.has_context_values()
        })
    }
}

/// View state stores internal state associated with a view which is owned and managed by Floem.
pub struct ViewState {
    pub(crate) layout_id: NodeId,
    pub(crate) element_id: crate::ElementId,
    pub(crate) style: StyleStack,
    /// We store the stack offset to the view style to keep the api consistent but it should
    /// always be the first offset.
    pub(crate) view_style_offset: StackOffset<Style>,
    // the translation value that this view applies to children elements. Scroll view can use this to scroll.
    pub(crate) child_translation: Vec2,
    pub(crate) animations: Stack<Animation>,
    pub(crate) classes: SmallVec<[StyleClassRef; 4]>,
    pub(crate) dragging_style: Option<Style>,
    /// Engine-owned per-node style state (resolved styles, extracted props,
    /// visibility phase, interaction cx, etc.). See [`StyleStorage`].
    pub(crate) style_storage: StyleStorage,
    /// Companion node in [`WindowState::style_tree`](crate::window::state::WindowState).
    /// Populated when the view is first seen by the window and cleared on
    /// teardown. Phase 2a wires lifecycle only; style data and cascade still
    /// live in `StyleStorage` / `StyleCx`.
    pub(crate) style_node: Option<floem_style::StyleNodeId>,
    /// this can be used to make it so that a view will pull it's style context from a different parent.
    /// This is useful for overlays that are children of the window root but should pull their style cx from the creating view
    pub(crate) style_cx_parent: Option<ViewId>,
    /// the cursor style that a user can set on a view through the `ViewId`. This takes precedance over `style_storage.style_cursor`.
    pub(crate) user_cursor: Option<CursorStyle>,
    pub(crate) taffy_style: taffy::style::Style,
    pub(crate) event_listeners: FxHashMap<EventListenerKey, EventListenerVec>,
    /// these are the listeners that are registered in the window state. This is used to efficiently clean up those listeners from the window state.
    pub(crate) registered_listener_keys: SmallVec<[listener::EventListenerKey; 2]>,
    pub(crate) layout: Option<LayoutChanged>,
    pub(crate) visual_change: Option<VisualChanged>,
    pub(crate) context_menu: Option<Rc<MenuCallback>>,
    pub(crate) popout_menu: Option<Rc<MenuCallback>>,
    pub(crate) cleanup_listeners: Rc<RefCell<CleanupListeners>>,
    pub(crate) disable_default_events: HashSet<EventListenerKey>,
    /// This transform is user settable and is a transfrom that is applied after the transfrom from the `view_transform_props` which is the transfrom applied by style properties.
    pub(crate) transform: Affine,
    pub(crate) debug_name: SmallVec<[String; 1]>,
    /// Scope for reactive children (used by `ParentView::derived_children`).
    /// When children are updated reactively, the old scope is disposed.
    pub(crate) children_scope: Option<Scope>,
    /// Keyed children state (used by `ParentView::keyed_children`).
    /// Each child has its own scope that gets disposed when the child is removed.
    pub(crate) keyed_children: Option<Vec<(ViewId, Scope)>>,
    /// The scope associated with this view, if any.
    /// Views that provide context to children should set this scope.
    /// When set, children can access context provided in this scope.
    /// The scope hierarchy is kept in sync with the view hierarchy for proper cleanup.
    pub(crate) scope: Option<Scope>,
}

impl ViewState {
    pub(crate) fn new(id: ViewId, taffy: &mut LayoutTree, box_tree: &mut crate::BoxTree) -> Self {
        let mut style = StyleStack::default();
        let view_style_offset = style.next_offset();
        style.push(Style::new());

        let element_id = crate::ElementId(
            box_tree.push_child(None, understory_box_tree::LocalNode::default()),
            id.as_raw(),
            true,
        );
        box_tree.set_element_meta(element_id.0, Some(crate::ElementMeta::new(element_id)));

        add_update_message(UpdateMessage::RequestStyle(
            element_id,
            StyleReason::full_recalc(),
        ));

        Self {
            layout_id: taffy.new_leaf(taffy::style::Style::DEFAULT).unwrap(),
            element_id,
            style,
            view_style_offset,
            animations: Default::default(),
            classes: SmallVec::new(),
            taffy_style: taffy::style::Style::DEFAULT,
            dragging_style: None,
            style_storage: StyleStorage::default(),
            style_node: None,
            event_listeners: FxHashMap::default(),
            registered_listener_keys: SmallVec::new(),
            layout: None,
            visual_change: None,
            context_menu: None,
            popout_menu: None,
            child_translation: Vec2::ZERO,
            cleanup_listeners: Default::default(),
            disable_default_events: HashSet::new(),
            transform: Affine::IDENTITY,
            debug_name: Default::default(),
            style_cx_parent: None,
            user_cursor: None,
            children_scope: None,
            keyed_children: None,
            scope: None,
        }
    }

    pub(crate) fn style(&mut self) -> Style {
        self.style.style()
    }

    pub fn cursor(&self) -> Option<CursorStyle> {
        self.style_storage.style_cursor.or(self.user_cursor)
    }

    pub fn apply_animations(
        &mut self,
        interact_state: &mut crate::style::InteractionState,
    ) -> bool {
        let mut combined = self.style_storage.combined_pre_animation_style.clone();
        // ─────────────────────────────────────────────────────────────────────
        // Process animations
        // ─────────────────────────────────────────────────────────────────────
        // Animations modify the computed style by interpolating between keyframe values.
        // We process animations here, after the base style is computed but before
        // it's stored, so animated values override static style values.
        let mut has_active_animation = false;
        {
            for animation in self
                .animations
                .stack
                .iter_mut()
                .filter(|anim| anim.can_advance() || anim.should_apply_folded())
            {
                if animation.can_advance() {
                    has_active_animation = true;
                    animation.animate_into(&mut combined);
                    animation.advance();
                } else {
                    animation.apply_folded(&mut combined);
                }
                debug_assert!(
                    !animation.is_idle(),
                    "Animation should not be idle after processing"
                );
            }
        }

        interact_state.is_hidden |= combined.builtin().display() == taffy::Display::None;
        interact_state.is_selected |= combined.builtin().set_selected();
        interact_state.is_disabled |= combined.builtin().set_disabled();
        self.style_storage.post_compute_combined_interaction = InheritedInteractionCx {
            hidden: combined.builtin().display() == taffy::Display::None,
            selected: combined.builtin().set_selected(),
            disabled: combined.builtin().set_disabled(),
        };

        self.style_storage.combined_style = combined;

        has_active_animation
    }

    pub(crate) fn add_event_listener(
        &mut self,
        listener: listener::EventListenerKey,
        action: Box<EventCallback>,
        config: EventCallbackConfig,
    ) {
        self.event_listeners
            .entry(listener)
            .or_default()
            .push((Rc::new(RefCell::new(action)), config));
    }

    pub(crate) fn add_cleanup_listener(&mut self, action: Rc<dyn Fn()>) {
        self.cleanup_listeners.borrow_mut().push(action);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use taffy::Display;

    // =========================================================================
    // VisibilityPhase Unit Tests
    // =========================================================================

    /// Test Initial → Visible transition when display is not none.
    #[test]
    fn test_phase_initial_to_visible() {
        let mut phase = VisibilityPhase::Initial;

        phase.transition(
            Display::Flex,
            || false, // no animations
            || {},
            || {},
            || 0,
        );

        assert_eq!(phase, VisibilityPhase::Visible(Display::Flex));
    }

    /// Test Initial → Hidden transition when display is none.
    #[test]
    fn test_phase_initial_to_hidden() {
        let mut phase = VisibilityPhase::Initial;

        phase.transition(Display::None, || false, || {}, || {}, || 0);

        assert_eq!(phase, VisibilityPhase::Hidden);
    }

    /// Test Visible → Hidden transition when display changes to none (no animations).
    #[test]
    fn test_phase_visible_to_hidden_no_animation() {
        let mut phase = VisibilityPhase::Visible(Display::Flex);

        phase.transition(
            Display::None,
            || false, // no animations to run
            || {},
            || {},
            || 0,
        );

        assert_eq!(phase, VisibilityPhase::Hidden);
    }

    /// Test Visible → Animating transition when display changes to none (with animations).
    #[test]
    fn test_phase_visible_to_animating_with_animation() {
        let mut phase = VisibilityPhase::Visible(Display::Flex);

        phase.transition(
            Display::None,
            || true, // has animations to run
            || {},
            || {},
            || 1,
        );

        // Should enter Animating phase, preserving the original display
        assert_eq!(phase, VisibilityPhase::Animating(Display::Flex));
    }

    /// Test Animating → Hidden transition when animations complete.
    #[test]
    fn test_phase_animating_to_hidden_when_complete() {
        let mut phase = VisibilityPhase::Animating(Display::Flex);

        phase.transition(
            Display::None,
            || false,
            || {},
            || {},
            || 0, // no waiting animations
        );

        assert_eq!(phase, VisibilityPhase::Hidden);
    }

    /// Test Animating stays Animating while animations are running.
    #[test]
    fn test_phase_animating_stays_while_running() {
        let mut phase = VisibilityPhase::Animating(Display::Flex);

        phase.transition(
            Display::None,
            || false,
            || {},
            || {},
            || 1, // still has waiting animations
        );

        assert_eq!(phase, VisibilityPhase::Animating(Display::Flex));
    }

    /// Test Animating → Visible when display changes back during animation.
    #[test]
    fn test_phase_animating_to_visible_on_cancel() {
        let mut phase = VisibilityPhase::Animating(Display::Flex);
        let mut stop_called = false;

        phase.transition(
            Display::Block, // display changed back to visible
            || false,
            || {},
            || {
                stop_called = true;
            },
            || 1,
        );

        assert!(stop_called, "stop_reset_animations should be called");
        assert_eq!(phase, VisibilityPhase::Visible(Display::Block));
    }

    /// Test Hidden → Visible transition when display changes from none.
    #[test]
    fn test_phase_hidden_to_visible() {
        let mut phase = VisibilityPhase::Hidden;
        let mut add_called = false;

        phase.transition(
            Display::Flex,
            || false,
            || {
                add_called = true;
            },
            || {},
            || 0,
        );

        assert!(add_called, "add_animations should be called");
        assert_eq!(phase, VisibilityPhase::Visible(Display::Flex));
    }

    /// Test Hidden stays Hidden when display is still none.
    #[test]
    fn test_phase_hidden_stays_hidden() {
        let mut phase = VisibilityPhase::Hidden;

        phase.transition(Display::None, || false, || {}, || {}, || 0);

        assert_eq!(phase, VisibilityPhase::Hidden);
    }

    /// Test get_display() returns the preserved display during Animating phase.
    #[test]
    fn test_get_display_during_animating() {
        let phase = VisibilityPhase::Animating(Display::Flex);
        assert_eq!(phase.get_display_override(), Some(Display::Flex));

        let phase = VisibilityPhase::Animating(Display::Block);
        assert_eq!(phase.get_display_override(), Some(Display::Block));
    }

    /// Test get_display() returns None for non-Animating phases.
    #[test]
    fn test_get_display_for_other_phases() {
        assert_eq!(VisibilityPhase::Initial.get_display_override(), None);
        assert_eq!(
            VisibilityPhase::Visible(Display::Flex).get_display_override(),
            None
        );
        assert_eq!(VisibilityPhase::Hidden.get_display_override(), None);
    }

    /// Test Visible stays Visible when display changes to different visible value.
    #[test]
    fn test_phase_visible_stays_with_different_display() {
        let mut phase = VisibilityPhase::Visible(Display::Flex);

        phase.transition(
            Display::Block, // different display but still visible
            || false,
            || {},
            || {},
            || 0,
        );

        // Should stay Visible but with the original display (Flex)
        // This is because the transition doesn't update the display value when staying visible
        assert_eq!(phase, VisibilityPhase::Visible(Display::Flex));
    }
}
