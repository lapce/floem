//! Module defining image view and its properties: style, position and fit.
#![deny(missing_docs)]
use std::{path::PathBuf, sync::Arc};

use floem_reactive::create_effect;
use peniko::{Blob, ImageAlphaType, ImageData};
use sha2::{Digest, Sha256};
use taffy::NodeId;

use crate::{id::ViewId, style::Style, unit::UnitExt, view::View, Renderer};

/// Holds information about image position and size inside container.
pub struct ImageStyle {
    fit: ObjectFit,
    position: ObjectPosition,
}

/// How the content of a replaced element, such as an img or video, should be resized to fit its container.
/// See <https://developer.mozilla.org/en-US/docs/Web/CSS/object-fit>.
pub enum ObjectFit {
    /// The replaced content is sized to fill the element's content box.
    /// The entire object will completely fill the box.
    /// If the object's aspect ratio does not match the aspect ratio of its box, then the object will be stretched to fit.
    Fill,
    /// The replaced content is scaled to maintain its aspect ratio while fitting within the element's content box.
    /// The entire object is made to fill the box, while preserving its aspect ratio, so the object will be "letterboxed"
    /// if its aspect ratio does not match the aspect ratio of the box.
    Contain,
    /// The content is sized to maintain its aspect ratio while filling the element's entire content box.
    /// If the object's aspect ratio does not match the aspect ratio of its box, then the object will be clipped to fit.
    Cover,
    /// The content is sized as if none or contain were specified, whichever would result in a smaller concrete object size.
    ScaleDown,
    /// The replaced content is not resized.
    None,
}

/// Specifies the alignment of the element's contents within the element's box.
///
/// Areas of the box which aren't covered by the replaced element's object will show the element's background.
/// See <https://developer.mozilla.org/en-US/docs/Web/CSS/object-position>.
pub struct ObjectPosition {
    #[allow(unused)]
    horiz: HorizPosition,
    #[allow(unused)]
    vert: VertPosition,
}

/// Specifies object position on horizontal axis inside the element's box.
pub enum HorizPosition {
    /// Top position inside the element's box on the horizontal axis.
    Top,
    /// Center position inside the element's box on the horizontal axis.
    Center,
    /// Bottom position inside the element's box on the horizontal axis.
    Bot,
    /// Horizontal position inside the element's box as **pixels**.
    Px(f64),
    /// Horizontal position inside the element's box as **percent**.
    Pct(f64),
}

/// Specifies object position on vertical axis inside the element's box.
pub enum VertPosition {
    /// Left position inside the element's box on the vertical axis.
    Left,
    /// Center position inside the element's box on the vertical axis.
    Center,
    /// Right position inside the element's box on the vertical axis.
    Right,
    /// Vertical position inside the element's box as **pixels**.
    Px(f64),
    /// Vertical position inside the element's box as **percent**.
    Pct(f64),
}

impl ImageStyle {
    /// Default setting for the image position (center & fit)
    pub const BASE: Self = ImageStyle {
        position: ObjectPosition {
            horiz: HorizPosition::Center,
            vert: VertPosition::Center,
        },
        fit: ObjectFit::Fill,
    };

    /// How the content should be resized to fit its container.
    pub fn fit(mut self, fit: ObjectFit) -> Self {
        self.fit = fit;
        self
    }

    /// Specifies the alignment of the element's contents within the element's box.
    ///
    /// Areas of the box which aren't covered by the replaced element's object will show the element's background.
    pub fn object_pos(mut self, obj_pos: ObjectPosition) -> Self {
        self.position = obj_pos;
        self
    }
}

/// Holds the data needed for [img] view fn to display images.
pub struct Img {
    id: ViewId,
    img: Option<peniko::ImageBrush>,
    img_hash: Option<Vec<u8>>,
    content_node: Option<NodeId>,
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
    create_effect(move |_| {
        id.update_state(image());
    });
    Img {
        id,
        img: None,
        img_hash: None,
        content_node: None,
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

            self.img = Some(*img);
            self.id.request_layout();
        }
    }

    fn layout(&mut self, cx: &mut crate::context::LayoutCx) -> taffy::tree::NodeId {
        cx.layout_node(self.id(), true, |_cx| {
            if self.content_node.is_none() {
                self.content_node = Some(
                    self.id
                        .taffy()
                        .borrow_mut()
                        .new_leaf(taffy::style::Style::DEFAULT)
                        .unwrap(),
                );
            }
            let content_node = self.content_node.unwrap();

            let (width, height) = self
                .img
                .as_ref()
                .map(|img| (img.image.width, img.image.height))
                .unwrap_or((0, 0));

            let style = Style::new()
                .width((width as f64).px())
                .height((height as f64).px())
                .to_taffy_style();
            let _ = self.id.taffy().borrow_mut().set_style(content_node, style);

            vec![content_node]
        })
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        if let Some(ref img) = self.img {
            let rect = self.id.get_content_rect();
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
