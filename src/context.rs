use std::collections::HashMap;

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
    peniko::{BrushRef, Stroke},
    util::{RenderContext, RenderSurface},
    Renderer, Scene, SceneBuilder, SceneFragment,
};

use crate::{id::Id, text::ParleyBrush};

pub struct ViewState {
    pub(crate) layout: Layout,
    pub(crate) node: Node,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            layout: Layout::new(),
            node: Node::default(),
        }
    }
}

pub struct LayoutState {
    pub(crate) root: Option<Node>,
    pub(crate) root_size: Size,
    pub taffy: taffy::Taffy,
    pub(crate) layouts: HashMap<Id, ViewState>,
}

impl LayoutState {
    pub fn new() -> Self {
        Self {
            root: None,
            root_size: Size::ZERO,
            taffy: taffy::Taffy::new(),
            layouts: HashMap::new(),
        }
    }

    pub fn set_root_size(&mut self, size: Size) {
        self.root_size = size;
        self.compute_layout();
    }

    pub fn compute_layout(&mut self) {
        if let Some(root) = self.root {
            println!("compute layou {:?}", self.root_size);
            self.taffy.compute_layout(
                root,
                taffy::prelude::Size {
                    width: AvailableSpace::Definite(self.root_size.width as f32),
                    height: AvailableSpace::Definite(self.root_size.height as f32),
                },
            );
        }
    }
}

pub struct LayoutCx<'a> {
    pub(crate) layout_state: &'a mut LayoutState,
    pub(crate) font_cx: &'a mut FontContext,
}

pub struct PaintCx<'a> {
    pub(crate) builder: &'a mut SceneBuilder<'a>,
    pub(crate) layout_state: &'a mut LayoutState,
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

    pub fn transform(&mut self, id: Id) -> Size {
        let size = if let Some(layout) = self.layout_state.layouts.get(&id) {
            let offset = layout.layout.location;
            let mut new = self.transform.as_coeffs();
            new[4] += offset.x as f64;
            new[5] += offset.y as f64;
            self.transform = Affine::new(new);
            Size::new(
                layout.layout.size.width as f64,
                layout.layout.size.height as f64,
            )
        } else {
            Size::ZERO
        };
        size
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
    async_handle: tokio::runtime::Handle,
}

impl PaintState {
    pub fn new(async_handle: tokio::runtime::Handle) -> Self {
        let render_cx = RenderContext::new().unwrap();

        Self {
            fragment: SceneFragment::new(),
            render_cx,
            surface: None,
            renderer: None,
            scene: Scene::default(),
            handle: Default::default(),
            async_handle,
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
            println!("render size: {:?}", size);
            self.surface = Some(
                self.async_handle
                    .block_on(self.render_cx.create_surface(handle, width, height)),
            );
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
