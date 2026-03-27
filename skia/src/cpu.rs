use anyhow::Result;
use floem_renderer::{
    BeginFrame, CpuBufferFormat, CpuBufferTarget, CustomRasterizer, DisplayCommandExt, RasterCore,
    RasterTarget, Rasterizer, RasterizerOutput,
};
use imaging::{
    BlurredRoundedRect, ClipRef, CustomPaintSink, FillRef, GlyphRunRef, GroupRef, PaintSink,
    StrokeRef,
};
use imaging_skia::{SkCanvasSink, SkiaCpuRenderState};
use peniko::ImageData;
use skia_safe as sk;

struct SkiaCpuCanvas<'a> {
    inner: SkCanvasSink<'a>,
}

impl PaintSink for SkiaCpuCanvas<'_> {
    fn push_clip(&mut self, clip: ClipRef<'_>) {
        self.inner.push_clip(clip);
    }

    fn pop_clip(&mut self) {
        self.inner.pop_clip();
    }

    fn push_group(&mut self, group: GroupRef<'_>) {
        self.inner.push_group(group);
    }

    fn pop_group(&mut self) {
        self.inner.pop_group();
    }

    fn fill(&mut self, draw: FillRef<'_>) {
        self.inner.fill(draw);
    }

    fn stroke(&mut self, draw: StrokeRef<'_>) {
        self.inner.stroke(draw);
    }

    fn glyph_run(
        &mut self,
        draw: GlyphRunRef<'_>,
        glyphs: &mut dyn Iterator<Item = imaging::record::Glyph>,
    ) {
        self.inner.glyph_run(draw, glyphs);
    }

    fn blurred_rounded_rect(&mut self, draw: BlurredRoundedRect) {
        self.inner.blurred_rounded_rect(draw);
    }
}

impl CustomPaintSink<DisplayCommandExt> for SkiaCpuCanvas<'_> {
    fn custom(&mut self, _command: &DisplayCommandExt) {}
}

pub struct SkiaCpuRenderer {
    inner: imaging_skia::SkiaCpuRenderer,
}

pub struct SkiaCpuTargetRenderer<'a> {
    state: SkiaCpuRenderState,
    surface: sk::Borrows<'a, sk::Surface>,
}

impl SkiaCpuRenderer {
    pub fn new(width: u32, height: u32, _scale: f64, _font_embolden: f32) -> Result<Self> {
        let width =
            u16::try_from(width).map_err(|_| anyhow::anyhow!("skia cpu width out of range"))?;
        let height =
            u16::try_from(height).map_err(|_| anyhow::anyhow!("skia cpu height out of range"))?;
        Ok(Self {
            inner: imaging_skia::SkiaCpuRenderer::new(width, height),
        })
    }

    pub fn debug_info(&self) -> String {
        "name: Skia CPU\ninfo: imaging_skia::SkCanvasSink".to_string()
    }

    pub fn inner(&mut self) -> &mut imaging_skia::SkiaCpuRenderer {
        &mut self.inner
    }

    fn readback_image(&mut self) -> Result<ImageData, String> {
        self.inner.read_image().map_err(|err| format!("{err:?}"))
    }

    fn with_canvas<R>(&mut self, f: &mut dyn FnMut(&mut SkiaCpuCanvas<'_>) -> R) -> R {
        let mut canvas = SkiaCpuCanvas {
            inner: SkCanvasSink::new(self.inner.surface().canvas()),
        };
        f(&mut canvas)
    }
}

impl<'a> SkiaCpuTargetRenderer<'a> {
    pub fn debug_info(&self) -> String {
        "name: Skia CPU\ninfo: imaging_skia::SkCanvasSink".to_string()
    }

    pub fn with_renderer<R>(
        &mut self,
        f: impl FnOnce(&mut imaging_skia::SkiaCpuRendererRef<'_>) -> R,
    ) -> R {
        let mut renderer = self.state.bind(&mut self.surface);
        f(&mut renderer)
    }

    fn readback_image(&mut self) -> Result<ImageData, String> {
        self.with_renderer(|renderer| renderer.read_image())
            .map_err(|err| format!("{err:?}"))
    }

    fn with_canvas<R>(&mut self, f: &mut dyn FnMut(&mut SkiaCpuCanvas<'_>) -> R) -> R {
        let mut canvas = SkiaCpuCanvas {
            inner: SkCanvasSink::new(self.surface.canvas()),
        };
        f(&mut canvas)
    }
}

impl RasterCore for SkiaCpuRenderer {
    fn with_paint_sink(&mut self, f: &mut dyn FnMut(&mut dyn PaintSink)) {
        self.with_canvas(&mut |canvas| f(canvas));
    }

    fn finish(&mut self) {
        self.with_canvas(&mut |canvas| {
            let _ = canvas.inner.finish();
        });
    }

    fn readback(&mut self) -> Option<RasterizerOutput> {
        self.readback_image().ok().map(RasterizerOutput::Image)
    }
}

impl Rasterizer for SkiaCpuRenderer {
    fn begin(&mut self, _frame: BeginFrame) {
        let canvas = self.inner.surface().canvas();
        canvas.restore_to_count(1);
        canvas.reset_matrix();
        canvas.clear(sk::Color::TRANSPARENT);
    }
}

impl CustomRasterizer for SkiaCpuRenderer {
    fn with_custom_paint_sink(
        &mut self,
        f: &mut dyn FnMut(&mut dyn CustomPaintSink<DisplayCommandExt>),
    ) {
        self.with_canvas(&mut |canvas| f(canvas));
    }

    fn debug_info(&self) -> String {
        Self::debug_info(self)
    }
}

impl RasterCore for SkiaCpuTargetRenderer<'_> {
    fn with_paint_sink(&mut self, f: &mut dyn FnMut(&mut dyn PaintSink)) {
        self.with_canvas(&mut |canvas| f(canvas));
    }

    fn finish(&mut self) {
        self.with_canvas(&mut |canvas| {
            let _ = canvas.inner.finish();
        });
    }

    fn readback(&mut self) -> Option<RasterizerOutput> {
        self.readback_image().ok().map(RasterizerOutput::Image)
    }
}

impl<'a> CustomRasterizer for SkiaCpuTargetRenderer<'a> {
    fn with_custom_paint_sink(
        &mut self,
        f: &mut dyn FnMut(&mut dyn CustomPaintSink<DisplayCommandExt>),
    ) {
        self.with_canvas(&mut |canvas| f(canvas));
    }

    fn debug_info(&self) -> String {
        Self::debug_info(self)
    }
}

impl<'a> RasterTarget for SkiaCpuTargetRenderer<'a> {
    type Target = CpuBufferTarget<'a>;

    fn create(target: Self::Target) -> Result<Self, String> {
        let color_type = match target.format {
            CpuBufferFormat::Rgba8Opaque => sk::ColorType::RGBA8888,
            CpuBufferFormat::Bgra8Opaque => sk::ColorType::BGRA8888,
        };
        let info = sk::ImageInfo::new(
            (target.width as i32, target.height as i32),
            color_type,
            sk::AlphaType::Opaque,
            None,
        );
        let surface =
            sk::surfaces::wrap_pixels(&info, target.buffer, Some(target.bytes_per_row), None)
                .ok_or_else(|| "wrap skia cpu target pixels".to_string())?;
        Ok(Self {
            state: SkiaCpuRenderState::new(),
            surface,
        })
    }
}
