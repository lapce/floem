use std::{
    any::Any,
    collections::{HashMap, HashSet},
    ops::{Deref, DerefMut},
    rc::Rc,
    time::{Duration, Instant},
};

use floem_renderer::{
    cosmic_text::{LineHeightValue, Style as FontStyle, Weight},
    Renderer as FloemRenderer,
};
use kurbo::{Affine, Insets, Point, Rect, RoundedRect, Shape, Size, Vec2};
use peniko::Color;
use taffy::{
    prelude::{Layout, Node},
    style::{AvailableSpace, Display},
};
use winit::window::CursorIcon;

use crate::{
    animate::{AnimId, AnimPropKind, Animation},
    event::{Event, EventListener},
    id::Id,
    inspector::CaptureState,
    menu::Menu,
    pointer::PointerInputEvent,
    prop_extracter,
    responsive::{GridBreakpoints, ScreenSizeBp},
    style::{
        Background, BorderBottom, BorderColor, BorderLeft, BorderRadius, BorderRight, BorderTop,
        BuiltinStyle, CursorStyle, DisplayProp, LayoutProps, Style, StyleClassRef, StyleProp,
        StyleSelector, StyleSelectors,
    },
    unit::PxPct,
    view::{ChangeFlags, View},
};

pub type EventCallback = dyn Fn(&Event) -> bool;
pub type ResizeCallback = dyn Fn(Rect);
pub type MenuCallback = dyn Fn() -> Menu;

pub(crate) struct ResizeListener {
    pub(crate) rect: Rect,
    pub(crate) callback: Box<ResizeCallback>,
}

/// The listener when the view is got moved to a different position in the window
pub(crate) struct MoveListener {
    pub(crate) window_origin: Point,
    pub(crate) callback: Box<dyn Fn(Point)>,
}

prop_extracter! {
    pub(crate) ViewStyleProps {
        pub border_left: BorderLeft,
        pub border_top: BorderTop,
        pub border_right: BorderRight,
        pub border_bottom: BorderBottom,
        pub border_radius: BorderRadius,

        pub border_color: BorderColor,
        pub background: Background,
    }
}

