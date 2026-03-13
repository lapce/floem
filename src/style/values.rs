//! Core style property value trait and implementations.

use floem_reactive::{RwSignal, SignalGet, SignalUpdate as _};
use floem_renderer::Renderer;
use floem_renderer::text::{FontWeight, LineHeightValue};
use peniko::color::{HueDirection, palette};
use peniko::kurbo::{self, Affine, Point, Stroke, Vec2};
use peniko::{
    Brush, Color, ColorStop, ColorStops, Gradient, GradientKind, InterpolationAlphaSpace,
    LinearGradientPosition,
};
use smallvec::SmallVec;
use std::collections::HashSet;
use std::fmt::Debug;
use std::rc::Rc;
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
use crate::style::CursorStyle;
use crate::theme::StyleThemeExt;
use crate::unit::{Length, LengthAuto, Pct, Pt};
use crate::view::ViewTupleFlat;
use crate::view::{IntoView, View};
use crate::views::{
    ButtonClass, ContainerExt, Decorators, Empty, Label, Stack, StackExt, TabSelectorClass,
    TooltipExt, canvas, dyn_view, svg, tab,
};

use super::FontSize;
use super::{
    ResponsiveSelectors, StructuralSelectors, Style, StyleDebugGroupInfo, StyleKey, StyleKeyInfo,
    StylePropRef, Transition,
};

pub struct ContextValue<T> {
    pub(crate) eval: Rc<dyn Fn(&Style) -> T>,
}

impl<T> Clone for ContextValue<T> {
    fn clone(&self) -> Self {
        Self {
            eval: self.eval.clone(),
        }
    }
}

impl<T> std::fmt::Debug for ContextValue<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ContextValue(..)")
    }
}

impl<T> PartialEq for ContextValue<T> {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.eval, &other.eval)
    }
}

impl<T> Eq for ContextValue<T> {}

impl<T> ContextValue<T> {
    pub(crate) fn new(eval: impl Fn(&Style) -> T + 'static) -> Self {
        Self {
            eval: Rc::new(eval),
        }
    }

    pub fn resolve(&self, style: &Style) -> T {
        let saved_effect = floem_reactive::Runtime::get_current_effect();
        if let Some(effect) = &style.effect_context {
            floem_reactive::Runtime::set_current_effect(Some(effect.clone()));
        }
        // todo use context
        let result = (self.eval)(style);
        floem_reactive::Runtime::set_current_effect(saved_effect);
        result
    }

    pub fn map<U>(self, f: impl Fn(T) -> U + 'static) -> ContextValue<U>
    where
        T: 'static,
    {
        let eval = self.eval;
        ContextValue::new(move |style| f(eval(style)))
    }
}

pub trait StylePropValue: Clone + PartialEq + Debug {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        None
    }

    fn interpolate(&self, _other: &Self, _value: f64) -> Option<Self> {
        None
    }

    /// Compute a content-based hash for this value.
    ///
    /// This hash is used for style caching - identical values should produce
    /// identical hashes. The default implementation uses the Debug representation,
    /// which works for most types. Types that implement Hash can override this
    /// for better performance.
    fn content_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = rustc_hash::FxHasher::default();
        // Use Debug representation as a stable string for hashing
        let debug_str = format!("{:?}", self);
        debug_str.hash(&mut hasher);
        hasher.finish()
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

/// How the content of a replaced element, such as an img, should be resized to fit its container.
/// Corresponds to the CSS `object-fit` property.
/// See <https://developer.mozilla.org/en-US/docs/Web/CSS/object-fit>.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ObjectFit {
    /// The replaced content is sized to fill the element's content box.
    /// The entire object will completely fill the box.
    /// If the object's aspect ratio does not match the aspect ratio of its box,
    /// then the object will be stretched to fit.
    #[default]
    Fill,
    /// The replaced content is scaled to maintain its aspect ratio while fitting
    /// within the element's content box. The entire object is made to fill the box,
    /// while preserving its aspect ratio, so the object will be "letterboxed"
    /// if its aspect ratio does not match the aspect ratio of the box.
    Contain,
    /// The content is sized to maintain its aspect ratio while filling the element's
    /// entire content box. If the object's aspect ratio does not match the aspect
    /// ratio of its box, then the object will be clipped to fit.
    Cover,
    /// The content is sized as if none or contain were specified, whichever would
    /// result in a smaller concrete object size.
    ScaleDown,
    /// The replaced content is not resized.
    None,
}

impl StylePropValue for ObjectFit {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        use peniko::kurbo::RoundedRect;

        let object_fit = *self;
        let container_color = RwSignal::new(palette::css::GRAY);
        let image_color = RwSignal::new(palette::css::BLUE);

