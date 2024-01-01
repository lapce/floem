use crate::{
    animate::{AnimPropKind, Animation},
    context::{EventCallback, InteractionState, MenuCallback, MoveListener, ResizeListener},
    event::EventListener,
    id::{Id, ID_PATHS},
    pointer::PointerInputEvent,
    prop_extracter,
    responsive::ScreenSizeBp,
    style::{
        Background, BorderBottom, BorderColor, BorderLeft, BorderRadius, BorderRight, BorderTop,
        LayoutProps, Outline, OutlineColor, Style, StyleClassRef, StyleSelectors,
    },
    view::View,
};
use bitflags::bitflags;
use kurbo::Rect;
use smallvec::SmallVec;
use std::{collections::HashMap, marker::PhantomData, time::Duration};
use taffy::node::Node;

/// A stack of view attributes. Each entry is associated with a view decorator call.
#[derive(Default)]
pub(crate) struct Stack<T> {
    stack: SmallVec<[T; 1]>,
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
}

/// View data stores internal state associated with a view.
/// Each view is expected to own and give access to this data.
pub struct ViewData {
    pub(crate) id: Id,
    pub(crate) style: Stack<Style>,
}

impl ViewData {
    pub fn new(id: Id) -> Self {
        Self {
            id,
            style: Default::default(),
        }
    }
    pub fn id(&self) -> Id {
        self.id
    }

    pub(crate) fn style(&self) -> Style {
        let mut result = Style::new();
        for entry in self.style.stack.iter() {
            result.apply_mut(entry.clone());
        }
        result
    }
}

pub(crate) fn update_data(id: Id, root: &mut dyn View, f: impl FnOnce(&mut ViewData)) {
    pub(crate) fn update_inner(id_path: &[Id], view: &mut dyn View, f: impl FnOnce(&mut ViewData)) {
        let id = id_path[0];
        let id_path = &id_path[1..];
        if id == view.id() {
            if id_path.is_empty() {
                f(view.view_data_mut());
            } else if let Some(child) = view.child_mut(id_path[0]) {
                update_inner(id_path, child, f);
            }
        }
    }

    let id_path = ID_PATHS.with(|paths| paths.borrow().get(&id).cloned());
    if let Some(id_path) = id_path {
        update_inner(id_path.dispatch(), root, f)
    }
}

prop_extracter! {
    pub(crate) ViewStyleProps {
        pub border_left: BorderLeft,
        pub border_top: BorderTop,
        pub border_right: BorderRight,
        pub border_bottom: BorderBottom,
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
    pub(crate) node: Node,
    pub(crate) children_nodes: Vec<Node>,
    pub(crate) requested_changes: ChangeFlags,
    /// Layout is requested on all direct and indirect children.
    pub(crate) request_style_recursive: bool,
    pub(crate) has_style_selectors: StyleSelectors,
    pub(crate) viewport: Option<Rect>,
    pub(crate) layout_rect: Rect,
    pub(crate) layout_props: LayoutProps,
    pub(crate) view_style_props: ViewStyleProps,
    pub(crate) animation: Option<Animation>,
    pub(crate) class: Option<StyleClassRef>,
    pub(crate) dragging_style: Option<Style>,
    pub(crate) combined_style: Style,
    pub(crate) taffy_style: taffy::style::Style,
    pub(crate) event_listeners: HashMap<EventListener, Box<EventCallback>>,
    pub(crate) context_menu: Option<Box<MenuCallback>>,
    pub(crate) popout_menu: Option<Box<MenuCallback>>,
    pub(crate) resize_listener: Option<ResizeListener>,
    pub(crate) move_listener: Option<MoveListener>,
    pub(crate) cleanup_listener: Option<Box<dyn Fn()>>,
    pub(crate) last_pointer_down: Option<PointerInputEvent>,
}

impl ViewState {
    pub(crate) fn new(taffy: &mut taffy::Taffy) -> Self {
        Self {
            node: taffy.new_leaf(taffy::style::Style::DEFAULT).unwrap(),
            viewport: None,
            layout_rect: Rect::ZERO,
            layout_props: Default::default(),
            view_style_props: Default::default(),
            requested_changes: ChangeFlags::all(),
            request_style_recursive: false,
            has_style_selectors: StyleSelectors::default(),
            animation: None,
            class: None,
            combined_style: Style::new(),
            taffy_style: taffy::style::Style::DEFAULT,
            dragging_style: None,
            children_nodes: Vec::new(),
            event_listeners: HashMap::new(),
            context_menu: None,
            popout_menu: None,
            resize_listener: None,
            move_listener: None,
            cleanup_listener: None,
            last_pointer_down: None,
        }
    }

    /// Returns `true` if a new frame is requested.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn compute_style(
        &mut self,
        view_data: &mut ViewData,
        view_style: Option<Style>,
        interact_state: InteractionState,
        screen_size_bp: ScreenSizeBp,
        view_class: Option<StyleClassRef>,
        classes: &[StyleClassRef],
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
            .apply_classes_from_context(classes, context)
            .apply(view_data.style());

        'anim: {
            if let Some(animation) = self.animation.as_mut() {
                if animation.is_completed() && animation.is_auto_reverse() {
                    break 'anim;
                }

                new_frame = true;

                let props = animation.props();

                for kind in props.keys() {
                    let val =
                        animation.animate_prop(animation.elapsed().unwrap_or(Duration::ZERO), kind);
                    match kind {
                        AnimPropKind::Width => {
                            computed_style = computed_style.width(val.get_f32());
                        }
                        AnimPropKind::Height => {
                            computed_style = computed_style.height(val.get_f32());
                        }
                        AnimPropKind::Prop { prop } => {
                            computed_style
                                .map
                                .insert(*prop, crate::style::StyleMapValue::Val(val.get_any()));
                        }
                        AnimPropKind::Scale => todo!(),
                    }
                }

                animation.advance();
                debug_assert!(!animation.is_idle());
            }
        }

        self.has_style_selectors = computed_style.selectors();

        computed_style.apply_interact_state(&interact_state, screen_size_bp);

        self.combined_style = computed_style;

        new_frame
    }
}
