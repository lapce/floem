//! Module defining image view and its properties: style, position and fit.
#![deny(missing_docs)]
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
};

use floem_reactive::{ReadSignal, RwSignal, SignalWith, UpdaterEffect};
use imaging::{ExternalImage, Image as ImagingImage};
use peniko::{Blob, ImageAlphaType, ImageData, kurbo::Rect};

use crate::{
    effects::{Brush, Image as FloemImage, ImageBrush},
    prop_extractor,
    style::{FontSizeCx, ObjectFit, ObjectPosition},
    view::{LayoutNodeCx, MeasureFn, View, ViewId},
};

/// Holds information about image dimensions for layout calculations.
#[derive(Clone)]
pub struct ImageLayoutData {
    /// Natural width of the image in pixels
    natural_width: u32,
    /// Natural height of the image in pixels
    natural_height: u32,
}

impl ImageLayoutData {
    /// Create new image layout data
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            natural_width: width,
            natural_height: height,
        }
    }

    /// Get the natural aspect ratio of the image
    pub fn aspect_ratio(&self) -> f32 {
        if self.natural_height == 0 {
            1.0
        } else {
            self.natural_width as f32 / self.natural_height as f32
        }
    }

    /// Create a taffy layout function for image sizing following CSS rules
    pub fn create_taffy_layout_fn(
        layout_data: Rc<RefCell<Self>>,
        _object_fit: ObjectFit,
    ) -> Box<MeasureFn> {
        Box::new(
            move |known_dimensions, _available_space, _node_id, _style, _measure_ctx| {
                use taffy::*;

                let data = layout_data.borrow();
                let natural_width = data.natural_width as f32;
                let natural_height = data.natural_height as f32;
                let natural_aspect = data.aspect_ratio();
                if natural_width == 0.0 || natural_height == 0.0 {
                    return Size {
                        width: 0.0,
                        height: 0.0,
                    };
                }

                // If only width is set, compute height from aspect ratio
                if let Some(w) = known_dimensions.width {
                    let h = known_dimensions.height.unwrap_or_else(|| {
                        // Use aspect ratio to compute height
                        w / natural_aspect
                    });
                    return Size {
                        width: w,
                        height: h,
                    };
                }

                // If only height is set, compute width from aspect ratio
                if let Some(h) = known_dimensions.height {
                    let w = h * natural_aspect;
                    return Size {
                        width: w,
                        height: h,
                    };
                }

                // No explicit dimensions: use intrinsic size to match CSS img default
                // behavior when width/height are `auto`.
                Size {
                    // Both dimensions provided by layout context still do not force a resized
                    // intrinsic image; object-fit is a paint-time behavior for sized boxes.
                    width: natural_width,
                    height: natural_height,
                }
            },
        )
    }
}

prop_extractor! {
    Extractor {
        object_fit: crate::style::ObjectFitProp,
        object_position: crate::style::ObjectPositionProp,
        image_sampler: crate::style::ImageSamplerProp,
    }
}

/// Holds the data needed for [img] view fn to display images.
pub struct Img {
    id: ViewId,
    img: ImagingImage,
    layout_data: Rc<RefCell<ImageLayoutData>>,
    style: Extractor,
}

#[doc(hidden)]
pub enum ImgReader {
    Static(ImagingImage),
    Reactive(Rc<dyn Fn() -> ImagingImage>),
}

/// A static input that can be converted into image content for [`Img`].
pub trait ImgDataSource {
    /// Convert this value into owned image data.
    fn into_image_data(self) -> ImagingImage;
}

impl ImgDataSource for ImageData {
    fn into_image_data(self) -> ImagingImage {
        ImagingImage::Raster(self)
    }
}

impl ImgDataSource for ExternalImage {
    fn into_image_data(self) -> ImagingImage {
        ImagingImage::External(self)
    }
}

impl ImgDataSource for ImagingImage {
    fn into_image_data(self) -> ImagingImage {
        self
    }
}

impl ImgDataSource for Vec<u8> {
    fn into_image_data(self) -> ImagingImage {
        ImagingImage::Raster(Img::image_data_from_bytes(&self))
    }
}

impl ImgDataSource for &'static [u8] {
    fn into_image_data(self) -> ImagingImage {
        ImagingImage::Raster(Img::image_data_from_bytes(self))
    }
}

