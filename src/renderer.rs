use crate::cosmic_text::TextLayout;
use floem_vger::VgerRenderer;
use glazier::{
    kurbo::{Affine, Rect, Shape, Size},
    Scalable, Scale, WindowHandle,
};
use peniko::BrushRef;

pub enum Renderer {
    Vger(VgerRenderer),
}

impl Renderer {
    pub fn new(handle: &WindowHandle) -> Self {
        let scale = handle.get_scale().unwrap_or_default();
        let size = handle.get_size().to_px(scale);
        Self::Vger(
            VgerRenderer::new(handle, size.width as u32, size.height as u32, scale.x()).unwrap(),
        )
    }

    pub fn resize(&mut self, scale: Scale, size: Size) {
        let size = size.to_px(scale);
        match self {
            Renderer::Vger(r) => r.resize(size.width as u32, size.height as u32, scale.x()),
        }
    }

    pub fn set_scale(&mut self, scale: Scale) {
        match self {
            Renderer::Vger(r) => r.set_scale(scale.x()),
        }
    }
}

impl floem_renderer::Renderer for Renderer {
    fn begin(&mut self) {
        match self {
            Renderer::Vger(r) => {
                r.begin();
            }
        }
    }

    fn clip(&mut self, shape: &impl Shape) {
        match self {
            Renderer::Vger(v) => {
                v.clip(shape);
            }
        }
    }

    fn clear_clip(&mut self) {
        match self {
            Renderer::Vger(v) => {
                v.clear_clip();
            }
        }
    }

    fn stroke<'b>(&mut self, shape: &impl Shape, brush: impl Into<BrushRef<'b>>, width: f64) {
        match self {
            Renderer::Vger(v) => {
                v.stroke(shape, brush, width);
            }
        }
    }

    fn fill<'b>(
        &mut self,
        path: &impl glazier::kurbo::Shape,
        brush: impl Into<peniko::BrushRef<'b>>,
        blur_radius: f64,
    ) {
        match self {
            Renderer::Vger(v) => {
                v.fill(path, brush, blur_radius);
            }
        }
    }

    fn draw_text(&mut self, layout: &TextLayout, pos: impl Into<glazier::kurbo::Point>) {
        match self {
            Renderer::Vger(v) => {
                v.draw_text(layout, pos);
            }
        }
    }

    fn draw_svg<'b>(
        &mut self,
        svg: floem_renderer::Svg<'b>,
        rect: Rect,
        brush: Option<impl Into<BrushRef<'b>>>,
    ) {
        match self {
            Renderer::Vger(v) => {
                v.draw_svg(svg, rect, brush);
            }
        }
    }

    fn transform(&mut self, transform: Affine) {
        match self {
            Renderer::Vger(v) => {
                v.transform(transform);
            }
        }
    }

    fn set_z_index(&mut self, z_index: i32) {
        match self {
            Renderer::Vger(v) => {
                v.set_z_index(z_index);
            }
        }
    }

    fn finish(&mut self) {
        match self {
            Renderer::Vger(r) => {
                r.finish();
            }
        }
    }
}