pub struct ViewState {
    pub(crate) node: Node,
    pub(crate) children_nodes: Vec<Node>,
    pub(crate) request_layout: bool,
    pub(crate) request_style: bool,
    /// Layout is requested on all direct and indirect children.
    pub(crate) request_style_recursive: bool,
    pub(crate) has_style_selectors: StyleSelectors,
    pub(crate) viewport: Option<Rect>,
    pub(crate) layout_rect: Rect,
    pub(crate) layout_props: LayoutProps,
    pub(crate) view_style_props: ViewStyleProps,
    pub(crate) animation: Option<Animation>,
    pub(crate) base_style: Option<Style>,
    pub(crate) style: Style,
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
    fn new(taffy: &mut taffy::Taffy) -> Self {
        Self {
            node: taffy.new_leaf(taffy::style::Style::DEFAULT).unwrap(),
            viewport: None,
            layout_rect: Rect::ZERO,
            layout_props: Default::default(),
            view_style_props: Default::default(),
            request_layout: true,
            request_style: true,
            request_style_recursive: false,
            has_style_selectors: StyleSelectors::default(),
            animation: None,
            base_style: None,
            style: Style::new(),
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

    pub(crate) fn compute_style(
        &mut self,
        view_style: Option<Style>,
        interact_state: InteractionState,
        screen_size_bp: ScreenSizeBp,
        view_class: Option<StyleClassRef>,
        classes: &[StyleClassRef],
        context: &Style,
    ) {
        let mut computed_style = Style::new();
        if let Some(view_style) = view_style {
            computed_style.apply_mut(view_style);
        }
        if let Some(view_class) = view_class {
            computed_style = computed_style.apply_classes_from_context(&[view_class], context);
        }
        if let Some(base_style) = self.base_style.clone() {
            computed_style.apply_mut(base_style);
        }
        computed_style = computed_style
            .apply_classes_from_context(classes, context)
            .apply(self.style.clone());

        'anim: {
            if let Some(animation) = self.animation.as_mut() {
                if animation.is_completed() && animation.is_auto_reverse() {
                    break 'anim;
                }

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
    }
}

pub struct DragState {
    pub(crate) id: Id,
    pub(crate) offset: Vec2,
    pub(crate) released_at: Option<std::time::Instant>,
}

/// Encapsulates and owns the global state of the application,
/// including the `ViewState` of each view.
pub struct AppState {
    /// keyboard focus
    pub(crate) focus: Option<Id>,
    /// when a view is active, it gets mouse event even when the mouse is
    /// not on it
    pub(crate) active: Option<Id>,
    pub(crate) root: Option<Node>,
    pub(crate) root_size: Size,
    pub(crate) scale: f64,
    pub taffy: taffy::Taffy,
    pub(crate) view_states: HashMap<Id, ViewState>,
    stale_view_state: ViewState,
    pub(crate) disabled: HashSet<Id>,
    pub(crate) keyboard_navigable: HashSet<Id>,
    pub(crate) draggable: HashSet<Id>,
    pub(crate) dragging: Option<DragState>,
    pub(crate) drag_start: Option<(Id, Point)>,
    pub(crate) dragging_over: HashSet<Id>,
    pub(crate) screen_size_bp: ScreenSizeBp,
    pub(crate) grid_bps: GridBreakpoints,
    pub(crate) clicking: HashSet<Id>,
    pub(crate) hovered: HashSet<Id>,
    /// This keeps track of all views that have an animation,
    /// regardless of the status of the animation
    pub(crate) animated: HashSet<Id>,
    /// This is a set of view which have active transition animations.
    pub(crate) transitioning: HashSet<Id>,
    pub(crate) cursor: Option<CursorStyle>,
    pub(crate) last_cursor: CursorIcon,
    pub(crate) keyboard_navigation: bool,
    pub(crate) window_menu: HashMap<usize, Box<dyn Fn()>>,
    pub(crate) context_menu: HashMap<usize, Box<dyn Fn()>>,

    /// This is set if we're currently capturing the window for the inspector.
    pub(crate) capture: Option<CaptureState>,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new() -> Self {
        let mut taffy = taffy::Taffy::new();
        taffy.disable_rounding();
        Self {
            root: None,
            focus: None,
            active: None,
            scale: 1.0,
            root_size: Size::ZERO,
            screen_size_bp: ScreenSizeBp::Xs,
            stale_view_state: ViewState::new(&mut taffy),
            taffy,
            view_states: HashMap::new(),
            animated: HashSet::new(),
            transitioning: HashSet::new(),
            disabled: HashSet::new(),
            keyboard_navigable: HashSet::new(),
            draggable: HashSet::new(),
            dragging: None,
            drag_start: None,
            dragging_over: HashSet::new(),
            clicking: HashSet::new(),
            hovered: HashSet::new(),
            cursor: None,
            last_cursor: CursorIcon::Default,
            keyboard_navigation: false,
            grid_bps: GridBreakpoints::default(),
            window_menu: HashMap::new(),
            context_menu: HashMap::new(),
            capture: None,
        }
    }

    pub fn view_state(&mut self, id: Id) -> &mut ViewState {
        if !id.has_id_path() {
            // if the id doesn't have a id path, that means it's been cleaned up,
            // so we shouldn't create a new ViewState for this Id.
            return &mut self.stale_view_state;
        }
        self.view_states
            .entry(id)
            .or_insert_with(|| ViewState::new(&mut self.taffy))
    }

    /// This removes a view from the app state.
    pub fn remove_view(&mut self, view: &mut dyn View) {
        for child in view.children_mut() {
            self.remove_view(child);
        }
        let id = view.id();
        let view_state = self.view_state(id);
        if let Some(action) = view_state.cleanup_listener.as_ref() {
            action();
        }
        let node = view_state.node;
        if let Ok(children) = self.taffy.children(node) {
            for child in children {
                let _ = self.taffy.remove(child);
            }
        }
        let _ = self.taffy.remove(node);
        id.remove_id_path();
        self.view_states.remove(&id);
        self.disabled.remove(&id);
        self.keyboard_navigable.remove(&id);
        self.draggable.remove(&id);
        self.dragging_over.remove(&id);
        self.clicking.remove(&id);
        self.hovered.remove(&id);
        self.animated.remove(&id);
        self.transitioning.remove(&id);
        self.clicking.remove(&id);
        if self.focus == Some(id) {
            self.focus = None;
        }
        if self.active == Some(id) {
            self.active = None;
        }
    }

    pub fn ids_with_anim_in_progress(&mut self) -> Vec<Id> {
        self.animated
            .clone()
            .into_iter()
            .filter(|id| {
                let anim = &self.view_state(*id).animation;
                if let Some(anim) = anim {
                    return !anim.is_completed();
                }
                false
            })
            .collect()
    }

    pub fn is_hidden(&self, id: Id) -> bool {
        self.view_states
            .get(&id)
            .map(|s| s.combined_style.get(DisplayProp) == Display::None)
            .unwrap_or(false)
    }

    /// Is this view, or any parent view, marked as hidden
    pub fn is_hidden_recursive(&self, id: Id) -> bool {
        let mut ancestor = Some(id);
        while let Some(current_ancestor) = ancestor {
            if self.is_hidden(current_ancestor) {
                return true;
            }
            ancestor = current_ancestor.parent();
        }

        false
    }

    pub fn is_hovered(&self, id: &Id) -> bool {
        self.hovered.contains(id)
    }

    pub fn is_disabled(&self, id: &Id) -> bool {
        self.disabled.contains(id)
    }

    pub fn is_focused(&self, id: &Id) -> bool {
        self.focus.map(|f| &f == id).unwrap_or(false)
    }

    pub fn is_active(&self, id: &Id) -> bool {
        self.active.map(|a| &a == id).unwrap_or(false)
    }

    pub fn is_clicking(&self, id: &Id) -> bool {
        self.clicking.contains(id)
    }

    pub fn is_dragging(&self) -> bool {
        self.dragging
            .as_ref()
            .map(|d| d.released_at.is_none())
            .unwrap_or(false)
    }

    pub fn get_interact_state(&self, id: &Id) -> InteractionState {
        InteractionState {
            is_hovered: self.is_hovered(id),
            is_disabled: self.is_disabled(id),
            is_focused: self.is_focused(id),
            is_clicking: self.is_clicking(id),
            using_keyboard_navigation: self.keyboard_navigation,
        }
    }

    pub fn set_root_size(&mut self, size: Size) {
        self.root_size = size;
        self.compute_layout();
    }

    pub(crate) fn compute_style(
        &mut self,
        id: Id,
        view_style: Option<Style>,
        view_class: Option<StyleClassRef>,
        classes: &[StyleClassRef],
        context: &Style,
    ) {
        let interact_state = self.get_interact_state(&id);
        let screen_size_bp = self.screen_size_bp;
        let view_state = self.view_state(id);
        view_state.compute_style(
            view_style,
            interact_state,
            screen_size_bp,
            view_class,
            classes,
            context,
        );
    }

    pub(crate) fn get_computed_style(&mut self, id: Id) -> &Style {
        let view_state = self.view_state(id);
        &view_state.combined_style
    }

    pub(crate) fn get_builtin_style(&mut self, id: Id) -> BuiltinStyle<'_> {
        self.get_computed_style(id).builtin()
    }

    pub fn compute_layout(&mut self) {
        if let Some(root) = self.root {
            let _ = self.taffy.compute_layout(
                root,
                taffy::prelude::Size {
                    width: AvailableSpace::Definite((self.root_size.width / self.scale) as f32),
                    height: AvailableSpace::Definite((self.root_size.height / self.scale) as f32),
                },
            );
        }
    }

    /// Requests style for a view and all direct and indirect children.
    pub(crate) fn request_style_recursive(&mut self, id: Id) {
        let view = self.view_state(id);
        view.request_style_recursive = true;
        self.request_style(id);
    }

    pub fn request_style(&mut self, id: Id) {
        let view = self.view_state(id);
        if view.request_style {
            return;
        }
        view.request_style = true;
        if let Some(parent) = id.parent() {
            self.request_style(parent);
        }
    }

    pub fn request_layout(&mut self, id: Id) {
        let view = self.view_state(id);
        if view.request_layout {
            return;
        }
        view.request_layout = true;
        if let Some(parent) = id.parent() {
            self.request_layout(parent);
        }
    }

    pub fn request_paint(&mut self, id: Id) {
        // Painting currently happens on any change, so request styling on the root
        // view.
        if let Some(id) = id.id_path().and_then(|path| path.0.get(1).copied()) {
            self.request_style(id)
        }
    }

    pub(crate) fn set_viewport(&mut self, id: Id, viewport: Rect) {
        let view = self.view_state(id);
        view.viewport = Some(viewport);
    }

    pub(crate) fn get_layout(&self, id: Id) -> Option<Layout> {
        self.view_states
            .get(&id)
            .map(|view| view.node)
            .and_then(|node| self.taffy.layout(node).ok())
            .copied()
    }

    pub(crate) fn get_content_rect(&mut self, id: Id) -> Rect {
        let view_state = self.view_state(id);
        let rect = view_state.layout_rect;
        let props = &view_state.layout_props;
        let pixels = |px_pct, abs| match px_pct {
            PxPct::Px(v) => v,
            PxPct::Pct(pct) => pct * abs,
        };
        let origin = rect.origin();
        rect.inset(-Insets {
            x0: props.border_left().0 + pixels(props.padding_left(), rect.width()),
            x1: props.border_right().0 + pixels(props.padding_right(), rect.width()),
            y0: props.border_top().0 + pixels(props.padding_top(), rect.height()),
            y1: props.border_bottom().0 + pixels(props.padding_bottom(), rect.height()),
        }) - origin.to_vec2()
    }

    pub(crate) fn get_layout_rect(&mut self, id: Id) -> Rect {
        self.view_state(id).layout_rect
    }

    pub(crate) fn update_active(&mut self, id: Id) {
        if self.active.is_some() {
            // the first update_active wins, so if there's active set,
            // don't do anything.
            return;
        }
        self.active = Some(id);

        // To apply the styles of the Active selector
        if self.has_style_for_sel(id, StyleSelector::Active) {
            self.request_style(id);
        }
    }

    pub(crate) fn update_screen_size_bp(&mut self, size: Size) {
        let bp = self.grid_bps.get_width_bp(size.width);
        self.screen_size_bp = bp;
    }

    pub(crate) fn clear_focus(&mut self) {
        if let Some(old_id) = self.focus {
            // To remove the styles applied by the Focus selector
            if self.has_style_for_sel(old_id, StyleSelector::Focus) {
                self.request_style(old_id);
            }
        }

        self.focus = None;
    }

    pub(crate) fn update_focus(&mut self, id: Id, keyboard_navigation: bool) {
        if self.focus.is_some() {
            return;
        }

        self.focus = Some(id);
        self.keyboard_navigation = keyboard_navigation;
    }

    pub(crate) fn has_style_for_sel(&mut self, id: Id, selector_kind: StyleSelector) -> bool {
        let view_state = self.view_state(id);

        view_state.has_style_selectors.has(selector_kind)
            || (selector_kind == StyleSelector::Dragging && view_state.dragging_style.is_some())
    }

    // TODO: animated should be a HashMap<Id, AnimId>
    // so we don't have to loop through all view states
    pub(crate) fn get_view_id_by_anim_id(&self, anim_id: AnimId) -> Id {
        *self
            .view_states
            .iter()
            .find(|(_, vs)| {
                vs.animation
                    .as_ref()
                    .map(|a| a.id() == anim_id)
                    .unwrap_or(false)
            })
            .unwrap()
            .0
    }

    pub(crate) fn update_context_menu(&mut self, menu: &mut Menu) {
        if let Some(action) = menu.item.action.take() {
            self.context_menu.insert(menu.item.id as usize, action);
        }
        for child in menu.children.iter_mut() {
            match child {
                crate::menu::MenuEntry::Separator => {}
                crate::menu::MenuEntry::Item(item) => {
                    if let Some(action) = item.action.take() {
                        self.context_menu.insert(item.id as usize, action);
                    }
                }
                crate::menu::MenuEntry::SubMenu(m) => {
                    self.update_context_menu(m);
                }
            }
        }
    }

    pub(crate) fn get_event_listener(
        &self,
        id: Id,
        listener: &EventListener,
    ) -> Option<&impl Fn(&Event) -> bool> {
        self.view_states
            .get(&id)
            .and_then(|s| s.event_listeners.get(listener))
    }

    pub(crate) fn focus_changed(&mut self, old: Option<Id>, new: Option<Id>) {
        if let Some(id) = new {
            // To apply the styles of the Focus selector
            if self.has_style_for_sel(id, StyleSelector::Focus)
                || self.has_style_for_sel(id, StyleSelector::FocusVisible)
            {
                self.request_style(id);
            }
            if let Some(action) = self.get_event_listener(id, &EventListener::FocusGained) {
                (*action)(&Event::FocusGained);
            }
        }

        if let Some(old_id) = old {
            // To remove the styles applied by the Focus selector
            if self.has_style_for_sel(old_id, StyleSelector::Focus)
                || self.has_style_for_sel(old_id, StyleSelector::FocusVisible)
            {
                self.request_style(old_id);
            }
            if let Some(action) = self.get_event_listener(old_id, &EventListener::FocusLost) {
                (*action)(&Event::FocusLost);
            }
        }
    }
}

/// A bundle of helper methods to be used by `View::event` handlers
pub struct EventCx<'a> {
    pub(crate) app_state: &'a mut AppState,
}

impl<'a> EventCx<'a> {
    /// request that this node be styled, laid out and painted again
    /// This will recursively request this for all parents.
    pub fn request_all(&mut self, id: Id) {
        self.app_state.request_style(id);
        self.app_state.request_layout(id);
    }

