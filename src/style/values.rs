//! Core style property value trait and implementations.

use floem_reactive::{RwSignal, SignalGet, SignalUpdate as _};
use floem_renderer::Renderer;
use floem_renderer::text::{LineHeightValue, Weight};
use peniko::color::{HueDirection, palette};
use peniko::kurbo::{self, Point, Stroke};
use peniko::{
    Brush, Color, ColorStop, ColorStops, Gradient, GradientKind, InterpolationAlphaSpace,
    LinearGradientPosition,
};
use smallvec::SmallVec;
use std::fmt::Debug;
use taffy::GridTemplateComponent;
use taffy::prelude::{auto, fr};

#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;
#[cfg(target_arch = "wasm32")]
use web_time::Duration;

use taffy::style::{
    AlignContent, AlignItems, BoxSizing, Display, FlexDirection, FlexWrap, Overflow, Position,
};
use taffy::{
    geometry::{MinMax, Size},
    prelude::{GridPlacement, Line},
    style::{LengthPercentage, MaxTrackSizingFunction, MinTrackSizingFunction},
};

use crate::AnyView;
use crate::prelude::ViewTuple;
use crate::theme::StyleThemeExt;
use crate::unit::{Pct, Px, PxPct, PxPctAuto};
use crate::view::ViewTupleFlat;
use crate::view::{IntoView, View};
use crate::views::{ContainerExt, Decorators, Label, TooltipExt, canvas, stack, v_stack_from_iter};

use super::FontSize;

pub enum CombineResult<T> {
    Other,  // The result is semantically `other` - caller can reuse it
    New(T), // A new value was created
}

pub trait StylePropValue: Clone + PartialEq + Debug {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        None
    }

    fn interpolate(&self, _other: &Self, _value: f64) -> Option<Self> {
        None
    }

    fn combine(&self, _other: &Self) -> CombineResult<Self> {
        CombineResult::Other
    }
}

impl StylePropValue for i32 {
    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        Some((*self as f64 + (*other as f64 - *self as f64) * value).round() as i32)
    }
}
impl StylePropValue for bool {}
impl StylePropValue for f32 {
    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        Some(*self * (1.0 - value as f32) + *other * value as f32)
    }
}
impl StylePropValue for u16 {
    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        Some((*self as f64 + (*other as f64 - *self as f64) * value).round() as u16)
    }
}
impl StylePropValue for usize {
    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        Some((*self as f64 + (*other as f64 - *self as f64) * value).round() as usize)
    }
}
impl StylePropValue for f64 {
    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        Some(*self * (1.0 - value) + *other * value)
    }
}
impl StylePropValue for Overflow {}
impl StylePropValue for Display {}
impl StylePropValue for Position {}
impl StylePropValue for FlexDirection {}
impl StylePropValue for FlexWrap {}
impl StylePropValue for AlignItems {}
impl StylePropValue for BoxSizing {}
impl StylePropValue for AlignContent {}
impl StylePropValue for GridTemplateComponent<String> {}
impl StylePropValue for MinTrackSizingFunction {}
impl StylePropValue for MaxTrackSizingFunction {}
impl<T: StylePropValue, M: StylePropValue> StylePropValue for MinMax<T, M> {}
impl<T: StylePropValue> StylePropValue for Line<T> {}
impl StylePropValue for taffy::GridAutoFlow {}
impl StylePropValue for GridPlacement {}

