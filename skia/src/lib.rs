use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use anyrender::{ImageRenderer as _, WindowRenderer as _};
use anyrender_skia::{SkiaImageRenderer, SkiaWindowRenderer};
use floem_renderer::text::{Glyph, GlyphRunProps};
use floem_renderer::{Img, Renderer, Svg};
use peniko::kurbo::{Affine, BezPath, Point, Rect, Shape, Size, Stroke};
use peniko::{
    Blob, Brush, BrushRef, Compose, Fill, ImageAlphaType, ImageData, ImageFormat, Mix, Style,
};
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};
use resvg::tiny_skia::{Pixmap, Transform};
use winit::window::Window;

const PATH_TOLERANCE: f64 = 0.1;

enum Command {
    Stroke {
        shape: BezPath,
        brush: Brush,
        stroke: Stroke,
        transform: Affine,
    },
    Fill {
        shape: BezPath,
        brush: Brush,
        transform: Affine,
    },
    BoxShadow {
        rect: Rect,
        brush: peniko::Color,
        radius: f64,
        std_dev: f64,
        transform: Affine,
    },
    PushLayer {
        blend: peniko::BlendMode,
        alpha: f32,
        transform: Affine,
        clip: BezPath,
    },
    PushClip {
        transform: Affine,
        clip: BezPath,
    },
    PopLayer,
    Glyphs {
        font: peniko::FontData,
        font_size: f32,
        hint: bool,
        normalized_coords: Vec<i16>,
        style: Style,
        brush: Brush,
        brush_alpha: f32,
        transform: Affine,
        glyph_transform: Option<Affine>,
        glyphs: Vec<anyrender::Glyph>,
    },
    Image {
        image: peniko::ImageBrush,
        rect: Rect,
        transform: Affine,
    },
    Svg {
        image: peniko::ImageBrush,
        rect: Rect,
        transform: Affine,
        brush: Option<Brush>,
    },
}

pub struct SkiaRenderer {
    window_renderer: SkiaWindowRenderer,
    capture_renderer: SkiaImageRenderer,
    commands: Vec<Command>,
    svg_cache: HashMap<(Vec<u8>, u32, u32), peniko::ImageBrush>,
    size: Size,
    transform: Affine,
    capture: bool,
}

#[derive(Clone)]
struct AnyrenderWindow(Arc<dyn Window>);

impl HasWindowHandle for AnyrenderWindow {
    fn window_handle(&self) -> std::result::Result<WindowHandle<'_>, HandleError> {
        self.0.window_handle()
    }
}

impl HasDisplayHandle for AnyrenderWindow {
    fn display_handle(&self) -> std::result::Result<DisplayHandle<'_>, HandleError> {
        self.0.display_handle()
    }
}