impl<const N: usize> ImgDataSource for &'static [u8; N] {
    fn into_image_data(self) -> ImagingImage {
        ImagingImage::Raster(Img::image_data_from_bytes(self.as_slice()))
    }
}

impl ImgDataSource for PathBuf {
    fn into_image_data(self) -> ImagingImage {
        ImagingImage::Raster(Img::image_data_from_path(&self))
    }
}

/// A source that can produce image content for [`Img`].
///
/// This supports:
/// - direct image bytes (`Vec<u8>`)
/// - direct file paths (`PathBuf`)
/// - direct [`ImageData`]
/// - direct [`ExternalImage`]
/// - closures returning any of the above
/// - [`ReadSignal`] and [`RwSignal`] values containing any of the above
pub trait ImgSource: 'static {
    /// Convert this source into either a static image or a reactive reader.
    fn into_img_reader(self) -> ImgReader;
}

impl ImgSource for Vec<u8> {
    fn into_img_reader(self) -> ImgReader {
        ImgReader::Static(self.into_image_data())
    }
}

impl ImgSource for &'static [u8] {
    fn into_img_reader(self) -> ImgReader {
        ImgReader::Static(self.into_image_data())
    }
}

impl ImgSource for PathBuf {
    fn into_img_reader(self) -> ImgReader {
        ImgReader::Static(self.into_image_data())
    }
}

impl ImgSource for ImageData {
    fn into_img_reader(self) -> ImgReader {
        ImgReader::Static(self.into_image_data())
    }
}

impl ImgSource for ExternalImage {
    fn into_img_reader(self) -> ImgReader {
        ImgReader::Static(self.into_image_data())
    }
}

impl ImgSource for ImagingImage {
    fn into_img_reader(self) -> ImgReader {
        ImgReader::Static(self)
    }
}

impl<T, F> ImgSource for F
where
    F: Fn() -> T + 'static,
    T: ImgDataSource,
{
    fn into_img_reader(self) -> ImgReader {
        ImgReader::Reactive(Rc::new(move || self().into_image_data()))
    }
}

impl<P, S> ImgSource for ReadSignal<P, S>
where
    ReadSignal<P, S>: SignalWith<P>,
    P: Clone + ImgDataSource + 'static,
    S: 'static,
{
    fn into_img_reader(self) -> ImgReader {
        ImgReader::Reactive(Rc::new(move || {
            self.with(|value| value.clone().into_image_data())
        }))
    }
}

impl<P, S> ImgSource for RwSignal<P, S>
where
    RwSignal<P, S>: SignalWith<P>,
    P: Clone + ImgDataSource + 'static,
    S: 'static,
{
    fn into_img_reader(self) -> ImgReader {
        ImgReader::Reactive(Rc::new(move || {
            self.with(|value| value.clone().into_image_data())
        }))
    }
}

/// A view that can display an image and controls its position.
///
/// It takes function that produce `Vec<u8>` and will convert it into [Image](peniko::Image).
///
/// ### Example:
/// ```rust
/// # use crate::floem::views::Decorators;
/// # use floem::views::img;
/// let ferris_png = include_bytes!("../../examples/widget-gallery/assets/ferris.png");
/// // Create an image from the function returning Vec<u8>:
/// img(move || ferris_png.to_vec())
///     .style(|s| s.size(50.,50.));
/// ```
/// # Reactivity
/// The `img` function is not reactive, so to make it change on event, wrap it
/// with [`dyn_view`](crate::views::dyn_view::dyn_view).
///
/// ### Example with reactive updates:
/// ```rust
/// # use floem::prelude::*;
/// # use crate::floem::views::Decorators;
/// # use floem::views::img;
/// # use floem::views::dyn_view;
/// # use floem::reactive::RwSignal;
///
/// #[derive(Clone)]
/// enum Image {
///     ImageA,
///     ImageB
/// }
///
/// let ferris = include_bytes!("../../examples/widget-gallery/assets/ferris.png");
/// let sunflower = include_bytes!("../../examples/widget-gallery/assets/sunflower.jpg");
/// let switch_image = RwSignal::new(Image::ImageA);
/// // Create an image from the function returning Vec<u8>:
/// dyn_view(move || {
///     let image = switch_image.get();
///     img(move || {
///         match image {
///             Image::ImageA => ferris.to_vec(),
///             Image::ImageB => sunflower.to_vec()
///        }
///     }).style(|s| s.size(50.,50.))
/// });
/// ```
#[deprecated(note = "Use Img::new(...) instead")]
pub fn img(image: impl Fn() -> Vec<u8> + 'static) -> Img {
    Img::new(image)
}