    /// request that this node be styled again
    /// This will recursively request style for all parents.
    pub fn request_style(&mut self, id: Id) {
        self.app_state.request_style(id);
    }

    /// request that this node be laid out again
    /// This will recursively request layout for all parents and set the `ChangeFlag::LAYOUT` at root
    pub fn request_layout(&mut self, id: Id) {
        self.app_state.request_layout(id);
    }

    /// request that this node be painted again
    /// This will recursively request painting for all parents
    pub fn request_paint(&mut self, id: Id) {
        self.app_state.request_paint(id);
    }

    pub fn update_active(&mut self, id: Id) {
        self.app_state.update_active(id);
    }

    pub fn is_active(&self, id: Id) -> bool {
        self.app_state.is_active(&id)
    }

    #[allow(unused)]
    pub(crate) fn update_focus(&mut self, id: Id, keyboard_navigation: bool) {
        self.app_state.update_focus(id, keyboard_navigation);
    }

    pub fn get_computed_style(&self, id: Id) -> Option<&Style> {
        self.app_state
            .view_states
            .get(&id)
            .map(|s| &s.combined_style)
    }

    pub fn get_layout(&self, id: Id) -> Option<Layout> {
        self.app_state.get_layout(id)
    }

    pub(crate) fn get_size(&self, id: Id) -> Option<Size> {
        self.app_state
            .get_layout(id)
            .map(|l| Size::new(l.size.width as f64, l.size.height as f64))
    }