        // Visual preview showing how an image with 2:1 aspect ratio fits in a square container
        let preview = canvas(move |cx, size| {
            let width = size.width;
            let height = size.height;
            let padding = 4.0;
            let container_size = width.min(height) - padding * 2.0;

            // Draw container box (square)
            let container_x = (width - container_size) / 2.0;
            let container_y = (height - container_size) / 2.0;
            let container_rect = RoundedRect::from_rect(
                kurbo::Rect::new(
                    container_x,
                    container_y,
                    container_x + container_size,
                    container_y + container_size,
                ),
                2.0,
            );
            cx.stroke(
                &container_rect,
                container_color.get(),
                &Stroke {
                    width: 1.5,
                    ..Default::default()
                },
            );

            // Simulate an image with 2:1 aspect ratio (wider than tall)
            let image_aspect = 2.0;
            let (img_width, img_height) = match object_fit {
                ObjectFit::Fill => {
                    // Stretch to fill container
                    (container_size, container_size)
                }
                ObjectFit::Contain => {
                    // Fit inside while maintaining aspect ratio
                    // Image is 2:1, container is 1:1, so width is the constraint
                    let w = container_size;
                    let h = w / image_aspect;
                    (w, h)
                }
                ObjectFit::Cover => {
                    // Cover entire container while maintaining aspect ratio
                    // Height is the constraint
                    let h = container_size;
                    let w = h * image_aspect;
                    (w, h)
                }
                ObjectFit::ScaleDown => {
                    // Like contain but don't scale up
                    // Assume natural image size is smaller than container
                    let natural_w = container_size * 0.6;
                    let natural_h = natural_w / image_aspect;
                    (natural_w, natural_h)
                }
                ObjectFit::None => {
                    // Natural size (simulated as 60% of container)
                    let natural_w = container_size * 0.6;
                    let natural_h = natural_w / image_aspect;
                    (natural_w, natural_h)
                }
            };

            // Center the image in the container
            let img_x = container_x + (container_size - img_width) / 2.0;
            let img_y = container_y + (container_size - img_height) / 2.0;

            // Clip to container bounds for Cover mode
            if matches!(object_fit, ObjectFit::Cover) {
                // Draw the image rect (it will extend beyond container)
                let img_rect = RoundedRect::from_rect(
                    kurbo::Rect::new(img_x, img_y, img_x + img_width, img_y + img_height),
                    2.0,
                );
                // Show it as semi-transparent to indicate it's clipped
                let clipped_color = image_color.get().with_alpha(0.7);
                cx.fill(&img_rect, clipped_color, 0.0);
            } else {
                // Draw the image rect normally
                let img_rect = RoundedRect::from_rect(
                    kurbo::Rect::new(img_x, img_y, img_x + img_width, img_y + img_height),
                    2.0,
                );
                cx.fill(&img_rect, image_color.get(), 0.0);
            }
        })
        .style(|s| s.width(70.0).height(70.0))
        .container()
        .style(move |s| {
            s.padding(4.0)
                .border(1.)
                .border_radius(5.0)
                .with_theme(move |s, t| s.border_color(t.border()))
        });

        let label_text = match object_fit {
            ObjectFit::Fill => "Fill",
            ObjectFit::Contain => "Contain",
            ObjectFit::Cover => "Cover",
            ObjectFit::ScaleDown => "ScaleDown",
            ObjectFit::None => "None",
        };

        let tooltip_view = move || {
            let description = match object_fit {
                ObjectFit::Fill => "Stretches content to fill the box.\nMay distort aspect ratio.",
                ObjectFit::Contain => {
                    "Scales content to fit inside the box.\nPreserves aspect ratio (letterboxed)."
                }
                ObjectFit::Cover => {
                    "Scales content to cover the box.\nPreserves aspect ratio (may clip)."
                }
                ObjectFit::ScaleDown => {
                    "Like 'contain' but won't scale up.\nNever larger than natural size."
                }
                ObjectFit::None => {
                    "Content keeps its natural size.\nMay overflow or be smaller than box."
                }
            };

            Stack::vertical((
                Label::new(label_text).style(|s| s.font_bold()),
                Label::new(description).style(|s| s.with_theme(|s, t| s.color(t.text_muted()))),
            ))
            .style(|s| s.gap(8.0).padding(12.0).max_width(220.0))
        };

        Some(
            preview
                .tooltip(tooltip_view)
                .style(|s| s.items_center())
                .into_any(),
        )
    }
}

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
                .text_align(parley::Alignment::Center)
                .border(1.)
                .border_radius(5.0)
                .margin_left(6.0)
                .with_theme(|s, t| s.color(t.text()).border_color(t.border()))
                .with::<FontSize>(|s, fs| s.font_size(fs.def(|fs| fs * 0.85)))
        });

        // Clone items for the tooltip view
        let items = self.clone();

        let tooltip_view = move || {
            Stack::vertical_from_iter(items.iter().enumerate().map(|(i, item)| {
                let index_label = Label::new(format!("[{}]", i))
                    .style(|s| s.with_theme(|s, t| s.color(t.text_muted())));

                let item_view = item.debug_view().unwrap_or_else(|| {
                    Label::new(format!("{:?}", item))
                        .style(|s| s.flex_grow(1.0))
                        .into_any()
                });

                Stack::new((index_label, item_view))
                    .style(|s| s.items_center().gap(8.0).padding(4.0))
            }))
            .style(|s| s.gap(4.0))
        };

        // Return the tooltip view wrapped in the preview
        Some(
            Stack::new((preview, tooltip_view()))
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
impl StylePropValue for FontWeight {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        let clone = *self;
        Some(
            format!("{clone:?}")
                .style(move |s| s.font_weight(clone))
                .into_any(),
        )
    }
    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        self.value()
            .interpolate(&other.value(), value)
            .map(FontWeight::new)
    }
}
impl StylePropValue for crate::text::FontStyle {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        let clone = *self;
        Some(
            format!("{clone:?}")
                .style(move |s| s.font_style(clone))
                .into_any(),
        )
    }
}
impl StylePropValue for crate::text::Alignment {}
impl StylePropValue for LineHeightValue {
    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        match (self, other) {
            (LineHeightValue::Normal(v1), LineHeightValue::Normal(v2)) => {
                v1.interpolate(v2, value).map(LineHeightValue::Normal)
            }
            (LineHeightValue::Pt(v1), LineHeightValue::Pt(v2)) => {
                v1.interpolate(v2, value).map(LineHeightValue::Pt)
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
                .with::<FontSize>(|s, fs| s.font_size(fs.def(|fs| fs * 0.85)))
        });

