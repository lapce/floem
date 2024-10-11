use std::rc::Rc;

use floem_reactive::create_effect;
use floem_renderer::Renderer;
use image::{DynamicImage, GenericImageView};
use sha2::{Digest, Sha256};

use crate::{id::ViewId, style::Style, unit::UnitExt, view::View};

use taffy::tree::NodeId;

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

pub enum HorizPosition {
    Top,
    Center,
    Bot,
    Px(f64),
    Pct(f64),
}

pub enum VertPosition {
    Left,
    Center,
    Right,
    Px(f64),
    Pct(f64),
}

impl ImageStyle {
    pub const BASE: Self = ImageStyle {
        position: ObjectPosition {
            horiz: HorizPosition::Center,
            vert: VertPosition::Center,
        },
        fit: ObjectFit::Fill,
    };

    pub fn fit(mut self, fit: ObjectFit) -> Self {
        self.fit = fit;
        self
    }

    pub fn object_pos(mut self, obj_pos: ObjectPosition) -> Self {
        self.position = obj_pos;
        self
    }
}

pub struct Img {
    id: ViewId,
    //FIXME: store the pixel format(once its added to vger), for now we only store RGBA(RGB is converted to RGBA)
    img: Option<Rc<DynamicImage>>,
    img_hash: Option<Vec<u8>>,
    img_dimensions: Option<(u32, u32)>,
    content_node: Option<NodeId>,
}

pub fn img(image: impl Fn() -> Vec<u8> + 'static) -> Img {
    img_dynamic(move || image::load_from_memory(&image()).ok().map(Rc::new))
}

pub(crate) fn img_dynamic(image: impl Fn() -> Option<Rc<DynamicImage>> + 'static) -> Img {
    let id = ViewId::new();
    create_effect(move |_| {
        id.update_state(image());
    });
    Img {
        id,
        img: None,
        img_hash: None,
        img_dimensions: None,
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
        if let Ok(img) = state.downcast::<Option<Rc<DynamicImage>>>() {
            self.img_hash = (*img).as_ref().map(|img| {
                let mut hasher = Sha256::new();
                hasher.update(img.as_bytes());
                hasher.finalize().to_vec()
            });
            self.img = *img;
            self.img_dimensions = self.img.as_ref().map(|img| img.dimensions());
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

            let (width, height) = self.img_dimensions.unwrap_or((0, 0));

            let style = Style::new()
                .width((width as f64).px())
                .height((height as f64).px())
                .to_taffy_style();
            let _ = self.id.taffy().borrow_mut().set_style(content_node, style);

            vec![content_node]
        })
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        if let Some(img) = self.img.as_ref() {
            let rect = self.id.get_content_rect();
            cx.draw_img(
                floem_renderer::Img {
                    img,
                    data: img.as_bytes(),
                    hash: self.img_hash.as_ref().unwrap(),
                },
                rect,
            );
        }
    }
}
