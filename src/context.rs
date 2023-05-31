use std::{
    collections::{HashMap, HashSet},
    ops::{Deref, DerefMut},
    time::Duration,
};

use floem_renderer::{
    cosmic_text::{LineHeightValue, Style as FontStyle, Weight},
    Renderer as FloemRenderer,
};
use glazier::{
    kurbo::{Affine, Point, Rect, Shape, Size, Vec2},
    PointerEvent, Scale,
};
use taffy::{
    prelude::{Layout, Node},
    style::{AvailableSpace, Display},
};
use vello::peniko::Color;

use crate::{
    animate::{AnimId, AnimPropKind, Animation},
    app_handle::StyleSelector,
    event::{Event, EventListner},
    id::Id,
    menu::Menu,
    responsive::{GridBreakpoints, ScreenSize, ScreenSizeBp},
    style::{ComputedStyle, CursorStyle, Style},
    AppContext,
};

thread_local! {
    pub(crate) static APP_CONTEXT_STORE: std::cell::RefCell<Option<AppContextStore>> = Default::default();
}

pub type EventCallback = dyn Fn(&Event) -> bool;
pub type ResizeCallback = dyn Fn(Point, Rect);

pub struct AppContextStore {
    pub cx: AppContext,
    pub saved_cx: Vec<AppContext>,
}

impl AppContextStore {
    pub fn save(&mut self) {
        self.saved_cx.push(self.cx);
    }

    pub fn set_current(&mut self, cx: AppContext) {
        self.cx = cx;
    }

    pub fn restore(&mut self) {
        if let Some(cx) = self.saved_cx.pop() {
            self.cx = cx;
        }
    }
}

pub(crate) struct ResizeListener {
    pub(crate) window_origin: Point,
    pub(crate) rect: Rect,
    pub(crate) callback: Box<ResizeCallback>,
}

pub struct ViewState {
    pub(crate) node: Node,
    pub(crate) children_nodes: Vec<Node>,
    pub(crate) request_layout: bool,
    pub(crate) viewport: Option<Rect>,
    pub(crate) layout_rect: Rect,
    pub(crate) animation: Option<Animation>,
    pub(crate) base_style: Option<Style>,
    pub(crate) style: Style,
    pub(crate) hover_style: Option<Style>,
    pub(crate) disabled_style: Option<Style>,
    pub(crate) focus_style: Option<Style>,
    pub(crate) focus_visible_style: Option<Style>,
    pub(crate) responsive_styles: HashMap<ScreenSizeBp, Vec<Style>>,
    pub(crate) active_style: Option<Style>,
    pub(crate) computed_style: ComputedStyle,
    pub(crate) event_listeners: HashMap<EventListner, Box<EventCallback>>,
    pub(crate) resize_listener: Option<ResizeListener>,
    pub(crate) last_pointer_down: Option<PointerEvent>,
}

impl ViewState {
    fn new(taffy: &mut taffy::Taffy) -> Self {
        Self {
            node: taffy.new_leaf(taffy::style::Style::DEFAULT).unwrap(),
            viewport: None,
            layout_rect: Rect::ZERO,
            request_layout: true,
            animation: None,
            base_style: None,
            style: Style::BASE,
            computed_style: ComputedStyle::default(),
            hover_style: None,
            disabled_style: None,
            focus_style: None,
            focus_visible_style: None,
            active_style: None,
            responsive_styles: HashMap::new(),
            children_nodes: Vec::new(),
            event_listeners: HashMap::new(),
            resize_listener: None,
            last_pointer_down: None,
        }
    }

