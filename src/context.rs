use std::{
    collections::{HashMap, HashSet},
    ops::{Deref, DerefMut},
};

use floem_renderer::{
    cosmic_text::{Style as FontStyle, Weight},
    Renderer as FloemRenderer,
};
use glazier::{
    kurbo::{Affine, Point, Rect, Shape, Size, Vec2},
    Scale,
};
use taffy::{
    prelude::{Layout, Node},
    style::{AvailableSpace, Display},
};
use vello::peniko::Color;

use crate::{
    app::StyleSelector,
    event::{Event, EventListner},
    id::Id,
    style::{ComputedStyle, CursorStyle, Style},
};

pub type EventCallback = dyn Fn(&Event) -> bool;
pub type ResizeCallback = dyn Fn(Point, Rect);

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
    pub(crate) style: Style,
    pub(crate) hover_style: Option<Style>,
    pub(crate) disabled_style: Option<Style>,
    pub(crate) focus_style: Option<Style>,
    pub(crate) active_style: Option<Style>,
    pub(crate) computed_style: ComputedStyle,
    pub(crate) event_listeners: HashMap<EventListner, Box<EventCallback>>,
    pub(crate) resize_listener: Option<ResizeListener>,
}

impl ViewState {
    fn new(taffy: &mut taffy::Taffy) -> Self {
        Self {
            node: taffy.new_leaf(taffy::style::Style::DEFAULT).unwrap(),
            viewport: None,
            request_layout: true,
            style: Style::default(),
            computed_style: ComputedStyle::default(),
            hover_style: None,
            disabled_style: None,
            focus_style: None,
            active_style: None,
            children_nodes: Vec::new(),
            event_listeners: HashMap::new(),
            resize_listener: None,
        }
    }

    pub(crate) fn compute_style(
        &mut self,
        view_style: Option<Style>,
        interact_state: InteractionState,
    ) {
        let mut computed_style = if let Some(view_style) = view_style {
            view_style.apply(self.style.clone())
        } else {
            self.style.clone()
        };

        if interact_state.is_hovered {
            if let Some(hover_style) = self.hover_style.clone() {
                computed_style = computed_style.apply(hover_style);
            }
        }

        if interact_state.is_disabled {
            if let Some(disabled_style) = self.disabled_style.clone() {
                computed_style = computed_style.apply(disabled_style);
            }
        }

        if interact_state.is_focused {
            if let Some(focus_style) = self.focus_style.clone() {
                computed_style = computed_style.apply(focus_style);
            }
        }

        if interact_state.is_active {
            if let Some(active_style) = self.active_style.clone() {
                computed_style = computed_style.apply(active_style);
            }
        }

        self.computed_style = computed_style.compute(&ComputedStyle::default());
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
    pub taffy: taffy::Taffy,
    pub(crate) view_states: HashMap<Id, ViewState>,
    pub(crate) disabled: HashSet<Id>,
    pub(crate) hovered: HashSet<Id>,
    pub(crate) cursor: CursorStyle,
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
            root_size: Size::ZERO,
            taffy,
            view_states: HashMap::new(),
            disabled: HashSet::new(),
            hovered: HashSet::new(),
            cursor: CursorStyle::Default,
        }
    }

    pub fn view_state(&mut self, id: Id) -> &mut ViewState {
        self.view_states
            .entry(id)
            .or_insert_with(|| ViewState::new(&mut self.taffy))
    }

    pub fn is_hidden(&self, id: Id) -> bool {
        self.view_states
            .get(&id)
            // TODO: this unwrap_or is wrong. The style might not specify it, but the underlying view style can
            .map(|s| s.style.display.unwrap_or(Display::Flex) == Display::None)
            .unwrap_or(false)
    }

    pub fn is_hovered(&self, id: &Id) -> bool {
        self.hovered.contains(&id)
    }

    pub fn is_disabled(&self, id: &Id) -> bool {
        self.disabled.contains(id)
    }

    pub fn get_interact_state(&self, id: &Id) -> InteractionState {
        InteractionState {
            is_hovered: self.is_hovered(id),
            is_disabled: self.is_disabled(id),
            is_focused: self.focus.map(|f| &f == id).unwrap_or(false),
            is_active: self.active.map(|a| &a == id).unwrap_or(false),
        }
    }

    pub fn set_root_size(&mut self, size: Size) {
        self.root_size = size;
        self.compute_layout();
    }

    pub(crate) fn compute_style(&mut self, id: Id, view_style: Option<Style>) {
        let interact_state = self.get_interact_state(&id);
        let view_state = self.view_state(id);
        view_state.compute_style(view_style, interact_state);
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
                    width: AvailableSpace::Definite(self.root_size.width as f32),
                    height: AvailableSpace::Definite(self.root_size.height as f32),
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

    pub(crate) fn reset_children_layout(&mut self, id: Id) {
        let view = self.view_state(id);
        view.children_nodes.clear();
        self.request_layout(id);
    }

    pub(crate) fn get_layout(&self, id: Id) -> Option<Layout> {
        self.view_states
            .get(&id)
            .map(|view| view.node)
            .and_then(|node| self.taffy.layout(node).ok())
            .copied()
    }

    pub(crate) fn update_active(&mut self, id: Id) {
        self.active = Some(id);

        // To apply the styles of the Active selector
        if self.has_style_for_sel(id, StyleSelector::Active) {
            self.request_layout(id);
        }
    }

    pub(crate) fn has_style_for_sel(&mut self, id: Id, selector_kind: StyleSelector) -> bool {
        let view_state = self.view_state(id);

        match selector_kind {
            StyleSelector::Hover => view_state.hover_style.is_some(),
            StyleSelector::Focus => view_state.focus_style.is_some(),
            StyleSelector::Disabled => view_state.disabled_style.is_some(),
            StyleSelector::Active => view_state.active_style.is_some(),
        }
    }
}