impl<A: smallvec::Array> StylePropValue for SmallVec<A>
where
    <A as smallvec::Array>::Item: StylePropValue,
{
    fn debug_view(&self) -> Option<Box<dyn View>> {
        if self.is_empty() {
            return Some(
                Label::new("smallvec\n[]")
                    .style(|s| s.with_theme(|s, t| s.color(t.text_muted())))
                    .into_any(),
            );
        }

        let count = self.len();
        let is_spilled = self.spilled();

        // Create a preview that shows count and whether it has spilled to heap
        let preview = Label::derived(move || {
            if is_spilled {
                format!("smallvec\n[{}] (heap)", count)
            } else {
                format!("smallvec\n[{}] (inline)", count)
            }
        })
        .style(|s| {
            s.padding(2.0)
                .padding_horiz(6.0)
                .items_center()
                .justify_center()
                .text_align(floem_renderer::text::Align::Center)
                .border(1.)
                .border_radius(5.0)
                .margin_left(6.0)
                .with_theme(|s, t| s.color(t.text()).border_color(t.border()))
                .with_context_opt::<FontSize, _>(|s, fs| s.font_size(fs * 0.85))
        });

        // Clone items for the tooltip view
        let items = self.clone();

        let tooltip_view = move || {
            v_stack_from_iter(items.iter().enumerate().map(|(i, item)| {
                let index_label = Label::new(format!("[{}]", i))
                    .style(|s| s.with_theme(|s, t| s.color(t.text_muted())));

                let item_view = item.debug_view().unwrap_or_else(|| {
                    Label::new(format!("{:?}", item))
                        .style(|s| s.flex_grow(1.0))
                        .into_any()
                });

                stack((index_label, item_view)).style(|s| s.items_center().gap(8.0).padding(4.0))
            }))
            .style(|s| s.gap(4.0))
        };

        // Return the tooltip view wrapped in the preview
        Some(
            stack((preview, tooltip_view()))
                .style(|s| s.gap(8.0))
                .into_any(),
        )
    }

    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        self.iter().zip(other.iter()).try_fold(
            SmallVec::with_capacity(self.len()),
            |mut acc, (v1, v2)| {
                if let Some(interpolated) = v1.interpolate(v2, value) {
                    acc.push(interpolated);
                    Some(acc)
                } else {
                    None
                }
            },
        )
    }
}
impl StylePropValue for String {}
impl StylePropValue for Weight {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        let clone = *self;
        Some(
            format!("{clone:?}")
                .style(move |s| s.font_weight(clone))
                .into_any(),
        )
    }
    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        self.0.interpolate(&other.0, value).map(Weight)
    }
}
impl StylePropValue for crate::text::Style {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        let clone = *self;
        Some(
            format!("{clone:?}")
                .style(move |s| s.font_style(clone))
                .into_any(),
        )
    }
}
impl StylePropValue for crate::text::Align {}
impl StylePropValue for LineHeightValue {
    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        match (self, other) {
            (LineHeightValue::Normal(v1), LineHeightValue::Normal(v2)) => {
                v1.interpolate(v2, value).map(LineHeightValue::Normal)
            }
            (LineHeightValue::Px(v1), LineHeightValue::Px(v2)) => {
                v1.interpolate(v2, value).map(LineHeightValue::Px)
            }
            _ => None,
        }
    }
}
impl StylePropValue for Size<LengthPercentage> {}