    pub(crate) fn compute_style(
        &mut self,
        view_style: Option<Style>,
        interact_state: InteractionState,
        screen_size_bp: ScreenSizeBp,
    ) {
        let mut computed_style = if let Some(view_style) = view_style {
            if let Some(base_style) = self.base_style.clone() {
                view_style.apply(base_style).apply(self.style.clone())
            } else {
                view_style.apply(self.style.clone())
            }
        } else if let Some(base_style) = self.base_style.clone() {
            base_style.apply(self.style.clone())
        } else {
            self.style.clone()
        };

        if let Some(resp_styles) = self.responsive_styles.get(&screen_size_bp) {
            for style in resp_styles {
                computed_style = computed_style.apply(style.clone());
            }
        }

        if interact_state.is_hovered && !interact_state.is_disabled {
            if let Some(hover_style) = self.hover_style.clone() {
                computed_style = computed_style.apply(hover_style);
            }
        }

        if interact_state.is_focused {
            if let Some(focus_style) = self.focus_style.clone() {
                computed_style = computed_style.apply(focus_style);
            }
        }

        let focused_keyboard =
            interact_state.using_keyboard_navigation && interact_state.is_focused;
        if focused_keyboard {
            if let Some(focus_visible_style) = self.focus_visible_style.clone() {
                computed_style = computed_style.apply(focus_visible_style);
            }
        }

        let active_mouse = interact_state.is_hovered && !interact_state.using_keyboard_navigation;
        if interact_state.is_active && (active_mouse || focused_keyboard) {
            if let Some(active_style) = self.active_style.clone() {
                computed_style = computed_style.apply(active_style);
            }
        }

        if interact_state.is_disabled {
            if let Some(disabled_style) = self.disabled_style.clone() {
                computed_style = computed_style.apply(disabled_style);
            }
        }

        'anim: {
            if let Some(animation) = self.animation.as_mut() {
                if animation.is_completed() && animation.is_auto_reverse() {
                    break 'anim;
                }

                let props = animation.props();

                for (kind, _) in props {
                    let val = animation
                        .animate_prop(animation.elapsed().unwrap_or(Duration::ZERO), &kind);
                    match kind {
                        AnimPropKind::Width => {
                            computed_style = computed_style.width_px(val.get_f32());
                        }
                        AnimPropKind::Height => {
                            computed_style = computed_style.height_px(val.get_f32());
                        }
                        AnimPropKind::Background => {
                            computed_style = computed_style.background(val.get_color());
                        }
                        AnimPropKind::Color => {
                            computed_style = computed_style.color(val.get_color());
                        }
                        AnimPropKind::BorderRadius => {
                            computed_style = computed_style.border_radius(val.get_f32());
                        }
                        AnimPropKind::BorderColor => {
                            computed_style = computed_style.border_color(val.get_color());
                        }
                        AnimPropKind::Scale => todo!(),
                    }
                }

                animation.advance();
                debug_assert!(!animation.is_idle());
            }
        }

