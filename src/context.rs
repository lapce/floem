use std::collections::{HashMap, HashSet};

use glazier::{
    kurbo::{Affine, Point, Rect, Shape, Size, Vec2},
    Scalable,
};
use parley::{swash::GlyphId, FontContext};
use taffy::{
    prelude::{Layout, Node},
    style::{AvailableSpace, Display},
};
use vello::{
    glyph::{
        pinot::{types::Tag, FontRef},
        GlyphContext,
    },
    peniko::{BrushRef, Color, Fill, Stroke},
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
    pub(crate) node: Node,
    pub(crate) children_nodes: Vec<Node>,
    pub(crate) request_layout: bool,
    pub(crate) viewport: Option<Rect>,
    pub(crate) style: Style,
    pub(crate) event_listeners: HashMap<EventListner, Box<EventCallback>>,
}

impl ViewState {
    fn new(taffy: &mut taffy::Taffy) -> Self {
        Self {
            node: taffy.new_leaf(taffy::style::Style::DEFAULT).unwrap(),
            viewport: None,
            request_layout: true,
            style: Style::default(),
            children_nodes: Vec::new(),
            event_listeners: HashMap::new(),
        }
    }
}

#[derive(Default)]
struct FontCache {
    glyphs: HashMap<GlyphId, Option<SceneFragment>>,
}

#[derive(Default)]
pub struct TextCache {
    fonts: HashMap<u64, FontCache>,
}

pub(crate) struct TransformContext {
    pub(crate) transform: Affine,
    pub(crate) viewport: Option<Rect>,
    pub(crate) color: Option<Color>,
    pub(crate) font_size: Option<f32>,
    pub(crate) saved_transforms: Vec<Affine>,
    pub(crate) saved_viewports: Vec<Option<Rect>>,
    pub(crate) saved_colors: Vec<Option<Color>>,
    pub(crate) saved_font_sizes: Vec<Option<f32>>,
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
    pub(crate) layout_changed: HashSet<Id>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            root: None,
            focus: None,
            active: None,
            root_size: Size::ZERO,
            taffy: taffy::Taffy::new(),
            view_states: HashMap::new(),
            layout_changed: HashSet::new(),
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
            .map(|s| s.style.display == Display::None)
            .unwrap_or(false)
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
        let view = self.view_state(id);
        view.request_layout = true;
        self.layout_changed.insert(id);
    }

    pub(crate) fn set_viewport(&mut self, id: Id, viewport: Rect) {
        let view = self.view_state(id);
        view.viewport = Some(viewport);
    }

    pub(crate) fn layout_node(&mut self, id: Id) -> Node {
        let view = self.view_state(id);
        view.node
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
    }
}

pub struct EventCx<'a> {
    pub(crate) app_state: &'a mut AppState,
}

impl<'a> EventCx<'a> {
    pub(crate) fn request_layout(&mut self, id: Id) {
        self.app_state.request_layout(id);
    }

    pub(crate) fn update_active(&mut self, id: Id) {
        self.app_state.update_active(id);
    }

    pub fn get_style(&self, id: Id) -> Option<&Style> {
        self.app_state.view_states.get(&id).map(|s| &s.style)
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
    pub(crate) app_state: &'a mut AppState,
    pub(crate) font_cx: &'a mut FontContext,
    pub(crate) viewport: Option<Rect>,
    pub(crate) font_size: Option<f32>,
    pub(crate) saved_viewports: Vec<Option<Rect>>,
    pub(crate) saved_font_sizes: Vec<Option<f32>>,
}

impl<'a> LayoutCx<'a> {
    pub(crate) fn clear(&mut self) {
        self.viewport = None;
        self.font_size = None;
        self.saved_viewports.clear();
        self.saved_font_sizes.clear();
    }

    pub fn save(&mut self) {
        self.saved_viewports.push(self.viewport);
        self.saved_font_sizes.push(self.font_size);
    }

    pub fn restore(&mut self) {
        self.viewport = self.saved_viewports.pop().unwrap_or_default();
        self.font_size = self.saved_font_sizes.pop().unwrap_or_default();
    }

    pub fn current_font_size(&self) -> Option<f32> {
        self.font_size
    }

    pub fn get_style(&self, id: Id) -> Option<&Style> {
        self.app_state.view_states.get(&id).map(|s| &s.style)
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
        let style = (&view.style).into();
        self.app_state.taffy.set_style(node, style);

        if has_children {
            let nodes = children(self);
            self.app_state.taffy.set_children(node, &nodes);
            let view = self.app_state.view_state(id);
            view.children_nodes = nodes;
        }

        node
    }
}

pub struct PaintCx<'a> {
    pub(crate) builder: &'a mut SceneBuilder<'a>,
    pub(crate) app_state: &'a mut AppState,
    pub(crate) paint_state: &'a mut PaintState,
    pub(crate) saved_transforms: Vec<Affine>,
    pub(crate) saved_viewports: Vec<Option<Rect>>,
    pub(crate) saved_colors: Vec<Option<Color>>,
    pub(crate) saved_font_sizes: Vec<Option<f32>>,
    pub(crate) transform: Affine,
    pub(crate) viewport: Option<Rect>,
    pub(crate) color: Option<Color>,
    pub(crate) font_size: Option<f32>,
}

impl<'a> PaintCx<'a> {
    pub fn save(&mut self) {
        self.saved_transforms.push(self.transform);
        self.saved_viewports.push(self.viewport);
        self.saved_colors.push(self.color);
        self.saved_font_sizes.push(self.font_size);
    }