/// A view that can display an image and controls its position.
///
/// It takes function that returns [`PathBuf`] and will convert it into [`Image`](peniko::Image).
///
/// ### Example:
/// ```rust
/// # use std::path::PathBuf;
/// # use floem::views::Decorators;
/// # use floem::views::img_from_path;
///
/// let path_to_ferris = PathBuf::from(r"../../examples/widget-gallery/assets/ferrig.png");
/// // Create an image from the function returning PathBuf:
/// img_from_path(move || path_to_ferris.clone())
///     .style(|s| s.size(50.,50.));
/// ```
/// # Reactivity
/// The `img` function is not reactive, so to make it change on event, wrap it
/// with [`dyn_view`](crate::views::dyn_view::dyn_view).
#[deprecated(note = "Use Img::new(...) instead")]
pub fn img_from_path(image: impl Fn() -> PathBuf + 'static) -> Img {
    Img::new(image)
}

impl Img {
    /// Decode static image bytes, a static image path, or reuse existing image
    /// content.
    pub fn image_data(source: impl ImgDataSource) -> ImagingImage {
        source.into_image_data()
    }

    fn image_data_from_dynamic(image: Option<image::DynamicImage>) -> ImageData {
        let width = image.as_ref().map_or(0, |img| img.width());
        let height = image.as_ref().map_or(0, |img| img.height());
        let data = Arc::new(image.map_or_else(Vec::new, |img| img.into_rgba8().into_vec()));
        let blob = Blob::new(data);
        ImageData {
            data: blob,
            format: peniko::ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::Alpha,
            width,
            height,
        }
    }

    fn image_data_from_bytes(bytes: &[u8]) -> ImageData {
        Self::image_data_from_dynamic(image::load_from_memory(bytes).ok())
    }

    fn image_data_from_path(path: &Path) -> ImageData {
        Self::image_data_from_dynamic(image::open(path).ok())
    }

    /// Create an image view from bytes, a path, image data,
    /// or a reactive closure/signal producing any of those.
    pub fn new(source: impl ImgSource) -> Self {
        let id = ViewId::new();
        let img = match source.into_img_reader() {
            ImgReader::Static(image) => image,
            ImgReader::Reactive(reader) => {
                let initial = reader();
                UpdaterEffect::new(
                    move || reader(),
                    move |image| {
                        id.update_state(image);
                    },
                );
                initial
            }
        };

        let layout_data = Rc::new(RefCell::new(ImageLayoutData::new(
            img.width(),
            img.height(),
        )));

        let mut img = Self {
            id,
            img,
            layout_data,
            style: Extractor::default(),
        };

        img.set_taffy_layout();
        img
    }

    fn set_image(&mut self, image: ImagingImage) {
        self.layout_data.borrow_mut().natural_width = image.width();
        self.layout_data.borrow_mut().natural_height = image.height();
        self.img = image;
        self.id.request_mark_view_layout_dirty();
        self.id.request_layout();
    }

    fn set_taffy_layout(&mut self) {
        let taffy = self.id.taffy();
        let taffy_node = self.id.taffy_node();
        let mut taffy = taffy.borrow_mut();

        let object_fit = self.style.object_fit();
        let layout_fn =
            ImageLayoutData::create_taffy_layout_fn(self.layout_data.clone(), object_fit);

        let _ = taffy.set_node_context(
            taffy_node,
            Some(LayoutNodeCx::Custom {
                measure: layout_fn,
                finalize: None,
            }),
        );
    }

    /// Compute the destination rect for the image within the content box,
    /// following CSS object-fit rules. The returned rect is in local coords
    /// and may be larger than `content_rect` (for Cover) or smaller (for Contain).
    pub fn object_fit_dest_rect(&self, content_rect: Rect) -> Rect {
        self.object_fit_dest_rect_with(
            content_rect,
            self.style.object_fit(),
            self.style.object_position(),
        )
    }