impl<T: StylePropValue> StylePropValue for Option<T> {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        self.as_ref().and_then(|v| v.debug_view())
    }

    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        self.as_ref().and_then(|this| {
            other
                .as_ref()
                .and_then(|other| this.interpolate(other, value).map(Some))
        })
    }
}
impl<T: StylePropValue + 'static> StylePropValue for Vec<T> {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        if self.is_empty() {
            return Some(
                Label::new("[]")
                    .style(|s| s.with_theme(|s, t| s.color(t.text_muted())))
                    .into_any(),
            );
        }

        let count = self.len();
        let _preview = Label::derived(move || format!("[{}]", count)).style(|s| {
            s.padding(2.0)
                .padding_horiz(6.0)
                .border(1.)
                .border_radius(5.0)
                .margin_left(6.0)
                .with_theme(|s, t| s.color(t.text()).border_color(t.border()))
                .with_context_opt::<FontSize, _>(|s, fs| s.font_size(fs * 0.85))
        });

        let items = self.clone();
        let tooltip_view = move || {
            v_stack_from_iter(items.iter().enumerate().map(|(i, item)| {
                let index_label = Label::new(format!("[{}]", i))
                    .style(|s| s.with_theme(|s, t| s.color(t.text_muted())));

                let item_view = item.debug_view().unwrap_or_else(|| {
                    Label::new(format!("{:?}", item))
                        .style(|s| s.flex_grow(1.0))
                        .into_any()
                });

                stack((index_label, item_view)).style(|s| s.items_center().gap(8.0).padding(4.0))
            }))
            .style(|s| s.gap(4.0))
        };

        Some(
            // preview
            tooltip_view().into_any(),
        )
    }

    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        self.iter().zip(other.iter()).try_fold(
            Vec::with_capacity(self.len()),
            |mut acc, (v1, v2)| {
                if let Some(interpolated) = v1.interpolate(v2, value) {
                    acc.push(interpolated);
                    Some(acc)
                } else {
                    None
                }
            },
        )
    }
}
impl StylePropValue for Px {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        Some(Label::new(format!("{} px", self.0)).into_any())
    }
    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        self.0.interpolate(&other.0, value).map(Px)
    }
}
impl StylePropValue for Pct {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        Some(Label::new(format!("{}%", self.0)).into_any())
    }
    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        self.0.interpolate(&other.0, value).map(Pct)
    }
}
impl StylePropValue for PxPctAuto {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        let label = match self {
            Self::Px(v) => format!("{v} px"),
            Self::Pct(v) => format!("{v}%"),
            Self::Auto => "auto".to_string(),
        };
        Some(Label::new(label).into_any())
    }
    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        match (self, other) {
            (Self::Px(v1), Self::Px(v2)) => Some(Self::Px(v1 + (v2 - v1) * value)),
            (Self::Pct(v1), Self::Pct(v2)) => Some(Self::Pct(v1 + (v2 - v1) * value)),
            (Self::Auto, Self::Auto) => Some(Self::Auto),
            // TODO: Figure out some way to get in the relevant layout information in order to interpolate between pixels and percent
            _ => None,
        }
    }
}
impl StylePropValue for PxPct {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        let label = match self {
            Self::Px(v) => format!("{v} px"),
            Self::Pct(v) => format!("{v}%"),
        };
        Some(Label::new(label).into_any())
    }

    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        match (self, other) {
            (Self::Px(v1), Self::Px(v2)) => Some(Self::Px(v1 + (v2 - v1) * value)),
            (Self::Pct(v1), Self::Pct(v2)) => Some(Self::Pct(v1 + (v2 - v1) * value)),
            // TODO: Figure out some way to get in the relevant layout information in order to interpolate between pixels and percent
            _ => None,
        }
    }
}

pub(crate) fn views(views: impl ViewTuple) -> Vec<AnyView> {
    views.into_views()
}

impl StylePropValue for Color {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        let color = *self;
        let swatch = ()
            .style(move |s| {
                s.background(color)
                    .width(22.0)
                    .height(14.0)
                    .border(1.)
                    .border_color(palette::css::WHITE.with_alpha(0.5))
                    .border_radius(5.0)
            })
            .container()
            .style(|s| {
                s.border(1.)
                    .border_color(palette::css::BLACK.with_alpha(0.5))
                    .border_radius(5.0)
            });

        let tooltip_view = move || {
            // Convert to RGBA8 for standard representations
            let c = color.to_rgba8();
            let (r, g, b, a) = (c.r, c.g, c.b, c.a);

            // Hex representation
            let hex = if a == 255 {
                format!("#{:02X}{:02X}{:02X}", r, g, b)
            } else {
                format!("#{:02X}{:02X}{:02X}{:02X}", r, g, b, a)
            };

            // RGBA string
            let rgba_str = format!("rgba({}, {}, {}, {:.3})", r, g, b, a as f32 / 255.0);

            // Alpha percentage
            let alpha_str = format!(
                "{:.1}% ({:.3})",
                (a as f32 / 255.0) * 100.0,
                a as f32 / 255.0
            );

            let components = color.components;
            let color_space_str = format!("{:?}", color.cs);

            let hex = views((
                "Hex:".style(|s| s.font_bold().min_width(80.0).justify_end()),
                Label::derived(move || hex.clone()),
            ));
            let rgba = views((
                "RGBA:".style(|s| s.font_bold().min_width(80.0).justify_end()),
                Label::derived(move || rgba_str.clone()),
            ));
            let components = views((
                "Components:".style(|s| s.font_bold().min_width(80.0).justify_end()),
                (
                    Label::derived(move || format!("[0]: {:.3}", components[0])),
                    Label::derived(move || format!("[1]: {:.3}", components[1])),
                    Label::derived(move || format!("[2]: {:.3}", components[2])),
                    Label::derived(move || format!("[3]: {:.3}", components[3])),
                )
                    .v_stack()
                    .style(|s| s.gap(2.0)),
            ));
            let color_space = views((
                "Color Space:".style(|s| s.font_bold().min_width(80.0).justify_end()),
                Label::derived(move || color_space_str.clone()),
            ));
            let alpha = views((
                "Alpha:".style(|s| s.font_bold().min_width(80.0).justify_end()),
                Label::derived(move || alpha_str.clone()),
            ));
            (hex, rgba, components, color_space, alpha)
                .flatten()
                .style(|s| {
                    s.grid()
                        .grid_template_columns([auto(), fr(1.)])
                        .justify_center()
                        .items_center()
                        .row_gap(20)
                        .col_gap(10)
                        .padding(30)
                })
        };

