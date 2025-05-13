use crate::{
    animate::Animation,
    context::{
        EventCallback, InteractionState, MenuCallback, MoveListener, ResizeCallback, ResizeListener,
    },
    event::EventListener,
    prop_extractor,
    responsive::ScreenSizeBp,
    style::{
        Background, BorderBottomColor, BorderBottomLeftRadius, BorderBottomRightRadius,
        BorderLeftColor, BorderRightColor, BorderTopColor, BorderTopLeftRadius,
        BorderTopRightRadius, BoxShadowProp, LayoutProps, Outline, OutlineColor, Style,
        StyleClassRef, StyleSelectors,
    },
};
use bitflags::bitflags;
use im::HashSet;
use peniko::kurbo::{Affine, Point, Rect};
use smallvec::SmallVec;
use std::{cell::RefCell, collections::HashMap, marker::PhantomData, rc::Rc};
use taffy::tree::NodeId;
use ui_events::pointer::PointerState;

/// A stack of view attributes. Each entry is associated with a view decorator call.
#[derive(Debug)]
pub(crate) struct Stack<T> {
    pub(crate) stack: SmallVec<[T; 1]>,
}

impl<T> Default for Stack<T> {
    fn default() -> Self {
        Stack {
            stack: SmallVec::new(),
        }
    }
}

pub(crate) struct StackOffset<T> {
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
}

#[cfg(feature = "vello")]
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
// removing outlines to make clippy happy about progress fields not being read
#[cfg(not(feature = "vello"))]
prop_extractor! {
    pub(crate) ViewStyleProps {
        pub border_top_left_radius: BorderTopLeftRadius,
        pub border_top_right_radius: BorderTopRightRadius,
        pub border_bottom_left_radius: BorderBottomLeftRadius,
        pub border_bottom_right_radius: BorderBottomRightRadius,

        pub outline: Outline,
        pub outline_color: OutlineColor,
        pub border_left_color: BorderLeftColor,
        pub border_top_color: BorderTopColor,
        pub border_right_color: BorderRightColor,
        pub border_bottom_color: BorderBottomColor,
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
    pub(crate) resize_listener: Option<Rc<RefCell<ResizeListener>>>,
    pub(crate) window_origin: Point,
    pub(crate) move_listener: Option<Rc<RefCell<MoveListener>>>,
    pub(crate) cleanup_listener: Option<Rc<dyn Fn()>>,
    pub(crate) last_pointer_down: Option<PointerState<Point>>,
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
            resize_listener: None,
            move_listener: None,
            cleanup_listener: None,
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
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn compute_style(
        &mut self,
        view_style: Option<Style>,
        interact_state: InteractionState,
        screen_size_bp: ScreenSizeBp,
        view_class: Option<StyleClassRef>,
        context: &Style,
    ) -> bool {
        let mut new_frame = false;
        // we are just using the combined style and then clearing here to avoid creating an entirely new style map
        // because the clone is cheap, this is fine
        let mut computed_style = self.combined_style.clone();
        computed_style.clear();
        // we will apply the views style to the context so that if a style class is used on a view, that class will be directly applied instead of only applying to children
        let mut context = context.clone();
        if let Some(view_class) = view_class {
            computed_style = computed_style.apply_classes_from_context(&[view_class], &context);
        }
        computed_style = computed_style.apply_classes_from_context(&self.classes, &context);

        if let Some(view_style) = view_style {
            context.apply_mut(view_style.clone());
            computed_style.apply_mut(view_style);
        }
        // self.style has precedence over the supplied view style so it comes after
        let self_style = self.style();
        context.apply_mut(self_style.clone());
        computed_style.apply_mut(self_style);

        self.has_style_selectors = computed_style.selectors();

        computed_style.apply_interact_state(&interact_state, screen_size_bp);

        for animation in self
            .animations
            .stack
            .iter_mut()
            .filter(|anim| anim.can_advance() || anim.should_apply_folded())
        {
            if animation.can_advance() {
                new_frame = true;

                animation.animate_into(&mut computed_style);

                animation.advance();
            } else {
                animation.apply_folded(&mut computed_style)
            }
            debug_assert!(!animation.is_idle());
        }

        self.combined_style = computed_style;

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

    pub(crate) fn update_resize_listener(&mut self, action: Box<ResizeCallback>) {
        self.resize_listener = Some(Rc::new(RefCell::new(ResizeListener {
            rect: Rect::ZERO,
            callback: action,
        })));
    }

    pub(crate) fn update_move_listener(&mut self, action: Box<dyn Fn(Point)>) {
        self.move_listener = Some(Rc::new(RefCell::new(MoveListener {
            window_origin: Point::ZERO,
            callback: action,
        })));
    }

    pub(crate) fn update_cleanup_listener(&mut self, action: impl Fn() + 'static) {
        self.cleanup_listener = Some(Rc::new(action));
    }
}
