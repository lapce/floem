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
        Background, BorderColorProp, BorderRadiusProp, BoxShadowProp, CursorStyle,
        InheritedInteractionCx, LayoutProps, Outline, OutlineColor, Style, StyleClassRef,
        StyleSelectors, TransformProps, recalc::StyleReasonSet,
    },
    view::LayoutTree,
};
use floem_reactive::Scope;
use imbl::HashSet;
use peniko::kurbo::{Affine, Point, Vec2};
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

#[cfg(feature = "vello")]
prop_extractor! {
    pub(crate) ViewStyleProps {
        pub border_radius: BorderRadiusProp,
        pub border_progress: crate::style::BorderProgress,

        pub outline: Outline,
        pub outline_color: OutlineColor,
        pub outline_progress: crate::style::OutlineProgress,
        pub border_color: BorderColorProp,
        pub background: Background,
        pub shadow: BoxShadowProp,
    }
}
// removing outlines to make clippy happy about progress fields not being read
#[cfg(not(feature = "vello"))]
prop_extractor! {
    pub(crate) ViewStyleProps {
        pub border_radius: BorderRadiusProp,

        pub outline: Outline,
        pub outline_color: OutlineColor,
        pub border_color: BorderColorProp,
        pub background: Background,
        pub shadow: BoxShadowProp,
    }
}

/// The current phase of visibility for enter/exit animations.
///
/// This enum tracks the display state during CSS-driven visibility transitions
/// (e.g., animating from visible to display:none).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VisibilityPhase {
    /// Initial state - display not yet computed.
    #[default]
    Initial,
    /// Visible with the given display mode.
    Visible(taffy::style::Display),
    /// Exit animation in progress.
    Animating(taffy::style::Display),
    /// Hidden (display: none).
    Hidden,
}

impl VisibilityPhase {
    pub(crate) fn get_display_override(&self) -> Option<taffy::style::Display> {
        match self {
            VisibilityPhase::Animating(dis) => Some(*dis),
            _ => None,
        }
    }

    pub(crate) fn transition(
        &mut self,
        computed_display: taffy::Display,
        remove_animations: impl FnOnce() -> bool,
        add_animations: impl FnOnce(),
        stop_reset_animations: impl FnOnce(),
        num_waiting_anim: impl FnOnce() -> u16,
    ) {
        let computed_has_hide = computed_display == taffy::Display::None;
        *self = match self {
            // initial states (makes it so that the animations aren't run on initial app/view load)
            Self::Initial if computed_has_hide => Self::Hidden,
            Self::Initial if !computed_has_hide => Self::Visible(computed_display),
            // do nothing
            Self::Visible(dis) if !computed_has_hide => Self::Visible(*dis),
            // transition to hidden
            Self::Visible(dis) if computed_has_hide => {
                let active_animations = remove_animations();
                if active_animations {
                    Self::Animating(*dis)
                } else {
                    Self::Hidden
                }
            }
            Self::Animating(_) if !computed_has_hide => {
                stop_reset_animations();
                Self::Visible(computed_display)
            }
            Self::Animating(dis) if computed_has_hide => {
                if num_waiting_anim() == 0 {
                    Self::Hidden
                } else {
                    Self::Animating(*dis)
                }
            }
            Self::Hidden if computed_has_hide => Self::Hidden,
            Self::Hidden if !computed_has_hide => {
                add_animations();
                Self::Visible(computed_display)
            }
            _ => unreachable!(),
        };
    }
}

/// Controls view visibility state.
///
/// This struct consolidates two related aspects of visibility:
/// - `phase`: CSS-driven visibility phase for enter/exit animations
/// - `force_hidden`: API-driven hiding (e.g., Tab hiding inactive children)
///
/// When `force_hidden` is true, the view is immediately hidden without animations.
#[derive(Debug, Clone, Copy, Default)]
pub struct Visibility {
    /// The current visibility phase (for enter/exit animations).
    pub phase: VisibilityPhase,
    // /// When true, view is force-hidden via set_hidden() API.
    // /// This bypasses the normal transition logic.
    // pub force_hidden: bool,
}