        let items = self.clone();
        let tooltip_view = move || {
            Stack::vertical_from_iter(items.iter().enumerate().map(|(i, item)| {
                let index_label = Label::new(format!("[{}]", i))
                    .style(|s| s.with_theme(|s, t| s.color(t.text_muted())));

                let item_view = item.debug_view().unwrap_or_else(|| {
                    Label::new(format!("{:?}", item))
                        .style(|s| s.flex_grow(1.0))
                        .into_any()
                });

                Stack::new((index_label, item_view))
                    .style(|s| s.items_center().gap(8.0).padding(4.0))
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
impl StylePropValue for Pt {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        Some(Label::new(format!("{} pt", self.0)).into_any())
    }
    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        self.0.interpolate(&other.0, value).map(Pt)
    }
}
#[allow(deprecated)]
impl StylePropValue for super::unit::Px {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        Pt(self.0).debug_view()
    }

    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        self.0.interpolate(&other.0, value).map(super::unit::Px)
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
impl StylePropValue for LengthAuto {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        let label = match self {
            Self::Pt(v) => format!("{v} pt"),
            Self::Pct(v) => format!("{v}%"),
            Self::Em(v) => format!("{v} em"),
            Self::Lh(v) => format!("{v} lh"),
            Self::Auto => "auto".to_string(),
        };
        Some(Label::new(label).into_any())
    }
    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        match (self, other) {
            (Self::Pt(v1), Self::Pt(v2)) => Some(Self::Pt(v1 + (v2 - v1) * value)),
            (Self::Pct(v1), Self::Pct(v2)) => Some(Self::Pct(v1 + (v2 - v1) * value)),
            (Self::Em(v1), Self::Em(v2)) => Some(Self::Em(v1 + (v2 - v1) * value)),
            (Self::Lh(v1), Self::Lh(v2)) => Some(Self::Lh(v1 + (v2 - v1) * value)),
            (Self::Auto, Self::Auto) => Some(Self::Auto),
            // TODO: Figure out some way to get in the relevant layout information in order to interpolate between pixels and percent
            _ => None,
        }
    }
}
#[allow(deprecated)]
impl StylePropValue for super::unit::PxPctAuto {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        LengthAuto::from(*self).debug_view()
    }

    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        LengthAuto::from(*self)
            .interpolate(&LengthAuto::from(*other), value)
            .map(Into::into)
    }
}
impl StylePropValue for Length {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        let label = match self {
            Self::Pt(v) => format!("{v} pt"),
            Self::Pct(v) => format!("{v}%"),
            Self::Em(v) => format!("{v} em"),
            Self::Lh(v) => format!("{v} lh"),
        };
        Some(Label::new(label).into_any())
    }

    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        match (self, other) {
            (Self::Pt(v1), Self::Pt(v2)) => Some(Self::Pt(v1 + (v2 - v1) * value)),
            (Self::Pct(v1), Self::Pct(v2)) => Some(Self::Pct(v1 + (v2 - v1) * value)),
            (Self::Em(v1), Self::Em(v2)) => Some(Self::Em(v1 + (v2 - v1) * value)),
            (Self::Lh(v1), Self::Lh(v2)) => Some(Self::Lh(v1 + (v2 - v1) * value)),
            // TODO: Figure out some way to get in the relevant layout information in order to interpolate between pixels and percent
            _ => None,
        }
    }
}
#[allow(deprecated)]
impl StylePropValue for super::unit::PxPct {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        Length::from(*self).debug_view()
    }

    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        Length::from(*self)
            .interpolate(&Length::from(*other), value)
            .map(Into::into)
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
            Stack::new((Label::new(format!("{self:?}")), color))
                .style(|s| s.items_center())
                .into_any(),
        )
    }

    fn interpolate(&self, _other: &Self, _value: f64) -> Option<Self> {
        None
    }
}

// this is a convenience wrapper so border/outline setters can accept numeric widths.
#[derive(Clone, Debug, Default)]
pub struct StrokeWrap(pub Stroke);
impl StrokeWrap {
    pub fn new(width: f64) -> Self {
        Self(Stroke::new(width))
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

impl StylePropValue for Stroke {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        let stroke = self.clone();
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
            s.with_theme(move |s, t| s.border_color(t.border()))
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

            Stack::vertical_from_iter(rows).style(|s| {
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

impl StylePropValue for super::Angle {
    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        Some(self.lerp(other, value))
    }
}

impl StylePropValue for super::AnchorAbout {
    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        Some(Self {
            x: self.x + (other.x - self.x) * value,
            y: self.y + (other.y - self.y) * value,
        })
    }
}

impl StylePropValue for kurbo::Rect {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        let r = *self;

        let w = r.x1 - r.x0;
        let h = r.y1 - r.y0;

        let coords = [
            format!("x0: {:.2}", r.x0),
            format!("y0: {:.2}", r.y0),
            format!("x1: {:.2}", r.x1),
            format!("y1: {:.2}", r.y1),
        ]
        .v_stack();

        let wh = [format!("w: {:.2}", w), format!("h: {:.2}", h)].h_stack();

        let preview = Empty::new().style(move |s| {
            let max = w.abs().max(h.abs()).max(1.0);
            let scale = 60.0 / max;

            s.width(w.abs() * scale)
                .height(h.abs() * scale)
                .border(1.0)
                .with_theme(|s, t| {
                    s.border_color(t.border())
                        .background(t.primary_muted())
                        .border_radius(t.border_radius())
                })
        });

        Some(
            (
                "Rect",
                preview,
                coords.style(|s| s.gap(2)),
                wh.style(|s| s.gap(8)),
            )
                .v_stack()
                .into_any(),
        )
    }

    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        let lerp = |a: f64, b: f64| a + (b - a) * value;

        Some(Self {
            x0: lerp(self.x0, other.x0),
            y0: lerp(self.y0, other.y0),
            x1: lerp(self.x1, other.x1),
            y1: lerp(self.y1, other.y1),
        })
    }
}

impl StylePropValue for Affine {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        let affine = *self;
        let coeffs = affine.as_coeffs();

        // Decompose to show meaningful transform components
        let (scale, rotation) = affine.svd();
        let translation = affine.translation();

