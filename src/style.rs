pub use taffy::style::{
    AlignContent, AlignItems, Dimension, Display, FlexDirection, JustifyContent, Position,
};
use taffy::{
    prelude::Rect,
    style::{LengthPercentage, LengthPercentageAuto, Style as TaffyStyle},
};
use vello::peniko::Color;

#[derive(Clone, Debug)]
pub struct Style {
    pub display: Display,
    pub position: Position,
    pub width: Dimension,
    pub height: Dimension,
    pub min_width: Dimension,
    pub min_height: Dimension,
    pub max_width: Dimension,
    pub max_height: Dimension,
    pub flex_direction: FlexDirection,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub flex_basis: Dimension,
    pub justify_content: Option<JustifyContent>,
    pub align_items: Option<AlignItems>,
    pub align_content: Option<AlignContent>,
    pub border: f32,
    pub border_left: f32,
    pub border_top: f32,
    pub border_right: f32,
    pub border_bottom: f32,
    pub border_radius: f32,
    pub padding: f32,
    pub padding_left: f32,
    pub padding_top: f32,
    pub padding_right: f32,
    pub padding_bottom: f32,
    pub margin: Option<f32>,
    pub margin_left: Option<f32>,
    pub margin_top: Option<f32>,
    pub margin_right: Option<f32>,
    pub margin_bottom: Option<f32>,
    pub color: Option<Color>,
    pub background: Option<Color>,
    pub font_size: Option<f32>,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            display: Display::Flex,
            position: Position::Relative,
            width: Dimension::Auto,
            height: Dimension::Auto,
            min_width: Dimension::Auto,
            min_height: Dimension::Auto,
            max_width: Dimension::Auto,
            max_height: Dimension::Auto,
            flex_direction: FlexDirection::default(),
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: Dimension::Auto,
            justify_content: None,
            align_items: None,
            align_content: None,
            border: 0.0,
            border_left: 0.0,
            border_top: 0.0,
            border_right: 0.0,
            border_bottom: 0.0,
            border_radius: 0.0,
            padding: 0.0,
            padding_left: 0.0,
            padding_top: 0.0,
            padding_right: 0.0,
            padding_bottom: 0.0,
            margin: None,
            margin_left: None,
            margin_top: None,
            margin_right: None,
            margin_bottom: None,
            color: None,
            background: None,
            font_size: None,
        }
    }
}

impl From<&Style> for TaffyStyle {
    fn from(value: &Style) -> Self {
        Self {
            display: value.display,
            position: value.position,
            size: taffy::prelude::Size {
                width: value.width,
                height: value.height,
            },
            min_size: taffy::prelude::Size {
                width: value.min_width,
                height: value.min_height,
            },
            max_size: taffy::prelude::Size {
                width: value.max_width,
                height: value.max_height,
            },
            flex_direction: value.flex_direction,
            flex_grow: value.flex_grow,
            flex_shrink: value.flex_shrink,
            flex_basis: value.flex_basis,
            justify_content: value.justify_content,
            align_items: value.align_items,
            align_content: value.align_content,
            border: Rect {
                left: LengthPercentage::Points(if value.border_left > 0.0 {
                    value.border_left
                } else {
                    value.border
                }),
                top: LengthPercentage::Points(if value.border_top > 0.0 {
                    value.border_top
                } else {
                    value.border
                }),
                right: LengthPercentage::Points(if value.border_right > 0.0 {
                    value.border_right
                } else {
                    value.border
                }),
                bottom: LengthPercentage::Points(if value.border_bottom > 0.0 {
                    value.border_bottom
                } else {
                    value.border
                }),
            },
            padding: Rect {
                left: LengthPercentage::Points(if value.padding_left > 0.0 {
                    value.padding_left
                } else {
                    value.padding
                }),
                top: LengthPercentage::Points(if value.padding_top > 0.0 {
                    value.padding_top
                } else {
                    value.padding
                }),
                right: LengthPercentage::Points(if value.padding_right > 0.0 {
                    value.padding_right
                } else {
                    value.padding
                }),
                bottom: LengthPercentage::Points(if value.padding_bottom > 0.0 {
                    value.padding_bottom
                } else {
                    value.padding
                }),
            },
            margin: Rect {
                left: if let Some(margin) = value.margin_left {
                    LengthPercentageAuto::Points(margin)
                } else if let Some(margin) = value.margin {
                    LengthPercentageAuto::Points(margin)
                } else {
                    LengthPercentageAuto::Points(0.0)
                },
                top: if let Some(margin) = value.margin_top {
                    LengthPercentageAuto::Points(margin)
                } else if let Some(margin) = value.margin {
                    LengthPercentageAuto::Points(margin)
                } else {
                    LengthPercentageAuto::Points(0.0)
                },
                right: if let Some(margin) = value.margin_right {
                    LengthPercentageAuto::Points(margin)
                } else if let Some(margin) = value.margin {
                    LengthPercentageAuto::Points(margin)
                } else {
                    LengthPercentageAuto::Points(0.0)
                },
                bottom: if let Some(margin) = value.margin_bottom {
                    LengthPercentageAuto::Points(margin)
                } else if let Some(margin) = value.margin {
                    LengthPercentageAuto::Points(margin)
                } else {
                    LengthPercentageAuto::Points(0.0)
                },
            },
            ..Default::default()
        }
    }
}