impl Visibility {
    /// Returns true if the view should be treated as hidden.
    pub fn is_hidden(&self) -> bool {
        self.phase == VisibilityPhase::Hidden
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
}

impl Default for StyleStack {
    fn default() -> Self {
        StyleStack {
            stack: Stack::default(),
            cache: SmallVec::new(),
            dirty_from: 0,
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
                combined.apply_mut(self.stack.stack[i].clone());
                combined
            };
        }

        self.dirty_from = len;
        self.cache[len - 1].clone()
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
    pub(crate) has_style_selectors: Option<StyleSelectors>,
    // the translation value that this view applies to children elements. Scroll view can use this to scroll.
    pub(crate) child_translation: Vec2,
    // total accumulated offset from all scroll ancestors. This is updated when updating the box tree
    pub(crate) scroll_cx: Vec2,
    pub(crate) layout_props: LayoutProps,
    pub(crate) view_style_props: ViewStyleProps,
    pub(crate) view_transform_props: TransformProps,
    pub(crate) animations: Stack<Animation>,
    pub(crate) classes: SmallVec<[StyleClassRef; 4]>,
    pub(crate) dragging_style: Option<Style>,
    pub(crate) combined_pre_animation_style: Style,
    /// The resolved style for this view (base + selectors + classes).
    /// Does NOT include inherited properties from ancestors.
    ///
    /// Use for style resolution logic (what did this view define?):
    /// - Checking if a property is explicitly set on this view
    /// - Computing class context propagation to children
    /// - Building style cache keys
    pub(crate) combined_style: Style,
    /// The final computed style including inherited properties from ancestors.
    /// This is combined_style merged with inherited context (font_size, color, etc.).
    ///
    /// Use for rendering and layout (what will the user see?):
    /// - Layout calculations via prop extractors
    /// - Visual properties (background, border, transform)
    /// - Anything that affects what gets rendered
    /// - Converting to taffy style for layout engine
    pub(crate) computed_style: Style,
    /// this can be used to make it so that a view will pull it's style context from a different parent.
    /// This is useful for overlays that are children of the window root but should pull their style cx from the creating view
    pub(crate) style_cx_parent: Option<ViewId>,
    /// The inherited properties context for children.
    /// Contains only properties marked as `inherited` (font-size, color, etc.).
    ///
    /// Derived from this view's computed_style (which includes inherited properties
    /// from ancestors). Children will merge this with their combined_style to produce
    /// their computed_style.
    pub(crate) style_cx: Style,
    /// The class context containing class definitions for descendants.
    /// Contains `.class(SomeClass, ...)` nested maps that flow down the tree.
    ///
    /// Derived from this view's combined_style (only explicitly set class definitions).
    /// Children will use this to resolve their class references when computing their
    /// combined_style.
    pub(crate) class_cx: Style,
    /// the style interaction cx that is saved after computing the final style.
    /// This will be used as the base interaction for all **children** of this view as these are the inherited interactions
    pub(crate) style_interaction_cx: InheritedInteractionCx,
    /// This interaction context can be set by a parent on this view. This will be used when building the StyleCx for **this** view.
    pub(crate) parent_set_style_interaction: InheritedInteractionCx,
    /// Controls view visibility for phase transitions.
    pub(crate) visibility: Visibility,
    /// The cursor style set by the style pass on the view. There is also the [`Self::user_cursor`] that takes precedance over this cursor.
    pub(crate) style_cursor: Option<CursorStyle>,
    /// the cursor style that a user can set on a view through the `ViewId`. This takes precedance over style_cursor.
    pub(crate) user_cursor: Option<CursorStyle>,
    pub(crate) taffy_style: taffy::style::Style,
    pub(crate) event_listeners: FxHashMap<EventListenerKey, EventListenerVec>,
    /// these are the listeners that are registered in the window state. This is used to efficiently clean up those listeners from the window state.
    pub(crate) registered_listener_keys: SmallVec<[listener::EventListenerKey; 2]>,
    pub(crate) layout_window_origin: Point,
    pub(crate) layout: Option<LayoutChanged>,
    pub(crate) visual_change: Option<VisualChanged>,
    pub(crate) context_menu: Option<Rc<MenuCallback>>,
    pub(crate) popout_menu: Option<Rc<MenuCallback>>,
    pub(crate) cleanup_listeners: Rc<RefCell<CleanupListeners>>,
    pub(crate) num_waiting_animations: u16,
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
            id,
            true,
        );
        box_tree.set_meta(element_id.0, Some(element_id));