    /// This method is mostly here for tests
    pub fn object_fit_dest_rect_with(
        &self,
        content_rect: Rect,
        object_fit: ObjectFit,
        object_position: ObjectPosition,
    ) -> Rect {
        let natural_w = self.img.width() as f64;
        let natural_h = self.img.height() as f64;
        if natural_w == 0.0 || natural_h == 0.0 {
            return content_rect;
        }
        let box_w = content_rect.width();
        let box_h = content_rect.height();
        if box_w == 0.0 || box_h == 0.0 {
            return content_rect;
        }
        let (rendered_w, rendered_h) = match object_fit {
            ObjectFit::Fill => (box_w, box_h),
            ObjectFit::None => (natural_w, natural_h),
            ObjectFit::Contain => {
                let scale = (box_w / natural_w).min(box_h / natural_h);
                (natural_w * scale, natural_h * scale)
            }
            ObjectFit::Cover => {
                let scale = (box_w / natural_w).max(box_h / natural_h);
                (natural_w * scale, natural_h * scale)
            }
            ObjectFit::ScaleDown => {
                let contain_scale = (box_w / natural_w).min(box_h / natural_h);
                let scale = contain_scale.min(1.0);
                (natural_w * scale, natural_h * scale)
            }
        };
        let (x_align, y_align) = match object_position {
            ObjectPosition::TopLeft => (0.0, 0.0),
            ObjectPosition::Top => (0.5, 0.0),
            ObjectPosition::TopRight => (1.0, 0.0),
            ObjectPosition::Left => (0.0, 0.5),
            ObjectPosition::Center => (0.5, 0.5),
            ObjectPosition::Right => (1.0, 0.5),
            ObjectPosition::BottomLeft => (0.0, 1.0),
            ObjectPosition::Bottom => (0.5, 1.0),
            ObjectPosition::BottomRight => (1.0, 1.0),
            ObjectPosition::Custom(x, y) => {
                let font_size_cx = FontSizeCx::new(16.0, 16.0);
                let free_x = box_w - rendered_w;
                let free_y = box_h - rendered_h;
                let x0 = content_rect.x0 + x.resolve(free_x, &font_size_cx);
                let y0 = content_rect.y0 + y.resolve(free_y, &font_size_cx);
                return Rect::new(x0, y0, x0 + rendered_w, y0 + rendered_h);
            }
        };
        let x0 = content_rect.x0 + (box_w - rendered_w) * x_align;
        let y0 = content_rect.y0 + (box_h - rendered_h) * y_align;
        Rect::new(x0, y0, x0 + rendered_w, y0 + rendered_h)
    }

    fn needs_clip(&self) -> bool {
        matches!(self.style.object_fit(), ObjectFit::Cover | ObjectFit::None)
    }
}

impl View for Img {
    fn id(&self) -> ViewId {
        self.id
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "Img".into()
    }

    fn update(&mut self, _cx: &mut crate::context::UpdateCx, state: Box<dyn std::any::Any>) {
        match state.downcast::<ImagingImage>() {
            Ok(img) => {
                self.set_image(*img);
            }
            Err(state) => {
                if let Ok(img) = state.downcast::<ImageData>() {
                    self.set_image(ImagingImage::Raster(*img));
                }
            }
        }
    }