    pub(crate) fn has_event_listener(&self, id: Id, listener: EventListener) -> bool {
        self.app_state
            .view_states
            .get(&id)
            .map(|s| s.event_listeners.contains_key(&listener))
            .unwrap_or(false)
    }

    pub(crate) fn get_event_listener(
        &self,
        id: Id,
        listener: &EventListener,
    ) -> Option<&impl Fn(&Event) -> bool> {
        self.app_state.get_event_listener(id, listener)
    }

    /// translate a window-positioned event to the local coordinate system of a view
    pub(crate) fn offset_event(&self, id: Id, event: Event) -> Event {
        let viewport = self
            .app_state
            .view_states
            .get(&id)
            .and_then(|view| view.viewport);

        if let Some(layout) = self.get_layout(id) {
            event.offset((
                layout.location.x as f64 - viewport.map(|rect| rect.x0).unwrap_or(0.0),
                layout.location.y as f64 - viewport.map(|rect| rect.y0).unwrap_or(0.0),
            ))
        } else {
            event
        }
    }

    /// Used to determine if you should send an event to another view. This is basically a check for pointer events to see if the pointer is inside a child view and to make sure the current view isn't hidden or disabled.
    /// Usually this is used if you want to propagate an event to a child view
    pub fn should_send(&mut self, id: Id, event: &Event) -> bool {
        if self.app_state.is_hidden(id)
            || (self.app_state.is_disabled(&id) && !event.allow_disabled())
        {
            return false;
        }
        if let Some(point) = event.point() {
            let layout_rect = self.app_state.get_layout_rect(id);
            if let Some(layout) = self.get_layout(id) {
                if layout_rect
                    .with_origin(Point::new(
                        layout.location.x as f64,
                        layout.location.y as f64,
                    ))
                    .contains(point)
                {
                    return true;
                }
            }
            false
        } else {
            true
        }
    }
}

#[derive(Default)]
pub struct InteractionState {
    pub(crate) is_hovered: bool,
    pub(crate) is_disabled: bool,
    pub(crate) is_focused: bool,
    pub(crate) is_clicking: bool,
    pub(crate) using_keyboard_navigation: bool,
}

pub struct StyleCx<'a> {
    pub(crate) app_state: &'a mut AppState,
    pub(crate) current_view: Id,
    pub(crate) current: Rc<Style>,
    pub(crate) direct: Style,
    saved: Vec<Rc<Style>>,
    pub(crate) now: Instant,
}

