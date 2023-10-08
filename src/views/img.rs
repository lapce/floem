use floem_reactive::create_effect;
use floem_renderer::Renderer;
use image::{EncodableLayout, GenericImageView};
use kurbo::Size;
use sha2::{Digest, Sha256};

use crate::{
    id::Id,
    style::{ComputedStyle, Style},
    unit::{PxPctAuto, UnitExt},
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
    img_hash: Option<Vec<u8>>,
    img_dimensions: Option<(u32, u32)>,
    content_node: Option<Node>,
}

impl Img {
    fn update_img_dimensions(&mut self, cx: &mut crate::context::LayoutCx) {
        let pixels = if let Some(pixels) = self.pixels.as_ref() {
            pixels
        } else {
            return;
        };

        let styles = cx.get_computed_style(self.id);
        let target_width_px = match styles.width {
            PxPctAuto::Px(px) => Some(px as u32),
            PxPctAuto::Pct(_) => todo!("Percentage-based width for image not supported yet"),
            PxPctAuto::Auto => None,
        };

        let target_height_px = match styles.height {
            PxPctAuto::Px(px) => Some(px as u32),
            PxPctAuto::Pct(_) => todo!("Percentage-based height for image not supported yet"),
            PxPctAuto::Auto => None,
        };

        self.img_dimensions = if target_width_px.is_none() || target_height_px.is_none() {
            // process the image pixels to determine its width and height - this operation is expensive,
            // so it should happen only once every time the width/height/pixels/object_fit change
            let img = image::load_from_memory(pixels.as_bytes()).unwrap();
            let (img_width, img_height) = img.dimensions();

            // TODO: computed width & height should take into account ObjectFit
            let computed_width = if let Some(width_px) = target_width_px {
                width_px
            } else {
                img_width
            };

            let computed_height = if let Some(height_px) = target_height_px {
                height_px
            } else {
                img_height
            };

            Some((computed_width, computed_height))
        } else {
            // since we need to conditionally load the img in memory when both width & height are
            // `Auto`, `if let` or `match` expressions result in less readable code
            #[allow(clippy::unnecessary_unwrap)]
            Some((target_width_px.unwrap(), target_height_px.unwrap()))
        }
    }
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

            self.pixels = Some(image.clone());

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

            if self.img_dimensions.is_none() {
                self.update_img_dimensions(cx);
            }
            let (width, height) = self.img_dimensions.unwrap();

            let style = Style::BASE
                .width((width as f64).px())
                .height((height as f64).px())
                .compute(&ComputedStyle::default())
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
        if self.img_dimensions.is_none() {
            return;
        }

        if let Some(image) = self.pixels.as_ref() {
            let size = cx.get_layout(self.id).unwrap().size;
            let rect = Size::new(size.width as f64, size.height as f64).to_rect();
            let (width, height) = self.img_dimensions.unwrap();
            assert!(width > 0);
            assert!(height > 0);

            cx.draw_img(
                floem_renderer::Img {
                    data: image.as_bytes(),
                    hash: self.img_hash.as_ref().unwrap(),
                },
                width,
                height,
                rect,
            );
        }
    }
}