        Some(
            swatch
                .tooltip(tooltip_view)
                .style(|s| s.items_center())
                .into_any(),
        )
    }

    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        Some(self.lerp(*other, value as f32, HueDirection::default()))
    }
}

impl StylePropValue for Gradient {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        let box_width = 22.;
        let box_height = 14.;
        let mut grad = self.clone();
        grad.kind = match grad.kind {
            GradientKind::Linear(LinearGradientPosition { start, end }) => {
                let dx = end.x - start.x;
                let dy = end.y - start.y;

                let scale_x = box_width / dx.abs();
                let scale_y = box_height / dy.abs();
                let scale = scale_x.min(scale_y);

                let new_dx = dx * scale;
                let new_dy = dy * scale;

                let new_start = Point {
                    x: if dx > 0.0 { 0.0 } else { box_width },
                    y: if dy > 0.0 { 0.0 } else { box_height },
                };

                let new_end = Point {
                    x: new_start.x + new_dx,
                    y: new_start.y + new_dy,
                };

                GradientKind::Linear(LinearGradientPosition {
                    start: new_start,
                    end: new_end,
                })
            }
            _ => grad.kind,
        };
        let color = ().style(move |s| {
            s.background(grad.clone())
                .width(box_width)
                .height(box_height)
                .border(1.)
                .border_color(palette::css::WHITE.with_alpha(0.5))
                .border_radius(5.0)
        });
        let color = color.container().style(|s| {
            s.border(1.)
                .border_color(palette::css::BLACK.with_alpha(0.5))
                .border_radius(5.0)
                .margin_left(6.0)
        });
        Some(
            stack((Label::new(format!("{self:?}")), color))
                .style(|s| s.items_center())
                .into_any(),
        )
    }

    fn interpolate(&self, _other: &Self, _value: f64) -> Option<Self> {
        None
    }
}

// this is necessary because Stroke doesn't impl partial eq. it probably should...
#[derive(Clone, Debug, Default)]
pub struct StrokeWrap(pub Stroke);
impl StrokeWrap {
    pub fn new(width: f64) -> Self {
        Self(Stroke::new(width))
    }
}
impl PartialEq for StrokeWrap {
    fn eq(&self, other: &Self) -> bool {
        let self_stroke = &self.0;
        let other_stroke = &other.0;

        self_stroke.width == other_stroke.width
            && self_stroke.join == other_stroke.join
            && self_stroke.miter_limit == other_stroke.miter_limit
            && self_stroke.start_cap == other_stroke.start_cap
            && self_stroke.end_cap == other_stroke.end_cap
            && self_stroke.dash_pattern == other_stroke.dash_pattern
            && self_stroke.dash_offset == other_stroke.dash_offset
    }
}
impl From<Stroke> for StrokeWrap {
    fn from(value: Stroke) -> Self {
        Self(value)
    }
}
impl From<f32> for StrokeWrap {
    fn from(value: f32) -> Self {
        Self(Stroke::new(value.into()))
    }
}
impl From<f64> for StrokeWrap {
    fn from(value: f64) -> Self {
        Self(Stroke::new(value))
    }
}
impl From<i32> for StrokeWrap {
    fn from(value: i32) -> Self {
        Self(Stroke::new(value.into()))
    }
}
impl StylePropValue for StrokeWrap {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        let stroke = self.0.clone();
        let clone = stroke.clone();