impl<'a> StyleCx<'a> {
    pub(crate) fn new(app_state: &'a mut AppState, root: Id) -> Self {
        Self {
            app_state,
            current_view: root,
            current: Default::default(),
            direct: Default::default(),
            saved: Default::default(),
            now: Instant::now(),
        }
    }

    /// Internal method used by Floem to compute the styles for the view.
    pub fn style_view(&mut self, view: &mut dyn View) {
        self.save();
        let id = view.id();
        let view_state = self.app_state_mut().view_state(id);
        if !view_state.request_style {
            return;
        }
        view_state.request_style = false;

        let view_style = view.view_style();
        let view_class = view.view_class();
        let class = view_state.class;
        let class_array;
        let classes = if let Some(class) = class {
            class_array = [class];
            &class_array[..]
        } else {
            &[]
        };

        // Propagate style requests to children if needed.
        if view_state.request_style_recursive {
            view_state.request_style_recursive = false;
            for child in view.children() {
                let state = self.app_state_mut().view_state(child.id());
                state.request_style_recursive = true;
                state.request_style = true;
            }
        }

        self.app_state
            .compute_style(id, view_style, view_class, classes, &self.current);
        let style = self.app_state_mut().get_computed_style(id).clone();
        self.direct = style;
        Style::apply_only_inherited(&mut self.current, &self.direct);
        CaptureState::capture_style(id, self);

        // If there's any changes to the Taffy style, request layout.
        let taffy_style = self.direct.to_taffy_style();
        let view_state = self.app_state_mut().view_state(id);
        if taffy_style != view_state.taffy_style {
            view_state.taffy_style = taffy_style;
            self.app_state_mut().request_layout(id);
        }

        // This is used by the `request_transition` and `style` methods below.
        self.current_view = id;

        let view_state = self.app_state.view_state(id);

        let mut transition = false;

        // Extract the relevant layout properties so the content rect can be calculated
        // when painting.
        view_state.layout_props.read_explicit(
            &self.direct,
            &self.current,
            &self.now,
            &mut transition,
        );

        view_state.view_style_props.read_explicit(
            &self.direct,
            &self.current,
            &self.now,
            &mut transition,
        );
        if transition {
            self.request_transition();
        }

        view.style(self);

        self.restore();
    }