        add_update_message(UpdateMessage::RequestStyle(
            element_id,
            StyleReasonSet::full_recalc(),
        ));
        add_update_message(UpdateMessage::RequestViewStyle(id));

        Self {
            layout_id: taffy.new_leaf(taffy::style::Style::DEFAULT).unwrap(),
            element_id,
            style,
            view_style_offset,
            layout_props: Default::default(),
            view_style_props: Default::default(),
            has_style_selectors: None,
            animations: Default::default(),
            classes: SmallVec::new(),
            combined_pre_animation_style: Style::new(),
            combined_style: Style::new(),
            computed_style: Style::new(),
            taffy_style: taffy::style::Style::DEFAULT,
            dragging_style: None,
            event_listeners: FxHashMap::default(),
            registered_listener_keys: SmallVec::new(),
            layout_window_origin: Point::ZERO,
            layout: None,
            visual_change: None,
            context_menu: None,
            popout_menu: None,
            child_translation: Vec2::ZERO,
            scroll_cx: Vec2::ZERO,
            cleanup_listeners: Default::default(),
            num_waiting_animations: 0,
            disable_default_events: HashSet::new(),
            view_transform_props: Default::default(),
            transform: Affine::IDENTITY,
            debug_name: Default::default(),
            style_cx_parent: None,
            style_cx: Style::new(),
            class_cx: Style::new(),
            style_interaction_cx: Default::default(),
            parent_set_style_interaction: Default::default(),
            visibility: Visibility::default(),
            style_cursor: None,
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
        self.style_cursor.or(self.user_cursor)
    }

    /// Compute the combined style by applying selectors, responsive styles, and classes.
    /// Returns the combined style and a flag indicating if new classes were applied.
    ///
    /// Takes two separate contexts:
    /// - `inherited_context`: Contains inherited properties (font-size, color, etc.)
    ///   Used for `with_context` evaluation and inherited prop resolution.
    /// - `class_context`: Contains class nested maps (`.class(SomeClass, ...)`)
    ///   Used for class styling that flows from ancestors.
    pub(crate) fn compute_combined(
        &mut self,
        interact_state: &mut crate::style::InteractionState,
        screen_size_bp: crate::layout::responsive::ScreenSizeBp,
        view_class: Option<crate::style::StyleClassRef>,
        inherited_context: &Style,
        class_context: &Style,
    ) {
        // Start with the combined stacked styles
        let base_style = self.style();
        let base_selectors = base_style.selectors() | class_context.selectors();

        // Build the full class list: view's classes + view type class
        let mut all_classes = self.classes.clone();
        if let Some(vc) = view_class {
            all_classes.push(vc);
        }

        // Create mutable contexts - inherited for with_context evaluation
        let inherited_ctx = inherited_context.clone();

        // Resolve all nested maps: selectors, responsive styles, and classes
        let (combined, selectors) = crate::style::resolve_nested_maps(
            base_style,
            interact_state,
            screen_size_bp,
            &all_classes,
            &inherited_ctx,
            class_context,
        );
        self.has_style_selectors = Some(selectors | base_selectors);
        self.combined_pre_animation_style = combined.clone();
    }

    pub fn apply_animations(&mut self) -> bool {
        let mut combined = self.combined_pre_animation_style.clone();
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

        self.combined_style = combined.clone();

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
