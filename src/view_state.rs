use crate::{
    animate::Animation,
    context::{
        CleanupListeners, EventCallback, InteractionState, MenuCallback, MoveListeners,
        ResizeCallback, ResizeListeners,
    },
    event::EventListener,
    pointer::PointerInputEvent,
    prop_extractor,
    responsive::ScreenSizeBp,
    style::{
        Background, BorderColorProp, BorderRadiusProp, BoxShadowProp, LayoutProps, Outline,
        OutlineColor, Style, StyleClassRef, StyleSelectors, resolve_nested_maps,
    },
};
use bitflags::bitflags;
use im_rc::HashSet;
use peniko::kurbo::{Affine, Point, Rect};
use smallvec::SmallVec;
use std::{cell::RefCell, collections::HashMap, marker::PhantomData, rc::Rc};
use taffy::tree::NodeId;

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
        const STYLE = 1;
        const LAYOUT = 1 << 1;
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
    pub(crate) requested_changes: ChangeFlags,
    pub(crate) style: Stack<Style>,
    /// Layout is requested on all direct and indirect children.
    pub(crate) request_style_recursive: bool,
    pub(crate) has_style_selectors: StyleSelectors,
    pub(crate) viewport: Option<Rect>,
    pub(crate) layout_rect: Rect,
    pub(crate) layout_props: LayoutProps,
    pub(crate) view_style_props: ViewStyleProps,
    pub(crate) animations: Stack<Animation>,
    pub(crate) classes: Vec<StyleClassRef>,
    pub(crate) dragging_style: Option<Style>,
    /// Combine the stacked style into one style, and apply the interact state
    pub(crate) combined_style: Style,
    /// The final style including inherited style from parent
    pub(crate) computed_style: Style,
    pub(crate) taffy_style: taffy::style::Style,
    pub(crate) event_listeners: HashMap<EventListener, Vec<Rc<RefCell<EventCallback>>>>,
    pub(crate) context_menu: Option<Rc<MenuCallback>>,
    pub(crate) popout_menu: Option<Rc<MenuCallback>>,
    pub(crate) resize_listeners: Rc<RefCell<ResizeListeners>>,
    pub(crate) window_origin: Point,
    pub(crate) move_listeners: Rc<RefCell<MoveListeners>>,
    pub(crate) cleanup_listeners: Rc<RefCell<CleanupListeners>>,
    pub(crate) last_pointer_down: Option<PointerInputEvent>,
    pub(crate) is_hidden_state: IsHiddenState,
    pub(crate) num_waiting_animations: u16,
    pub(crate) disable_default_events: HashSet<EventListener>,
    pub(crate) transform: Affine,
    pub(crate) debug_name: SmallVec<[String; 1]>,
}

impl ViewState {
    pub(crate) fn new(taffy: &mut taffy::TaffyTree) -> Self {
        Self {
            node: taffy.new_leaf(taffy::style::Style::DEFAULT).unwrap(),
            viewport: None,
            style: Default::default(),
            layout_rect: Rect::ZERO,
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
            transform: Affine::IDENTITY,
            debug_name: Default::default(),
        }
    }

    /// Returns `true` if a new frame is requested.
    ///
    // The context has the nested maps of classes and inherited properties
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn compute_combined(
        &mut self,
        view_style: Option<Style>,
        interact_state: InteractionState,
        screen_size_bp: ScreenSizeBp,
        view_class: Option<StyleClassRef>,
        context: &Style,
        cx_hidden: bool,
    ) -> bool {
        let mut new_frame = false;

        // Build the initial combined style
        let mut combined_style = Style::new();

        let mut classes: SmallVec<[_; 4]> = SmallVec::new();

        // Apply view class if provided
        if let Some(view_class) = view_class {
            classes.insert(0, view_class);
        }

        for class in &self.classes {
            classes.push(*class);
        }

        let mut new_context = context.clone();

        combined_style = resolve_nested_maps(
            combined_style,
            &interact_state,
            screen_size_bp,
            &classes,
            &mut new_context,
        );

        if let Some(view_style) = &view_style {
            combined_style.apply_mut(view_style.clone());
        }

        let self_style = self.style();

        combined_style.apply_mut(self_style.clone());

        combined_style = resolve_nested_maps(
            combined_style,
            &interact_state,
            screen_size_bp,
            &classes,
            &mut new_context,
        );

        // Track if this style has selectors for optimization purposes
        self.has_style_selectors = combined_style.selectors();

        // Process animations
        for animation in self
            .animations
            .stack
            .iter_mut()
            .filter(|anim| anim.can_advance() || anim.should_apply_folded())
        {
            if animation.can_advance() {
                new_frame = true;
                animation.animate_into(&mut combined_style);
                animation.advance();
            } else {
                animation.apply_folded(&mut combined_style)
            }
            debug_assert!(!animation.is_idle());
        }

        // Apply visibility
        if cx_hidden {
            combined_style = combined_style.hide();
        }

        self.combined_style = combined_style;
        new_frame
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