    pub fn restore(&mut self) {
        self.transform = self.saved_transforms.pop().unwrap_or_default();
        self.viewport = self.saved_viewports.pop().unwrap_or_default();
        self.color = self.saved_colors.pop().unwrap_or_default();
        self.font_size = self.saved_font_sizes.pop().unwrap_or_default();
    }

    pub fn current_color(&self) -> Option<Color> {
        self.color
    }

    pub fn current_font_size(&self) -> Option<f32> {
        self.font_size
    }

    pub fn layout(&self, node: Node) -> Option<Layout> {
        self.app_state.taffy.layout(node).ok().copied()
    }

    pub fn get_layout(&mut self, id: Id) -> Option<Layout> {
        self.app_state.get_layout(id)
    }

    pub fn get_style(&self, id: Id) -> Option<&Style> {
        self.app_state.view_states.get(&id).map(|s| &s.style)
    }

    pub fn offset(&mut self, offset: (f64, f64)) {
        let mut new = self.transform.as_coeffs();
        new[4] += offset.0;
        new[5] += offset.1;
        self.transform = Affine::new(new);
    }

    pub fn transform(&mut self, id: Id) -> Size {
        if let Some(layout) = self.get_layout(id) {
            let offset = layout.location;
            let mut new = self.transform.as_coeffs();
            new[4] += offset.x as f64;
            new[5] += offset.y as f64;
            self.transform = Affine::new(new);

            let parent_viewport = self.viewport.map(|rect| {
                rect.with_origin(Point::new(
                    rect.x0 - offset.x as f64,
                    rect.y0 - offset.y as f64,
                ))
            });
            let viewport = self
                .app_state
                .view_states
                .get(&id)
                .and_then(|view| view.viewport);
            let size = Size::new(layout.size.width as f64, layout.size.height as f64);
            match (parent_viewport, viewport) {
                (Some(parent_viewport), Some(viewport)) => {
                    self.viewport = Some(
                        parent_viewport
                            .intersect(viewport)
                            .intersect(size.to_rect()),
                    );
                }
                (Some(parent_viewport), None) => {
                    self.viewport = Some(parent_viewport.intersect(size.to_rect()));
                }
                (None, Some(viewport)) => {
                    self.viewport = Some(viewport.intersect(size.to_rect()));
                }
                (None, None) => {
                    self.viewport = None;
                }
            }

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
        let viewport = self.viewport;
        for line in layout.lines() {
            if let Some(rect) = viewport {
                let metrics = line.metrics();
                let y = point.y + metrics.baseline as f64;
                if y + (metrics.descent as f64) < rect.y0 {
                    continue;
                }
                if y - ((metrics.ascent + metrics.leading) as f64) > rect.y1 {
                    break;
                }
            }
            'line_loop: for glyph_run in line.glyph_runs() {
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

                for glyph in glyph_run.glyphs() {
                    // let fragment = if let Some(fragment) =
                    //     self.paint_state.get_glyph(font.key.value(), glyph.id)
                    // {
                    //     fragment
                    // } else {
                    let mut gp = self.paint_state.glyph_contex.new_provider(
                        &font_ref,
                        Some(font.key.value()),
                        font_size,
                        false,
                        vars,
                    );
                    let fragment = gp.get(glyph.id, Some(&style.brush.0));
                    //     self.paint_state
                    //         .insert_glyph(font.key.value(), glyph.id, fragment);
                    //     self.paint_state
                    //         .get_glyph(font.key.value(), glyph.id)
                    //         .unwrap()
                    // };

                    if let Some(fragment) = fragment {
                        let gx = x + glyph.x;
                        let gy = y - glyph.y;
                        let xform = Affine::translate((gx as f64, gy as f64))
                            * Affine::scale_non_uniform(1.0, -1.0);
                        if let Some(rect) = viewport {
                            let xform = Affine::translate((point.x, point.y)) * xform;
                            let xform = xform.as_coeffs();
                            let cx = xform[4];
                            if cx + (glyph.advance as f64) < rect.x0 {
                                x += glyph.advance;
                                continue;
                            } else if cx > rect.x1 {
                                break 'line_loop;
                            }
                        }
                        self.builder.append(&fragment, Some(transform * xform));
                    }
                    x += glyph.advance;
                }
            }
        }
    }
}

pub struct PaintState {
    pub(crate) render_cx: RenderContext,
    pub(crate) text_cache: TextCache,
    glyph_contex: GlyphContext,
    surface: Option<RenderSurface>,
    renderer: Option<Renderer>,
    scene: Scene,
    handle: glazier::WindowHandle,
}

impl PaintState {
    pub fn new() -> Self {
        let render_cx = RenderContext::new().unwrap();

        Self {
            render_cx,
            surface: None,
            renderer: None,
            scene: Scene::default(),
            handle: Default::default(),
            text_cache: TextCache::default(),
            glyph_contex: GlyphContext::new(),
        }
    }

    pub(crate) fn connect(&mut self, handle: &glazier::WindowHandle) {
        self.handle = handle.clone();
    }

    fn get_glyph(&mut self, font_id: u64, glyph_id: GlyphId) -> Option<&Option<SceneFragment>> {
        let font = self.text_cache.fonts.entry(font_id).or_default();
        font.glyphs.get(&glyph_id)
    }

    fn insert_glyph(&mut self, font_id: u64, glyph_id: GlyphId, fragment: Option<SceneFragment>) {
        let font = self.text_cache.fonts.entry(font_id).or_default();
        font.glyphs.insert(glyph_id, fragment);
    }

    pub(crate) fn render(&mut self, fragment: &SceneFragment) {
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
            builder.append(fragment, transform);
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

    pub fn request_layout(&mut self, id: Id) {
        self.app_state.request_layout(id);
    }
}
