use crate::{
    ViewId,
    animate::Animation,
    context::{
        CleanupListeners, EventCallback, EventListenerVec, MenuCallback, MoveListeners,
        ResizeCallback, ResizeListeners,
    },
    event::EventListener,
    message::{CENTRAL_UPDATE_MESSAGES, UpdateMessage},
    prop_extractor,
    style::InheritedInteractionCx,
    style::{
        Background, BorderColorProp, BorderRadiusProp, BoxShadowProp, LayoutProps, Outline,
        OutlineColor, Style, StyleClassRef, StyleSelectors, TransformProps,
    },
};
use bitflags::bitflags;
use floem_reactive::Scope;
use imbl::HashSet;
use peniko::kurbo::{Affine, Point, Rect};
use smallvec::SmallVec;
use std::{cell::RefCell, collections::HashMap, marker::PhantomData, rc::Rc};
use taffy::tree::NodeId;
use ui_events::pointer::PointerState;

/// A stack of view attributes. Each entry is associated with a view decorator call.
#[derive(Debug)]
pub struct Stack<T> {
    pub stack: SmallVec<[T; 1]>,
}

impl<T> Default for Stack<T> {
    fn default() -> Self {
        Stack {
            stack: SmallVec::new(),
        }
    }
}

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

bitflags! {
    #[derive(Default, Copy, Clone, Debug)]
    #[must_use]
    pub(crate) struct ChangeFlags: u8 {
        const LAYOUT = 1 << 1;
        const STYLE = 1 << 2;
        const VIEW_STYLE = 1 << 3;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsHiddenState {
    Visible(taffy::style::Display),
    AnimatingOut(taffy::style::Display),
    Hidden,
    None,
}
impl IsHiddenState {
    pub(crate) fn get_display(&self) -> Option<taffy::style::Display> {
        match self {
            IsHiddenState::AnimatingOut(dis) => Some(*dis),
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
            Self::None if computed_has_hide => Self::Hidden,
            Self::None if !computed_has_hide => Self::Visible(computed_display),
            // do nothing
            Self::Visible(dis) if !computed_has_hide => Self::Visible(*dis),
            // transition to hidden
            Self::Visible(dis) if computed_has_hide => {
                let active_animations = remove_animations();
                if active_animations {
                    Self::AnimatingOut(*dis)
                } else {
                    Self::Hidden
                }
            }
            Self::AnimatingOut(_) if !computed_has_hide => {
                stop_reset_animations();
                Self::Visible(computed_display)
            }
            Self::AnimatingOut(dis) if computed_has_hide => {
                if num_waiting_anim() == 0 {
                    Self::Hidden
                } else {
                    Self::AnimatingOut(*dis)
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

/// Information about a view's stacking context.
/// Used to determine paint order and event dispatch order.
///
/// In the simplified stacking model:
/// - Every view is implicitly a stacking context
/// - z-index only competes with siblings
/// - Children are always bounded within their parent (no "escaping")
#[derive(Debug, Clone, Copy, Default)]
pub struct StackingInfo {
    /// The effective z-index for sorting (0 if no z-index specified).
    pub effective_z_index: i32,
}

/// View state stores internal state associated with a view which is owned and managed by Floem.
pub struct ViewState {
    pub(crate) node: NodeId,
    pub(crate) requested_changes: ChangeFlags,
    pub(crate) style: Stack<Style>,
    /// We store the stack offset to the view style to keep the api consistent but it should
    /// always be the first offset.
    pub(crate) view_style_offset: StackOffset<Style>,
    /// Layout is requested on all direct and indirect children.
    pub(crate) request_style_recursive: bool,
    pub(crate) has_style_selectors: StyleSelectors,
    pub(crate) viewport: Option<Rect>,
    pub(crate) layout_rect: Rect,
    /// The visible clip area in window coordinates. This is the intersection of
    /// the view's layout_rect with all ancestor clip bounds (from overflow: hidden/scroll).
    /// Used for clip-aware hit testing - clicks outside this rect should not hit the view.
    pub(crate) clip_rect: Rect,
    pub(crate) layout_props: LayoutProps,
    pub(crate) view_style_props: ViewStyleProps,
    pub(crate) view_transform_props: TransformProps,
    pub(crate) animations: Stack<Animation>,
    pub(crate) classes: Vec<StyleClassRef>,
    pub(crate) dragging_style: Option<Style>,
    /// Combine the stacked style into one style, and apply the interact state.
    pub(crate) combined_style: Style,
    /// The final style including inherited style from parent.
    pub(crate) computed_style: Style,
    /// this can be used to make it so that a view will pull it's style context from a different parent.
    /// This is useful for overlays that are children of the window root but should pull their style cx from the creating view
    pub(crate) style_cx_parent: Option<ViewId>,
    /// the style map that has the inherited properties that the chilren should use
    pub(crate) style_cx: Option<Style>,
    /// the style interaction cx that is saved after computing the final style.
    /// This will be used as the base interaction for all **children** of this view as these are the inherited interactions
    pub(crate) style_interaction_cx: InheritedInteractionCx,
    /// This interaction context can be set by a parent on this view. This will be used when building the StyleCx for **this** view.
    pub(crate) parent_set_style_interaction: InheritedInteractionCx,
    pub(crate) taffy_style: taffy::style::Style,
    pub(crate) event_listeners: HashMap<EventListener, EventListenerVec>,
    pub(crate) context_menu: Option<Rc<MenuCallback>>,
    pub(crate) popout_menu: Option<Rc<MenuCallback>>,
    pub(crate) resize_listeners: Rc<RefCell<ResizeListeners>>,
    pub(crate) window_origin: Point,
    pub(crate) move_listeners: Rc<RefCell<MoveListeners>>,
    pub(crate) cleanup_listeners: Rc<RefCell<CleanupListeners>>,
    pub(crate) last_pointer_down: Option<PointerState>,
    pub(crate) is_hidden_state: IsHiddenState,
    pub(crate) num_waiting_animations: u16,
    pub(crate) disable_default_events: HashSet<EventListener>,
    pub(crate) transform: Affine,
    /// The cumulative transform from this view's local coordinates to root (window) coordinates.
    /// This combines the view's position (window_origin) and any CSS transforms.
    /// Use the inverse to convert from root coordinates to local coordinates.
    pub(crate) local_to_root_transform: Affine,
    pub(crate) stacking_info: StackingInfo,
    pub(crate) debug_name: SmallVec<[String; 1]>,
    /// Scope for reactive children (used by `ParentView::derived_children`).
    /// When children are updated reactively, the old scope is disposed.
    pub(crate) children_scope: Option<Scope>,
    /// Keyed children state (used by `ParentView::keyed_children`).
    /// Each child has its own scope that gets disposed when the child is removed.
    pub(crate) keyed_children: Option<Vec<(ViewId, Scope)>>,
}

impl ViewState {
    pub(crate) fn new(id: ViewId, taffy: &mut taffy::TaffyTree) -> Self {
        let mut style = Stack::<Style>::default();
        let view_style_offset = style.next_offset();
        style.push(Style::new());

        CENTRAL_UPDATE_MESSAGES.with_borrow_mut(|m| m.push((id, UpdateMessage::RequestStyle(id))));
        CENTRAL_UPDATE_MESSAGES
            .with_borrow_mut(|m| m.push((id, UpdateMessage::RequestViewStyle(id))));
        Self {
            node: taffy.new_leaf(taffy::style::Style::DEFAULT).unwrap(),
            viewport: None,
            style,
            view_style_offset,
            layout_rect: Rect::ZERO,
            clip_rect: Rect::ZERO,
            layout_props: Default::default(),
            view_style_props: Default::default(),
            requested_changes: ChangeFlags::all(),
            request_style_recursive: false,
            has_style_selectors: StyleSelectors::default(),
            animations: Default::default(),
            classes: Vec::new(),
            combined_style: Style::new(),
            computed_style: Style::new(),
            taffy_style: taffy::style::Style::DEFAULT,
            dragging_style: None,
            event_listeners: HashMap::new(),
            context_menu: None,
            popout_menu: None,
            resize_listeners: Default::default(),
            move_listeners: Default::default(),
            cleanup_listeners: Default::default(),
            last_pointer_down: None,
            window_origin: Point::ZERO,
            is_hidden_state: IsHiddenState::None,
            num_waiting_animations: 0,
            disable_default_events: HashSet::new(),
            view_transform_props: Default::default(),
            transform: Affine::IDENTITY,
            local_to_root_transform: Affine::IDENTITY,
            stacking_info: StackingInfo::default(),
            debug_name: Default::default(),
            style_cx_parent: None,
            style_cx: None,
            style_interaction_cx: Default::default(),
            parent_set_style_interaction: Default::default(),
            children_scope: None,
            keyed_children: None,
        }
    }

    pub(crate) fn has_active_animation(&self) -> bool {
        for animation in self.animations.stack.iter() {
            if animation.is_in_progress() {
                return true;
            }
        }
        false
    }

    pub(crate) fn style(&self) -> Style {
        let mut result = Style::new();
        for entry in self.style.stack.iter() {
            result.apply_mut(entry.clone());
        }
        result
    }

    /// Compute the combined style by applying selectors, responsive styles, and classes.
    /// Returns the combined style and a flag indicating if new classes were applied.
    pub(crate) fn compute_combined(
        &mut self,
        interact_state: crate::style::InteractionState,
        screen_size_bp: crate::layout::responsive::ScreenSizeBp,
        view_class: Option<crate::style::StyleClassRef>,
        parent_style: &std::rc::Rc<Style>,
    ) -> (Style, bool) {
        // Start with the combined stacked styles
        let base_style = self.style();

        // Extract and store selectors from the base style for selector detection.
        // This enables has_style_for_selector() to work correctly, including for
        // selectors defined inside with_context closures.
        self.has_style_selectors = base_style.selectors();

        // Build the full class list: view's classes + view type class
        let mut all_classes = self.classes.clone();
        if let Some(vc) = view_class {
            all_classes.push(vc);
        }

        // Create a mutable context from the parent style
        let mut context = (**parent_style).clone();

        // Resolve all nested maps: selectors, responsive styles, and classes
        let (combined, classes_applied) = crate::style::resolve_nested_maps(
            base_style,
            &interact_state,
            screen_size_bp,
            &all_classes,
            &mut context,
        );

        self.combined_style = combined.clone();
        (combined, classes_applied)
    }

    pub(crate) fn add_event_listener(
        &mut self,
        listener: EventListener,
        action: Box<EventCallback>,
    ) {
        self.event_listeners
            .entry(listener)
            .or_default()
            .push(Rc::new(RefCell::new(action)));
    }

    pub(crate) fn add_resize_listener(&mut self, action: Rc<ResizeCallback>) {
        self.resize_listeners.borrow_mut().callbacks.push(action);
    }

    pub(crate) fn add_move_listener(&mut self, action: Rc<dyn Fn(Point)>) {
        self.move_listeners.borrow_mut().callbacks.push(action);
    }

    pub(crate) fn add_cleanup_listener(&mut self, action: Rc<dyn Fn()>) {
        self.cleanup_listeners.borrow_mut().push(action);
    }
}
