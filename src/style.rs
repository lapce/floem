pub use taffy::style::{Dimension, FlexDirection};
use taffy::{
    prelude::Rect,
    style::{LengthPercentage, Style as TaffyStyle},
};

#[derive(Clone, Debug)]
pub struct Style {
    pub width: Dimension,
    pub height: Dimension,
    pub flex_direction: FlexDirection,
    pub flex_grow: f32,
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
}

impl Default for Style {
    fn default() -> Self {
        Self {
            width: Dimension::Auto,
            height: Dimension::Auto,
            flex_direction: FlexDirection::default(),
            flex_grow: 0.0,
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
        }
    }
}

impl From<&Style> for TaffyStyle {
    fn from(value: &Style) -> Self {
        Self {
            size: taffy::prelude::Size {
                width: value.width,
                height: value.height,
            },
            flex_direction: value.flex_direction,
            flex_grow: value.flex_grow,
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
            ..Default::default()
        }
    }
}
