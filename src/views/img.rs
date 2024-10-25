use std::sync::Arc;

use floem_reactive::create_effect;
use peniko::Blob;
use sha2::{Digest, Sha256};

use crate::{id::ViewId, style::Style, unit::UnitExt, view::View, Renderer};

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
    img: Option<peniko::Image>,
    img_hash: Option<Vec<u8>>,
    content_node: Option<NodeId>,
}

pub fn img(image: impl Fn() -> Vec<u8> + 'static) -> Img {
    let image = image::load_from_memory(&image()).ok();
    let width = image.as_ref().map_or(0, |img| img.width());
    let height = image.as_ref().map_or(0, |img| img.height());
    let data = Arc::new(image.map_or(Default::default(), |img| img.into_rgba8().into_vec()));
    let blob = Blob::new(data);
    let image = peniko::Image::new(blob, peniko::Format::Rgba8, width, height);
    img_dynamic(move || image.clone())
}

pub(crate) fn img_dynamic(image: impl Fn() -> peniko::Image + 'static) -> Img {
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
        if let Ok(img) = state.downcast::<peniko::Image>() {
            let mut hasher = Sha256::new();
            hasher.update(img.data.data());
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
                .map(|img| (img.width, img.height))
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