        // Create a visual preview showing the transform effect
        let preview = canvas(move |cx, size| {
            let center = Point::new(size.width / 2., size.height / 2.);
            let box_size = 20.0;

            // Draw original position (dashed outline)
            let original_rect =
                kurbo::Rect::from_center_size(center, kurbo::Size::new(box_size, box_size));
            cx.stroke(
                &original_rect,
                palette::css::GRAY.with_alpha(0.5),
                &kurbo::Stroke::new(1.0).with_dashes(0., [3., 3.]),
            );

            // Draw transformed position
            let transform_offset =
                Affine::translate((center.x - box_size / 2., center.y - box_size / 2.));
            let display_transform = transform_offset * affine * transform_offset.inverse();

            let transformed_rect = kurbo::Rect::new(0., 0., box_size, box_size);
            cx.fill(
                &display_transform.transform_rect_bbox(transformed_rect),
                palette::css::BLUE.with_alpha(0.7),
                0.,
            );
            cx.stroke(
                &(display_transform.transform_rect_bbox(transformed_rect)),
                palette::css::BLUE,
                &kurbo::Stroke::new(2.0),
            );

            // Draw origin point
            let origin_marker = kurbo::Circle::new(display_transform * Point::ZERO, 3.0);
            cx.fill(&origin_marker, palette::css::RED, 0.);
        })
        .style(|s| s.width(80.0).height(60.0))
        .container()
        .style(|s| {
            s.padding(4.0)
                .border(1.)
                .border_radius(5.0)
                .with_theme(|s, t| s.border_color(t.border()))
        });

        let tooltip_view = move || {
            // Matrix coefficients in a grid
            let matrix_label = Label::new("Matrix:").style(|s| s.font_bold().margin_bottom(8.0));

            let matrix_grid = (
                views((
                    Label::new(format!("{:.3}", coeffs[0])),
                    Label::new(format!("{:.3}", coeffs[2])),
                    Label::new(format!("{:.3}", coeffs[4])),
                )),
                views((
                    Label::new(format!("{:.3}", coeffs[1])),
                    Label::new(format!("{:.3}", coeffs[3])),
                    Label::new(format!("{:.3}", coeffs[5])),
                )),
                views((Label::new("0"), Label::new("0"), Label::new("1"))),
            )
                .v_stack()
                .style(|s| {
                    s.gap(4.0)
                        .padding(8.0)
                        .border(1.)
                        .border_radius(4.0)
                        .with_theme(|s, t| {
                            s.background(t.def(|t| t.primary().with_alpha(0.5)))
                                .border_color(t.border())
                        })
                });

            // Decomposed components
            let components_label = Label::new("Components:")
                .style(|s| s.font_bold().margin_top(16.0).margin_bottom(8.0));

            let translate_row = views((
                "Translate:".style(|s| s.font_bold().min_width(100.0).justify_end()),
                Label::derived(move || format!("({:.2}, {:.2})", translation.x, translation.y)),
            ));

            let rotate_row = views((
                "Rotate:".style(|s| s.font_bold().min_width(100.0).justify_end()),
                Label::derived(move || format!("{:.1}°", rotation.to_degrees())),
            ));

            let scale_row = views((
                "Scale:".style(|s| s.font_bold().min_width(100.0).justify_end()),
                Label::derived(move || format!("({:.2}, {:.2})", scale.x, scale.y)),
            ));

            // Check for special properties
            let is_identity = affine == Affine::IDENTITY;
            let determinant = coeffs[0] * coeffs[3] - coeffs[1] * coeffs[2];
            let has_reflection = determinant < 0.0;

            let properties = if is_identity {
                Some(
                    Label::new("Identity (no transform)")
                        .style(|s| s.with_theme(|s, t| s.color(t.text_muted()))),
                )
            } else if has_reflection {
                Some(
                    Label::new("⚠ Contains reflection")
                        .style(|s| s.with_theme(|s, t| s.color(t.warning()))),
                )
            } else {
                None
            };

            let components_grid = (translate_row, rotate_row, scale_row).flatten().style(|s| {
                s.grid()
                    .grid_template_columns([auto(), fr(1.)])
                    .justify_center()
                    .items_center()
                    .row_gap(8)
                    .col_gap(10)
            });

            let mut content = vec![
                matrix_label.into_any(),
                matrix_grid.into_any(),
                components_label.into_any(),
                components_grid.into_any(),
            ];

            if let Some(props) = properties {
                content.push(props.into_any());
            }

            Stack::vertical_from_iter(content).style(|s| s.padding(20))
        };

        Some(
            preview
                .tooltip(tooltip_view)
                .style(|s| s.items_center())
                .into_any(),
        )
    }

    fn interpolate(&self, other: &Self, t: f64) -> Option<Self> {
        Some(self.lerp(other, t))
    }

    fn content_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = rustc_hash::FxHasher::default();

        let coeffs = self.as_coeffs();
        for coeff in coeffs {
            coeff.to_bits().hash(&mut hasher);
        }

        hasher.finish()
    }
}

pub trait AffineLerp {
    fn svd(self) -> (Vec2, f64);

    /// Linearly interpolate between two affine transforms.
    ///
    /// This implements the CSS Transforms interpolation algorithm:
    /// - Decompose both transforms into translation, rotation, and scale components
    /// - Interpolate each component separately
    /// - Recompose the result
    ///
    /// `t` should be in the range [0.0, 1.0] where:
    /// - t = 0.0 returns `self`
    /// - t = 1.0 returns `other`
    /// - t = 0.5 returns the midpoint
    fn lerp(&self, other: &Affine, t: f64) -> Affine;
}

impl AffineLerp for Affine {
    fn svd(self) -> (Vec2, f64) {
        let [a, b, c, d, _, _] = self.as_coeffs();
        let a2 = a * a;
        let b2 = b * b;
        let c2 = c * c;
        let d2 = d * d;
        let ab = a * b;
        let cd = c * d;
        let angle = 0.5 * (2.0 * (ab + cd)).atan2(a2 - b2 + c2 - d2);
        let s1 = a2 + b2 + c2 + d2;
        let s2 = ((a2 - b2 + c2 - d2).powi(2) + 4.0 * (ab + cd).powi(2)).sqrt();
        (
            Vec2 {
                x: (0.5 * (s1 + s2)).sqrt(),
                y: (0.5 * (s1 - s2)).sqrt(),
            },
            angle,
        )
    }

