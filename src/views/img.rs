//! Module defining image view and its properties: style, position and fit.
#![deny(missing_docs)]
use std::{cell::RefCell, path::PathBuf, rc::Rc, sync::Arc};

use floem_reactive::UpdaterEffect;
use peniko::{Blob, ImageAlphaType, ImageData, kurbo::Rect};
use sha2::{Digest, Sha256};

use crate::{
    Renderer, prop_extractor,
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
    }
}

/// Holds the data needed for [img] view fn to display images.
pub struct Img {
    id: ViewId,
    img: peniko::ImageBrush,
    img_hash: Vec<u8>,
    layout_data: Rc<RefCell<ImageLayoutData>>,
    style: Extractor,
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
pub fn img(image: impl Fn() -> Vec<u8> + 'static) -> Img {
    let image = image::load_from_memory(&image()).ok();
    let width = image.as_ref().map_or(0, |img| img.width());
    let height = image.as_ref().map_or(0, |img| img.height());
    let data = Arc::new(image.map_or(Default::default(), |img| img.into_rgba8().into_vec()));
    let blob = Blob::new(data);
    let image = peniko::ImageBrush::new(ImageData {
        data: blob,
        format: peniko::ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,
        width,
        height,
    })
    .with_quality(peniko::ImageQuality::High);
    img_dynamic(move || image.clone())
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
pub fn img_from_path(image: impl Fn() -> PathBuf + 'static) -> Img {
    let image = image::open(image()).ok();
    let width = image.as_ref().map_or(0, |img| img.width());
    let height = image.as_ref().map_or(0, |img| img.height());
    let data = Arc::new(image.map_or(Default::default(), |img| img.into_rgba8().into_vec()));
    let blob = Blob::new(data);
    let image = peniko::ImageBrush::new(ImageData {
        data: blob,
        format: peniko::ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,
        width,
        height,
    });
    img_dynamic(move || image.clone())
}

pub(crate) fn img_dynamic(image: impl Fn() -> peniko::ImageBrush + 'static) -> Img {
    let id = ViewId::new();

    let img = UpdaterEffect::new(image, move |image| {
        id.update_state(image);
    });

    let layout_data = Rc::new(RefCell::new(ImageLayoutData::new(
        img.image.width,
        img.image.height,
    )));

    let mut hasher = Sha256::new();
    hasher.update(img.image.data.data());
    let img_hash = hasher.finalize().to_vec();

    let mut img = Img {
        id,
        img,
        img_hash,
        layout_data,
        style: Extractor::default(),
    };

    img.set_taffy_layout();
    img
}

impl Img {
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
        let natural_w = self.img.image.width as f64;
        let natural_h = self.img.image.height as f64;
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
        if let Ok(img) = state.downcast::<peniko::ImageBrush>() {
            let mut hasher = Sha256::new();
            hasher.update(img.image.data.data());
            self.img_hash = hasher.finalize().to_vec();

            // Update layout data with new image dimensions
            let width = img.image.width;
            let height = img.image.height;
            self.layout_data.borrow_mut().natural_width = width;
            self.layout_data.borrow_mut().natural_height = height;

            self.img = *img;
            self.id.request_mark_view_layout_dirty();
            self.id.request_layout();
        }
    }

    fn style_pass(&mut self, cx: &mut crate::context::StyleCx<'_>) {
        let mut transitioning = false;
        if self.style.read(cx, &mut transitioning) {
            // object_fit changed, update taffy layout
            self.set_taffy_layout();
            self.id.request_layout();
        }
        if transitioning {
            cx.request_transition();
        }
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        let content_rect = self.id.get_content_rect_local();
        let dest_rect = self.object_fit_dest_rect(content_rect);

        if self.needs_clip() {
            cx.clip(&content_rect);
        }

        cx.draw_img(
            floem_renderer::Img {
                img: self.img.clone(),
                hash: &self.img_hash,
            },
            dest_rect,
        );
        if self.needs_clip() {
            cx.clear_clip();
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