        let color = RwSignal::new(palette::css::RED);

        // Visual preview of the stroke
        let preview = canvas(move |cx, size| {
            cx.stroke(
                &kurbo::Line::new(
                    Point::new(0., size.height / 2.),
                    Point::new(size.width, size.height / 2.),
                ),
                color.get(),
                &clone,
            );
        })
        .style(move |s| s.width(80.0).height(20.0))
        .container()
        .style(move |s| {
            s.with_theme(move |s, t| {
                color.set(t.primary());
                s.border_color(t.border())
            })
            .padding(4.0)
        });

        let tooltip_view = move || {
            let stroke = stroke.clone();

            let width_row = views((
                "Width:".style(|s| s.font_bold().min_width(100.0).justify_end()),
                Label::derived(move || format!("{:.1}px", stroke.width)),
            ));

            let join_row = views((
                "Join:".style(|s| s.font_bold().min_width(100.0).justify_end()),
                Label::derived(move || format!("{:?}", stroke.join)),
            ));

            let miter_row = views((
                "Miter Limit:".style(|s| s.font_bold().min_width(100.0).justify_end()),
                Label::derived(move || format!("{:.2}", stroke.miter_limit)),
            ));

            let start_cap_row = views((
                "Start Cap:".style(|s| s.font_bold().min_width(100.0).justify_end()),
                Label::derived(move || format!("{:?}", stroke.start_cap)),
            ));

            let end_cap_row = views((
                "End Cap:".style(|s| s.font_bold().min_width(100.0).justify_end()),
                Label::derived(move || format!("{:?}", stroke.end_cap)),
            ));

            let pattern_clone = stroke.dash_pattern.clone();

            let dash_pattern_row = views((
                "Dash Pattern:".style(|s| s.font_bold().min_width(100.0).justify_end()),
                Label::derived(move || {
                    if pattern_clone.is_empty() {
                        "Solid".to_string()
                    } else {
                        format!("{:?}", pattern_clone.as_slice())
                    }
                }),
            ));

            let dash_offset_row = if !stroke.dash_pattern.is_empty() {
                Some(views((
                    "Dash Offset:".style(|s| s.font_bold().min_width(100.0).justify_end()),
                    Label::derived(move || format!("{:.1}", stroke.dash_offset)),
                )))
            } else {
                None
            };

            let mut rows = vec![
                width_row.into_any(),
                join_row.into_any(),
                miter_row.into_any(),
                start_cap_row.into_any(),
                end_cap_row.into_any(),
                dash_pattern_row.into_any(),
            ];

            if let Some(offset_row) = dash_offset_row {
                rows.push(offset_row.into_any());
            }

            v_stack_from_iter(rows).style(|s| {
                s.grid()
                    .grid_template_columns([auto(), fr(1.)])
                    .justify_center()
                    .items_center()
                    .row_gap(12)
                    .col_gap(10)
                    .padding(20)
            })
        };