    fn style_pass(&mut self, cx: &mut crate::context::StyleCx<'_>) {
        if self.style.read(cx) {
            // object_fit or image quality changed
            self.set_taffy_layout();
            self.id.request_layout();
            cx.window_state.request_paint(self.id);
        }
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        let content_rect = self.id.get_content_rect_local();
        let dest_rect = self.object_fit_dest_rect(content_rect);
        let image_brush = Brush::Image(ImageBrush(peniko::ImageBrush {
            image: FloemImage::Imaging(self.img.clone()),
            sampler: self.style.image_sampler(),
        }));
        let source_width = self.img.width() as f64;
        let source_height = self.img.height() as f64;

        if source_width <= 0.0 || source_height <= 0.0 {
            return;
        }

        let source_rect = Rect::new(0.0, 0.0, source_width, source_height);
        let image_transform = peniko::kurbo::Affine::translate((dest_rect.x0, dest_rect.y0))
            .then_scale_non_uniform(
                dest_rect.width() / source_width,
                dest_rect.height() / source_height,
            );

        if self.needs_clip() {
            cx.painter.with_fill_clip(content_rect, |p| {
                p.fill(source_rect, &image_brush)
                    .transform(image_transform)
                    .draw();
            });
        } else {
            cx.painter
                .fill(source_rect, &image_brush)
                .transform(image_transform)
                .draw();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::MeasureCx;

    fn run_measure(
        width: u32,
        height: u32,
        object_fit: ObjectFit,
        known: taffy::Size<Option<f32>>,
        available: taffy::Size<taffy::AvailableSpace>,
    ) -> taffy::Size<f32> {
        let layout_data = Rc::new(RefCell::new(ImageLayoutData::new(width, height)));
        let mut measure = ImageLayoutData::create_taffy_layout_fn(layout_data, object_fit);
        let mut tree = taffy::TaffyTree::<()>::new();
        let node_id = tree.new_leaf(taffy::Style::default()).unwrap();
        let mut measure_ctx = MeasureCx::default();

        measure(
            known,
            available,
            node_id,
            &taffy::Style::default(),
            &mut measure_ctx,
        )
    }

    fn assert_close(actual: f32, expected: f32) {
        let epsilon = 0.01f64;
        assert!((actual as f64 - expected as f64).abs() < epsilon);
    }

    #[test]
    fn img_object_fit_fill_uses_natural_size_without_explicit_dimensions() {
        let result = run_measure(
            4,
            3,
            ObjectFit::Fill,
            taffy::Size {
                width: None,
                height: None,
            },
            taffy::Size {
                width: taffy::AvailableSpace::Definite(300.0),
                height: taffy::AvailableSpace::Definite(200.0),
            },
        );
        assert_close(result.width, 4.0);
        assert_close(result.height, 3.0);
    }

    #[test]
    fn img_object_fit_contain_uses_natural_size_without_explicit_dimensions() {
        let result = run_measure(
            4,
            3,
            ObjectFit::Contain,
            taffy::Size {
                width: None,
                height: None,
            },
            taffy::Size {
                width: taffy::AvailableSpace::Definite(100.0),
                height: taffy::AvailableSpace::Definite(300.0),
            },
        );
        assert_close(result.width, 4.0);
        assert_close(result.height, 3.0);
    }

    #[test]
    fn img_object_fit_cover_uses_natural_size_without_explicit_dimensions() {
        let result = run_measure(
            4,
            3,
            ObjectFit::Cover,
            taffy::Size {
                width: None,
                height: None,
            },
            taffy::Size {
                width: taffy::AvailableSpace::Definite(300.0),
                height: taffy::AvailableSpace::Definite(300.0),
            },
        );
        assert_close(result.width, 4.0);
        assert_close(result.height, 3.0);
    }

    #[test]
    fn img_object_fit_scale_down_never_scales_above_natural() {
        let result = run_measure(
            120,
            80,
            ObjectFit::ScaleDown,
            taffy::Size {
                width: None,
                height: None,
            },
            taffy::Size {
                width: taffy::AvailableSpace::Definite(2.0),
                height: taffy::AvailableSpace::Definite(2.0),
            },
        );
        assert_close(result.width, 120.0);
        assert_close(result.height, 80.0);
    }

    #[test]
    fn img_explicit_width_keeps_aspect_ratio_without_height() {
        let result = run_measure(
            4,
            3,
            ObjectFit::Contain,
            taffy::Size {
                width: Some(100.0),
                height: None,
            },
            taffy::Size {
                width: taffy::AvailableSpace::MaxContent,
                height: taffy::AvailableSpace::MaxContent,
            },
        );
        assert_close(result.width, 100.0);
        assert_close(result.height, 75.0);
    }

    #[test]
    fn img_explicit_dimensions_override_object_fit() {
        let result = run_measure(
            4,
            300,
            ObjectFit::Fill,
            taffy::Size {
                width: Some(200.0),
                height: Some(60.0),
            },
            taffy::Size {
                width: taffy::AvailableSpace::Definite(1000.0),
                height: taffy::AvailableSpace::Definite(1000.0),
            },
        );
        assert_close(result.width, 200.0);
        assert_close(result.height, 60.0);
    }
}
