use crate::{
    ViewId,
    action::add_update_message,
    animate::Animation,
    context::{
        CleanupListeners, EventCallback, InheritedInteractionCx, InteractionState, MenuCallback,
        MoveListeners, ResizeCallback, ResizeListeners,
    },
    event::EventListener,
    prop_extractor,
    responsive::ScreenSizeBp,
    style::{
        Background, BorderColorProp, BorderRadiusProp, BoxShadowProp, BoxTreeProps, CursorStyle,
        LayoutProps, Outline, OutlineColor, Style, StyleClassRef, StyleSelectors, TransformProps,
        resolve_nested_maps,
    },
    update::{CENTRAL_UPDATE_MESSAGES, UpdateMessage},
    view_storage::NodeContext,
    window_state::ScrollContext,
};
use bitflags::bitflags;
use imbl::HashSet;
use peniko::kurbo::{Affine, Point, Vec2};
use smallvec::SmallVec;
use std::{cell::RefCell, collections::HashMap, marker::PhantomData, rc::Rc};
use taffy::tree::NodeId;
use understory_box_tree::LocalNode;

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

/// View state stores internal state associated with a view which is owned and managed by Floem.
pub struct ViewState {
    pub(crate) node: NodeId,
    pub(crate) visual_id: understory_box_tree::NodeId,
    pub(crate) style: Stack<Style>,
    /// We store the stack offset to the view style to keep the api consistent but it should
    /// always be the first offset.
    pub(crate) view_style_offset: StackOffset<Style>,
    /// Layout is requested on all direct and indirect children.
    pub(crate) request_style_recursive: bool,
    pub(crate) needs_first_style: bool,
    pub(crate) has_style_selectors: StyleSelectors,
    pub(crate) scroll_offset: Vec2,
    pub(crate) scroll_ctx: ScrollContext,
    pub(crate) layout_props: LayoutProps,
    pub(crate) view_style_props: ViewStyleProps,
    pub(crate) view_transform_props: TransformProps,
    pub(crate) box_tree_props: BoxTreeProps,
    pub(crate) animations: Stack<Animation>,
    pub(crate) classes: Vec<StyleClassRef>,
    pub(crate) dragging_style: Option<Style>,
    pub(crate) style_cursor: Option<CursorStyle>,
    pub(crate) user_cursor: Option<CursorStyle>,
    /// Combine the stacked style into one style, and apply the interact state.
    pub(crate) combined_style: Style,
    /// The final style including inherited style from parent.
    pub(crate) computed_style: Style,
    /// this is the inherited properties from this view's style that this view's chilren can pull from
    pub(crate) style_cx: Option<Style>,
    pub(crate) style_interaction_cx: InheritedInteractionCx,
    pub(crate) parent_set_style_interaction_cx: InheritedInteractionCx,
    pub(crate) taffy_style: taffy::style::Style,
    pub(crate) event_listeners: HashMap<EventListener, Vec<Rc<RefCell<EventCallback>>>>,
    pub(crate) context_menu: Option<Rc<MenuCallback>>,
    pub(crate) popout_menu: Option<Rc<MenuCallback>>,
    pub(crate) resize_listeners: Rc<RefCell<ResizeListeners>>,
    pub(crate) move_listeners: Rc<RefCell<MoveListeners>>,
    pub(crate) cleanup_listeners: Rc<RefCell<CleanupListeners>>,
    pub(crate) is_hidden_state: IsHiddenState,
    pub(crate) num_waiting_animations: u16,
    pub(crate) disable_default_events: HashSet<EventListener>,
    pub(crate) transform: Affine,
    pub(crate) debug_name: SmallVec<[String; 1]>,
}

impl ViewState {
    pub(crate) fn new(
        id: ViewId,
        taffy: &mut taffy::TaffyTree<NodeContext>,
        under_tree: &mut understory_box_tree::Tree,
    ) -> Self {
        let mut style = Stack::<Style>::default();
        let view_style_offset = style.next_offset();
        style.push(Style::new());

        let visual_id = under_tree.insert(None, LocalNode::default());
        CENTRAL_UPDATE_MESSAGES
            .with_borrow_mut(|m| m.push((id, UpdateMessage::RequestStyle(visual_id))));
        CENTRAL_UPDATE_MESSAGES
            .with_borrow_mut(|m| m.push((id, UpdateMessage::RequestViewStyle(id))));

        Self {
            node: taffy.new_leaf(taffy::style::Style::DEFAULT).unwrap(),
            visual_id,
            style,
            view_style_offset,
            scroll_offset: Default::default(),
            scroll_ctx: Default::default(),
            layout_props: Default::default(),
            view_style_props: Default::default(),
            view_transform_props: Default::default(),
            box_tree_props: Default::default(),
            style_cursor: None,
            user_cursor: None,
            request_style_recursive: false,
            needs_first_style: true,
            has_style_selectors: StyleSelectors::default(),
            animations: Default::default(),
            classes: Vec::new(),
            combined_style: Style::new(),
            computed_style: Style::new(),
            style_cx: None,
            parent_set_style_interaction_cx: Default::default(),
            style_interaction_cx: Default::default(),
            taffy_style: taffy::style::Style::DEFAULT,
            dragging_style: None,
            event_listeners: HashMap::new(),
            context_menu: None,
            popout_menu: None,
            resize_listeners: Default::default(),
            move_listeners: Default::default(),
            cleanup_listeners: Default::default(),
            is_hidden_state: IsHiddenState::None,
            num_waiting_animations: 0,
            disable_default_events: HashSet::new(),
            transform: Affine::IDENTITY,
            debug_name: Default::default(),
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

    pub fn cursor(&self) -> Option<CursorStyle> {
        self.style_cursor.or(self.user_cursor)
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
