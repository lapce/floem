use std::collections::{HashMap, HashSet};

use glazier::{
    kurbo::{Affine, Point, Shape, Size, Vec2},
    Scalable,
};
use parley::FontContext;
use taffy::{
    prelude::{Layout, Node},
    style::AvailableSpace,
};
use vello::{
    glyph::{
        pinot::{types::Tag, FontRef},
        GlyphContext,
    },
    peniko::{BrushRef, Fill, Stroke},
    util::{RenderContext, RenderSurface},
    Renderer, Scene, SceneBuilder, SceneFragment,
};

use crate::{
    event::{Event, EventListner},
    id::{Id, IDPATHS},
    style::Style,
    text::ParleyBrush,
};

pub type EventCallback = dyn Fn(&Event) -> bool;

pub struct ViewState {
    pub(crate) node: Option<Node>,
    pub(crate) style: Style,
    pub(crate) children_nodes: Option<Vec<Node>>,
    pub(crate) event_listeners: HashMap<EventListner, Box<EventCallback>>,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            node: None,
            style: Style::default(),
            children_nodes: None,
            event_listeners: HashMap::new(),
        }
    }
}

pub struct AppState {
    pub(crate) focus: Option<Id>,
    pub(crate) root: Option<Node>,
    pub(crate) root_size: Size,
    pub taffy: taffy::Taffy,
    pub(crate) view_states: HashMap<Id, ViewState>,
    pub(crate) layout_changed: HashSet<Id>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            root: None,
            focus: None,
            root_size: Size::ZERO,
            taffy: taffy::Taffy::new(),
            view_states: HashMap::new(),
            layout_changed: HashSet::new(),
        }
    }

    pub fn set_root_size(&mut self, size: Size) {
        self.root_size = size;
        self.compute_layout();
    }

    pub fn compute_layout(&mut self) {
        if let Some(root) = self.root {
            self.taffy.compute_layout(
                root,
                taffy::prelude::Size {
                    width: AvailableSpace::Definite(self.root_size.width as f32),
                    height: AvailableSpace::Definite(self.root_size.height as f32),
                },
            );
        }
    }

    pub(crate) fn process_layout_changed(&mut self) {
        let changed = std::mem::take(&mut self.layout_changed);
        IDPATHS.with(|paths| {
            let paths = paths.borrow();
            for id in changed {
                if let Some(id_path) = paths.get(&id) {
                    for parent in &id_path.0[..&id_path.0.len() - 1] {
                        self.reset_children_layout(*parent);
                    }
                }
            }
        });
        self.layout_changed.clear();
    }

    pub(crate) fn request_layout(&mut self, id: Id) {
        let view = self.view_states.entry(id).or_default();
        view.node = None;
        self.layout_changed.insert(id);
    }

    pub(crate) fn layout_node(&mut self, id: Id) -> Option<Node> {
        let view = self.view_states.entry(id).or_default();
        view.node
    }

    pub(crate) fn reset_children_layout(&mut self, id: Id) {
        let view = self.view_states.entry(id).or_default();
        view.children_nodes = None;
        self.request_layout(id);
    }

    fn get_layout(&self, id: Id) -> Option<Layout> {
        self.view_states
            .get(&id)
            .and_then(|view| view.node.as_ref())
            .and_then(|node| self.taffy.layout(*node).ok())
            .copied()
    }
}

pub struct EventCx<'a> {
    pub(crate) app_state: &'a mut AppState,
}

impl<'a> EventCx<'a> {
    pub(crate) fn get_layout(&self, id: Id) -> Option<Layout> {
        self.app_state.get_layout(id)
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
        if let Some(layout) = self.get_layout(id) {
            event.offset((layout.location.x as f64, layout.location.y as f64))
        } else {
            event
        }
    }

    pub(crate) fn should_send(&mut self, id: Id, event: &Event) -> bool {
        let point = event.point();
        if let Some(point) = point {
            if let Some(layout) = self.get_layout(id) {
                if layout.location.x as f64 <= point.x
                    && point.x <= (layout.location.x + layout.size.width) as f64
                    && layout.location.y as f64 <= point.y
                    && point.y <= (layout.location.y + layout.size.height) as f64
                {
                    return true;
                }
            }
        }
        false
    }
}

pub struct LayoutCx<'a> {
    pub(crate) layout_state: &'a mut AppState,
    pub(crate) font_cx: &'a mut FontContext,
}

impl<'a> LayoutCx<'a> {
    pub fn get_style(&self, id: Id) -> Option<&Style> {
        self.layout_state.view_states.get(&id).map(|s| &s.style)
    }

    pub(crate) fn layout_node(
        &mut self,
        id: Id,
        has_children: bool,
        mut children: impl FnMut(&mut LayoutCx) -> Vec<Node>,
    ) -> Node {
        if let Some(node) = self.layout_state.layout_node(id) {
            return node;
        }
        let view = self.layout_state.view_states.entry(id).or_default();
        let style = (&view.style).into();
        let node = if !has_children {
            self.layout_state.taffy.new_leaf(style).unwrap()
        } else if let Some(nodes) = view.children_nodes.as_ref() {
            self.layout_state
                .taffy
                .new_with_children(style, nodes)
                .unwrap()
        } else {
            let nodes = children(self);
            let node = self
                .layout_state
                .taffy
                .new_with_children(style, &nodes)
                .unwrap();
            let view = self.layout_state.view_states.entry(id).or_default();
            view.children_nodes = Some(nodes);
            node
        };
        let view = self.layout_state.view_states.entry(id).or_default();
        view.node = Some(node);
        node
    }
}