    pub fn save(&mut self) {
        self.saved.push(self.current.clone());
    }

    pub fn restore(&mut self) {
        self.current = self.saved.pop().unwrap_or_default();
    }

    pub fn get_prop<P: StyleProp>(&self, _prop: P) -> Option<P::Type> {
        self.direct
            .get_prop::<P>()
            .or_else(|| self.current.get_prop::<P>())
    }

    pub fn style(&self) -> Style {
        (*self.current).clone().apply(self.direct.clone())
    }

    pub(crate) fn request_transition(&mut self) {
        let id = self.current_view;
        self.app_state_mut().transitioning.insert(id);
    }

    pub fn app_state_mut(&mut self) -> &mut AppState {
        self.app_state
    }

    pub fn app_state(&self) -> &AppState {
        self.app_state
    }
}

/// Holds current layout state for given position in the tree.
/// You'll use this in the `View::layout` implementation to call `layout_node` on children and to access any font
pub struct LayoutCx<'a> {
    pub(crate) app_state: &'a mut AppState,
    pub(crate) viewport: Option<Rect>,
    pub(crate) window_origin: Point,
    pub(crate) saved_viewports: Vec<Option<Rect>>,
    pub(crate) saved_window_origins: Vec<Point>,
}

impl<'a> LayoutCx<'a> {
    pub(crate) fn new(app_state: &'a mut AppState) -> Self {
        Self {
            app_state,
            viewport: None,
            window_origin: Point::ZERO,
            saved_viewports: Vec::new(),
            saved_window_origins: Vec::new(),
        }
    }

    pub(crate) fn clear(&mut self) {
        self.viewport = None;
        self.window_origin = Point::ZERO;
        self.saved_viewports.clear();
        self.saved_window_origins.clear();
    }

    pub fn save(&mut self) {
        self.saved_viewports.push(self.viewport);
        self.saved_window_origins.push(self.window_origin);
    }

    pub fn restore(&mut self) {
        self.viewport = self.saved_viewports.pop().unwrap_or_default();
        self.window_origin = self.saved_window_origins.pop().unwrap_or_default();
    }

    pub fn app_state_mut(&mut self) -> &mut AppState {
        self.app_state
    }

    pub fn app_state(&self) -> &AppState {
        self.app_state
    }

    pub fn current_viewport(&self) -> Option<Rect> {
        self.viewport
    }

    pub fn get_layout(&self, id: Id) -> Option<Layout> {
        self.app_state.get_layout(id)
    }

    pub fn get_computed_style(&mut self, id: Id) -> &Style {
        self.app_state.get_computed_style(id)
    }

    pub fn set_style(&mut self, node: Node, style: taffy::style::Style) {
        let _ = self.app_state.taffy.set_style(node, style);
    }

    pub fn layout(&self, node: Node) -> Option<Layout> {
        self.app_state.taffy.layout(node).ok().copied()
    }

    pub fn new_node(&mut self) -> Node {
        self.app_state
            .taffy
            .new_leaf(taffy::style::Style::DEFAULT)
            .unwrap()
    }

    /// Responsible for invoking the recalculation of style and thus the layout and
    /// creating or updating the layout of child nodes within the closure.
    ///
    /// You should ensure that all children are laid out within the closure and/or whatever
    /// other work you need to do to ensure that the layout for the returned nodes is correct.
    pub fn layout_node(
        &mut self,
        id: Id,
        has_children: bool,
        mut children: impl FnMut(&mut LayoutCx) -> Vec<Node>,
    ) -> Node {
        let view_state = self.app_state.view_state(id);
        let node = view_state.node;
        if !view_state.request_layout {
            return node;
        }
        view_state.request_layout = false;
        let style = view_state.combined_style.to_taffy_style();
        let _ = self.app_state.taffy.set_style(node, style);

        if has_children {
            let nodes = children(self);
            let _ = self.app_state.taffy.set_children(node, &nodes);
            let view = self.app_state.view_state(id);
            view.children_nodes = nodes;
        }

        node
    }

    /// Internal method used by Floem to invoke the user-defined `View::layout` method.
    pub fn layout_view(&mut self, view: &mut dyn View) -> Node {
        self.save();
        let node = view.layout(self);
        self.restore();
        node
    }

    pub(crate) fn get_resize_listener(&mut self, id: Id) -> Option<&mut ResizeListener> {
        self.app_state
            .view_states
            .get_mut(&id)
            .and_then(|s| s.resize_listener.as_mut())
    }

    pub(crate) fn get_move_listener(&mut self, id: Id) -> Option<&mut MoveListener> {
        self.app_state
            .view_states
            .get_mut(&id)
            .and_then(|s| s.move_listener.as_mut())
    }
}

