use std::{
    collections::{HashMap, HashSet},
    ops::{Deref, DerefMut},
};

use floem_renderer::{
    cosmic_text::{LineHeightValue, Style as FontStyle, Weight},
    Renderer as FloemRenderer,
};
use kurbo::{Affine, Point, Rect, RoundedRect, Shape, Size, Vec2};
use peniko::Color;
use taffy::{
    prelude::{Layout, Node},
    style::{AvailableSpace, Display},
};
use winit::window::CursorIcon;

use crate::{
    animate::StyleAnim,
    event::{Event, EventListener},
    id::Id,
    menu::Menu,
    pointer::PointerInputEvent,
    responsive::{GridBreakpoints, ScreenSize, ScreenSizeBp},
    style::{CursorStyle, Style, StyleAnimCtx, StyleSelector},
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

pub struct ViewState {
    pub(crate) node: Node,
    pub(crate) children_nodes: Vec<Node>,
    pub(crate) request_layout: bool,
    pub(crate) viewport: Option<Rect>,
    pub(crate) layout_rect: Rect,
    base_style: Option<StyleAnim>,
    main_style: Option<StyleAnim>,
    override_style: Option<StyleAnim>,
    pub(crate) dragging_style: Option<StyleAnim>,
    hover_style: Option<StyleAnim>,
    disabled_style: Option<StyleAnim>,
    focus_style: Option<StyleAnim>,
    focus_visible_style: Option<StyleAnim>,
    pub(crate) responsive_styles: HashMap<ScreenSizeBp, Vec<StyleAnim>>,
    active_style: Option<StyleAnim>,
    pub(crate) computed_style: Style,
    pub(crate) last_interaction_state: InteractionState,
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
            request_layout: true,
            base_style: None,
            main_style: None,
            override_style: None,
            computed_style: Style::default(),
            last_interaction_state: InteractionState::default(),
            hover_style: None,
            dragging_style: None,
            disabled_style: None,
            focus_style: None,
            focus_visible_style: None,
            active_style: None,
            responsive_styles: HashMap::new(),
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

    fn style_should_be_active(
        &self,
        selector: StyleSelector,
        interaction_state: &InteractionState,
    ) -> bool {
        let focused_keyboard =
            interaction_state.using_keyboard_navigation && interaction_state.is_focused;

        let active_mouse =
            interaction_state.is_hovered && !interaction_state.using_keyboard_navigation;

        match selector {
            StyleSelector::Base => true,
            StyleSelector::Main => true,
            StyleSelector::Hover => interaction_state.is_hovered && !interaction_state.is_disabled,
            StyleSelector::Focus => interaction_state.is_focused,
            StyleSelector::FocusVisible => focused_keyboard,
            StyleSelector::Disabled => interaction_state.is_disabled,
            StyleSelector::Active => {
                interaction_state.is_active && (active_mouse || focused_keyboard)
            }
            // applied manually in view.rs
            StyleSelector::Dragging => false,
            StyleSelector::Override => true,
        }
    }

    pub(crate) fn set_style(&mut self, selector: StyleSelector, mut style_anim: StyleAnim) {
        let should_be_active = self.style_should_be_active(selector, &self.last_interaction_state);
        style_anim.driver.set_enabled(should_be_active, false);

        match selector {
            StyleSelector::Base => self.base_style = Some(style_anim),
            StyleSelector::Main => self.main_style = Some(style_anim),
            StyleSelector::Hover => self.hover_style = Some(style_anim),
            StyleSelector::Focus => self.focus_style = Some(style_anim),
            StyleSelector::FocusVisible => self.focus_visible_style = Some(style_anim),
            StyleSelector::Disabled => self.disabled_style = Some(style_anim),
            StyleSelector::Active => self.active_style = Some(style_anim),
            StyleSelector::Dragging => self.dragging_style = Some(style_anim),
            StyleSelector::Override => self.override_style = Some(style_anim),
        }
    }

    pub(crate) fn get_style(&mut self, selector: StyleSelector) -> Option<&mut StyleAnim> {
        match selector {
            StyleSelector::Base => self.base_style.as_mut(),
            StyleSelector::Main => self.main_style.as_mut(),
            StyleSelector::Hover => self.hover_style.as_mut(),
            StyleSelector::Focus => self.focus_style.as_mut(),
            StyleSelector::FocusVisible => self.focus_visible_style.as_mut(),
            StyleSelector::Disabled => self.disabled_style.as_mut(),
            StyleSelector::Active => self.active_style.as_mut(),
            StyleSelector::Dragging => self.dragging_style.as_mut(),
            StyleSelector::Override => self.override_style.as_mut(),
        }
    }

    pub(crate) fn compute_style(
        &mut self,
        interaction_state: InteractionState,
        screen_size_bp: ScreenSizeBp,
    ) -> bool {
        let mut new_computed_style = Style::default();
        let mut request_next_frame = false;

        fn apply_anim(anim: &mut StyleAnim, style: Style) -> (Style, bool) {
            if anim.driver.should_run() {
                // save requests_next_frame before progressing the animation
                // is_finished is delayed by one step, which ensures a run without the animation is performed when disabling it
                let requests_next_frame = anim.driver.requests_next_frame();
                let value = anim.driver.next_value();
                let step_result = anim.anim_fn.call(StyleAnimCtx {
                    style,
                    blend_style: false,
                    animation_value: value,
                });
                return (step_result.style, requests_next_frame);
            }

            (style, false)
        }

        for selector in [StyleSelector::Base, StyleSelector::Main] {
            let should_be_active = self.style_should_be_active(selector, &interaction_state);
            if let Some(anim) = self.get_style(selector) {
                anim.driver.set_enabled(should_be_active, true);
                let result = apply_anim(anim, new_computed_style);
                new_computed_style = result.0;
                request_next_frame |= result.1;
            }
        }

        for (screen_size, anims) in self.responsive_styles.iter_mut() {
            for anim in anims {
                anim.driver
                    .set_enabled(*screen_size == screen_size_bp, true);
                let result = apply_anim(anim, new_computed_style);
                new_computed_style = result.0;
                request_next_frame |= result.1;
            }
        }

        for selector in [
            StyleSelector::Hover,
            StyleSelector::Focus,
            StyleSelector::FocusVisible,
            StyleSelector::Active,
            StyleSelector::Disabled,
            StyleSelector::Override,
        ] {
            let should_be_active = self.style_should_be_active(selector, &interaction_state);
            if let Some(anim) = self.get_style(selector) {
                anim.driver.set_enabled(should_be_active, true);
                let result = apply_anim(anim, new_computed_style);
                new_computed_style = result.0;
                request_next_frame |= result.1;
            }
        }

        self.last_interaction_state = interaction_state;
        self.computed_style = new_computed_style;
        if request_next_frame {
            self.request_layout = true;
        }
        request_next_frame
    }

    pub(crate) fn add_responsive_style(&mut self, size: ScreenSize, anim: StyleAnim) {
        let breakpoints = size.breakpoints();

        for breakpoint in breakpoints {
            self.responsive_styles
                .entry(breakpoint)
                .or_insert_with(Vec::new)
                .push(anim.clone())
        }
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
    pub(crate) hovered: HashSet<Id>,
    pub(crate) cursor: Option<CursorStyle>,
    pub(crate) last_cursor: CursorIcon,
    pub(crate) keyboard_navigation: bool,
    pub(crate) window_menu: HashMap<usize, Box<dyn Fn()>>,
    pub(crate) context_menu: HashMap<usize, Box<dyn Fn()>>,
    pub(crate) requesting_layout_next_frame: Vec<Id>,
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
            disabled: HashSet::new(),
            keyboard_navigable: HashSet::new(),
            draggable: HashSet::new(),
            dragging: None,
            drag_start: None,
            dragging_over: HashSet::new(),
            hovered: HashSet::new(),
            cursor: None,
            last_cursor: CursorIcon::Default,
            keyboard_navigation: false,
            grid_bps: GridBreakpoints::default(),
            window_menu: HashMap::new(),
            context_menu: HashMap::new(),
            requesting_layout_next_frame: Vec::new(),
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

    pub fn is_hidden(&self, id: Id) -> bool {
        self.view_states
            .get(&id)
            .map(|s| s.computed_style.display == Display::None)
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
            is_active: self.is_active(id),
            using_keyboard_navigation: self.keyboard_navigation,
        }
    }

    pub fn set_root_size(&mut self, size: Size) {
        self.root_size = size;
        self.compute_layout();
    }

    pub(crate) fn compute_style(&mut self, id: Id) {
        let interact_state = self.get_interact_state(&id);
        let screen_size_bp = self.screen_size_bp;
        let view_state = self.view_state(id);
        let requests_layout_next_frame = view_state.compute_style(interact_state, screen_size_bp);
        if requests_layout_next_frame {
            self.requesting_layout_next_frame.push(id);
            self.request_layout(id);
        }
    }

    pub(crate) fn get_computed_style(&mut self, id: Id) -> &Style {
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

    pub(crate) fn update_screen_size_bp(&mut self, size: Size) {
        let bp = self.grid_bps.get_width_bp(size.width);
        self.screen_size_bp = bp;
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

    pub(crate) fn has_style_for_sel(&mut self, id: Id, selector: StyleSelector) -> bool {
        let view_state = self.view_state(id);
        view_state.get_style(selector).is_some()
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
                self.request_layout(id);
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
                self.request_layout(old_id);
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
            .map(|s| &s.computed_style)
    }

    pub fn get_hover_style(&self, id: Id) -> Option<&StyleAnim> {
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

#[derive(Default, PartialEq, Eq)]
pub struct InteractionState {
    pub(crate) is_hovered: bool,
    pub(crate) is_disabled: bool,
    pub(crate) is_focused: bool,
    pub(crate) is_active: bool,
    pub(crate) using_keyboard_navigation: bool,
}

/// Holds current layout state for given position in the tree.
/// You'll use this in the `View::layout` implementation to call `layout_node` on children and to access any font
pub struct LayoutCx<'a> {
    app_state: &'a mut AppState,
    pub(crate) viewport: Option<Rect>,
    pub(crate) color: Option<Color>,
    pub(crate) scroll_bar_color: Option<Color>,
    pub(crate) scroll_bar_rounded: Option<bool>,
    pub(crate) scroll_bar_thickness: Option<f32>,
    pub(crate) scroll_bar_edge_width: Option<f32>,
    pub(crate) font_size: Option<f32>,
    pub(crate) font_family: Option<String>,
    pub(crate) font_weight: Option<Weight>,
    pub(crate) font_style: Option<FontStyle>,
    pub(crate) line_height: Option<LineHeightValue>,
    pub(crate) window_origin: Point,
    pub(crate) saved_viewports: Vec<Option<Rect>>,
    pub(crate) saved_colors: Vec<Option<Color>>,
    pub(crate) saved_scroll_bar_colors: Vec<Option<Color>>,
    pub(crate) saved_scroll_bar_roundeds: Vec<Option<bool>>,
    pub(crate) saved_scroll_bar_thicknesses: Vec<Option<f32>>,
    pub(crate) saved_scroll_bar_edge_widths: Vec<Option<f32>>,
    pub(crate) saved_font_sizes: Vec<Option<f32>>,
    pub(crate) saved_font_families: Vec<Option<String>>,
    pub(crate) saved_font_weights: Vec<Option<Weight>>,
    pub(crate) saved_font_styles: Vec<Option<FontStyle>>,
    pub(crate) saved_line_heights: Vec<Option<LineHeightValue>>,
    pub(crate) saved_window_origins: Vec<Point>,
}

impl<'a> LayoutCx<'a> {
    pub(crate) fn new(app_state: &'a mut AppState) -> Self {
        Self {
            app_state,
            viewport: None,
            color: None,
            font_size: None,
            font_family: None,
            font_weight: None,
            font_style: None,
            line_height: None,
            window_origin: Point::ZERO,
            saved_viewports: Vec::new(),
            saved_colors: Vec::new(),
            saved_font_sizes: Vec::new(),
            saved_font_families: Vec::new(),
            saved_font_weights: Vec::new(),
            saved_font_styles: Vec::new(),
            saved_line_heights: Vec::new(),
            saved_window_origins: Vec::new(),
            scroll_bar_color: None,
            scroll_bar_rounded: None,
            scroll_bar_thickness: None,
            scroll_bar_edge_width: None,
            saved_scroll_bar_colors: Vec::new(),
            saved_scroll_bar_roundeds: Vec::new(),
            saved_scroll_bar_thicknesses: Vec::new(),
            saved_scroll_bar_edge_widths: Vec::new(),
        }
    }

    pub(crate) fn clear(&mut self) {
        self.viewport = None;
        self.scroll_bar_color = None;
        self.scroll_bar_rounded = None;
        self.scroll_bar_thickness = None;
        self.scroll_bar_edge_width = None;
        self.font_size = None;
        self.window_origin = Point::ZERO;
        self.saved_colors.clear();
        self.saved_viewports.clear();
        self.saved_scroll_bar_colors.clear();
        self.saved_scroll_bar_roundeds.clear();
        self.saved_scroll_bar_thicknesses.clear();
        self.saved_scroll_bar_edge_widths.clear();
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
        self.saved_scroll_bar_colors.push(self.scroll_bar_color);
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
        self.saved_window_origins.push(self.window_origin);
    }

    pub fn restore(&mut self) {
        self.viewport = self.saved_viewports.pop().unwrap_or_default();
        self.color = self.saved_colors.pop().unwrap_or_default();
        self.scroll_bar_color = self.saved_scroll_bar_colors.pop().unwrap_or_default();
        self.scroll_bar_rounded = self.saved_scroll_bar_roundeds.pop().unwrap_or_default();
        self.scroll_bar_thickness = self.saved_scroll_bar_thicknesses.pop().unwrap_or_default();
        self.scroll_bar_edge_width = self.saved_scroll_bar_edge_widths.pop().unwrap_or_default();
        self.font_size = self.saved_font_sizes.pop().unwrap_or_default();
        self.font_family = self.saved_font_families.pop().unwrap_or_default();
        self.font_weight = self.saved_font_weights.pop().unwrap_or_default();
        self.font_style = self.saved_font_styles.pop().unwrap_or_default();
        self.line_height = self.saved_line_heights.pop().unwrap_or_default();
        self.window_origin = self.saved_window_origins.pop().unwrap_or_default();
    }

    pub fn app_state_mut(&mut self) -> &mut AppState {
        self.app_state
    }

    pub fn app_state(&self) -> &AppState {
        self.app_state
    }

    pub fn current_scroll_bar_color(&self) -> Option<Color> {
        self.scroll_bar_color
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
        let style = view_state.computed_style.to_taffy_style();
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
    pub(crate) scroll_bar_color: Option<Color>,
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
    pub(crate) saved_scroll_bar_colors: Vec<Option<Color>>,
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
        self.saved_scroll_bar_colors.push(self.scroll_bar_color);
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
        self.scroll_bar_color = self.saved_scroll_bar_colors.pop().unwrap_or_default();
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

    pub fn current_scroll_bar_color(&self) -> Option<Color> {
        self.scroll_bar_color
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

    pub fn get_computed_style(&mut self, id: Id) -> &Style {
        self.app_state.get_computed_style(id)
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
    /// request that this node be laid out again
    /// This will recursively request layout for all parents and set the `ChangeFlag::LAYOUT` at root
    pub fn request_layout(&mut self, id: Id) {
        self.app_state.request_layout(id);
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