        self.computed_style = computed_style.compute(&ComputedStyle::default());
    }

    pub(crate) fn add_responsive_style(&mut self, size: ScreenSize, style: Style) {
        let breakpoints = size.breakpoints();

        for breakpoint in breakpoints {
            self.responsive_styles
                .entry(breakpoint)
                .or_insert_with(Vec::new)
                .push(style.clone())
        }
    }
}

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
    pub(crate) disabled: HashSet<Id>,
    pub(crate) keyboard_navigatable: HashSet<Id>,
    pub(crate) screen_size_bp: ScreenSizeBp,
    pub(crate) grid_breakpts: GridBreakpoints,
    pub(crate) hovered: HashSet<Id>,
    /// This keeps track of all views that have an animation,
    /// regardless of the status of the animation
    pub(crate) animated: HashSet<Id>,
    pub(crate) cursor: Option<CursorStyle>,
    pub(crate) keyboard_navigation: bool,
    pub(crate) contex_menu: HashMap<u32, Box<dyn Fn()>>,
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
            taffy,
            view_states: HashMap::new(),
            animated: HashSet::new(),
            disabled: HashSet::new(),
            keyboard_navigatable: HashSet::new(),
            hovered: HashSet::new(),
            cursor: None,
            keyboard_navigation: false,
            grid_breakpts: GridBreakpoints::default(),
            contex_menu: HashMap::new(),
        }
    }

    pub fn view_state(&mut self, id: Id) -> &mut ViewState {
        self.view_states
            .entry(id)
            .or_insert_with(|| ViewState::new(&mut self.taffy))
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
            // TODO: this unwrap_or is wrong. The style might not specify it, but the underlying view style can
            .map(|s| s.style.display.unwrap_or(Display::Flex) == Display::None)
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

    pub fn get_interact_state(&self, id: &Id) -> InteractionState {
        InteractionState {
            is_hovered: self.is_hovered(id),
            is_disabled: self.is_disabled(id),
            is_focused: self.is_focused(id),
            is_active: self.is_active(id),
            using_keyboard_navigation: self.keyboard_navigation,
        }
    }

    pub fn set_root_size(&mut self, size: Size) {
        self.root_size = size;
        self.compute_layout();
    }

    pub(crate) fn compute_style(&mut self, id: Id, view_style: Option<Style>) {
        let interact_state = self.get_interact_state(&id);
        let screen_size_bp = self.screen_size_bp;
        let view_state = self.view_state(id);
        view_state.compute_style(view_style, interact_state, screen_size_bp);
    }

    pub(crate) fn get_computed_style(&mut self, id: Id) -> &ComputedStyle {
        let view_state = self.view_state(id);
        &view_state.computed_style
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

    pub(crate) fn request_layout(&mut self, id: Id) {
        let view = self.view_state(id);
        if view.request_layout {
            return;
        }
        view.request_layout = true;
        if let Some(parent) = id.parent() {
            self.request_layout(parent);
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
            self.request_layout(id);
        }
    }

    pub(crate) fn update_scr_size_breakpt(&mut self, size: Size) {
        let breakpt = self.grid_breakpts.get_width_breakpt(size.width);
        self.screen_size_bp = breakpt;
    }

    pub(crate) fn clear_focus(&mut self) {
        if let Some(old_id) = self.focus {
            // To remove the styles applied by the Focus selector
            if self.has_style_for_sel(old_id, StyleSelector::Focus) {
                self.request_layout(old_id);
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

        match selector_kind {
            StyleSelector::Hover => view_state.hover_style.is_some(),
            StyleSelector::Focus => view_state.focus_style.is_some(),
            StyleSelector::FocusVisible => view_state.focus_visible_style.is_some(),
            StyleSelector::Disabled => view_state.disabled_style.is_some(),
            StyleSelector::Active => view_state.active_style.is_some(),
        }
    }

    // TODO: animated should be a HashMap<Id, AnimId>
    // so we don't have to loop through all view states
    pub(crate) fn get_view_id_by_anim_id(&self, anim_id: AnimId) -> Id {
        self.view_states
            .iter()
            .filter(|(_, vs)| {
                vs.animation
                    .as_ref()
                    .map(|a| a.id() == anim_id)
                    .unwrap_or(false)
            })
            .nth(0)
            .unwrap()
            .0
            .clone()
    }

    pub(crate) fn update_context_menu(&mut self, mut menu: Menu) {
        if let Some(action) = menu.item.action.take() {
            self.contex_menu.insert(menu.item.id as u32, action);
        }
        for child in menu.children {
            match child {
                crate::menu::MenuEntry::Seperator => {}
                crate::menu::MenuEntry::Item(mut item) => {
                    if let Some(action) = item.action.take() {
                        self.contex_menu.insert(item.id as u32, action);
                    }
                }
                crate::menu::MenuEntry::SubMenu(m) => {
                    self.update_context_menu(m);
                }
            }
        }
    }
}

pub struct EventCx<'a> {
    pub(crate) app_state: &'a mut AppState,
}

impl<'a> EventCx<'a> {
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

    pub fn get_style(&self, id: Id) -> Option<&Style> {
        self.app_state.view_states.get(&id).map(|s| &s.style)
    }

    pub fn get_computed_style(&self, id: Id) -> Option<&ComputedStyle> {
        self.app_state
            .view_states
            .get(&id)
            .map(|s| &s.computed_style)
    }

    pub fn get_hover_style(&self, id: Id) -> Option<&Style> {
        if let Some(vs) = self.app_state.view_states.get(&id) {
            return vs.hover_style.as_ref();
        }

        None
    }

    pub fn get_layout(&self, id: Id) -> Option<Layout> {
        self.app_state.get_layout(id)
    }

    pub(crate) fn get_size(&self, id: Id) -> Option<Size> {
        self.app_state
            .get_layout(id)
            .map(|l| Size::new(l.size.width as f64, l.size.height as f64))
    }

    pub(crate) fn has_event_listener(&self, id: Id, listner: EventListner) -> bool {
        self.app_state
            .view_states
            .get(&id)
            .map(|s| s.event_listeners.contains_key(&listner))
            .unwrap_or(false)
    }

    pub(crate) fn get_event_listener(
        &self,
        id: Id,
        listner: &EventListner,
    ) -> Option<&impl Fn(&Event) -> bool> {
        self.app_state
            .view_states
            .get(&id)
            .and_then(|s| s.event_listeners.get(listner))
    }

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

    pub(crate) fn should_send(&mut self, id: Id, event: &Event) -> bool {
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
    pub(crate) is_active: bool,
    pub(crate) using_keyboard_navigation: bool,
}

pub struct LayoutCx<'a> {
    pub(crate) app_state: &'a mut AppState,
    pub(crate) viewport: Option<Rect>,
    pub(crate) color: Option<Color>,
    pub(crate) font_size: Option<f32>,
    pub(crate) font_family: Option<String>,
    pub(crate) font_weight: Option<Weight>,
    pub(crate) font_style: Option<FontStyle>,
    pub(crate) line_height: Option<LineHeightValue>,
    pub(crate) window_origin: Point,
    pub(crate) saved_viewports: Vec<Option<Rect>>,
    pub(crate) saved_colors: Vec<Option<Color>>,
    pub(crate) saved_font_sizes: Vec<Option<f32>>,
    pub(crate) saved_font_families: Vec<Option<String>>,
    pub(crate) saved_font_weights: Vec<Option<Weight>>,
    pub(crate) saved_font_styles: Vec<Option<FontStyle>>,
    pub(crate) saved_line_heights: Vec<Option<LineHeightValue>>,
    pub(crate) saved_window_origins: Vec<Point>,
}

impl<'a> LayoutCx<'a> {
    pub(crate) fn clear(&mut self) {
        self.viewport = None;
        self.font_size = None;
        self.window_origin = Point::ZERO;
        self.saved_colors.clear();
        self.saved_viewports.clear();
        self.saved_font_sizes.clear();
        self.saved_font_families.clear();
        self.saved_font_weights.clear();
        self.saved_font_styles.clear();
        self.saved_line_heights.clear();
        self.saved_window_origins.clear();
    }

    pub fn save(&mut self) {
        self.saved_viewports.push(self.viewport);
        self.saved_colors.push(self.color);
        self.saved_font_sizes.push(self.font_size);
        self.saved_font_families.push(self.font_family.clone());
        self.saved_font_weights.push(self.font_weight);
        self.saved_font_styles.push(self.font_style);
        self.saved_line_heights.push(self.line_height);
        self.saved_window_origins.push(self.window_origin);
    }

    pub fn restore(&mut self) {
        self.viewport = self.saved_viewports.pop().unwrap_or_default();
        self.color = self.saved_colors.pop().unwrap_or_default();
        self.font_size = self.saved_font_sizes.pop().unwrap_or_default();
        self.font_family = self.saved_font_families.pop().unwrap_or_default();
        self.font_weight = self.saved_font_weights.pop().unwrap_or_default();
        self.font_style = self.saved_font_styles.pop().unwrap_or_default();
        self.line_height = self.saved_line_heights.pop().unwrap_or_default();
        self.window_origin = self.saved_window_origins.pop().unwrap_or_default();
    }

    pub fn current_font_size(&self) -> Option<f32> {
        self.font_size
    }

    pub fn current_font_family(&self) -> Option<&str> {
        self.font_family.as_deref()
    }

    pub fn current_font_weight(&self) -> Option<Weight> {
        self.font_weight
    }

    pub fn current_font_style(&self) -> Option<FontStyle> {
        self.font_style
    }

    pub fn current_line_height(&self) -> Option<LineHeightValue> {
        self.line_height
    }

    pub fn current_viewport(&self) -> Option<Rect> {
        self.viewport
    }

    pub fn get_layout(&self, id: Id) -> Option<Layout> {
        self.app_state.get_layout(id)
    }

    pub fn get_computed_style(&mut self, id: Id) -> &ComputedStyle {
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

    pub fn layout_node(
        &mut self,
        id: Id,
        has_children: bool,
        mut children: impl FnMut(&mut LayoutCx) -> Vec<Node>,
    ) -> Node {
        let view = self.app_state.view_state(id);
        let node = view.node;
        if !view.request_layout {
            return node;
        }
        view.request_layout = false;
        let style = view.computed_style.to_taffy_style();
        let _ = self.app_state.taffy.set_style(node, style);

        if has_children {
            let nodes = children(self);
            let _ = self.app_state.taffy.set_children(node, &nodes);
            let view = self.app_state.view_state(id);
            view.children_nodes = nodes;
        }

        node
    }

    pub(crate) fn get_resize_listener(&mut self, id: Id) -> Option<&mut ResizeListener> {
        self.app_state
            .view_states
            .get_mut(&id)
            .and_then(|s| s.resize_listener.as_mut())
    }
}

pub struct PaintCx<'a> {
    pub(crate) app_state: &'a mut AppState,
    pub(crate) paint_state: &'a mut PaintState,
    pub(crate) transform: Affine,
    pub(crate) clip: Option<Rect>,
    pub(crate) color: Option<Color>,
    pub(crate) font_size: Option<f32>,
    pub(crate) font_family: Option<String>,
    pub(crate) font_weight: Option<Weight>,
    pub(crate) font_style: Option<FontStyle>,
    pub(crate) line_height: Option<LineHeightValue>,
    pub(crate) z_index: Option<i32>,
    pub(crate) saved_transforms: Vec<Affine>,
    pub(crate) saved_clips: Vec<Option<Rect>>,
    pub(crate) saved_colors: Vec<Option<Color>>,
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
        self.font_size = self.saved_font_sizes.pop().unwrap_or_default();
        self.font_family = self.saved_font_families.pop().unwrap_or_default();
        self.font_weight = self.saved_font_weights.pop().unwrap_or_default();
        self.font_style = self.saved_font_styles.pop().unwrap_or_default();
        self.line_height = self.saved_line_heights.pop().unwrap_or_default();
        self.z_index = self.saved_z_indexes.pop().unwrap_or_default();
        let renderer = self.paint_state.renderer.as_mut().unwrap();
        renderer.transform(self.transform);
        if let Some(z_index) = self.z_index {
            renderer.set_z_index(z_index);
        } else {
            renderer.set_z_index(0);
        }
        if let Some(rect) = self.clip {
            renderer.clip(&rect);
        } else {
            renderer.clear_clip();
        }
    }

    pub fn current_color(&self) -> Option<Color> {
        self.color
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

    pub fn get_computed_style(&mut self, id: Id) -> &ComputedStyle {
        self.app_state.get_computed_style(id)
    }

    pub fn clip(&mut self, shape: &impl Shape) {
        let rect = shape.bounding_box();
        let rect = if let Some(existing) = self.clip {
            existing.intersect(rect)
        } else {
            rect
        };
        self.clip = Some(rect);
        self.paint_state.renderer.as_mut().unwrap().clip(&rect);
    }

    pub fn offset(&mut self, offset: (f64, f64)) {
        let mut new = self.transform.as_coeffs();
        new[4] += offset.0;
        new[5] += offset.1;
        self.transform = Affine::new(new);
        self.paint_state
            .renderer
            .as_mut()
            .unwrap()
            .transform(self.transform);
        if let Some(rect) = self.clip.as_mut() {
            *rect = rect.with_origin(rect.origin() - Vec2::new(offset.0, offset.1));
        }
    }

    pub fn transform(&mut self, id: Id) -> Size {
        if let Some(layout) = self.get_layout(id) {
            let offset = layout.location;
            let mut new = self.transform.as_coeffs();
            new[4] += offset.x as f64;
            new[5] += offset.y as f64;
            self.transform = Affine::new(new);
            self.paint_state
                .renderer
                .as_mut()
                .unwrap()
                .transform(self.transform);

            if let Some(rect) = self.clip.as_mut() {
                *rect =
                    rect.with_origin(rect.origin() - Vec2::new(offset.x as f64, offset.y as f64));
            }

            Size::new(layout.size.width as f64, layout.size.height as f64)
        } else {
            Size::ZERO
        }
    }

    pub(crate) fn set_z_index(&mut self, z_index: i32) {
        self.z_index = Some(z_index);
        self.paint_state
            .renderer
            .as_mut()
            .unwrap()
            .set_z_index(z_index);
    }

    pub fn is_focused(&self, id: Id) -> bool {
        self.app_state.is_focused(&id)
    }
}

pub struct PaintState {
    pub(crate) renderer: Option<crate::renderer::Renderer>,
    handle: glazier::WindowHandle,
}

impl Default for PaintState {
    fn default() -> Self {
        Self::new()
    }
}

impl PaintState {
    pub fn new() -> Self {
        Self {
            renderer: None,
            handle: Default::default(),
        }
    }

    pub(crate) fn connect(&mut self, handle: &glazier::WindowHandle) {
        self.handle = handle.clone();
        self.renderer = Some(crate::renderer::Renderer::new(handle));
    }

    pub(crate) fn resize(&mut self, scale: Scale, size: Size) {
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.resize(scale, size);
        }
    }

    pub(crate) fn set_scale(&mut self, scale: Scale) {
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.set_scale(scale);
        }
    }
}

pub struct UpdateCx<'a> {
    pub(crate) app_state: &'a mut AppState,
}

impl<'a> UpdateCx<'a> {
    pub fn request_layout(&mut self, id: Id) {
        self.app_state.request_layout(id);
    }
}

impl Deref for PaintCx<'_> {
    type Target = crate::renderer::Renderer;

    fn deref(&self) -> &Self::Target {
        self.paint_state.renderer.as_ref().unwrap()
    }
}

impl DerefMut for PaintCx<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.paint_state.renderer.as_mut().unwrap()
    }
}