pub struct EventCx<'a> {
    pub(crate) app_state: &'a mut AppState,
}

impl<'a> EventCx<'a> {
    pub(crate) fn update_active(&mut self, id: Id) {
        self.app_state.update_active(id);
    }

    pub fn get_style(&self, id: Id) -> Option<&Style> {
        self.app_state.view_states.get(&id).map(|s| &s.style)
    }

    pub fn get_hover_style(&self, id: Id) -> Option<&Style> {
        if let Some(vs) = self.app_state.view_states.get(&id) {
            return vs.hover_style.as_ref();
        }

        None
    }

    pub(crate) fn get_layout(&self, id: Id) -> Option<Layout> {
        self.app_state.get_layout(id)
    }

    pub(crate) fn get_size(&self, id: Id) -> Option<Size> {
        self.app_state
            .get_layout(id)
            .map(|l| Size::new(l.size.width as f64, l.size.height as f64))
    }

    pub(crate) fn get_event_listener(
        &self,
        id: Id,
    ) -> Option<&HashMap<EventListner, Box<EventCallback>>> {
        self.app_state
            .view_states
            .get(&id)
            .map(|s| &s.event_listeners)
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
        let point = event.point();
        if let Some(point) = point {
            if self.app_state.is_hidden(id)
                || (self.app_state.is_disabled(&id) && !event.allows_disabled())
            {
                return false;
            }
            if let Some(layout) = self.get_layout(id) {
                if layout.location.x as f64 <= point.x
                    && point.x <= (layout.location.x + layout.size.width) as f64
                    && layout.location.y as f64 <= point.y
                    && point.y <= (layout.location.y + layout.size.height) as f64
                {
                    return true;
                }
                // Needed to handle mouse leave for hovered views
                if self.app_state.is_hovered(&id) {
                    return true;
                }
            }
        }
        false
    }
}

#[derive(Default)]
pub struct InteractionState {
    pub(crate) is_hovered: bool,
    pub(crate) is_disabled: bool,
    pub(crate) is_focused: bool,
    pub(crate) is_active: bool,
}

