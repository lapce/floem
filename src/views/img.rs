use floem_reactive::create_effect;
use floem_renderer::Renderer;
use image::{DynamicImage, GenericImageView};
use kurbo::Size;
use sha2::{Digest, Sha256};

use crate::{
    id::Id,
    style::Style,
    unit::UnitExt,
    view::{ChangeFlags, View},
};

use taffy::prelude::Node;

pub struct ImageStyle {
    fit: ObjectFit,
    position: ObjectPosition,
}

/// How the content of a replaced element, such as an img or video, should be resized to fit its container.
/// See  https://developer.mozilla.org/en-US/docs/Web/CSS/object-fit
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
/// Areas of the box which aren't covered by the replaced element's object will show the element's background.
/// See https://developer.mozilla.org/en-US/docs/Web/CSS/object-position.
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
    id: Id,
    //FIXME: store the pixel format(once its added to vger), for now we only store RGBA(RGB is converted to RGBA)
    pixels: Option<Vec<u8>>,
    img: Option<DynamicImage>,
    img_hash: Option<Vec<u8>>,
    img_dimensions: Option<(u32, u32)>,
    content_node: Option<Node>,
}

pub fn img(image: impl Fn() -> Vec<u8> + 'static) -> Img {
    let id = Id::next();
    create_effect(move |_| {
        let img_data = image();
        id.update_state(img_data, false);
    });
    Img {
        id,
        pixels: None,
        img: None,
        img_hash: None,
        img_dimensions: None,
        content_node: None,
    }
}

impl View for Img {
    fn id(&self) -> Id {
        self.id
    }

    fn child(&self, _id: Id) -> Option<&dyn View> {
        None
    }

    fn child_mut(&mut self, _id: Id) -> Option<&mut dyn View> {
        None
    }

    fn children(&self) -> Vec<&dyn View> {
        Vec::new()
    }

    fn children_mut(&mut self) -> Vec<&mut dyn View> {
        Vec::new()
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "Img".into()
    }

    fn update(
        &mut self,
        cx: &mut crate::context::UpdateCx,
        state: Box<dyn std::any::Any>,
    ) -> crate::view::ChangeFlags {
        if let Ok(state) = state.downcast::<Vec<u8>>() {
            let image = &*state;

            let img = image::load_from_memory(image).ok();
            self.img = img;
            self.pixels = Some(image.clone());
            self.img_dimensions = self.img.as_ref().map(|img| img.dimensions());

            let mut hasher = Sha256::new();
            hasher.update(image);
            let hash = hasher.finalize().to_vec();

            self.img_hash = Some(hash);
            cx.request_layout(self.id());
            ChangeFlags::LAYOUT
        } else {
            eprintln!("downcast failed");
            ChangeFlags::empty()
        }
    }

    fn layout(&mut self, cx: &mut crate::context::LayoutCx) -> taffy::prelude::Node {
        cx.layout_node(self.id, true, |cx| {
            if self.content_node.is_none() {
                self.content_node = Some(
                    cx.app_state_mut()
                        .taffy
                        .new_leaf(taffy::style::Style::DEFAULT)
                        .unwrap(),
                );
            }
            let content_node = self.content_node.unwrap();

            let (width, height) = self.img_dimensions.unwrap_or((0, 0));

            let style = Style::BASE
                .width((width as f64).px())
                .height((height as f64).px())
                .compute()
                .to_taffy_style();
            let _ = cx.app_state_mut().taffy.set_style(content_node, style);

            vec![content_node]
        })
    }

    fn event(
        &mut self,
        _cx: &mut crate::context::EventCx,
        _id_path: Option<&[Id]>,
        _event: crate::event::Event,
    ) -> bool {
        false
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        if let Some(img) = self.img.as_ref() {
            let size = cx.get_layout(self.id).unwrap().size;
            let rect = Size::new(size.width as f64, size.height as f64).to_rect();
            cx.draw_img(
                floem_renderer::Img {
                    img,
                    data: self.pixels.as_ref().unwrap(),
                    hash: self.img_hash.as_ref().unwrap(),
                },
                rect,
            );
        }
    }
}