pub struct PaintCx<'a> {
    pub(crate) builder: &'a mut SceneBuilder<'a>,
    pub(crate) layout_state: &'a mut AppState,
    pub(crate) saved_transforms: Vec<Affine>,
    pub(crate) transform: Affine,
}

impl<'a> PaintCx<'a> {
    pub fn save(&mut self) {
        self.saved_transforms.push(self.transform);
    }

    pub fn restore(&mut self) {
        self.transform = self.saved_transforms.pop().unwrap_or_default();
    }

    pub fn get_layout(&mut self, id: Id) -> Option<Layout> {
        self.layout_state.get_layout(id)
    }

    pub fn get_style(&self, id: Id) -> Option<&Style> {
        self.layout_state.view_states.get(&id).map(|s| &s.style)
    }

    pub fn transform(&mut self, id: Id) -> Size {
        if let Some(layout) = self.get_layout(id) {
            let offset = layout.location;
            let mut new = self.transform.as_coeffs();
            new[4] += offset.x as f64;
            new[5] += offset.y as f64;
            self.transform = Affine::new(new);
            Size::new(layout.size.width as f64, layout.size.height as f64)
        } else {
            Size::ZERO
        }
    }

    pub fn stroke<'b>(
        &mut self,
        path: &impl Shape,
        brush: impl Into<BrushRef<'b>>,
        stroke_width: f64,
    ) {
        self.builder.stroke(
            &Stroke::new(stroke_width as f32),
            self.transform,
            brush,
            None,
            path,
        )
    }

    pub fn fill<'b>(&mut self, path: &impl Shape, brush: impl Into<BrushRef<'b>>) {
        self.builder
            .fill(Fill::NonZero, self.transform, brush, None, path)
    }

    pub fn render_text(&mut self, layout: &parley::Layout<ParleyBrush>, point: Point) {
        let transform = self.transform * Affine::translate((point.x, point.y));
        let mut gcx = GlyphContext::new();
        for line in layout.lines() {
            for glyph_run in line.glyph_runs() {
                let mut x = glyph_run.offset();
                let y = glyph_run.baseline();
                let run = glyph_run.run();
                let font = run.font().as_ref();
                let font_size = run.font_size();
                let font_ref = FontRef {
                    data: font.data,
                    offset: font.offset,
                };
                let style = glyph_run.style();
                let vars: [(Tag, f32); 0] = [];
                let mut gp = gcx.new_provider(&font_ref, None, font_size, false, vars);
                for glyph in glyph_run.glyphs() {
                    if let Some(fragment) = gp.get(glyph.id, Some(&style.brush.0)) {
                        let gx = x + glyph.x;
                        let gy = y - glyph.y;
                        let xform = Affine::translate((gx as f64, gy as f64))
                            * Affine::scale_non_uniform(1.0, -1.0);
                        self.builder.append(&fragment, Some(transform * xform));
                    }
                    x += glyph.advance;
                }
            }
        }
    }
}

pub struct PaintState {
    pub(crate) fragment: SceneFragment,
    pub(crate) render_cx: RenderContext,
    surface: Option<RenderSurface>,
    renderer: Option<Renderer>,
    scene: Scene,
    handle: glazier::WindowHandle,
}

impl PaintState {
    pub fn new() -> Self {
        let render_cx = RenderContext::new().unwrap();

        Self {
            fragment: SceneFragment::new(),
            render_cx,
            surface: None,
            renderer: None,
            scene: Scene::default(),
            handle: Default::default(),
        }
    }

    pub(crate) fn connect(&mut self, handle: &glazier::WindowHandle) {
        self.handle = handle.clone();
    }

    pub(crate) fn render(&mut self) {
        let handle = &self.handle;
        let scale = handle.get_scale().unwrap_or_default();
        let insets = handle.content_insets().to_px(scale);
        let mut size = handle.get_size().to_px(scale);
        size.width -= insets.x_value();
        size.height -= insets.y_value();
        let width = size.width as u32;
        let height = size.height as u32;
        if self.surface.is_none() {
            self.surface = Some(futures::executor::block_on(
                self.render_cx.create_surface(handle, width, height),
            ));
        }
        if let Some(surface) = self.surface.as_mut() {
            if surface.config.width != width || surface.config.height != height {
                self.render_cx.resize_surface(surface, width, height);
            }
            let (scale_x, scale_y) = (scale.x(), scale.y());
            let transform = if scale_x != 1.0 || scale_y != 1.0 {
                Some(Affine::scale_non_uniform(scale_x, scale_y))
            } else {
                None
            };
            let mut builder = SceneBuilder::for_scene(&mut self.scene);
            builder.append(&self.fragment, transform);
            // self.counter += 1;
            let surface_texture = surface
                .surface
                .get_current_texture()
                .expect("failed to acquire next swapchain texture");
            let dev_id = surface.dev_id;
            let device = &self.render_cx.devices[dev_id].device;
            let queue = &self.render_cx.devices[dev_id].queue;
            self.renderer
                .get_or_insert_with(|| Renderer::new(device).unwrap())
                .render_to_surface(device, queue, &self.scene, &surface_texture, width, height)
                .expect("failed to render to surface");
            surface_texture.present();
            device.poll(wgpu::Maintain::Wait);
        }
    }
}

pub struct UpdateCx<'a> {
    pub(crate) app_state: &'a mut AppState,
}

impl<'a> UpdateCx<'a> {
    pub(crate) fn reset_children_layout(&mut self, id: Id) {
        self.app_state.reset_children_layout(id);
    }

    pub(crate) fn request_layout(&mut self, id: Id) {
        self.app_state.request_layout(id);
    }
}