        Some(
            preview
                .tooltip(tooltip_view)
                .style(|s| s.items_center())
                .into_any(),
        )
    }
}
impl StylePropValue for Brush {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        match self {
            Brush::Solid(color) => color.debug_view(),
            Brush::Gradient(grad) => grad.debug_view(),
            Brush::Image(_) => None,
        }
    }

    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        match (self, other) {
            (Brush::Solid(color), Brush::Solid(other)) => Some(Self::Solid(color.lerp(
                *other,
                value as f32,
                HueDirection::default(),
            ))),
            (Brush::Gradient(gradient), Brush::Solid(solid)) => {
                let interpolated_stops: Vec<ColorStop> = gradient
                    .stops
                    .iter()
                    .map(|stop| {
                        let interpolated_color = stop.color.to_alpha_color().lerp(
                            *solid,
                            value as f32,
                            HueDirection::default(),
                        );
                        ColorStop::from((stop.offset, interpolated_color))
                    })
                    .collect();
                Some(Brush::Gradient(Gradient {
                    kind: gradient.kind,
                    extend: gradient.extend,
                    interpolation_cs: gradient.interpolation_cs,
                    hue_direction: gradient.hue_direction,
                    stops: ColorStops::from(&*interpolated_stops),
                    interpolation_alpha_space: InterpolationAlphaSpace::Premultiplied,
                }))
            }
            (Brush::Solid(solid), Brush::Gradient(gradient)) => {
                let interpolated_stops: Vec<ColorStop> = gradient
                    .stops
                    .iter()
                    .map(|stop| {
                        let interpolated_color = solid.lerp(
                            stop.color.to_alpha_color(),
                            value as f32,
                            HueDirection::default(),
                        );
                        ColorStop::from((stop.offset, interpolated_color))
                    })
                    .collect();
                Some(Brush::Gradient(Gradient {
                    kind: gradient.kind,
                    extend: gradient.extend,
                    interpolation_cs: gradient.interpolation_cs,
                    hue_direction: gradient.hue_direction,
                    stops: ColorStops::from(&*interpolated_stops),
                    interpolation_alpha_space: InterpolationAlphaSpace::Premultiplied,
                }))
            }

            (Brush::Gradient(gradient1), Brush::Gradient(gradient2)) => {
                gradient1.interpolate(gradient2, value).map(Brush::Gradient)
            }
            _ => None,
        }
    }
}
impl StylePropValue for Duration {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        None
    }

    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        self.as_secs_f64()
            .interpolate(&other.as_secs_f64(), value)
            .map(Duration::from_secs_f64)
    }
}

/// Internal storage for style property values in the style map.
///
/// Unlike `StyleValue<T>` which is used in the public API, `StyleMapValue<T>`
/// is the internal representation stored in the style hashmap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleMapValue<T> {
    /// Value inserted by animation interpolation
    Animated(T),
    /// Value set directly
    Val(T),
    /// Use the default value for the style, typically from the underlying `ComputedStyle`
    Unset,
}

impl<T> StyleMapValue<T> {
    pub(crate) fn as_ref(&self) -> Option<&T> {
        match self {
            Self::Val(v) => Some(v),
            Self::Animated(v) => Some(v),
            Self::Unset => None,
        }
    }
}

/// The value for a [`Style`] property in the public API.
///
/// This represents the result of reading a style property, with additional
/// states like `Base` that indicate inheritance from parent styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StyleValue<T> {
    /// Value inserted by animation interpolation
    Animated(T),
    /// Value set directly
    Val(T),
    /// Use the default value for the style, typically from the underlying `ComputedStyle`.
    Unset,
    /// Use whatever the base style is. For an overriding style like hover, this uses the base
    /// style. For the base style, this is equivalent to `Unset`.
    #[default]
    Base,
}

impl<T> StyleValue<T> {
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> StyleValue<U> {
        match self {
            Self::Val(x) => StyleValue::Val(f(x)),
            Self::Animated(x) => StyleValue::Animated(f(x)),
            Self::Unset => StyleValue::Unset,
            Self::Base => StyleValue::Base,
        }
    }

    pub fn unwrap_or(self, default: T) -> T {
        match self {
            Self::Val(x) => x,
            Self::Animated(x) => x,
            Self::Unset => default,
            Self::Base => default,
        }
    }

    pub fn unwrap_or_else(self, f: impl FnOnce() -> T) -> T {
        match self {
            Self::Val(x) => x,
            Self::Animated(x) => x,
            Self::Unset => f(),
            Self::Base => f(),
        }
    }

    pub fn as_mut(&mut self) -> Option<&mut T> {
        match self {
            Self::Val(x) => Some(x),
            Self::Animated(x) => Some(x),
            Self::Unset => None,
            Self::Base => None,
        }
    }
}

impl<T> From<T> for StyleValue<T> {
    fn from(x: T) -> Self {
        Self::Val(x)
    }
}
