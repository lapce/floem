//! Module defining image view and its properties: style, position and fit.
#![deny(missing_docs)]
use std::{cell::RefCell, path::PathBuf, rc::Rc, sync::Arc};

use floem_reactive::Effect;
use peniko::{Blob, ImageAlphaType, ImageData};
use sha2::{Digest, Sha256};

use crate::{
    Renderer, prop_extractor,
    style::ObjectFit,
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
        object_fit: ObjectFit,
    ) -> Box<MeasureFn> {
        Box::new(
            move |known_dimensions, available_space, _node_id, _style, _measure_ctx| {
                use taffy::*;

                let data = layout_data.borrow();
                let natural_width = data.natural_width as f32;
                let natural_height = data.natural_height as f32;
                let natural_aspect = data.aspect_ratio();

                // If both dimensions are explicitly set, use them
                if let (Some(w), Some(h)) = (known_dimensions.width, known_dimensions.height) {
                    return Size {
                        width: w,
                        height: h,
                    };
                }

                // Helper to compute size based on object-fit behavior
                let compute_size = |container_width: f32, container_height: f32| -> Size<f32> {
                    match object_fit {
                        ObjectFit::Fill => {
                            // Stretch to fill - use container size
                            Size {
                                width: container_width,
                                height: container_height,
                            }
                        }
                        ObjectFit::Contain => {
                            // Fit inside maintaining aspect ratio
                            let container_aspect = container_width / container_height;
                            if natural_aspect > container_aspect {
                                // Image is wider - constrain by width
                                Size {
                                    width: container_width,
                                    height: container_width / natural_aspect,
                                }
                            } else {
                                // Image is taller - constrain by height
                                Size {
                                    width: container_height * natural_aspect,
                                    height: container_height,
                                }
                            }
                        }
                        ObjectFit::Cover => {
                            // Cover entire container maintaining aspect ratio
                            let container_aspect = container_width / container_height;
                            if natural_aspect > container_aspect {
                                // Image is wider - constrain by height
                                Size {
                                    width: container_height * natural_aspect,
                                    height: container_height,
                                }
                            } else {
                                // Image is taller - constrain by width
                                Size {
                                    width: container_width,
                                    height: container_width / natural_aspect,
                                }
                            }
                        }
                        ObjectFit::None => {
                            // Use natural size
                            Size {
                                width: natural_width,
                                height: natural_height,
                            }
                        }
                        ObjectFit::ScaleDown => {
                            // Like contain but don't scale up
                            let container_aspect = container_width / container_height;
                            let (scaled_width, scaled_height) = if natural_aspect > container_aspect
                            {
                                (container_width, container_width / natural_aspect)
                            } else {
                                (container_height * natural_aspect, container_height)
                            };

                            // Don't scale up beyond natural size
                            Size {
                                width: scaled_width.min(natural_width),
                                height: scaled_height.min(natural_height),
                            }
                        }
                    }
                };

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

                // No explicit dimensions - use available space and object-fit
                match (available_space.width, available_space.height) {
                    (AvailableSpace::Definite(w), AvailableSpace::Definite(h)) => {
                        compute_size(w, h)
                    }
                    (AvailableSpace::Definite(w), _) => {
                        // Only width available
                        Size {
                            width: w,
                            height: w / natural_aspect,
                        }
                    }
                    (_, AvailableSpace::Definite(h)) => {
                        // Only height available
                        Size {
                            width: h * natural_aspect,
                            height: h,
                        }
                    }
                    _ => {
                        // No constraints - use natural size
                        Size {
                            width: natural_width,
                            height: natural_height,
                        }
                    }
                }
            },
        )
    }
}

prop_extractor! {
    Extractor {
        object_fit: crate::style::ObjectFitProp,
    }
}

/// Holds the data needed for [img] view fn to display images.
pub struct Img {
    id: ViewId,
    img: Option<peniko::ImageBrush>,
    img_hash: Option<Vec<u8>>,
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
        alpha_type: ImageAlphaType::AlphaPremultiplied,
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
        alpha_type: ImageAlphaType::AlphaPremultiplied,
        width,
        height,
    });
    img_dynamic(move || image.clone())
}

pub(crate) fn img_dynamic(image: impl Fn() -> peniko::ImageBrush + 'static) -> Img {
    let id = ViewId::new();
    let layout_data = Rc::new(RefCell::new(ImageLayoutData::new(0, 0)));

    Effect::new(move |_| {
        id.update_state(image());
    });

    let mut img = Img {
        id,
        img: None,
        img_hash: None,
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
            self.img_hash = Some(hasher.finalize().to_vec());

            // Update layout data with new image dimensions
            let width = img.image.width;
            let height = img.image.height;
            self.layout_data.borrow_mut().natural_width = width;
            self.layout_data.borrow_mut().natural_height = height;

            self.img = Some(*img);
            self.id.request_layout();
        }
    }

    fn style_pass(&mut self, cx: &mut crate::context::StyleCx<'_>) {
        if self.style.read(cx) {
            // object_fit changed, update taffy layout
            self.set_taffy_layout();
            self.id.request_layout();
        }
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        if let Some(ref img) = self.img {
            let rect = self.id.get_content_rect_local();
            cx.draw_img(
                floem_renderer::Img {
                    img: img.clone(),
                    hash: self.img_hash.as_ref().unwrap(),
                },
                rect,
            );
        }
    }
}