pub struct LayoutCx<'a> {
    pub(crate) app_state: &'a mut AppState,
    pub(crate) viewport: Option<Rect>,
    pub(crate) font_size: Option<f32>,
    pub(crate) font_family: Option<String>,
    pub(crate) font_weight: Option<Weight>,
    pub(crate) font_style: Option<FontStyle>,
    pub(crate) window_origin: Point,
    pub(crate) saved_viewports: Vec<Option<Rect>>,
    pub(crate) saved_font_sizes: Vec<Option<f32>>,
    pub(crate) saved_font_families: Vec<Option<String>>,
    pub(crate) saved_font_weights: Vec<Option<Weight>>,
    pub(crate) saved_font_styles: Vec<Option<FontStyle>>,
    pub(crate) saved_window_origins: Vec<Point>,
}

impl<'a> LayoutCx<'a> {
    pub(crate) fn clear(&mut self) {
        self.viewport = None;
        self.font_size = None;
        self.window_origin = Point::ZERO;
        self.saved_viewports.clear();
        self.saved_font_sizes.clear();
        self.saved_font_families.clear();
        self.saved_font_weights.clear();
        self.saved_font_styles.clear();
        self.saved_window_origins.clear();
    }

    pub fn save(&mut self) {
        self.saved_viewports.push(self.viewport);
        self.saved_font_sizes.push(self.font_size);
        self.saved_font_families.push(self.font_family.clone());
        self.saved_font_weights.push(self.font_weight);
        self.saved_font_styles.push(self.font_style);
        self.saved_window_origins.push(self.window_origin);
    }

    pub fn restore(&mut self) {
        self.viewport = self.saved_viewports.pop().unwrap_or_default();
        self.font_size = self.saved_font_sizes.pop().unwrap_or_default();
        self.font_family = self.saved_font_families.pop().unwrap_or_default();
        self.font_weight = self.saved_font_weights.pop().unwrap_or_default();
        self.font_style = self.saved_font_styles.pop().unwrap_or_default();
        self.window_origin = self.saved_window_origins.pop().unwrap_or_default();
    }

    pub fn current_font_size(&self) -> Option<f32> {
        self.font_size
    }

    pub fn current_font_family(&self) -> Option<&str> {
        self.font_family.as_deref()
    }

    pub fn get_layout(&self, id: Id) -> Option<Layout> {
        self.app_state.get_layout(id)
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
    pub(crate) saved_transforms: Vec<Affine>,
    pub(crate) saved_clips: Vec<Option<Rect>>,
    pub(crate) saved_colors: Vec<Option<Color>>,
    pub(crate) saved_font_sizes: Vec<Option<f32>>,
    pub(crate) saved_font_families: Vec<Option<String>>,
    pub(crate) saved_font_weights: Vec<Option<Weight>>,
    pub(crate) saved_font_styles: Vec<Option<FontStyle>>,
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
    }

    pub fn restore(&mut self) {
        self.transform = self.saved_transforms.pop().unwrap_or_default();
        self.clip = self.saved_clips.pop().unwrap_or_default();
        self.color = self.saved_colors.pop().unwrap_or_default();
        self.font_size = self.saved_font_sizes.pop().unwrap_or_default();
        self.font_family = self.saved_font_families.pop().unwrap_or_default();
        self.font_weight = self.saved_font_weights.pop().unwrap_or_default();
        self.font_style = self.saved_font_styles.pop().unwrap_or_default();
        let renderer = self.paint_state.renderer.as_mut().unwrap();
        renderer.transform(self.transform);
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

    pub fn clip(&mut self, shape: &impl Shape) {
        let rect = shape.bounding_box();
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
        self.renderer.as_mut().unwrap().resize(scale, size);
    }
}

pub struct UpdateCx<'a> {
    pub(crate) app_state: &'a mut AppState,
}

impl<'a> UpdateCx<'a> {
    pub(crate) fn reset_children_layout(&mut self, id: Id) {
        self.app_state.reset_children_layout(id);
    }

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