impl SkiaRenderer {
    pub fn new(
        window: Arc<dyn Window>,
        width: u32,
        height: u32,
        scale: f64,
        _font_embolden: f32,
    ) -> Result<Self> {
        let width = width.max(1);
        let height = height.max(1);

        let mut window_renderer = SkiaWindowRenderer::new();
        let handle: Arc<dyn anyrender::WindowHandle> = Arc::new(AnyrenderWindow(window));
        window_renderer.resume(handle, width, height);

        let mut capture_renderer = SkiaImageRenderer::new(width, height);
        capture_renderer.reset();

        Ok(Self {
            window_renderer,
            capture_renderer,
            commands: Vec::new(),
            svg_cache: HashMap::new(),
            size: Size::new(width as f64, height as f64),
            transform: Affine::scale(scale),
            capture: false,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32, _scale: f64) {
        let width = width.max(1);
        let height = height.max(1);
        self.size = Size::new(width as f64, height as f64);
        self.window_renderer.set_size(width, height);
        self.capture_renderer.resize(width, height);
    }

    pub const fn set_scale(&mut self, _scale: f64) {}

    pub const fn size(&self) -> Size {
        self.size
    }

    fn current_scale(&self) -> (f64, f64) {
        let coeffs = self.transform.as_coeffs();
        let scale_x = coeffs[0].hypot(coeffs[1]);
        let scale_y = coeffs[2].hypot(coeffs[3]);
        (scale_x, scale_y)
    }

    fn replay<S: anyrender::PaintScene>(commands: &[Command], scene: &mut S) {
        for command in commands {
            match command {
                Command::Stroke {
                    shape,
                    brush,
                    stroke,
                    transform,
                } => {
                    scene.stroke(
                        stroke,
                        *transform,
                        anyrender::PaintRef::from(BrushRef::from(brush)),
                        None,
                        shape,
                    );
                }
                Command::Fill {
                    shape,
                    brush,
                    transform,
                } => {
                    scene.fill(
                        Fill::NonZero,
                        *transform,
                        anyrender::PaintRef::from(BrushRef::from(brush)),
                        None,
                        shape,
                    );
                }
                Command::BoxShadow {
                    rect,
                    brush,
                    radius,
                    std_dev,
                    transform,
                } => {
                    scene.draw_box_shadow(*transform, *rect, *brush, *radius, *std_dev);
                }
                Command::PushLayer {
                    blend,
                    alpha,
                    transform,
                    clip,
                } => {
                    scene.push_layer(*blend, *alpha, *transform, clip);
                }
                Command::PushClip { transform, clip } => {
                    scene.push_clip_layer(*transform, clip);
                }
                Command::PopLayer => {
                    scene.pop_layer();
                }
                Command::Glyphs {
                    font,
                    font_size,
                    hint,
                    normalized_coords,
                    style,
                    brush,
                    brush_alpha,
                    transform,
                    glyph_transform,
                    glyphs,
                } => {
                    scene.draw_glyphs(
                        font,
                        *font_size,
                        *hint,
                        normalized_coords,
                        style,
                        anyrender::PaintRef::from(BrushRef::from(brush)),
                        *brush_alpha,
                        *transform,
                        *glyph_transform,
                        glyphs.iter().copied(),
                    );
                }
                Command::Image {
                    image,
                    rect,
                    transform,
                } => {
                    let image_transform = image_transform(*transform, *rect, image);
                    scene.fill(
                        Fill::NonZero,
                        image_transform,
                        image.as_ref(),
                        None,
                        &Rect::new(
                            0.0,
                            0.0,
                            image.image.width as f64,
                            image.image.height as f64,
                        ),
                    );
                }
                Command::Svg {
                    image,
                    rect,
                    transform,
                    brush,
                } => {
                    let image_transform = image_transform(*transform, *rect, image);
                    let src_rect = Rect::new(
                        0.0,
                        0.0,
                        image.image.width as f64,
                        image.image.height as f64,
                    );

                    if let Some(brush) = brush {
                        scene.push_layer(
                            peniko::BlendMode::default(),
                            1.0,
                            image_transform,
                            &src_rect,
                        );
                        scene.fill(
                            Fill::NonZero,
                            image_transform,
                            image.as_ref(),
                            None,
                            &src_rect,
                        );
                        scene.push_layer(
                            peniko::BlendMode {
                                mix: Mix::Normal,
                                compose: Compose::SrcIn,
                            },
                            1.0,
                            image_transform,
                            &src_rect,
                        );
                        scene.fill(
                            Fill::NonZero,
                            image_transform,
                            anyrender::PaintRef::from(BrushRef::from(brush)),
                            None,
                            &src_rect,
                        );
                        scene.pop_layer();
                        scene.pop_layer();
                    } else {
                        scene.fill(
                            Fill::NonZero,
                            image_transform,
                            image.as_ref(),
                            None,
                            &src_rect,
                        );
                    }
                }
            }
        }
    }

    fn cached_svg_image(
        &mut self,
        svg: Svg<'_>,
        width: u32,
        height: u32,
    ) -> Result<peniko::ImageBrush> {
        let key = (svg.hash.to_vec(), width, height);
        if let Some(image) = self.svg_cache.get(&key) {
            return Ok(image.clone());
        }

        let mut pixmap =
            Pixmap::new(width, height).ok_or_else(|| anyhow!("failed to allocate svg pixmap"))?;
        let transform = Transform::from_scale(
            width as f32 / svg.tree.size().width(),
            height as f32 / svg.tree.size().height(),
        );
        resvg::render(svg.tree, transform, &mut pixmap.as_mut());
        let image = image_brush_from_rgba(width, height, pixmap.take());
        self.svg_cache.insert(key, image.clone());
        Ok(image)
    }
}

impl Renderer for SkiaRenderer {
    fn begin(&mut self, capture: bool) {
        self.commands.clear();
        self.transform = Affine::IDENTITY;
        self.capture = capture;
    }

    fn set_transform(&mut self, transform: Affine) {
        self.transform = transform;
    }

    fn set_z_index(&mut self, _z_index: i32) {}

    fn clip(&mut self, shape: &impl Shape) {
        self.commands.push(Command::PushClip {
            transform: self.transform,
            clip: shape.to_path(PATH_TOLERANCE),
        });
    }

    fn clear_clip(&mut self) {
        self.commands.push(Command::PopLayer);
    }

    fn stroke<'b, 's>(
        &mut self,
        shape: &impl Shape,
        brush: impl Into<BrushRef<'b>>,
        stroke: &'s Stroke,
    ) {
        self.commands.push(Command::Stroke {
            shape: shape.to_path(PATH_TOLERANCE),
            brush: brush.into().to_owned(),
            stroke: stroke.clone(),
            transform: self.transform,
        });
    }

    fn fill<'b>(&mut self, path: &impl Shape, brush: impl Into<BrushRef<'b>>, blur_radius: f64) {
        let brush = brush.into();

        if blur_radius > 0.0
            && let BrushRef::Solid(color) = brush
        {
            if let Some(rounded) = path.as_rounded_rect() {
                let radii = rounded.radii();
                if radii.top_left == radii.top_right
                    && radii.top_left == radii.bottom_left
                    && radii.top_left == radii.bottom_right
                {
                    self.commands.push(Command::BoxShadow {
                        rect: rounded.rect(),
                        brush: color,
                        radius: radii.top_left,
                        std_dev: blur_radius,
                        transform: self.transform,
                    });
                    return;
                }
            } else if let Some(rect) = path.as_rect() {
                self.commands.push(Command::BoxShadow {
                    rect,
                    brush: color,
                    radius: 0.0,
                    std_dev: blur_radius,
                    transform: self.transform,
                });
                return;
            }
        }

        self.commands.push(Command::Fill {
            shape: path.to_path(PATH_TOLERANCE),
            brush: brush.to_owned(),
            transform: self.transform,
        });
    }

    fn push_layer(
        &mut self,
        blend: impl Into<peniko::BlendMode>,
        alpha: f32,
        transform: Affine,
        clip: &impl Shape,
    ) {
        self.commands.push(Command::PushLayer {
            blend: blend.into(),
            alpha,
            transform: self.transform * transform,
            clip: clip.to_path(PATH_TOLERANCE),
        });
    }

    fn pop_layer(&mut self) {
        self.commands.push(Command::PopLayer);
    }

    fn draw_glyphs<'a>(
        &mut self,
        origin: Point,
        props: &GlyphRunProps<'a>,
        glyphs: impl Iterator<Item = Glyph> + 'a,
    ) {
        let transform = self.transform * Affine::translate((origin.x, origin.y)) * props.transform;
        self.commands.push(Command::Glyphs {
            font: props.font.clone(),
            font_size: props.font_size,
            hint: props.hint,
            normalized_coords: props.normalized_coords.to_vec(),
            style: props.style.to_owned(),
            brush: props.brush.to_owned(),
            brush_alpha: props.brush_alpha,
            transform,
            glyph_transform: props.glyph_transform,
            glyphs: glyphs
                .map(|glyph| anyrender::Glyph {
                    id: glyph.id,
                    x: glyph.x,
                    y: glyph.y,
                })
                .collect(),
        });
    }