    fn lerp(&self, other: &Affine, t: f64) -> Affine {
        // Extract translations
        let trans_a = self.translation();
        let trans_b = other.translation();

        // Remove translations to get the linear parts
        let linear_a = self.with_translation(Vec2::ZERO);
        let linear_b = other.with_translation(Vec2::ZERO);

        // Decompose into scale and rotation using SVD
        let (scale_a, rotation_a) = linear_a.svd();
        let (scale_b, rotation_b) = linear_b.svd();

        // Interpolate translation
        let trans = Vec2 {
            x: trans_a.x + (trans_b.x - trans_a.x) * t,
            y: trans_a.y + (trans_b.y - trans_a.y) * t,
        };

        // Interpolate scale
        let scale = Vec2 {
            x: scale_a.x + (scale_b.x - scale_a.x) * t,
            y: scale_a.y + (scale_b.y - scale_a.y) * t,
        };

        // Interpolate rotation (taking the shorter path)
        let mut angle_diff = rotation_b - rotation_a;
        // Normalize to [-π, π] to take the shorter rotation path
        while angle_diff > std::f64::consts::PI {
            angle_diff -= 2.0 * std::f64::consts::PI;
        }
        while angle_diff < -std::f64::consts::PI {
            angle_diff += 2.0 * std::f64::consts::PI;
        }
        let rotation = rotation_a + angle_diff * t;

        // Recompose: rotate -> scale -> translate
        Affine::rotate(rotation)
            .then_scale_non_uniform(scale.x, scale.y)
            .then_translate(trans)
    }
}

#[cfg(test)]
mod affine_lerp_tests {
    use super::*;

    #[test]
    fn test_lerp_identity() {
        let a = Affine::IDENTITY;
        let b = Affine::translate(Vec2::new(100.0, 50.0));

        let result = a.lerp(&b, 0.0);
        assert_eq!(result.as_coeffs(), a.as_coeffs());

        let result = a.lerp(&b, 1.0);
        assert_eq!(result.as_coeffs(), b.as_coeffs());
    }

    #[test]
    fn test_lerp_translation() {
        let a = Affine::translate(Vec2::new(0.0, 0.0));
        let b = Affine::translate(Vec2::new(100.0, 50.0));

        let result = a.lerp(&b, 0.5);
        let trans = result.translation();
        assert!((trans.x - 50.0).abs() < 1e-10);
        assert!((trans.y - 25.0).abs() < 1e-10);
    }

    #[test]
    fn test_lerp_rotation() {
        let a = Affine::rotate(0.0);
        let b = Affine::rotate(std::f64::consts::PI / 2.0);

        let result = a.lerp(&b, 0.5);
        // Should be rotated by π/4
        let point = result * Point::new(1.0, 0.0);
        let expected_angle = std::f64::consts::PI / 4.0;
        assert!((point.x - expected_angle.cos()).abs() < 1e-10);
        assert!((point.y - expected_angle.sin()).abs() < 1e-10);
    }

    #[test]
    fn test_lerp_scale() {
        let a = Affine::scale(1.0);
        let b = Affine::scale(2.0);

        let result = a.lerp(&b, 0.5);
        let point = result * Point::new(1.0, 1.0);
        assert!((point.x - 1.5).abs() < 1e-10);
        assert!((point.y - 1.5).abs() < 1e-10);
    }
}

/// Internal storage for style property values in the style map.
///
/// Unlike `StyleValue<T>` which is used in the public API, `StyleMapValue<T>`
/// is the internal representation stored in the style hashmap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StyleMapValue<T> {
    /// Value inserted by animation interpolation
    Animated(T),
    /// Value set directly
    Val(T),
    /// Value resolved from inherited context when the property is read.
    Context(ContextValue<T>),
    /// Use the default value for the style, typically from the underlying `ComputedStyle`
    Unset,
}

/// The value for a [`Style`] property in the public API.
///
/// This represents the result of reading a style property, with additional
/// states like `Base` that indicate inheritance from parent styles.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum StyleValue<T> {
    /// Value resolved from inherited context when the property is read.
    Context(ContextValue<T>),
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