pub struct PaintCx<'a> {
    pub(crate) app_state: &'a mut AppState,
    pub(crate) paint_state: &'a mut PaintState,
    pub(crate) transform: Affine,
    pub(crate) clip: Option<RoundedRect>,
    pub(crate) color: Option<Color>,
    pub(crate) scroll_bar_rounded: Option<bool>,
    pub(crate) scroll_bar_thickness: Option<f32>,
    pub(crate) scroll_bar_edge_width: Option<f32>,
    pub(crate) font_size: Option<f32>,
    pub(crate) font_family: Option<String>,
    pub(crate) font_weight: Option<Weight>,
    pub(crate) font_style: Option<FontStyle>,
    pub(crate) line_height: Option<LineHeightValue>,
    pub(crate) z_index: Option<i32>,
    pub(crate) saved_transforms: Vec<Affine>,
    pub(crate) saved_clips: Vec<Option<RoundedRect>>,
    pub(crate) saved_colors: Vec<Option<Color>>,
    pub(crate) saved_scroll_bar_roundeds: Vec<Option<bool>>,
    pub(crate) saved_scroll_bar_thicknesses: Vec<Option<f32>>,
    pub(crate) saved_scroll_bar_edge_widths: Vec<Option<f32>>,
    pub(crate) saved_font_sizes: Vec<Option<f32>>,
    pub(crate) saved_font_families: Vec<Option<String>>,
    pub(crate) saved_font_weights: Vec<Option<Weight>>,
    pub(crate) saved_font_styles: Vec<Option<FontStyle>>,
    pub(crate) saved_line_heights: Vec<Option<LineHeightValue>>,
    pub(crate) saved_z_indexes: Vec<Option<i32>>,
}

impl<'a> PaintCx<'a> {
    pub fn save(&mut self) {
        self.saved_transforms.push(self.transform);
        self.saved_clips.push(self.clip);
        self.saved_colors.push(self.color);
        self.saved_scroll_bar_roundeds.push(self.scroll_bar_rounded);
        self.saved_scroll_bar_thicknesses
            .push(self.scroll_bar_thickness);
        self.saved_scroll_bar_edge_widths
            .push(self.scroll_bar_edge_width);
        self.saved_font_sizes.push(self.font_size);
        self.saved_font_families.push(self.font_family.clone());
        self.saved_font_weights.push(self.font_weight);
        self.saved_font_styles.push(self.font_style);
        self.saved_line_heights.push(self.line_height);
        self.saved_z_indexes.push(self.z_index);
    }

    pub fn restore(&mut self) {
        self.transform = self.saved_transforms.pop().unwrap_or_default();
        self.clip = self.saved_clips.pop().unwrap_or_default();
        self.color = self.saved_colors.pop().unwrap_or_default();
        self.scroll_bar_rounded = self.saved_scroll_bar_roundeds.pop().unwrap_or_default();
        self.scroll_bar_thickness = self.saved_scroll_bar_thicknesses.pop().unwrap_or_default();
        self.scroll_bar_edge_width = self.saved_scroll_bar_edge_widths.pop().unwrap_or_default();
        self.font_size = self.saved_font_sizes.pop().unwrap_or_default();
        self.font_family = self.saved_font_families.pop().unwrap_or_default();
        self.font_weight = self.saved_font_weights.pop().unwrap_or_default();
        self.font_style = self.saved_font_styles.pop().unwrap_or_default();
        self.line_height = self.saved_line_heights.pop().unwrap_or_default();
        self.z_index = self.saved_z_indexes.pop().unwrap_or_default();
        self.paint_state.renderer.transform(self.transform);
        if let Some(z_index) = self.z_index {
            self.paint_state.renderer.set_z_index(z_index);
        } else {
            self.paint_state.renderer.set_z_index(0);
        }
        if let Some(rect) = self.clip {
            self.paint_state.renderer.clip(&rect);
        } else {
            self.paint_state.renderer.clear_clip();
        }
    }

    pub fn current_color(&self) -> Option<Color> {
        self.color
    }

    pub fn current_scroll_bar_rounded(&self) -> Option<bool> {
        self.scroll_bar_rounded
    }

    pub fn current_scroll_bar_thickness(&self) -> Option<f32> {
        self.scroll_bar_thickness
    }

    pub fn current_scroll_bar_edge_width(&self) -> Option<f32> {
        self.scroll_bar_edge_width
    }

    pub fn current_font_size(&self) -> Option<f32> {
        self.font_size
    }

    pub fn current_font_family(&self) -> Option<&str> {
        self.font_family.as_deref()
    }

    pub fn layout(&self, node: Node) -> Option<Layout> {
        self.app_state.taffy.layout(node).ok().copied()
    }

    pub fn get_layout(&mut self, id: Id) -> Option<Layout> {
        self.app_state.get_layout(id)
    }

    /// Returns the layout rect excluding borders, padding and position.
    pub fn get_content_rect(&mut self, id: Id) -> Rect {
        self.app_state.get_content_rect(id)
    }