    fn draw_svg<'b>(&mut self, svg: Svg<'b>, rect: Rect, brush: Option<impl Into<BrushRef<'b>>>) {
        let (scale_x, scale_y) = self.current_scale();
        let width = (rect.width() * scale_x.abs()).round().max(1.0) as u32;
        let height = (rect.height() * scale_y.abs()).round().max(1.0) as u32;
        let image = match self.cached_svg_image(svg, width, height) {
            Ok(image) => image,
            Err(_) => return,
        };

        self.commands.push(Command::Svg {
            image,
            rect,
            transform: self.transform,
            brush: brush.map(|brush| brush.into().to_owned()),
        });
    }

    fn draw_img(&mut self, img: Img<'_>, rect: Rect) {
        self.commands.push(Command::Image {
            image: img.img,
            rect,
            transform: self.transform,
        });
    }

    fn finish(&mut self) -> Option<peniko::ImageBrush> {
        if self.capture {
            let commands = std::mem::take(&mut self.commands);
            let mut buffer = vec![0; self.size.width as usize * self.size.height as usize * 4];
            self.capture_renderer.reset();
            self.capture_renderer
                .render(|scene| Self::replay(&commands, scene), &mut buffer);
            return Some(image_brush_from_rgba(
                self.size.width as u32,
                self.size.height as u32,
                buffer,
            ));
        }

        let commands = std::mem::take(&mut self.commands);
        self.window_renderer
            .render(|scene| Self::replay(&commands, scene));
        None
    }

    fn debug_info(&self) -> String {
        "name: Skia\ninfo: AnyRender Skia".to_string()
    }
}

fn image_transform(transform: Affine, rect: Rect, image: &peniko::ImageBrush) -> Affine {
    transform
        .pre_scale_non_uniform(
            rect.width().max(1.0) / image.image.width as f64,
            rect.height().max(1.0) / image.image.height as f64,
        )
        .pre_translate((rect.min_x(), rect.min_y()).into())
}

fn image_brush_from_rgba(width: u32, height: u32, data: Vec<u8>) -> peniko::ImageBrush {
    peniko::ImageBrush::new(ImageData {
        data: Blob::new(Arc::new(data)),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::AlphaPremultiplied,
        width,
        height,
    })
}