impl<T: 'static> StyleValue<T> {
    pub fn map<U>(self, f: impl Fn(T) -> U + 'static) -> StyleValue<U> {
        match self {
            Self::Context(x) => StyleValue::Context(x.map(f)),
            Self::Val(x) => StyleValue::Val(f(x)),
            Self::Animated(x) => StyleValue::Animated(f(x)),
            Self::Unset => StyleValue::Unset,
            Self::Base => StyleValue::Base,
        }
    }

    pub fn unwrap_or(self, default: T) -> T {
        match self {
            Self::Context(_) => default,
            Self::Val(x) => x,
            Self::Animated(x) => x,
            Self::Unset => default,
            Self::Base => default,
        }
    }

    pub fn unwrap_or_else(self, f: impl FnOnce() -> T) -> T {
        match self {
            Self::Context(_) => f(),
            Self::Val(x) => x,
            Self::Animated(x) => x,
            Self::Unset => f(),
            Self::Base => f(),
        }
    }

    pub fn as_mut(&mut self) -> Option<&mut T> {
        match self {
            Self::Context(_) => None,
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

impl<T> From<ContextValue<T>> for StyleValue<T> {
    fn from(x: ContextValue<T>) -> Self {
        Self::Context(x)
    }
}

fn short_style_name(name: &str) -> String {
    name.strip_prefix("floem::style::")
        .unwrap_or(name)
        .to_string()
}

struct StyleDebugRow {
    render: Rc<dyn Fn(bool) -> AnyView>,
    is_empty: bool,
}

fn effective_inherited_debug_groups(
    style: &Style,
    parent_groups: &HashSet<StyleKey>,
) -> HashSet<StyleKey> {
    let mut groups = parent_groups.clone();
    for key in style.map.keys() {
        if let StyleKeyInfo::DebugGroup(info) = key.info {
            if style.debug_group_enabled(*key) {
                if info.inherited {
                    groups.insert(*key);
                }
            } else {
                groups.remove(key);
            }
        }
    }
    groups
}

fn style_debug_active_groups(
    style: &Style,
    inherited_groups: &HashSet<StyleKey>,
) -> Vec<&'static StyleDebugGroupInfo> {
    let mut groups = style
        .map
        .keys()
        .filter_map(|key| match key.info {
            StyleKeyInfo::DebugGroup(info) if style.debug_group_enabled(*key) => Some(info),
            _ => None,
        })
        .collect::<Vec<_>>();

    for key in inherited_groups {
        if let StyleKeyInfo::DebugGroup(info) = key.info
            && !style.map.contains_key(key)
        {
            groups.push(info);
        }
    }

    groups.sort_unstable_by_key(|info| short_style_name((info.name)()));
    groups.dedup_by_key(|info| (info.name)());
    groups
}

fn style_debug_is_empty(style: &Style, inherited_groups: &HashSet<StyleKey>) -> bool {
    let mut hidden_props = HashSet::new();

    for info in style_debug_active_groups(style, inherited_groups) {
        let members = (info.member_props)();
        let present = members
            .iter()
            .copied()
            .filter(|key| style.map.contains_key(key) && !hidden_props.contains(key))
            .collect::<Vec<_>>();
        if !present.is_empty() {
            hidden_props.extend(present);
        }
    }

    if style
        .map
        .iter()
        .any(|(key, _)| matches!(key.info, StyleKeyInfo::Prop(..)) && !hidden_props.contains(key))
    {
        return false;
    }

    if style.map.iter().any(|(key, value)| match key.info {
        StyleKeyInfo::Selector(..) | StyleKeyInfo::Class(..) => {
            value.downcast_ref::<Style>().is_some_and(|nested| {
                !style_debug_is_empty(
                    nested,
                    &effective_inherited_debug_groups(nested, inherited_groups),
                )
            })
        }
        _ => false,
    }) {
        return false;
    }

    for value in style.map.values() {
        if let Some(rules) = value.downcast_ref::<StructuralSelectors>()
            && rules.0.iter().any(|(_, nested)| {
                !style_debug_is_empty(
                    nested,
                    &effective_inherited_debug_groups(nested, inherited_groups),
                )
            })
        {
            return false;
        }
        if let Some(rules) = value.downcast_ref::<ResponsiveSelectors>()
            && rules.0.iter().any(|(_, nested)| {
                !style_debug_is_empty(
                    nested,
                    &effective_inherited_debug_groups(nested, inherited_groups),
                )
            })
        {
            return false;
        }
    }

    true
}

fn debug_name_cell(name: String, is_direct: bool, indent: usize) -> AnyView {
    let indent = (indent as f64) * 16.0;
    let name = if is_direct {
        Label::new(name).into_any()
    } else {
        Stack::new((
            "Inherited".style(|s| {
                s.margin_right(5.0)
                    .border(1.)
                    .border_radius(5.0)
                    .with_theme(|s, t| {
                        s.color(t.text_muted())
                            .border_color(t.border())
                            .apply(Style::new().padding_horiz(4.0))
                    })
                    .with::<FontSize>(|s, fs| s.font_size(fs.def(|fs| fs * 0.8)))
            }),
            Label::new(name),
        ))
        .style(|s| s.items_center().gap(6.0))
        .into_any()
    };

    name.container()
        .style(move |s| {
            s.padding_left(indent)
                .min_width(170.)
                .padding_right(5.0)
                .flex_direction(FlexDirection::RowReverse)
        })
        .into_any()
}

fn style_debug_prop_row(
    style: &Style,
    prop: StylePropRef,
    value: &Rc<dyn std::any::Any>,
    is_direct: bool,
    indent: usize,
) -> StyleDebugRow {
    let style = style.clone();
    let value = value.clone();
    let name = short_style_name(&format!("{:?}", prop.key));
    StyleDebugRow {
        render: Rc::new(move |_| {
            let mut value_view = (prop.info().debug_view)(&*value)
                .unwrap_or_else(|| Label::new((prop.info().debug_any)(&*value)).into_any());

            if let Some(transition) = style
                .map
                .get(&prop.info().transition_key)
                .and_then(|v| v.downcast_ref::<Transition>())
            {
                value_view = Stack::vertical((
                    value_view,
                    Stack::new((
                        "Transition".style(|s| {
                            s.margin_top(4.0)
                                .margin_right(5.0)
                                .border(1.)
                                .border_radius(5.0)
                                .padding_horiz(4.0)
                                .with_theme(|s, t| s.color(t.text_muted()).border_color(t.border()))
                                .with::<FontSize>(|s, fs| s.font_size(fs.def(|fs| fs * 0.8)))
                        }),
                        transition.debug_view(),
                    ))
                    .style(|s| s.items_center().gap(6.0)),
                ))
                .into_any();
            }

            Stack::new((debug_name_cell(name.clone(), is_direct, indent), value_view))
                .style(|s| s.items_center().width_full().padding_vert(4.0).gap(8.0))
                .into_any()
        }),
        is_empty: false,
    }
}

fn style_debug_group_row<V>(
    name: String,
    value_view: V,
    is_direct: bool,
    indent: usize,
) -> StyleDebugRow
where
    V: Fn() -> AnyView + 'static,
{
    StyleDebugRow {
        render: Rc::new(move |_| {
            Stack::new((
                debug_name_cell(name.clone(), is_direct, indent),
                value_view(),
            ))
            .style(|s| s.items_center().width_full().padding_vert(4.0).gap(8.0))
            .into_any()
        }),
        is_empty: false,
    }
}

fn style_debug_section(title: String, child: StyleDebugRow, indent: usize) -> StyleDebugRow {
    let expanded = RwSignal::new(false);
    let title_text = title.clone();
    let child_is_empty = child.is_empty;
    let chevron = move || {
        if expanded.get() {
            svg(
                r#"<svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor"><path d="M4.427 6.427l3.396 3.396a.25.25 0 00.354 0l3.396-3.396A.25.25 0 0011.396 6H4.604a.25.25 0 00-.177.427z"/></svg>"#,
            )
        } else {
            svg(
                r#"<svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor"><path d="M6.427 4.427l3.396 3.396a.25.25 0 010 .354l-3.396 3.396A.25.25 0 016 11.396V4.604a.25.25 0 01.427-.177z"/></svg>"#,
            )
        }
        .style(|s| s.size_full().with_theme(|s, t| s.color(t.text())))
    };

    StyleDebugRow {
        render: Rc::new(move |row_is_base| {
            let child_render = child.render.clone();
            Stack::vertical((
                Stack::new((
                    dyn_view(chevron)
                        .class(ButtonClass)
                        .style(|s| s.size(16.0, 16.0).padding(0.)),
                    Label::new(title_text.clone()).style(|s| {
                        s.font_bold()
                            .cursor(CursorStyle::Pointer)
                            .with_theme(|s, t| s.color(t.primary()))
                    }),
                    Label::new("empty")
                        .style(|s| {
                            s.padding_horiz(6.0)
                                .border(1.)
                                .border_radius(999.0)
                                .with_theme(|s, t| s.color(t.text_muted()).border_color(t.border()))
                                .with::<FontSize>(|s, fs| s.font_size(fs.def(|fs| fs * 0.75)))
                        })
                        .style(move |s| s.apply_if(!child_is_empty, |s| s.hide())),
                ))
                .style(move |s| {
                    s.items_center()
                        .gap(6.0)
                        .padding_left((indent as f64) * 16.0)
                        .cursor(super::CursorStyle::Pointer)
                })
                .on_event_stop(crate::event::listener::Click, move |_cx, _event| {
                    expanded.update(|value| *value = !*value)
                }),
                dyn_view(move || {
                    if expanded.get() {
                        child_render(!row_is_base)
                            .style(|s| s.padding_left(12.0))
                            .into_any()
                    } else {
                        Empty::new().into_any()
                    }
                })
                .into_any(),
            ))
            .style(|s| s.gap(6.0).width_full().padding_vert(4.0))
            .into_any()
        }),
        is_empty: false,
    }
}

fn style_debug_sections(
    title: &str,
    children: Vec<StyleDebugRow>,
    indent: usize,
) -> Option<StyleDebugRow> {
    if children.is_empty() {
        return None;
    }

    Some(style_debug_section(
        title.to_string(),
        StyleDebugRow {
            render: Rc::new(move |start_with_base| style_debug_rows(&children, start_with_base)),
            is_empty: false,
        },
        indent,
    ))
}

fn style_debug_style_section(
    title: String,
    style: &Style,
    inherited_groups: &HashSet<StyleKey>,
    indent: usize,
) -> StyleDebugRow {
    let nested_inherited = effective_inherited_debug_groups(style, inherited_groups);
    style_debug_section(
        title,
        style_debug_body(style, None, &nested_inherited, indent + 1),
        indent,
    )
}

fn style_debug_prop_rows(
    style: &Style,
    direct_keys: Option<&HashSet<StyleKey>>,
    inherited_groups: &HashSet<StyleKey>,
    indent: usize,
) -> Vec<StyleDebugRow> {
    let mut rows: Vec<StyleDebugRow> = Vec::new();
    let mut hidden_props = HashSet::new();

    for info in style_debug_active_groups(style, inherited_groups) {
        let members = (info.member_props)();
        let present = members
            .iter()
            .copied()
            .filter(|key| style.map.contains_key(key) && !hidden_props.contains(key))
            .collect::<Vec<_>>();
        if present.is_empty() {
            continue;
        }
        hidden_props.extend(present);
        if (info.debug_view)(style).is_some() {
            let info = info.clone();
            let style = style.clone();
            rows.push(style_debug_group_row(
                short_style_name((info.name)()),
                move || {
                    (info.debug_view)(&style)
                        .unwrap_or_else(|| Label::new("empty").into_any())
                        .into_any()
                },
                true,
                indent,
            ));
        }
    }

    let mut props = style
        .map
        .iter()
        .filter_map(|(key, value)| match key.info {
            StyleKeyInfo::Prop(..) if !hidden_props.contains(key) => {
                Some((StylePropRef { key: *key }, value))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    props.sort_unstable_by_key(|(prop, _)| short_style_name(&format!("{:?}", prop.key)));

    for (prop, value) in props {
        let is_direct = direct_keys
            .as_ref()
            .is_none_or(|keys| keys.contains(&prop.key));
        rows.push(style_debug_prop_row(style, prop, value, is_direct, indent));
    }

    rows
}

fn style_debug_selector_rows(
    style: &Style,
    inherited_groups: &HashSet<StyleKey>,
    indent: usize,
) -> Vec<StyleDebugRow> {
    let mut selector_rows: Vec<StyleDebugRow> = Vec::new();
    let mut selectors = style
        .map
        .iter()
        .filter_map(|(key, value)| match key.info {
            StyleKeyInfo::Selector(selector) => Some((selector.debug_string(), value)),
            _ => None,
        })
        .collect::<Vec<_>>();
    selectors.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    for (name, value) in selectors {
        if let Some(nested_style) = value.downcast_ref::<Style>() {
            selector_rows.push(style_debug_style_section(
                name,
                nested_style,
                inherited_groups,
                indent,
            ));
        }
    }

    for value in style.map.values() {
        if let Some(rules) = value.downcast_ref::<StructuralSelectors>() {
            for (selector, nested_style) in &rules.0 {
                selector_rows.push(style_debug_style_section(
                    format!("Structural: {selector:?}"),
                    nested_style,
                    inherited_groups,
                    indent,
                ));
            }
        }
        if let Some(rules) = value.downcast_ref::<ResponsiveSelectors>() {
            for (selector, nested_style) in &rules.0 {
                selector_rows.push(style_debug_style_section(
                    format!("Responsive: {selector:?}"),
                    nested_style,
                    inherited_groups,
                    indent,
                ));
            }
        }
    }

    selector_rows
}

fn style_debug_class_rows(
    style: &Style,
    inherited_groups: &HashSet<StyleKey>,
    indent: usize,
) -> Vec<StyleDebugRow> {
    let mut class_rows: Vec<StyleDebugRow> = Vec::new();
    let mut classes = style
        .map
        .iter()
        .filter_map(|(key, value)| match key.info {
            StyleKeyInfo::Class(info) => Some((short_style_name((info.name)()), value)),
            _ => None,
        })
        .collect::<Vec<_>>();
    classes.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    for (name, value) in classes {
        if let Some(nested_style) = value.downcast_ref::<Style>() {
            class_rows.push(style_debug_style_section(
                name,
                nested_style,
                inherited_groups,
                indent,
            ));
        }
    }
    class_rows
}

fn style_debug_body(
    style: &Style,
    direct_keys: Option<&HashSet<StyleKey>>,
    inherited_groups: &HashSet<StyleKey>,
    indent: usize,
) -> StyleDebugRow {
    let style = style.clone();
    let inherited_groups = inherited_groups.clone();
    let is_empty = style_debug_is_empty(&style, &inherited_groups);
    let direct_keys = direct_keys.cloned();
    StyleDebugRow {
        render: Rc::new(move |start_with_base| {
            let mut rows =
                style_debug_prop_rows(&style, direct_keys.as_ref(), &inherited_groups, indent);
            if let Some(selectors_section) = style_debug_sections(
                "Selectors",
                style_debug_selector_rows(&style, &inherited_groups, indent),
                indent,
            ) {
                rows.push(selectors_section);
            }
            if let Some(classes_section) = style_debug_sections(
                "Classes",
                style_debug_class_rows(&style, &inherited_groups, indent),
                indent,
            ) {
                rows.push(classes_section);
            }

            if rows.is_empty() {
                return Label::new("empty")
                    .style(|s| s.with_theme(|s, t| s.color(t.text_muted())))
                    .into_any();
            }

            style_debug_rows(&rows, start_with_base)
        }),
        is_empty,
    }
}

fn style_debug_rows(rows: &[StyleDebugRow], start_with_base: bool) -> AnyView {
    Stack::vertical_from_iter(rows.iter().enumerate().map(|(idx, row)| {
        let is_base = if start_with_base {
            idx.is_multiple_of(2)
        } else {
            !idx.is_multiple_of(2)
        };
        (row.render)(is_base).style(move |s| {
            s.width_full().padding_horiz(4.0).with_theme(move |s, t| {
                s.apply_if(is_base, |s| s.background(t.bg_base()))
                    .apply_if(!is_base, |s| s.background(t.bg_elevated()))
            })
        })
    }))
    .style(|s| s.gap(4.0).width_full())
    .into_any()
}

impl Style {
    pub fn debug_view(&self, direct_style: Option<&Style>) -> Box<dyn View> {
        let direct_keys =
            direct_style.map(|style| style.map.keys().copied().collect::<HashSet<_>>());
        let style = self.clone();
        let inherited_groups = effective_inherited_debug_groups(&style, &HashSet::new());
        let selected_tab = RwSignal::new(0);
        let tab_item = move |name, index| {
            Label::new(name)
                .class(TabSelectorClass)
                .action(move || selected_tab.set(index))
                .style(move |s| s.set_selected(selected_tab.get() == index))
        };
        let tabs = (
            tab_item("View Style", 0),
            tab_item("Selectors", 1),
            tab_item("Classes", 2),
        )
            .h_stack()
            .style(|s| s.with_theme(|s, t| s.background(t.bg_base())));
        let direct_keys_for_body = direct_keys.clone();
        let style_for_body = style.clone();
        let style_for_selectors = style.clone();
        let style_for_classes = style.clone();
        Stack::vertical((
            tabs,
            tab(
                move || Some(selected_tab.get()),
                move || [0, 1, 2],
                |it| *it,
                move |it| match it {
                    0 => {
                        let rows = style_debug_prop_rows(
                            &style_for_body,
                            direct_keys_for_body.as_ref(),
                            &inherited_groups,
                            0,
                        );
                        if rows.is_empty() {
                            Label::new("empty")
                                .style(|s| s.with_theme(|s, t| s.color(t.text_muted())))
                                .into_any()
                        } else {
                            style_debug_rows(&rows, true)
                        }
                    }
                    1 => {
                        let rows =
                            style_debug_selector_rows(&style_for_selectors, &inherited_groups, 0);
                        if rows.is_empty() {
                            Label::new("empty")
                                .style(|s| s.with_theme(|s, t| s.color(t.text_muted())))
                                .into_any()
                        } else {
                            style_debug_rows(&rows, true)
                        }
                    }
                    2 => {
                        let rows = style_debug_class_rows(&style_for_classes, &inherited_groups, 0);
                        if rows.is_empty() {
                            Label::new("empty")
                                .style(|s| s.with_theme(|s, t| s.color(t.text_muted())))
                                .into_any()
                        } else {
                            style_debug_rows(&rows, true)
                        }
                    }
                    _ => Label::new("empty").into_any(),
                },
            ),
        ))
        .style(|s| s.width_full().gap(6.0))
        .into_any()
    }
}