    pub fn get_computed_style(&mut self, id: Id) -> &Style {
        self.app_state.get_computed_style(id)
    }

    pub(crate) fn get_builtin_style(&mut self, id: Id) -> BuiltinStyle<'_> {
        self.app_state.get_builtin_style(id)
    }

    /// Clip the drawing area to the given shape.
    pub fn clip(&mut self, shape: &impl Shape) {
        let rect = if let Some(rect) = shape.as_rect() {
            rect.to_rounded_rect(0.0)
        } else if let Some(rect) = shape.as_rounded_rect() {
            rect
        } else {
            let rect = shape.bounding_box();
            rect.to_rounded_rect(0.0)
        };

        let rect = if let Some(existing) = self.clip {
            let rect = existing.rect().intersect(rect.rect());
            self.paint_state.renderer.clip(&rect);
            rect.to_rounded_rect(0.0)
        } else {
            self.paint_state.renderer.clip(&shape);
            rect
        };
        self.clip = Some(rect);
    }

    pub fn offset(&mut self, offset: (f64, f64)) {
        let mut new = self.transform.as_coeffs();
        new[4] += offset.0;
        new[5] += offset.1;
        self.transform = Affine::new(new);
        self.paint_state.renderer.transform(self.transform);
        if let Some(rect) = self.clip.as_mut() {
            let raidus = rect.radii();
            *rect = rect
                .rect()
                .with_origin(rect.origin() - Vec2::new(offset.0, offset.1))
                .to_rounded_rect(raidus);
        }
    }

    pub fn transform(&mut self, id: Id) -> Size {
        if let Some(layout) = self.get_layout(id) {
            let offset = layout.location;
            let mut new = self.transform.as_coeffs();
            new[4] += offset.x as f64;
            new[5] += offset.y as f64;
            self.transform = Affine::new(new);
            self.paint_state.renderer.transform(self.transform);

            if let Some(rect) = self.clip.as_mut() {
                let raidus = rect.radii();
                *rect = rect
                    .rect()
                    .with_origin(rect.origin() - Vec2::new(offset.x as f64, offset.y as f64))
                    .to_rounded_rect(raidus);
            }

            Size::new(layout.size.width as f64, layout.size.height as f64)
        } else {
            Size::ZERO
        }
    }

    pub(crate) fn set_z_index(&mut self, z_index: i32) {
        self.z_index = Some(z_index);
        self.paint_state.renderer.set_z_index(z_index);
    }

    pub fn is_focused(&self, id: Id) -> bool {
        self.app_state.is_focused(&id)
    }
}

// TODO: should this be private?
pub struct PaintState {
    pub(crate) renderer: crate::renderer::Renderer,
}

impl PaintState {
    pub fn new<W>(window: &W, scale: f64, size: Size) -> Self
    where
        W: raw_window_handle::HasRawDisplayHandle + raw_window_handle::HasRawWindowHandle,
    {
        Self {
            renderer: crate::renderer::Renderer::new(window, scale, size),
        }
    }

    pub(crate) fn resize(&mut self, scale: f64, size: Size) {
        self.renderer.resize(scale, size);
    }

    pub(crate) fn set_scale(&mut self, scale: f64) {
        self.renderer.set_scale(scale);
    }
}

pub struct UpdateCx<'a> {
    pub(crate) app_state: &'a mut AppState,
}

impl<'a> UpdateCx<'a> {
    /// request that this node be styled, laid out and painted again
    /// This will recursively request this for all parents.
    pub fn request_all(&mut self, id: Id) {
        self.app_state.request_style(id);
        self.app_state.request_layout(id);
    }

    /// request that this node be styled again
    /// This will recursively request style for all parents.
    pub fn request_style(&mut self, id: Id) {
        self.app_state.request_style(id);
    }

    /// request that this node be laid out again
    /// This will recursively request layout for all parents and set the `ChangeFlag::LAYOUT` at root
    pub fn request_layout(&mut self, id: Id) {
        self.app_state.request_layout(id);
    }

    /// Used internally by Floem to send an update to the correct view based on the `Id` path.
    /// It will invoke only once `update` when the correct view is located.
    pub fn update_view(
        &mut self,
        view: &mut dyn View,
        id_path: &[Id],
        state: Box<dyn Any>,
    ) -> ChangeFlags {
        let id = id_path[0];
        let id_path = &id_path[1..];
        if id == view.id() {
            if id_path.is_empty() {
                return view.update(self, state);
            } else if let Some(child) = view.child_mut(id_path[0]) {
                return self.update_view(child, id_path, state);
            }
        }
        ChangeFlags::empty()
    }
}

impl Deref for PaintCx<'_> {
    type Target = crate::renderer::Renderer;

    fn deref(&self) -> &Self::Target {
        &self.paint_state.renderer
    }
}

impl DerefMut for PaintCx<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.paint_state.renderer
    }
}
