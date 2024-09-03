use crate::{
    animate::Animation,
    context::{
        EventCallback, InteractionState, MenuCallback, MoveListener, ResizeCallback, ResizeListener,
    },
    event::EventListener,
    pointer::PointerInputEvent,
    prop_extractor,
    responsive::ScreenSizeBp,
    style::{
        Background, BorderColor, BorderRadius, LayoutProps, Outline, OutlineColor, Style,
        StyleClassRef, StyleSelectors,
    },
};
use bitflags::bitflags;
use peniko::kurbo::{Point, Rect};
use smallvec::SmallVec;
use std::{cell::RefCell, collections::HashMap, marker::PhantomData, rc::Rc};
use taffy::tree::NodeId;

/// A stack of view attributes. Each entry is associated with a view decorator call.
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

prop_extractor! {
    pub(crate) ViewStyleProps {
        pub border_radius: BorderRadius,

        pub outline: Outline,
        pub outline_color: OutlineColor,
        pub border_color: BorderColor,
        pub background: Background,
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
    pub(crate) animation: Stack<Animation>,
    pub(crate) classes: Vec<StyleClassRef>,
    pub(crate) dragging_style: Option<Style>,
    pub(crate) combined_style: Style,
    pub(crate) taffy_style: taffy::style::Style,
    pub(crate) event_listeners: HashMap<EventListener, Vec<Rc<EventCallback>>>,
    pub(crate) context_menu: Option<Rc<MenuCallback>>,
    pub(crate) popout_menu: Option<Rc<MenuCallback>>,
    pub(crate) resize_listener: Option<Rc<RefCell<ResizeListener>>>,
    pub(crate) window_origin: Point,
    pub(crate) move_listener: Option<Rc<RefCell<MoveListener>>>,
    pub(crate) cleanup_listener: Option<Rc<dyn Fn()>>,
    pub(crate) last_pointer_down: Option<PointerInputEvent>,
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
            animation: Default::default(),
            classes: Vec::new(),
            combined_style: Style::new(),
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
        let mut computed_style = Style::new();
        if let Some(view_style) = view_style {
            computed_style.apply_mut(view_style);
        }
        if let Some(view_class) = view_class {
            computed_style = computed_style.apply_classes_from_context(&[view_class], context);
        }
        computed_style = computed_style
            .apply_classes_from_context(&self.classes, context)
            .apply(self.style());

        for animation in self
            .animation
            .stack
            .iter_mut()
            .filter(|anim| !(anim.is_completed() && anim.is_auto_reverse()))
        {
            new_frame = true;

            animation.animate_into(&mut computed_style);

            if animation.can_advance() {
                animation.advance();
                debug_assert!(!animation.is_idle());
            }
        }

        self.has_style_selectors = computed_style.selectors();

        computed_style.apply_interact_state(&interact_state, screen_size_bp);

        self.combined_style = computed_style;

        new_frame
    }

    pub(crate) fn has_active_animation(&self) -> bool {
        for animation in self.animation.stack.iter() {
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
            .push(Rc::new(action));
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
