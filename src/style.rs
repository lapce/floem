//! # Style
//! Traits and functions that allow for styling `Views`.
//!
//! # The Floem Style System
//!
//! ## The [Style] struct
//!
//! The style system is centered around a [Style] struct.
//! `Style` internally is just a hashmap (although one from the im crate so it is cheap to clone).
//! It maps from a [StyleKey] to `Rc<dyn Any>`.
//!
//! ## The [StyleKey]
//!
//! [StyleKey] holds a static reference (that is used as the hash value) to a [StyleKeyInfo] enum which enumerates the different kinds of values that can be in the map.
//! Which value is in the `StyleKeyInfo` enum is used to know how to downcast the `Rc<dyn Any`.
//!
//! The key types from the [StyleKeyInfo] are: (these are all of the different things that can be added to a [Style]).
//! - Transition,
//! - Prop(StylePropInfo),
//! - Selector(StyleSelectors),
//! - Class(StyleClassInfo),
//! - ContextMappings,
//!
//! Transitions and context mappings don't hold any extra information, they are just used to know how to downcast the `Rc<dyn Any>`.
//!
//! [StyleSelectors] is a bit mask of which selectors are active.
//!
//! [StyleClassInfo] holds a function pointer that returns the name of the class as a String.
//! The function pointer is basically used as a vtable for the class.
//! If classes needed more methods other than `name`, those methods would be added to `StyleClassInfo`.
//!
//! [StylePropInfo] is another vtable, similar to `StyleClassInfo` and holds function pointers for getting the name of a prop, the props interpolation function from the [StylePropValue] trait, the associated transition key for the prop, and others.
//!
//! Props store props.
//! Transitions store transition values.
//! Classes, context mappings, and selectors store nested [Style] maps.
//!
//! ## Applying `Style`s to `View`s
//!
//! A style can be applied to a view in two different ways.
//! A single `Style` can be added to the [view_style](crate::view::View::view_style) method of the view trait or multiple `Style`s can be added by calling [style](crate::views::Decorators::style) on an `IntoView` from the [Decorators](crate::views::Decorators) trait.
//!
//! Calls to `style` from the decorators trait have a higher precedence than the `view_style` method, meaning calls to `style` will override any matching `StyleKeyInfo` that came from the `view_style` method.
//!
//! If you make repeated calls to `style` from the decorators trait, each will be added separately to the `ViewState` that is managed by Floem and associated with the `ViewId` of the view that `style` was called on.
//! The `ViewState` stores a `Stack` of styles and later calls to `style` (and thus larger indicies in the style stack) will take precedence over earlier calls.
//!
//! `style` from the deocrators trait is reactive and the function that returns the style map with be re-run in response to any reactive updates that it depends on.
//! If it gets a reactive update, it will have tracked which index into the style stack it had when it was first called and will overrite that index and only that index so that other calls to `style` are not affected.
//!
//! ## Style Resolution
//!
//! A final `computed_style` is resolved in the `style_pass` of the `View` trait.
//!
//! ### Context
//!
//! It first received a `Style` map that is used as context.
//! The context is passed down the view tree and carries the inherited properties that were applied to any parent.
//! Inherited properties include all classes and any prop that has been marked as `inherited`.
//!
//! ### View Style
//!
//! The `style` first gets the `Style` (if any) from the `view_style` method.
//!
//! ### Style
//!
//! Then it gets the style from any calls to `style` from the decorators trait.
//! It starts with the first index in the style `Stack` and applies each successive `Style` over the combination of any previous ones.
//!
//! Then the style from the `Decorators` / `ViewState` is applied over (overriding any matching props) the style from `view_style`.
//!
//!
//! ### Nested map resolution
//!
//! Then any classes that have been applied to the view, and the active selector set are used to resolve nested maps.
//!
//! Nested maps such as classes and selectors are recursively applied, breadth first. So, deeper / more nested style maps take precendence.
//!
//! This style map is the combined style of the `View`.
//!
//! ### Updated context
//!
//! Finally, the context style is updated using the combined style, applying any style key that is `inherited` to the context so that the children will have acces to them.
//!
//! ## Prop Extraction
//!
//! The final computed style of a view will be passed to the `style_pass` method from the `View` trait.
//!
//! Views will store fields that are struct that are prop extractors.
//! These structs are created using the `prop_extractor!` macro.
//!
//! These structs can then be used from in the `style_pass` to extract props using the `read` (or `read_exact`) methods that are created by the `prop_extractor` macro.
//!
//! The read methods will take in the combined style for that `View` and will automatically extract any matching prop values and transitions for those props.
//!
//! ### Transition interpolation
//!
//! If there is a transition for a prop, the extractor will keep track of the current time and transition state and will set the final extracted value to a properly interpolated value using the state and current time.
//!
//!
//! ## Custom Style Props, Classes, and Extractors.
//!
//!
//! You can create custom style props with the [prop!] macro, classes with the [style_class!] macro, and extractors with the [prop_extractor!] macro.
//!
//!
//! ### Custom Props
//!
//! You can create custom props.
//!
//! Doing this allows you to store arbitrary values in the style system.
//!
//! You can use these to style the view, change it's behavior, update it's state, or anything else.
//!
//! By implementing the [StylePropValue] trait for your prop (which you must do) you can
//!
//! - optionally set how the prop should be interpolated (allowing you to customize what interpolating means in the context of your prop)
//!
//! - optionally provide a `debug_view` for your prop, which debug view will be used in the Floem inspector. This means that you can customize a complex debug experience for your prop with very little effort (and it really can be any arbitrary view. no restrictions.)
//!
//! - optionally add a custom implementation of how a prop should be combined with another prop. This is different from interpolation and is useful when you want to specify how properties should override each other. The default implementation just replaces the old value with a new value, but if you have a prop with multiple optional fields, you might want to only replace the fields that have a `Some` value.
//!
//! ### Custom Classes
//!
//! If you create a custom class, you can apply that class to any view, and when the final style for that view is being resolved, if the style has that class as a nested map, it will be applied, overriding any prviously set values.
//!
//! ### Custom Extractors
//!
//! You can create custom extractors and embed them in your custom views so that you can get out any built in prop, or any of your custom props from the final combined style that is applied to your `View`.

use floem_reactive::{RwSignal, SignalGet, SignalUpdate as _, create_updater};
use floem_renderer::Renderer;
use floem_renderer::text::{LineHeightValue, Weight};
use imbl::hashmap::Entry;
use imbl::shared_ptr::DefaultSharedPtr;
use peniko::color::{HueDirection, palette};
use peniko::kurbo::{self, Point, Stroke};
use peniko::{
    Brush, Color, ColorStop, ColorStops, Gradient, GradientKind, InterpolationAlphaSpace,
    LinearGradientPosition,
};
use rustc_hash::FxHasher;
use smallvec::SmallVec;
use std::any::{Any, type_name};
use std::collections::HashMap;
use std::fmt::{self, Debug};
use std::hash::Hasher;
use std::hash::{BuildHasherDefault, Hash};
use std::ptr;
use std::rc::Rc;
use taffy::GridTemplateComponent;
use taffy::prelude::{auto, fr};

#[cfg(not(target_arch = "wasm32"))]
use std::time::{Duration, Instant};
#[cfg(target_arch = "wasm32")]
use web_time::{Duration, Instant};

pub use taffy::style::{
    AlignContent, AlignItems, BoxSizing, Dimension, Display, FlexDirection, FlexWrap,
    JustifyContent, JustifyItems, Position,
};
use taffy::{
    geometry::{MinMax, Size},
    prelude::{GridPlacement, Line, Rect},
    style::{
        LengthPercentage, MaxTrackSizingFunction, MinTrackSizingFunction, Overflow,
        Style as TaffyStyle,
    },
};

use crate::context::InteractionState;
use crate::prelude::ViewTuple;
use crate::responsive::{ScreenSize, ScreenSizeBp};
use crate::theme::StyleThemeExt;
use crate::unit::{Pct, Px, PxPct, PxPctAuto, UnitExt};
use crate::view::{IntoView, View};
use crate::view_tuple::ViewTupleFlat;
use crate::views::{
    ContainerExt, Decorators, TooltipExt, canvas, empty, h_stack, label, stack, text, v_stack,
    v_stack_from_iter,
};
use crate::{AnyView, easing::*};

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
impl StylePropValue for CursorStyle {}
impl StylePropValue for BoxShadow {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        // Create a preview container that shows a visual representation of the shadow
        let shadow = *self;

        // Shadow preview box
        let shadow_preview = empty()
            .style(move |s| s.width(50.0).height(50.0))
            .container()
            .style(move |s| {
                s.with_theme(|s, t| {
                    s.background(Color::TRANSPARENT)
                        .border_color(t.border())
                        .border(1.)
                        .border_radius(t.border_radius())
                })
                .apply_box_shadows(vec![shadow])
                .margin(10.0)
            });

        // Create a details section showing the shadow properties
        let details_view = move || {
            v_stack((
                h_stack((
                    "Color:".style(|s| s.font_weight(Weight::BOLD).width(80.0)),
                    shadow.color.debug_view().unwrap(),
                ))
                .style(|s| s.items_center().gap(4.0)),
                h_stack((
                    "Blur:".style(|s| s.font_weight(Weight::BOLD).width(80.0)),
                    format!("{:?}", shadow.blur_radius),
                ))
                .style(|s| s.items_center().gap(4.0)),
                h_stack((
                    "Spread:".style(|s| s.font_weight(Weight::BOLD).width(80.0)),
                    format!("{:?}", shadow.spread),
                ))
                .style(|s| s.items_center().gap(4.0)),
                h_stack((
                    "Offset:".style(|s| s.font_weight(Weight::BOLD).width(80.0)),
                    format!(
                        "L: {:?}, R: {:?}, T: {:?}, B: {:?}",
                        shadow.left_offset,
                        shadow.right_offset,
                        shadow.top_offset,
                        shadow.bottom_offset
                    ),
                ))
                .style(|s| s.items_center().gap(4.0)),
            ))
            .style(|s| s.gap(4.0).padding(8.0))
        };

        // Combine preview and details
        let view = shadow_preview.tooltip(details_view);

        Some(view.into_any())
    }

    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        Some(Self {
            blur_radius: self
                .blur_radius
                .interpolate(&other.blur_radius, value)
                .unwrap(),
            color: self.color.interpolate(&other.color, value).unwrap(),
            spread: self.spread.interpolate(&other.spread, value).unwrap(),
            left_offset: self
                .left_offset
                .interpolate(&other.left_offset, value)
                .unwrap(),
            right_offset: self
                .right_offset
                .interpolate(&other.right_offset, value)
                .unwrap(),
            top_offset: self
                .top_offset
                .interpolate(&other.top_offset, value)
                .unwrap(),
            bottom_offset: self
                .bottom_offset
                .interpolate(&other.bottom_offset, value)
                .unwrap(),
        })
    }
}
impl<A: smallvec::Array> StylePropValue for SmallVec<A>
where
    <A as smallvec::Array>::Item: StylePropValue,
{
    fn debug_view(&self) -> Option<Box<dyn View>> {
        if self.is_empty() {
            return Some(
                text("smallvec\n[]")
                    .style(|s| s.with_theme(|s, t| s.color(t.text_muted())))
                    .into_any(),
            );
        }

        let count = self.len();
        let is_spilled = self.spilled();

        // Create a preview that shows count and whether it has spilled to heap
        let preview = label(move || {
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
                let index_label = text(format!("[{}]", i))
                    .style(|s| s.with_theme(|s, t| s.color(t.text_muted())));

                let item_view = item.debug_view().unwrap_or_else(|| {
                    text(format!("{:?}", item))
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
impl StylePropValue for TextOverflow {}
impl StylePropValue for PointerEvents {}
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
                text("[]")
                    .style(|s| s.with_theme(|s, t| s.color(t.text_muted())))
                    .into_any(),
            );
        }

        let count = self.len();
        let _preview = label(move || format!("[{}]", count)).style(|s| {
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
                let index_label = text(format!("[{}]", i))
                    .style(|s| s.with_theme(|s, t| s.color(t.text_muted())));

                let item_view = item.debug_view().unwrap_or_else(|| {
                    text(format!("{:?}", item))
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
        Some(text(format!("{} px", self.0)).into_any())
    }
    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        self.0.interpolate(&other.0, value).map(Px)
    }
}
impl StylePropValue for Pct {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        Some(text(format!("{}%", self.0)).into_any())
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
        Some(text(label).into_any())
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
        Some(text(label).into_any())
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

fn views(views: impl ViewTuple) -> Vec<AnyView> {
    views.into_views()
}

impl StylePropValue for Color {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        let color = *self;
        let swatch = empty()
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
                label(move || hex.clone()),
            ));
            let rgba = views((
                "RGBA:".style(|s| s.font_bold().min_width(80.0).justify_end()),
                label(move || rgba_str.clone()),
            ));
            let components = views((
                "Components:".style(|s| s.font_bold().min_width(80.0).justify_end()),
                (
                    label(move || format!("[0]: {:.3}", components[0])),
                    label(move || format!("[1]: {:.3}", components[1])),
                    label(move || format!("[2]: {:.3}", components[2])),
                    label(move || format!("[3]: {:.3}", components[3])),
                )
                    .v_stack()
                    .style(|s| s.gap(2.0)),
            ));
            let color_space = views((
                "Color Space:".style(|s| s.font_bold().min_width(80.0).justify_end()),
                label(move || color_space_str.clone()),
            ));
            let alpha = views((
                "Alpha:".style(|s| s.font_bold().min_width(80.0).justify_end()),
                label(move || alpha_str.clone()),
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
        let color = empty().style(move |s| {
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
            stack((text(format!("{self:?}")), color))
                .style(|s| s.items_center())
                .into_any(),
        )
    }

    fn interpolate(&self, _other: &Self, _value: f64) -> Option<Self> {
        None
        /*
        let mut interpolated_stops = ColorStops::new();

        let mut i = 0;
        let mut j = 0;

        while i < self.stops.len() && j < other.stops.len() {
            let stop1 = &self.stops[i];
            let stop2 = &other.stops[j];

            if stop1.offset == stop2.offset {
                let interpolated_color = stop1.color.interpolate(&stop2.color, value).unwrap();
                interpolated_stops.push(ColorStop::from((stop1.offset, interpolated_color)));
                i += 1;
                j += 1;
            } else if stop1.offset < stop2.offset {
                if j > 0 {
                    let prev_stop2 = &other.stops[j - 1];
                    let t = (stop1.offset - prev_stop2.offset) / (stop2.offset - prev_stop2.offset);
                    let interpolated_color = prev_stop2
                        .color
                        .interpolate(&stop2.color, t as f64)
                        .unwrap();
                    interpolated_stops.push(ColorStop::from((stop1.offset, interpolated_color)));
                } else {
                    interpolated_stops.push(*stop1);
                }
                i += 1;
            } else {
                if i > 0 {
                    let prev_stop1 = &self.stops[i - 1];
                    let t = (stop2.offset - prev_stop1.offset) / (stop1.offset - prev_stop1.offset);
                    let interpolated_color = prev_stop1
                        .color
                        .interpolate(&stop1.color, t as f64)
                        .unwrap();
                    interpolated_stops.push(ColorStop::from((stop2.offset, interpolated_color)));
                } else {
                    interpolated_stops.push(*stop2);
                }
                j += 1;
            }
        }

        while i < self.stops.len() {
            let stop1 = &self.stops[i];
            if !other.stops.is_empty() {
                let last_stop2 = &other.stops.last().unwrap();
                let interpolated_color = stop1.color.interpolate(&last_stop2.color, value).unwrap();
                interpolated_stops.push(ColorStop::from((stop1.offset, interpolated_color)));
            } else {
                interpolated_stops.push(*stop1);
            }
            i += 1;
        }

        while j < other.stops.len() {
            let stop2 = &other.stops[j];
            if !self.stops.is_empty() {
                let last_stop1 = &self.stops.last().unwrap();
                let interpolated_color = last_stop1.color.interpolate(&stop2.color, value).unwrap();
                interpolated_stops.push(ColorStop::from((stop2.offset, interpolated_color)));
            } else {
                interpolated_stops.push(*stop2);
            }
            j += 1;
        }

        Some(Self {
            kind: self.kind,
            extend: self.extend,
            stops: interpolated_stops,
        })
        */
    }
}

// this is necessary because Stroke doesn't impl partial eq. it probably should...
#[derive(Clone, Debug, Default)]
pub struct StrokeWrap(pub Stroke);
impl StrokeWrap {
    fn new(width: f64) -> Self {
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
                label(move || format!("{:.1}px", stroke.width)),
            ));

            let join_row = views((
                "Join:".style(|s| s.font_bold().min_width(100.0).justify_end()),
                label(move || format!("{:?}", stroke.join)),
            ));

            let miter_row = views((
                "Miter Limit:".style(|s| s.font_bold().min_width(100.0).justify_end()),
                label(move || format!("{:.2}", stroke.miter_limit)),
            ));

            let start_cap_row = views((
                "Start Cap:".style(|s| s.font_bold().min_width(100.0).justify_end()),
                label(move || format!("{:?}", stroke.start_cap)),
            ));

            let end_cap_row = views((
                "End Cap:".style(|s| s.font_bold().min_width(100.0).justify_end()),
                label(move || format!("{:?}", stroke.end_cap)),
            ));

            let pattern_clone = stroke.dash_pattern.clone();

            let dash_pattern_row = views((
                "Dash Pattern:".style(|s| s.font_bold().min_width(100.0).justify_end()),
                label(move || {
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
                    label(move || format!("{:.1}", stroke.dash_offset)),
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

pub trait StyleClass: Default + Copy + 'static {
    fn key() -> StyleKey;
    fn class_ref() -> StyleClassRef {
        StyleClassRef { key: Self::key() }
    }
}

#[derive(Debug, Clone)]
pub struct StyleClassInfo {
    pub(crate) name: fn() -> &'static str,
}

impl StyleClassInfo {
    pub const fn new<Name>() -> Self {
        StyleClassInfo {
            name: || std::any::type_name::<Name>(),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct StyleClassRef {
    pub key: StyleKey,
}

macro_rules! style_key_selector {
    ($v:vis $name:ident, $sel:expr) => {
        fn $name() -> $crate::style::StyleKey {
            static INFO: $crate::style::StyleKeyInfo = $crate::style::StyleKeyInfo::Selector($sel);
            $crate::style::StyleKey { info: &INFO }
        }
    };
}

#[macro_export]
macro_rules! style_class {
    ($(#[$meta:meta])* $v:vis $name:ident) => {
        $(#[$meta])*
        #[derive(Default, Copy, Clone)]
        $v struct $name;

        impl $crate::style::StyleClass for $name {
            fn key() -> $crate::style::StyleKey {
                static INFO: $crate::style::StyleKeyInfo = $crate::style::StyleKeyInfo::Class(
                    $crate::style::StyleClassInfo::new::<$name>()
                );
                $crate::style::StyleKey { info: &INFO }
            }
        }
    };
}

pub trait StyleProp: Default + Copy + 'static {
    type Type: StylePropValue;
    fn key() -> StyleKey;
    fn prop_ref() -> StylePropRef {
        StylePropRef { key: Self::key() }
    }
    fn default_value() -> Self::Type;
}

pub(crate) type InterpolateFn =
    fn(val1: &dyn Any, val2: &dyn Any, time: f64) -> Option<Rc<dyn Any>>;

pub(crate) type CombineFn = fn(val1: Rc<dyn Any>, val2: Rc<dyn Any>) -> Rc<dyn Any>;

#[derive(Debug)]
pub struct StylePropInfo {
    pub(crate) name: fn() -> &'static str,
    pub(crate) inherited: bool,
    #[allow(unused)]
    pub(crate) default_as_any: fn() -> Rc<dyn Any>,
    pub(crate) interpolate: InterpolateFn,
    pub(crate) debug_any: fn(val: &dyn Any) -> String,
    pub(crate) debug_view: fn(val: &dyn Any) -> Option<Box<dyn View>>,
    pub(crate) combine: CombineFn,
    pub(crate) transition_key: StyleKey,
}

impl StylePropInfo {
    pub const fn new<Name, T: StylePropValue + 'static>(
        inherited: bool,
        default_as_any: fn() -> Rc<dyn Any>,
        transition_key: StyleKey,
    ) -> Self {
        StylePropInfo {
            name: || std::any::type_name::<Name>(),
            inherited,
            default_as_any,
            debug_any: |val| {
                if let Some(v) = val.downcast_ref::<StyleMapValue<T>>() {
                    match v {
                        StyleMapValue::Val(v) | StyleMapValue::Animated(v) => format!("{v:?}"),
                        StyleMapValue::Unset => "Unset".to_owned(),
                    }
                } else {
                    panic!(
                        "expected type {} for property {}",
                        type_name::<T>(),
                        std::any::type_name::<Name>(),
                    )
                }
            },
            interpolate: |val1, val2, time| {
                if let (Some(v1), Some(v2)) = (
                    val1.downcast_ref::<StyleMapValue<T>>(),
                    val2.downcast_ref::<StyleMapValue<T>>(),
                ) {
                    if let (
                        StyleMapValue::Val(v1) | StyleMapValue::Animated(v1),
                        StyleMapValue::Val(v2) | StyleMapValue::Animated(v2),
                    ) = (v1, v2)
                    {
                        v1.interpolate(v2, time)
                            .map(|val| Rc::new(StyleMapValue::Animated(val)) as Rc<dyn Any>)
                    } else {
                        None
                    }
                } else {
                    panic!(
                        "expected type {} for property {}. Got typeids {:?} and {:?}",
                        type_name::<T>(),
                        std::any::type_name::<Name>(),
                        val1.type_id(),
                        val2.type_id()
                    )
                }
            },
            debug_view: |val| {
                if let Some(v) = val.downcast_ref::<StyleMapValue<T>>() {
                    match v {
                        StyleMapValue::Val(v) | StyleMapValue::Animated(v) => v.debug_view(),

                        StyleMapValue::Unset => Some(text("Unset").into_any()),
                    }
                } else {
                    panic!(
                        "expected type {} for property {}",
                        type_name::<T>(),
                        std::any::type_name::<Name>(),
                    )
                }
            },
            combine: |val1, val2| {
                if let (Some(v1), Some(v2)) = (
                    val1.downcast_ref::<StyleMapValue<T>>(),
                    val2.downcast_ref::<StyleMapValue<T>>(),
                ) {
                    match (v1, v2) {
                        (StyleMapValue::Val(a), StyleMapValue::Val(b)) => match a.combine(b) {
                            CombineResult::Other => val2,
                            CombineResult::New(result) => {
                                Rc::new(StyleMapValue::Val(result)) as Rc<dyn Any>
                            }
                        },
                        (StyleMapValue::Unset, _) => val2,
                        (_, StyleMapValue::Unset) => val2,
                        (
                            StyleMapValue::Val(a) | StyleMapValue::Animated(a),
                            StyleMapValue::Animated(b) | StyleMapValue::Val(b),
                        ) => match a.combine(b) {
                            CombineResult::Other => val2,
                            CombineResult::New(result) => {
                                Rc::new(StyleMapValue::Animated(result)) as Rc<dyn Any>
                            }
                        },
                    }
                } else {
                    panic!(
                        "expected type {} for property {}. Got typeids {:?} and {:?}",
                        type_name::<StyleMapValue<T>>(),
                        std::any::type_name::<Name>(),
                        val1.type_id(),
                        val2.type_id()
                    )
                }
            },
            transition_key,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct StylePropRef {
    pub key: StyleKey,
}

impl StylePropRef {
    pub(crate) fn info(&self) -> &StylePropInfo {
        if let StyleKeyInfo::Prop(prop) = self.key.info {
            prop
        } else {
            panic!()
        }
    }
}

pub trait StylePropReader {
    type State: Debug;
    type Type: Clone;

    /// Reads the property from the style.
    /// Returns true if the property changed.
    fn read(
        state: &mut Self::State,
        style: &Style,
        fallback: &Style,
        now: &Instant,
        request_transition: &mut bool,
    ) -> bool;

    fn get(state: &Self::State) -> Self::Type;
    fn new() -> Self::State;
}

impl<P: StyleProp> StylePropReader for P {
    type State = (P::Type, TransitionState<P::Type>);
    type Type = P::Type;

    // returns true if the value has changed
    fn read(
        state: &mut Self::State,
        style: &Style,
        fallback: &Style,
        now: &Instant,
        request_transition: &mut bool,
    ) -> bool {
        // get the style property
        let style_value = style.get_prop_style_value::<P>();
        let mut prop_animated = false;
        let new = match style_value {
            StyleValue::Animated(val) => {
                *request_transition = true;
                prop_animated = true;
                val
            }
            StyleValue::Val(val) => val,
            StyleValue::Unset | StyleValue::Base => fallback
                .get_prop::<P>()
                .unwrap_or_else(|| P::default_value()),
        };
        // set the transition state to the transition if one is found
        state.1.read(
            style
                .get_transition::<P>()
                .or_else(|| fallback.get_transition::<P>()),
        );

        // there is a previously stored value in state.0. if the values are different, a transition should be started if there is one
        let changed = new != state.0;
        if changed && !prop_animated {
            state.1.transition(&Self::get(state), &new);
            state.0 = new;
        } else if prop_animated {
            state.0 = new;
        }
        changed | state.1.step(now, request_transition)
    }

    // get the current value from the transition state if one is active, else just return the value that was read from the style map
    fn get(state: &Self::State) -> Self::Type {
        state.1.get(&state.0)
    }

    fn new() -> Self::State {
        (P::default_value(), TransitionState::default())
    }
}

impl<P: StyleProp> StylePropReader for Option<P> {
    type State = Option<P::Type>;
    type Type = Option<P::Type>;
    fn read(
        state: &mut Self::State,
        style: &Style,
        fallback: &Style,
        _now: &Instant,
        _transition: &mut bool,
    ) -> bool {
        let new = style.get_prop::<P>().or_else(|| fallback.get_prop::<P>());
        let changed = new != *state;
        *state = new;
        changed
    }
    fn get(state: &Self::State) -> Self::Type {
        state.clone()
    }
    fn new() -> Self::State {
        None
    }
}

#[derive(Clone)]
pub struct ExtractorField<R: StylePropReader> {
    state: R::State,
}

impl<R: StylePropReader> Debug for ExtractorField<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.state.fmt(f)
    }
}

impl<R: StylePropReader> ExtractorField<R> {
    pub fn read(
        &mut self,
        style: &Style,
        fallback: &Style,
        now: &Instant,
        request_transition: &mut bool,
    ) -> bool {
        R::read(&mut self.state, style, fallback, now, request_transition)
    }
    pub fn get(&self) -> R::Type {
        R::get(&self.state)
    }
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self { state: R::new() }
    }
}

impl<R: StylePropReader> PartialEq for ExtractorField<R>
where
    R::Type: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.get() == other.get()
    }
}

impl<R: StylePropReader> Eq for ExtractorField<R> where R::Type: Eq {}

impl<R: StylePropReader> std::hash::Hash for ExtractorField<R>
where
    R::Type: std::hash::Hash,
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.get().hash(state)
    }
}

#[macro_export]
macro_rules! prop {
    ($(#[$meta:meta])* $v:vis $name:ident: $ty:ty { $($options:tt)* } = $default:expr
    ) => {
        $(#[$meta])*
        #[derive(Default, Copy, Clone)]
        #[allow(missing_docs)]
        $v struct $name;
        impl $crate::style::StyleProp for $name {
            type Type = $ty;
            fn key() -> $crate::style::StyleKey {
                static TRANSITION_INFO: $crate::style::StyleKeyInfo = $crate::style::StyleKeyInfo::Transition;
                static INFO: $crate::style::StyleKeyInfo = $crate::style::StyleKeyInfo::Prop($crate::style::StylePropInfo::new::<$name, $ty>(
                    prop!([impl inherited][$($options)*]),
                    || std::rc::Rc::new($crate::style::StyleMapValue::Val($name::default_value())),
                    $crate::style::StyleKey { info: &TRANSITION_INFO },
                ));
                $crate::style::StyleKey { info: &INFO }
            }
            fn default_value() -> Self::Type {
                $default
            }
        }
    };
    ([impl inherited][inherited]) => {
        true
    };
    ([impl inherited][]) => {
        false
    };
}

#[macro_export]
macro_rules! prop_extractor {
    (
        $(#[$attrs:meta])* $vis:vis $name:ident {
            $($prop_vis:vis $prop:ident: $reader:ty),*
            $(,)?
        }
    ) => {
        #[derive(Debug, Clone)]
        $(#[$attrs])?
        $vis struct $name {
            $(
                $prop_vis $prop: $crate::style::ExtractorField<$reader>,
            )*
        }

        impl $name {
            #[allow(dead_code)]
            $vis fn read_style(&mut self, cx: &mut $crate::context::StyleCx, style: &$crate::style::Style) -> bool {
                let mut transition = false;
                let changed = false $(| self.$prop.read(style, style, &cx.now(), &mut transition))*;
                if transition {
                    cx.request_transition();
                }
                changed
            }

           #[allow(dead_code)]
            $vis fn read(&mut self, cx: &mut $crate::context::StyleCx) -> bool {
                let mut transition = false;
                let changed = self.read_explicit(&cx.direct_style(), &cx.indirect_style(), &cx.now(), &mut transition);
                if transition {
                    cx.request_transition();
                }
                changed
            }

            #[allow(dead_code)]
            $vis fn read_explicit(
                &mut self,
                style: &$crate::style::Style,
                fallback: &$crate::style::Style,
                #[cfg(not(target_arch = "wasm32"))]
                now: &std::time::Instant,
                #[cfg(target_arch = "wasm32")]
                now: &web_time::Instant,
                request_transition: &mut bool
            ) -> bool {
                false $(| self.$prop.read(style, fallback, now, request_transition))*
            }

            $($prop_vis fn $prop(&self) -> <$reader as $crate::style::StylePropReader>::Type
            {
                self.$prop.get()
            })*
        }

        impl Default for $name {
            fn default() -> Self {
                Self {
                    $(
                        $prop: $crate::style::ExtractorField::new(),
                    )*
                }
            }
        }
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleMapValue<T> {
    Animated(T),
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

#[derive(Clone, Debug)]
pub(crate) struct ActiveTransition<T: StylePropValue> {
    start: Instant,
    before: T,
    current: T,
    after: T,
}

#[derive(Clone, Debug)]
pub struct TransitionState<T: StylePropValue> {
    transition: Option<Transition>,
    active: Option<ActiveTransition<T>>,
    initial: bool,
}

impl<T: StylePropValue> TransitionState<T> {
    fn read(&mut self, transition: Option<Transition>) {
        self.transition = transition;
    }

    fn transition(&mut self, before: &T, after: &T) {
        if !self.initial {
            return;
        }
        if self.transition.is_some() {
            self.active = Some(ActiveTransition {
                start: Instant::now(),
                before: before.clone(),
                current: before.clone(),
                after: after.clone(),
            });
        }
    }

    // returns true if changed
    fn step(&mut self, now: &Instant, request_transition: &mut bool) -> bool {
        if !self.initial {
            // We have observed the initial value. Any further changes may trigger animations.
            self.initial = true;
        }
        if let Some(active) = &mut self.active {
            if let Some(transition) = &self.transition {
                let time = now.saturating_duration_since(active.start);
                let time_percent = time.as_secs_f64() / transition.duration.as_secs_f64();
                if time < transition.duration || !transition.easing.finished(time_percent) {
                    if let Some(i) = T::interpolate(
                        &active.before,
                        &active.after,
                        transition.easing.eval(time_percent),
                    ) {
                        active.current = i;
                        *request_transition = true;
                        return true;
                    }
                }
            }
            // time has past duration, or the value is not interpolatable
            self.active = None;

            true
        } else {
            false
        }
    }

    fn get(&self, value: &T) -> T {
        if let Some(active) = &self.active {
            active.current.clone()
        } else {
            value.clone()
        }
    }
}

impl<T: StylePropValue> Default for TransitionState<T> {
    fn default() -> Self {
        Self {
            transition: None,
            active: None,
            initial: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Transition {
    pub duration: Duration,
    pub easing: Rc<dyn Easing>,
}

impl Transition {
    pub fn debug_view(&self) -> Box<dyn View> {
        let transition = self.clone();
        let easing_clone = transition.easing.clone();

        let curve_color = RwSignal::new(palette::css::BLUE);
        let axis_color = RwSignal::new(palette::css::GRAY);

        // Visual preview of the easing curve
        let preview = canvas(move |cx, size| {
            let width = size.width;
            let height = size.height;
            let padding = 4.0;
            let graph_width = width - padding * 2.0;
            let graph_height = height - padding * 2.0;

            // Sample the easing function
            let sample_count = 50;
            let mut path = kurbo::BezPath::new();

            for i in 0..=sample_count {
                let t = i as f64 / sample_count as f64;
                let eased = easing_clone.eval(t);
                let x = padding + t * graph_width;
                let y = padding + (1.0 - eased) * graph_height;

                if i == 0 {
                    path.move_to(Point::new(x, y));
                } else {
                    path.line_to(Point::new(x, y));
                }
            }

            // Draw the curve
            cx.stroke(
                &path,
                curve_color.get(),
                &Stroke {
                    width: 2.0,
                    ..Default::default()
                },
            );

            // Draw axes
            let axis_stroke = Stroke {
                width: 1.0,
                ..Default::default()
            };

            // X axis
            cx.stroke(
                &kurbo::Line::new(
                    Point::new(padding, height - padding),
                    Point::new(width - padding, height - padding),
                ),
                axis_color.get(),
                &axis_stroke,
            );

            // Y axis
            cx.stroke(
                &kurbo::Line::new(
                    Point::new(padding, padding),
                    Point::new(padding, height - padding),
                ),
                axis_color.get(),
                &axis_stroke,
            );
        })
        .style(|s| s.width(80.0).height(60.0))
        .container()
        .style(move |s| {
            s.padding(4.0)
                .border(1.)
                .border_radius(5.0)
                .with_theme(move |s, t| {
                    curve_color.set(t.primary());
                    axis_color.set(t.text_muted());
                    s.border_color(t.border())
                })
        });

        let tooltip_view = move || {
            let transition = transition.clone();

            let duration_row = views((
                "Duration:".style(|s| s.font_bold().min_width(80.0).justify_end()),
                label(move || format!("{:.0}ms", transition.duration.as_millis())),
            ));

            let easing_name = format!("{:?}", transition.easing);
            let easing_row = views((
                "Easing:".style(|s| s.font_bold().min_width(80.0).justify_end()),
                label(move || easing_name.clone()),
            ));

            // Show velocity at key points if available
            let velocity_samples = if transition.easing.velocity(0.0).is_some() {
                let samples = vec![0.0, 0.25, 0.5, 0.75, 1.0]
                    .into_iter()
                    .filter_map(|t| {
                        transition
                            .easing
                            .velocity(t)
                            .map(|v| text(format!("t={:.2}: {:.3}", t, v)))
                    })
                    .collect::<Vec<_>>();

                if !samples.is_empty() {
                    Some(views((
                        "Velocity:".style(|s| s.font_bold().min_width(80.0).justify_end()),
                        v_stack_from_iter(samples).style(|s| s.gap(2.0)),
                    )))
                } else {
                    None
                }
            } else {
                None
            };

            let mut rows = vec![duration_row.into_any(), easing_row.into_any()];

            if let Some(velocity_row) = velocity_samples {
                rows.push(velocity_row.into_any());
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

        preview
            .tooltip(tooltip_view)
            .style(|s| s.items_center())
            .into_any()
    }
}

impl Transition {
    pub fn new(duration: Duration, easing: impl Easing + 'static) -> Self {
        Self {
            duration,
            easing: Rc::new(easing),
        }
    }

    pub fn linear(duration: Duration) -> Self {
        Self::new(duration, Linear)
    }

    pub fn ease_in_out(duration: Duration) -> Self {
        Self::new(duration, Bezier::ease_in_out())
    }

    pub fn spring(duration: Duration) -> Self {
        Self::new(duration, Spring::default())
    }
}

/// Direct transition controller using TransitionState without the Style system.
///
/// This allows you to animate any value that implements `StylePropValue` by managing
/// the transition state directly. You control when transitions start, how they're
/// configured, and when to step them forward.
///
/// # Example
///
/// ```rust
/// use std::time::{Duration, Instant};
/// use floem::style::{DirectTransition, Transition};
///
/// // Create a transition for animating opacity
/// let mut opacity = DirectTransition::new(1., None);
///
/// // Configure transition timing and easing
/// opacity.set_transition(Some(
///     Transition::ease_in_out(Duration::from_millis(300))
/// ));
///
/// // Start transition to new value
/// opacity.transition_to(0.0);
///
/// // Animation loop - call this every frame
/// let start_time = Instant::now();
/// loop {
///     let now = Instant::now();
///
///     // Step the transition forward
///     let changed = opacity.step(&now);
///
///     // Get current interpolated value
///     let current_opacity = opacity.get();
///
///     // Only update rendering if value changed
///     if changed {
///         println!("Current opacity: {:.3}", current_opacity);
///         // render_with_opacity(current_opacity);
///     }
///
///     // Exit when transition completes
///     if !opacity.is_active() {
///         println!("Transition complete!");
///         break;
///     }
///
///     // Wait for next frame (~60fps)
///     std::thread::sleep(Duration::from_millis(16));
///
///     // Safety timeout
///     if now.duration_since(start_time) > Duration::from_secs(2) {
///         break;
///     }
/// }
///
/// // Chain multiple transitions
/// opacity.transition_to(0.5); // Start new transition from current position
/// // ... repeat animation loop
///
/// // Or jump immediately without animation
/// opacity.set_immediate(1.0);
/// ```
#[derive(Debug, Clone)]
pub struct DirectTransition<T: StylePropValue> {
    pub current_value: T,
    transition_state: TransitionState<T>,
}

impl<T: StylePropValue> DirectTransition<T> {
    /// Create a new transition starting at the given value
    pub fn new(initial_value: T, transition: Option<Transition>) -> Self {
        let mut t = Self {
            current_value: initial_value,
            transition_state: TransitionState::default(),
        };
        t.transition_state.read(transition);
        t
    }

    /// Configure the transition timing and easing function
    ///
    /// Pass `None` to disable transitions (values will change immediately)
    pub fn set_transition(&mut self, transition: Option<Transition>) {
        // If we're currently transitioning, preserve the current interpolated state
        // as the new baseline instead of reverting to the original target
        if self.transition_state.active.is_some() {
            let current_interpolated = self.get();
            self.current_value = current_interpolated;
            self.transition_state.active = None;
        }

        self.transition_state.read(transition);
    }

    /// Start transitioning to a new target value
    ///
    /// Returns `true` if a transition was started, `false` if no transition
    /// is configured or the target equals the current value
    pub fn transition_to(&mut self, target: T) -> bool {
        let before = if self.transition_state.active.is_some() {
            // If already transitioning, start from current interpolated position
            self.get()
        } else {
            self.current_value.clone()
        };

        self.current_value = target;

        // Ensure transitions can start by marking as initialized
        if !self.transition_state.initial {
            self.transition_state.initial = true;
        }

        self.transition_state
            .transition(&before, &self.current_value);
        self.transition_state.active.is_some()
    }

    /// Step the transition forward in time
    ///
    /// Call this every frame with the current time. Returns `true` if the
    /// interpolated value changed this frame, `false` otherwise.
    ///
    /// You can use the return value to optimize rendering - only update
    /// when something actually changed.
    pub fn step(&mut self, now: &Instant) -> bool {
        let mut request_transition = false;
        self.transition_state.step(now, &mut request_transition)
    }

    /// Get the current interpolated value
    ///
    /// During a transition, this returns the smoothly interpolated value.
    /// When no transition is active, returns the target value.
    pub fn get(&self) -> T {
        self.transition_state.get(&self.current_value)
    }

    /// Check if a transition is currently active
    pub fn is_active(&self) -> bool {
        self.transition_state.active.is_some()
    }

    /// Get the target value (final destination of current/last transition)
    pub fn target(&self) -> &T {
        &self.current_value
    }

    /// Set value immediately without any transition
    ///
    /// This cancels any active transition and jumps directly to the new value
    pub fn set_immediate(&mut self, value: T) {
        self.current_value = value;
        self.transition_state.active = None;
    }

    /// Get the progress of the current transition as a value from 0.0 to 1.0
    ///
    /// Returns `None` if no transition is active or configured
    pub fn progress(&self, now: &Instant) -> Option<f64> {
        if let Some(active) = &self.transition_state.active {
            if let Some(transition) = &self.transition_state.transition {
                let elapsed = now.saturating_duration_since(active.start);
                let progress = elapsed.as_secs_f64() / transition.duration.as_secs_f64();
                Some(progress.min(1.0))
            } else {
                None
            }
        } else {
            None
        }
    }
}

#[derive(Debug)]
pub enum StyleKeyInfo {
    Transition,
    Prop(StylePropInfo),
    Selector(StyleSelectors),
    Class(StyleClassInfo),
    ContextMappings,
}

static CONTEXT_MAPPINGS_INFO: StyleKeyInfo = StyleKeyInfo::ContextMappings;

type ContextMapFn = Rc<dyn Fn(Style, &Style) -> Style>;

#[derive(Copy, Clone)]
pub struct StyleKey {
    pub info: &'static StyleKeyInfo,
}
impl StyleKey {
    pub(crate) fn debug_any(&self, value: &dyn Any) -> String {
        match self.info {
            StyleKeyInfo::Selector(selectors) => selectors.debug_string(),
            StyleKeyInfo::Transition | StyleKeyInfo::ContextMappings => String::new(),
            StyleKeyInfo::Class(info) => (info.name)().to_string(),
            StyleKeyInfo::Prop(v) => (v.debug_any)(value),
        }
    }
    fn inherited(&self) -> bool {
        match self.info {
            StyleKeyInfo::Selector(..)
            | StyleKeyInfo::Transition
            | StyleKeyInfo::ContextMappings => false,
            StyleKeyInfo::Class(..) => true,
            StyleKeyInfo::Prop(v) => v.inherited,
        }
    }
}
impl PartialEq for StyleKey {
    fn eq(&self, other: &Self) -> bool {
        ptr::eq(self.info, other.info)
    }
}
impl Hash for StyleKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_usize(self.info as *const _ as usize)
    }
}
impl Eq for StyleKey {}
impl Debug for StyleKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.info {
            StyleKeyInfo::Selector(selectors) => {
                write!(f, "selectors: {}", selectors.debug_string())
            }
            StyleKeyInfo::Transition => write!(f, "transition"),
            StyleKeyInfo::ContextMappings => write!(f, "ContextMappings"),
            StyleKeyInfo::Class(v) => write!(f, "{}", (v.name)()),
            StyleKeyInfo::Prop(v) => write!(f, "{}", (v.name)()),
        }
    }
}

type ImHashMap<K, V> = imbl::GenericHashMap<K, V, BuildHasherDefault<FxHasher>, DefaultSharedPtr>;
// type ImHashMap<K, V> = std::collections::HashMap<K, V, BuildHasherDefault<FxHasher>>;

style_key_selector!(selector_xs, StyleSelectors::new().responsive());
style_key_selector!(selector_sm, StyleSelectors::new().responsive());
style_key_selector!(selector_md, StyleSelectors::new().responsive());
style_key_selector!(selector_lg, StyleSelectors::new().responsive());
style_key_selector!(selector_xl, StyleSelectors::new().responsive());
style_key_selector!(selector_xxl, StyleSelectors::new().responsive());

pub(crate) fn screen_size_bp_to_key(breakpoint: ScreenSizeBp) -> StyleKey {
    match breakpoint {
        ScreenSizeBp::Xs => selector_xs(),
        ScreenSizeBp::Sm => selector_sm(),
        ScreenSizeBp::Md => selector_md(),
        ScreenSizeBp::Lg => selector_lg(),
        ScreenSizeBp::Xl => selector_xl(),
        ScreenSizeBp::Xxl => selector_xxl(),
    }
}

/// the bool in the return is a classes_applied flag. if a new class has been applied, we need to do a request_style_recursive
pub(crate) fn resolve_nested_maps(
    style: Style,
    interact_state: &InteractionState,
    screen_size_bp: ScreenSizeBp,
    classes: &[StyleClassRef],
    context: &mut Style,
) -> (Style, bool) {
    // Start with depth 0 for the initial call
    resolve_nested_maps_internal(style, interact_state, screen_size_bp, context, classes, 0)
}

#[allow(
    clippy::only_used_in_recursion,
    reason = "for debugging it's nice to have the depth"
)]
fn resolve_nested_maps_internal(
    style: Style,
    interact_state: &InteractionState,
    screen_size_bp: ScreenSizeBp,
    context: &mut Style,
    classes: &[StyleClassRef],
    depth: u32,
) -> (Style, bool) {
    // const MAX_DEPTH: u32 = 20;
    // if depth >= MAX_DEPTH {
    //     return (style, false);
    // }
    // if depth > 10 {
    //     dbg!(depth);
    // }

    let mut changed = false;
    let mut classes_applied = false;

    let (style, changed_new) = style.apply_classes_from_context(classes, context);
    if changed_new {
        for class in classes {
            if let Some(nested) = context.remove_nested_map(class.key) {
                classes_applied |= nested.any_inherited();
            }
        }
        changed = true;
    }

    // Apply context mappings first
    let (mut style, changed_new) = style.apply_context_mappings(context);
    if changed_new {
        changed = true;
    }

    // Apply screen size breakpoints
    if let Some(map) = style.get_nested_map(screen_size_bp_to_key(screen_size_bp)) {
        classes_applied |= map.any_inherited();
        style.apply_mut(map);
        style.remove_nested_map(screen_size_bp_to_key(screen_size_bp));
        changed = true;
    }

    // DarkMode
    if interact_state.is_dark_mode {
        if let Some(map) = style.get_nested_map(StyleSelector::DarkMode.to_key()) {
            classes_applied |= map.any_inherited();
            style.apply_mut(map);
            style.remove_nested_map(StyleSelector::DarkMode.to_key());
            changed = true;
        }
    }

    // Disabled state (takes precedence)
    if interact_state.is_disabled {
        if let Some(map) = style.get_nested_map(StyleSelector::Disabled.to_key()) {
            classes_applied |= map.any_inherited();
            style.apply_mut(map);
            style.remove_nested_map(StyleSelector::Disabled.to_key());
            changed = true;
        }
    } else {
        // Other states only apply if not disabled

        // Selected
        if interact_state.is_selected || style.get(Selected) {
            if let Some(map) = style.get_nested_map(StyleSelector::Selected.to_key()) {
                classes_applied |= map.any_inherited();
                style.apply_mut(map);
                style.remove_nested_map(StyleSelector::Selected.to_key());
                changed = true;
            }
        }

        // Hover
        if interact_state.is_hovered {
            if let Some(map) = style.get_nested_map(StyleSelector::Hover.to_key()) {
                classes_applied |= map.any_inherited();
                style.apply_mut(map);
                style.remove_nested_map(StyleSelector::Hover.to_key());
                changed = true;
            }
        }

        // File Hover
        if interact_state.is_file_hover {
            if let Some(map) = style.get_nested_map(StyleSelector::FileHover.to_key()) {
                classes_applied |= map.any_inherited();
                style.apply_mut(map);
                style.remove_nested_map(StyleSelector::FileHover.to_key());
                changed = true;
            }
        }

        // Focus states
        if interact_state.is_focused {
            if let Some(map) = style.get_nested_map(StyleSelector::Focus.to_key()) {
                classes_applied |= map.any_inherited();
                style.apply_mut(map);
                style.remove_nested_map(StyleSelector::Focus.to_key());
                changed = true;
            }

            if interact_state.using_keyboard_navigation {
                if let Some(map) = style.get_nested_map(StyleSelector::FocusVisible.to_key()) {
                    classes_applied |= map.any_inherited();
                    style.apply_mut(map);
                    style.remove_nested_map(StyleSelector::FocusVisible.to_key());
                    changed = true;
                }

                if interact_state.is_clicking {
                    if let Some(map) = style.get_nested_map(StyleSelector::Active.to_key()) {
                        classes_applied |= map.any_inherited();
                        style.apply_mut(map);
                        style.remove_nested_map(StyleSelector::Active.to_key());
                        changed = true;
                    }
                }
            }
        }

        // Active (mouse)
        if interact_state.is_clicking && !interact_state.using_keyboard_navigation {
            if let Some(map) = style.get_nested_map(StyleSelector::Active.to_key()) {
                classes_applied |= map.any_inherited();
                style.apply_mut(map);
                style.remove_nested_map(StyleSelector::Active.to_key());
                changed = true;
            }
        }
    }

    // Recurse once at the end if anything changed
    // if changed && depth + 1 < MAX_DEPTH {
    if changed {
        let (new_style, recursive_classes_applied) = resolve_nested_maps_internal(
            style,
            interact_state,
            screen_size_bp,
            context,
            classes,
            depth + 1,
        );
        style = new_style;
        classes_applied |= recursive_classes_applied;
    }

    (style, classes_applied)
}

#[derive(Default, Clone)]
pub struct Style {
    pub(crate) map: ImHashMap<StyleKey, Rc<dyn Any>>,
}

impl Style {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn get_transition<P: StyleProp>(&self) -> Option<Transition> {
        self.map
            .get(&P::prop_ref().info().transition_key)
            .map(|v| v.downcast_ref::<Transition>().unwrap().clone())
    }

    pub(crate) fn get_prop_or_default<P: StyleProp>(&self) -> P::Type {
        self.get_prop::<P>().unwrap_or_else(|| P::default_value())
    }

    pub(crate) fn get_prop<P: StyleProp>(&self) -> Option<P::Type> {
        self.map.get(&P::key()).and_then(|v| {
            v.downcast_ref::<StyleMapValue<P::Type>>()
                .unwrap()
                .as_ref()
                .cloned()
        })
    }

    pub(crate) fn get_prop_style_value<P: StyleProp>(&self) -> StyleValue<P::Type> {
        self.map
            .get(&P::key())
            .map(
                |v| match v.downcast_ref::<StyleMapValue<P::Type>>().unwrap() {
                    StyleMapValue::Val(v) => StyleValue::Val(v.clone()),
                    StyleMapValue::Animated(v) => StyleValue::Animated(v.clone()),
                    StyleMapValue::Unset => StyleValue::Unset,
                },
            )
            .unwrap_or(StyleValue::Base)
    }

    pub(crate) fn style_props(&self) -> impl Iterator<Item = StylePropRef> + '_ {
        self.map.keys().filter_map(|p| match p.info {
            StyleKeyInfo::Prop(..) => Some(StylePropRef { key: *p }),
            _ => None,
        })
    }

    pub(crate) fn selectors(&self) -> StyleSelectors {
        let mut result = StyleSelectors::new();
        for (k, v) in &self.map {
            if let StyleKeyInfo::Selector(selector) = k.info {
                result = result
                    .union(*selector)
                    .union(v.downcast_ref::<Style>().unwrap().selectors());
            }
        }
        result
    }

    /// the returned boolean is true if a nested map was applied
    pub fn apply_classes_from_context(
        mut self,
        classes: &[StyleClassRef],
        context: &Style,
    ) -> (Style, bool) {
        let mut changed = false;
        for class in classes {
            if let Some(map) = context.get_nested_map(class.key) {
                self.apply_mut(map);
                changed = true;
            }
        }
        (self, changed)
    }

    pub fn apply_class<C: StyleClass>(mut self, _class: C) -> Style {
        if let Some(map) = self.map.get(&C::key()) {
            self.apply_mut(map.downcast_ref::<Style>().unwrap().clone());
            self.apply_context_mappings_mut();
        }
        self
    }

    pub fn apply_selectors(mut self, selectors: &[StyleSelector]) -> Style {
        for selector in selectors {
            if let Some(map) = self.get_nested_map(selector.to_key()) {
                self.apply_mut(map.apply_selectors(selectors));
                self.apply_context_mappings_mut();
            }
        }
        if self.get(Selected) {
            if let Some(map) = self.get_nested_map(StyleSelector::Selected.to_key()) {
                self.apply_mut(map.apply_selectors(&[StyleSelector::Selected]));
                self.apply_context_mappings_mut();
            }
        }
        self
    }

    /// Store a context mapping to be applied to nested styles
    // Then update your with_context function:
    pub fn with_context<P: StyleProp>(
        mut self,
        f: impl Fn(Self, &P::Type) -> Self + 'static,
    ) -> Self {
        let mapper: ContextMapFn = Rc::new(move |style: Style, context: &Style| {
            // Try getting the property from style first, then from context if not found
            let value = style.get_prop::<P>().or_else(|| {
                // Only look in context for props
                let prop_key = P::key();
                if let StyleKeyInfo::Prop(_) = prop_key.info {
                    context.get_prop::<P>()
                } else {
                    None
                }
            });

            if let Some(value) = value {
                f(style, &value)
            } else {
                style
            }
        });

        let key = StyleKey {
            info: &CONTEXT_MAPPINGS_INFO,
        };

        let mut mappings = self
            .map
            .get(&key)
            .and_then(|v| v.downcast_ref::<Vec<ContextMapFn>>())
            .cloned()
            .unwrap_or_default();

        mappings.push(mapper);
        self.map.insert(key, Rc::new(mappings));
        self
    }

    pub fn with_context_opt<P: StyleProp<Type = Option<T>>, T: 'static>(
        mut self,
        f: impl Fn(Self, T) -> Self + 'static,
    ) -> Self {
        let mapper: ContextMapFn = Rc::new(move |style: Style, context: &Style| {
            // Try getting the property from style first, then from context if not found
            let value = style.get_prop::<P>().or_else(|| {
                // Only look in context for props
                let prop_key = P::key();
                if let StyleKeyInfo::Prop(_) = prop_key.info {
                    context.get_prop::<P>()
                } else {
                    None
                }
            });

            match value {
                Some(Some(value)) => f(style, value),
                _ => style,
            }
        });

        let key = StyleKey {
            info: &CONTEXT_MAPPINGS_INFO,
        };

        let mut mappings = self
            .map
            .get(&key)
            .and_then(|v| v.downcast_ref::<Vec<ContextMapFn>>())
            .cloned()
            .unwrap_or_default();

        mappings.push(mapper);
        self.map.insert(key, Rc::new(mappings));
        self
    }

    pub(crate) fn get_nested_map(&self, key: StyleKey) -> Option<Style> {
        self.map
            .get(&key)
            .map(|map| map.downcast_ref::<Style>().unwrap().clone())
    }

    pub(crate) fn remove_nested_map(&mut self, key: StyleKey) -> Option<Style> {
        self.map
            .remove(&key)
            .map(|map| map.downcast_ref::<Style>().unwrap().clone())
    }

    pub(crate) fn any_inherited(&self) -> bool {
        self.map.iter().any(|(p, _)| p.inherited())
    }

    pub(crate) fn apply_only_inherited(this: &mut Rc<Style>, over: &Style) {
        if over.any_inherited() {
            let inherited = over.map.iter().filter(|(p, _)| p.inherited());

            let this = Rc::make_mut(this);
            this.apply_iter(inherited);
        }
    }

    pub(crate) fn inherited(&self) -> Style {
        let mut new = Style::new();
        if self.any_inherited() {
            let inherited = self.map.iter().filter(|(p, _)| p.inherited());

            new.apply_iter(inherited);
        }
        new
    }

    fn set_selector(&mut self, selector: StyleSelector, map: Style) {
        self.set_map_selector(selector.to_key(), map)
    }

    fn set_map_selector(&mut self, key: StyleKey, map: Style) {
        match self.map.entry(key) {
            Entry::Occupied(mut e) => {
                let mut current = e.get_mut().downcast_ref::<Style>().unwrap().clone();
                current.apply_mut(map);
                *e.get_mut() = Rc::new(current);
            }
            Entry::Vacant(e) => {
                e.insert(Rc::new(map));
            }
        }
    }

    fn set_breakpoint(&mut self, breakpoint: ScreenSizeBp, map: Style) {
        self.set_map_selector(screen_size_bp_to_key(breakpoint), map)
    }

    fn set_class(&mut self, class: StyleClassRef, map: Style) {
        self.set_map_selector(class.key, map)
    }

    pub fn builtin(&self) -> BuiltinStyle<'_> {
        BuiltinStyle { style: self }
    }

    fn apply_iter<'a>(&mut self, iter: impl Iterator<Item = (&'a StyleKey, &'a Rc<dyn Any>)>) {
        for (k, v) in iter {
            match k.info {
                StyleKeyInfo::Class(..) | StyleKeyInfo::Selector(..) => match self.map.entry(*k) {
                    Entry::Occupied(mut e) => {
                        // We need to merge the new map with the existing map.

                        let v = v.downcast_ref::<Style>().unwrap();
                        match Rc::get_mut(e.get_mut()) {
                            Some(current) => {
                                current
                                    .downcast_mut::<Style>()
                                    .unwrap()
                                    .apply_mut(v.clone());
                            }
                            None => {
                                let mut current =
                                    e.get_mut().downcast_ref::<Style>().unwrap().clone();
                                current.apply_mut(v.clone());
                                *e.get_mut() = Rc::new(current);
                            }
                        }
                    }
                    Entry::Vacant(e) => {
                        e.insert(v.clone());
                    }
                },
                StyleKeyInfo::ContextMappings => match self.map.entry(*k) {
                    Entry::Occupied(mut e) => {
                        // Append the new mappings to existing ones
                        let new_mappings = v.downcast_ref::<Vec<ContextMapFn>>().unwrap();
                        match Rc::get_mut(e.get_mut()) {
                            Some(current) => {
                                let current_mappings =
                                    current.downcast_mut::<Vec<ContextMapFn>>().unwrap();
                                current_mappings.extend(new_mappings.iter().cloned());
                            }
                            None => {
                                let mut current = e
                                    .get_mut()
                                    .downcast_ref::<Vec<ContextMapFn>>()
                                    .unwrap()
                                    .clone();
                                current.extend(new_mappings.iter().cloned());
                                *e.get_mut() = Rc::new(current);
                            }
                        }
                    }
                    Entry::Vacant(e) => {
                        e.insert(v.clone());
                    }
                },
                StyleKeyInfo::Transition => {
                    self.map.insert(*k, v.clone());
                }
                StyleKeyInfo::Prop(info) => {
                    match self.map.entry(*k) {
                        Entry::Occupied(mut e) => {
                            // We need to merge the new map with the existing map.
                            e.insert((info.combine)(e.get().clone(), v.clone()));
                        }
                        Entry::Vacant(e) => {
                            e.insert(v.clone());
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn apply_mut(&mut self, over: Style) {
        self.apply_iter(over.map.iter());
    }

    /// Apply another `Style` to this style, returning a new `Style` with the overrides
    ///
    /// `StyleValue::Val` will override the value with the given value
    /// `StyleValue::Unset` will unset the value, causing it to fall back to the default.
    /// `StyleValue::Base` will leave the value as-is, whether falling back to the default
    /// or using the value in the `Style`.
    pub fn apply(mut self, over: Style) -> Style {
        self.apply_mut(over);
        self
    }

    pub fn map(self, over: impl FnOnce(Self) -> Self) -> Self {
        over(self)
    }

    /// Apply multiple `Style`s to this style, returning a new `Style` with the overrides.
    /// Later styles take precedence over earlier styles.
    pub fn apply_overriding_styles(self, overrides: impl Iterator<Item = Style>) -> Style {
        overrides.fold(self, |acc, x| acc.apply(x))
    }

    pub(crate) fn apply_context_mappings(mut self, context: &Style) -> (Self, bool) {
        let key = StyleKey {
            info: &CONTEXT_MAPPINGS_INFO,
        };
        let mut changed = false;

        if let Some(mappings) = self
            .map
            .get(&key)
            .and_then(|v| v.downcast_ref::<Vec<ContextMapFn>>())
            .cloned()
        {
            self.map.remove(&key);
            changed = true;

            for mapping in mappings {
                self = mapping(self, context);
            }

            let (style, new_changed) = self.apply_context_mappings(context);
            self = style;
            changed |= new_changed;
        }

        (self, changed)
    }

    pub(crate) fn apply_context_mappings_mut(&mut self) {
        let key = StyleKey {
            info: &CONTEXT_MAPPINGS_INFO,
        };

        if let Some(mappings) = self
            .map
            .get(&key)
            .and_then(|v| v.downcast_ref::<Vec<ContextMapFn>>())
            .cloned()
        {
            self.map.remove(&key);

            for mapping in mappings {
                *self = mapping(self.clone(), self);
            }

            self.apply_context_mappings_mut();
        }
    }
}

impl Debug for Style {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Style")
            .field(
                "map",
                &self
                    .map
                    .iter()
                    .map(|(p, v)| (*p, (p.debug_any(&**v))))
                    .collect::<HashMap<StyleKey, String>>(),
            )
            .finish()
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum StyleSelector {
    Hover,
    Focus,
    FocusVisible,
    Disabled,
    DarkMode,
    Active,
    Dragging,
    Selected,
    FileHover,
}
impl StyleSelector {
    pub const fn all() -> &'static [StyleSelector] {
        &[
            StyleSelector::Hover,
            StyleSelector::Focus,
            StyleSelector::FocusVisible,
            StyleSelector::Disabled,
            StyleSelector::Active,
            StyleSelector::Dragging,
            StyleSelector::Selected,
            StyleSelector::DarkMode,
            StyleSelector::FileHover,
        ]
    }

    pub const fn name(self) -> &'static str {
        match self {
            StyleSelector::Hover => "Hover",
            StyleSelector::Focus => "Focus",
            StyleSelector::FocusVisible => "FocusVisible",
            StyleSelector::Disabled => "Disabled",
            StyleSelector::Active => "Active",
            StyleSelector::Dragging => "Dragging",
            StyleSelector::Selected => "Selected",
            StyleSelector::DarkMode => "DarkMode",
            StyleSelector::FileHover => "FileHover",
        }
    }
}

style_key_selector!(hover, StyleSelectors::new().set(StyleSelector::Hover, true));
style_key_selector!(
    file_hover,
    StyleSelectors::new().set(StyleSelector::FileHover, true)
);
style_key_selector!(focus, StyleSelectors::new().set(StyleSelector::Focus, true));
style_key_selector!(
    focus_visible,
    StyleSelectors::new().set(StyleSelector::FocusVisible, true)
);
style_key_selector!(
    disabled,
    StyleSelectors::new().set(StyleSelector::Disabled, true)
);
style_key_selector!(
    active,
    StyleSelectors::new().set(StyleSelector::Active, true)
);
style_key_selector!(
    dragging,
    StyleSelectors::new().set(StyleSelector::Dragging, true)
);
style_key_selector!(
    selected,
    StyleSelectors::new().set(StyleSelector::Selected, true)
);
style_key_selector!(
    darkmode,
    StyleSelectors::new().set(StyleSelector::DarkMode, true)
);

impl StyleSelector {
    fn to_key(self) -> StyleKey {
        match self {
            StyleSelector::Hover => hover(),
            StyleSelector::Focus => focus(),
            StyleSelector::FocusVisible => focus_visible(),
            StyleSelector::Disabled => disabled(),
            StyleSelector::Active => active(),
            StyleSelector::Dragging => dragging(),
            StyleSelector::Selected => selected(),
            StyleSelector::DarkMode => darkmode(),
            StyleSelector::FileHover => file_hover(),
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Default)]
pub struct StyleSelectors {
    selectors: u8,
    responsive: bool,
}

impl StyleSelectors {
    pub(crate) const fn new() -> Self {
        StyleSelectors {
            selectors: 0,
            responsive: false,
        }
    }

    pub(crate) const fn set(mut self, selector: StyleSelector, value: bool) -> Self {
        let v = selector as u8;
        if value {
            self.selectors |= v;
        } else {
            self.selectors &= !v;
        }
        self
    }

    pub(crate) fn has(self, selector: StyleSelector) -> bool {
        let v = selector as u8;
        self.selectors & v == v
    }

    pub(crate) fn union(self, other: StyleSelectors) -> StyleSelectors {
        StyleSelectors {
            selectors: self.selectors | other.selectors,
            responsive: self.responsive | other.responsive,
        }
    }

    pub(crate) const fn responsive(mut self) -> Self {
        self.responsive = true;
        self
    }

    pub(crate) fn has_responsive(self) -> bool {
        self.responsive
    }
}

impl StyleSelectors {
    /// Returns a formatted string representation of the active selectors
    pub fn debug_string(&self) -> String {
        let parts = self.active_selectors();

        if parts.is_empty() {
            if self.responsive {
                "Responsive".to_string()
            } else {
                "None".to_string()
            }
        } else {
            let selector_str = parts.join(" + ");
            if self.responsive {
                format!("{} (Responsive)", selector_str)
            } else {
                selector_str
            }
        }
    }

    /// Returns a vector of individual selector names
    pub fn active_selectors(&self) -> Vec<&'static str> {
        StyleSelector::all()
            .iter()
            .filter(|&&selector| self.has(selector))
            .map(|&selector| selector.name())
            .collect()
    }

    /// Returns true if any selectors are active
    pub fn is_empty(&self) -> bool {
        self.selectors == 0 && !self.responsive
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerEvents {
    Auto,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextOverflow {
    Wrap,
    Clip,
    Ellipsis,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum CursorStyle {
    #[default]
    Default,
    Pointer,
    Progress,
    Wait,
    Crosshair,
    Text,
    Move,
    Grab,
    Grabbing,
    ColResize,
    RowResize,
    WResize,
    EResize,
    SResize,
    NResize,
    NwResize,
    NeResize,
    SwResize,
    SeResize,
    NeswResize,
    NwseResize,
}

/// Structure holding data about the shadow.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxShadow {
    pub blur_radius: PxPct,
    pub color: Color,
    pub spread: PxPct,

    pub left_offset: PxPct,
    pub right_offset: PxPct,
    pub top_offset: PxPct,
    pub bottom_offset: PxPct,
}

impl BoxShadow {
    /// Create new default shadow.
    pub fn new() -> Self {
        Self::default()
    }

    /// Specifies shadow blur. The larger this value, the bigger the blur,
    /// so the shadow becomes bigger and lighter.
    pub fn blur_radius(mut self, radius: impl Into<PxPct>) -> Self {
        self.blur_radius = radius.into();
        self
    }

    /// Specifies shadow blur spread. Positive values will cause the shadow
    /// to expand and grow bigger, negative values will cause the shadow to shrink.
    pub fn spread(mut self, spread: impl Into<PxPct>) -> Self {
        self.spread = spread.into();
        self
    }

    /// Specifies color for the current shadow.
    pub fn color(mut self, color: impl Into<Color>) -> Self {
        self.color = color.into();
        self
    }

    /// Specifies the offset of the left edge.
    pub fn left_offset(mut self, left_offset: impl Into<PxPct>) -> Self {
        self.left_offset = left_offset.into();
        self
    }

    /// Specifies the offset of the right edge.
    pub fn right_offset(mut self, right_offset: impl Into<PxPct>) -> Self {
        self.right_offset = right_offset.into();
        self
    }

    /// Specifies the offset of the top edge.
    pub fn top_offset(mut self, top_offset: impl Into<PxPct>) -> Self {
        self.top_offset = top_offset.into();
        self
    }

    /// Specifies the offset of the bottom edge.
    pub fn bottom_offset(mut self, bottom_offset: impl Into<PxPct>) -> Self {
        self.bottom_offset = bottom_offset.into();
        self
    }

    /// Specifies the offset on vertical axis.
    /// Negative offset value places the shadow above the element.
    pub fn v_offset(mut self, v_offset: impl Into<PxPct>) -> Self {
        let offset = v_offset.into();
        self.top_offset = -offset;
        self.bottom_offset = offset;
        self
    }

    /// Specifies the offset on horizontal axis.
    /// Negative offset value places the shadow to the left of the element.
    pub fn h_offset(mut self, h_offset: impl Into<PxPct>) -> Self {
        let offset = h_offset.into();
        self.left_offset = -offset;
        self.right_offset = offset;
        self
    }
}

impl Default for BoxShadow {
    fn default() -> Self {
        Self {
            blur_radius: PxPct::Px(0.),
            color: palette::css::BLACK,
            spread: PxPct::Px(0.),
            left_offset: PxPct::Px(0.),
            right_offset: PxPct::Px(0.),
            top_offset: PxPct::Px(0.),
            bottom_offset: PxPct::Px(0.),
        }
    }
}

/// Structure holding border widths for all four sides
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Border {
    pub left: Option<StrokeWrap>,
    pub top: Option<StrokeWrap>,
    pub right: Option<StrokeWrap>,
    pub bottom: Option<StrokeWrap>,
}

impl Border {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn all(border: impl Into<StrokeWrap>) -> Self {
        let border = border.into();
        Self {
            left: Some(border.clone()),
            top: Some(border.clone()),
            right: Some(border.clone()),
            bottom: Some(border),
        }
    }

    pub fn left(mut self, border: impl Into<StrokeWrap>) -> Self {
        self.left = Some(border.into());
        self
    }

    pub fn top(mut self, border: impl Into<StrokeWrap>) -> Self {
        self.top = Some(border.into());
        self
    }

    pub fn right(mut self, border: impl Into<StrokeWrap>) -> Self {
        self.right = Some(border.into());
        self
    }

    pub fn bottom(mut self, border: impl Into<StrokeWrap>) -> Self {
        self.bottom = Some(border.into());
        self
    }

    pub fn horiz(mut self, border: impl Into<StrokeWrap>) -> Self {
        let border = border.into();
        self.left = Some(border.clone());
        self.right = Some(border);
        self
    }

    pub fn vert(mut self, border: impl Into<StrokeWrap>) -> Self {
        let border = border.into();
        self.top = Some(border.clone());
        self.bottom = Some(border);
        self
    }
}

impl StylePropValue for Border {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        let border = self.clone();
        let details_view = move || {
            let sides = [
                ("Left:", border.left),
                ("Top:", border.top),
                ("Right:", border.right),
                ("Bottom:", border.bottom),
            ];

            v_stack_from_iter(
                sides
                    .into_iter()
                    .filter_map(|(l, v)| v.map(|v| (l, v)))
                    .map(|(label, value)| {
                        h_stack((
                            label.style(|s| s.font_weight(Weight::BOLD).width(80.0)),
                            value.debug_view().unwrap(),
                        ))
                        .style(|s| s.items_center().gap(4.0))
                        .into_any()
                    }),
            )
            .style(|s| s.gap(4.0).padding(8.0))
        };
        Some(details_view().into_any())
    }

    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        Some(Self {
            left: self.left.interpolate(&other.left, value)?,
            top: self.top.interpolate(&other.top, value)?,
            right: self.right.interpolate(&other.right, value)?,
            bottom: self.bottom.interpolate(&other.bottom, value)?,
        })
    }

    fn combine(&self, other: &Self) -> CombineResult<Self> {
        let result = Border {
            left: other.left.clone().or_else(|| self.left.clone()),
            top: other.top.clone().or_else(|| self.top.clone()),
            right: other.right.clone().or_else(|| self.right.clone()),
            bottom: other.bottom.clone().or_else(|| self.bottom.clone()),
        };

        if result == *other {
            CombineResult::Other
        } else {
            CombineResult::New(result)
        }
    }
}

/// Structure holding border colors for all four sides
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BorderColor {
    pub left: Option<Brush>,
    pub top: Option<Brush>,
    pub right: Option<Brush>,
    pub bottom: Option<Brush>,
}

impl BorderColor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn all(color: impl Into<Brush>) -> Self {
        let color = color.into();
        Self {
            left: Some(color.clone()),
            top: Some(color.clone()),
            right: Some(color.clone()),
            bottom: Some(color),
        }
    }

    pub fn left(mut self, color: impl Into<Brush>) -> Self {
        self.left = Some(color.into());
        self
    }

    pub fn top(mut self, color: impl Into<Brush>) -> Self {
        self.top = Some(color.into());
        self
    }

    pub fn right(mut self, color: impl Into<Brush>) -> Self {
        self.right = Some(color.into());
        self
    }

    pub fn bottom(mut self, color: impl Into<Brush>) -> Self {
        self.bottom = Some(color.into());
        self
    }

    pub fn horiz(mut self, color: impl Into<Brush>) -> Self {
        let color = color.into();
        self.left = Some(color.clone());
        self.right = Some(color);
        self
    }

    pub fn vert(mut self, color: impl Into<Brush>) -> Self {
        let color = color.into();
        self.top = Some(color.clone());
        self.bottom = Some(color);
        self
    }
}

impl StylePropValue for BorderColor {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        let border_color = self.clone();
        let details_view = move || {
            let sides = [
                ("Left:", border_color.left),
                ("Top:", border_color.top),
                ("Right:", border_color.right),
                ("Bottom:", border_color.bottom),
            ];

            v_stack_from_iter(
                sides
                    .into_iter()
                    .filter_map(|(l, v)| v.map(|v| (l, v)))
                    .map(|(label, color)| {
                        h_stack((
                            label.style(|s| s.font_weight(Weight::BOLD).width(80.0)),
                            color.debug_view().unwrap(),
                        ))
                        .style(|s| s.items_center().gap(4.0))
                    }),
            )
            .style(|s| s.gap(4.0).padding(8.0))
        };
        Some(details_view().into_any())
    }

    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        Some(Self {
            left: self.left.interpolate(&other.left, value)?,
            top: self.top.interpolate(&other.top, value)?,
            right: self.right.interpolate(&other.right, value)?,
            bottom: self.bottom.interpolate(&other.bottom, value)?,
        })
    }

    fn combine(&self, other: &Self) -> CombineResult<Self> {
        let result = BorderColor {
            left: other.left.clone().or_else(|| self.left.clone()),
            top: other.top.clone().or_else(|| self.top.clone()),
            right: other.right.clone().or_else(|| self.right.clone()),
            bottom: other.bottom.clone().or_else(|| self.bottom.clone()),
        };

        if result == *other {
            CombineResult::Other
        } else {
            CombineResult::New(result)
        }
    }
}

/// Structure holding border radius for all four corners
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BorderRadius {
    pub top_left: Option<PxPct>,
    pub top_right: Option<PxPct>,
    pub bottom_left: Option<PxPct>,
    pub bottom_right: Option<PxPct>,
}

impl BorderRadius {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn all(radius: impl Into<PxPct>) -> Self {
        let radius = radius.into();
        Self {
            top_left: Some(radius),
            top_right: Some(radius),
            bottom_left: Some(radius),
            bottom_right: Some(radius),
        }
    }

    pub fn top_left(mut self, radius: impl Into<PxPct>) -> Self {
        self.top_left = Some(radius.into());
        self
    }

    pub fn top_right(mut self, radius: impl Into<PxPct>) -> Self {
        self.top_right = Some(radius.into());
        self
    }

    pub fn bottom_left(mut self, radius: impl Into<PxPct>) -> Self {
        self.bottom_left = Some(radius.into());
        self
    }

    pub fn bottom_right(mut self, radius: impl Into<PxPct>) -> Self {
        self.bottom_right = Some(radius.into());
        self
    }

    pub fn top(mut self, radius: impl Into<PxPct>) -> Self {
        let radius = radius.into();
        self.top_left = Some(radius);
        self.top_right = Some(radius);
        self
    }

    pub fn bottom(mut self, radius: impl Into<PxPct>) -> Self {
        let radius = radius.into();
        self.bottom_left = Some(radius);
        self.bottom_right = Some(radius);
        self
    }

    pub fn left(mut self, radius: impl Into<PxPct>) -> Self {
        let radius = radius.into();
        self.top_left = Some(radius);
        self.bottom_left = Some(radius);
        self
    }

    pub fn right(mut self, radius: impl Into<PxPct>) -> Self {
        let radius = radius.into();
        self.top_right = Some(radius);
        self.bottom_right = Some(radius);
        self
    }
}

impl StylePropValue for BorderRadius {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        let border_radius = *self;
        let details_view = move || {
            let corners = [
                ("Top Left:", border_radius.top_left),
                ("Top Right:", border_radius.top_right),
                ("Bottom Left:", border_radius.bottom_left),
                ("Bottom Right:", border_radius.bottom_right),
            ];

            v_stack_from_iter(
                corners
                    .into_iter()
                    .filter_map(|(l, v)| v.map(|v| (l, v)))
                    .map(|(label, radius)| {
                        h_stack((
                            label.style(|s| s.font_weight(Weight::BOLD).width(80.0)),
                            radius.debug_view().unwrap(),
                        ))
                        .style(|s| s.items_center().gap(4.0))
                    }),
            )
            .style(|s| s.gap(4.0).padding(8.0))
        };
        Some(details_view().into_any())
    }

    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        Some(Self {
            top_left: self.top_left.interpolate(&other.top_left, value)?,
            top_right: self.top_right.interpolate(&other.top_right, value)?,
            bottom_left: self.bottom_left.interpolate(&other.bottom_left, value)?,
            bottom_right: self.bottom_right.interpolate(&other.bottom_right, value)?,
        })
    }

    fn combine(&self, other: &Self) -> CombineResult<Self> {
        let result = BorderRadius {
            top_left: other.top_left.or(self.top_left),
            top_right: other.top_right.or(self.top_right),
            bottom_left: other.bottom_left.or(self.bottom_left),
            bottom_right: other.bottom_right.or(self.bottom_right),
        };

        if result == *other {
            CombineResult::Other
        } else {
            CombineResult::New(result)
        }
    }
}

/// Structure holding padding values for all four sides
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Padding {
    pub left: Option<PxPct>,
    pub top: Option<PxPct>,
    pub right: Option<PxPct>,
    pub bottom: Option<PxPct>,
}

impl Padding {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn all(padding: impl Into<PxPct>) -> Self {
        let padding = padding.into();
        Self {
            left: Some(padding),
            top: Some(padding),
            right: Some(padding),
            bottom: Some(padding),
        }
    }

    pub fn left(mut self, padding: impl Into<PxPct>) -> Self {
        self.left = Some(padding.into());
        self
    }

    pub fn top(mut self, padding: impl Into<PxPct>) -> Self {
        self.top = Some(padding.into());
        self
    }

    pub fn right(mut self, padding: impl Into<PxPct>) -> Self {
        self.right = Some(padding.into());
        self
    }

    pub fn bottom(mut self, padding: impl Into<PxPct>) -> Self {
        self.bottom = Some(padding.into());
        self
    }

    pub fn horiz(mut self, padding: impl Into<PxPct>) -> Self {
        let padding = padding.into();
        self.left = Some(padding);
        self.right = Some(padding);
        self
    }

    pub fn vert(mut self, padding: impl Into<PxPct>) -> Self {
        let padding = padding.into();
        self.top = Some(padding);
        self.bottom = Some(padding);
        self
    }
}

impl StylePropValue for Padding {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        let padding = *self;
        let details_view = move || {
            let sides = [
                ("Left:", padding.left),
                ("Top:", padding.top),
                ("Right:", padding.right),
                ("Bottom:", padding.bottom),
            ];

            v_stack_from_iter(
                sides
                    .into_iter()
                    .filter_map(|(l, v)| v.map(|v| (l, v)))
                    .map(|(label, padding)| {
                        h_stack((
                            label.style(|s| s.font_weight(Weight::BOLD).width(80.0)),
                            padding.debug_view().unwrap(),
                        ))
                        .style(|s| s.items_center().gap(4.0))
                    }),
            )
            .style(|s| s.gap(4.0).padding(8.0))
        };
        Some(details_view().into_any())
    }

    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        Some(Self {
            left: self.left.interpolate(&other.left, value)?,
            top: self.top.interpolate(&other.top, value)?,
            right: self.right.interpolate(&other.right, value)?,
            bottom: self.bottom.interpolate(&other.bottom, value)?,
        })
    }

    fn combine(&self, other: &Self) -> CombineResult<Self> {
        let result = Padding {
            left: other.left.or(self.left),
            top: other.top.or(self.top),
            right: other.right.or(self.right),
            bottom: other.bottom.or(self.bottom),
        };

        if result == *other {
            CombineResult::Other
        } else {
            CombineResult::New(result)
        }
    }
}

/// Structure holding margin values for all four sides
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Margin {
    pub left: Option<PxPctAuto>,
    pub top: Option<PxPctAuto>,
    pub right: Option<PxPctAuto>,
    pub bottom: Option<PxPctAuto>,
}

impl Margin {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn all(margin: impl Into<PxPctAuto>) -> Self {
        let margin = margin.into();
        Self {
            left: Some(margin),
            top: Some(margin),
            right: Some(margin),
            bottom: Some(margin),
        }
    }

    pub fn left(mut self, margin: impl Into<PxPctAuto>) -> Self {
        self.left = Some(margin.into());
        self
    }

    pub fn top(mut self, margin: impl Into<PxPctAuto>) -> Self {
        self.top = Some(margin.into());
        self
    }

    pub fn right(mut self, margin: impl Into<PxPctAuto>) -> Self {
        self.right = Some(margin.into());
        self
    }

    pub fn bottom(mut self, margin: impl Into<PxPctAuto>) -> Self {
        self.bottom = Some(margin.into());
        self
    }

    pub fn horiz(mut self, margin: impl Into<PxPctAuto>) -> Self {
        let margin = margin.into();
        self.left = Some(margin);
        self.right = Some(margin);
        self
    }

    pub fn vert(mut self, margin: impl Into<PxPctAuto>) -> Self {
        let margin = margin.into();
        self.top = Some(margin);
        self.bottom = Some(margin);
        self
    }
}

impl StylePropValue for Margin {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        let margin = *self;
        let details_view = move || {
            let sides = [
                ("Left:", margin.left),
                ("Top:", margin.top),
                ("Right:", margin.right),
                ("Bottom:", margin.bottom),
            ];

            v_stack_from_iter(
                sides
                    .into_iter()
                    .filter_map(|(l, v)| v.map(|v| (l, v)))
                    .map(|(label, margin)| {
                        h_stack((
                            label.style(|s| s.font_weight(Weight::BOLD).width(80.0)),
                            margin.debug_view().unwrap(),
                        ))
                        .style(|s| s.items_center().gap(4.0))
                    }),
            )
            .style(|s| s.gap(4.0).padding(8.0))
        };
        Some(details_view().into_any())
    }

    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        Some(Self {
            left: self.left.interpolate(&other.left, value)?,
            top: self.top.interpolate(&other.top, value)?,
            right: self.right.interpolate(&other.right, value)?,
            bottom: self.bottom.interpolate(&other.bottom, value)?,
        })
    }

    fn combine(&self, other: &Self) -> CombineResult<Self> {
        let result = Margin {
            left: other.left.or(self.left),
            top: other.top.or(self.top),
            right: other.right.or(self.right),
            bottom: other.bottom.or(self.bottom),
        };

        if result == *other {
            CombineResult::Other
        } else {
            CombineResult::New(result)
        }
    }
}

/// The value for a [`Style`] property
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StyleValue<T> {
    // A value that has been inserted into the map by an animation.
    Animated(T),
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

macro_rules! define_builtin_props {
    (
        $($type_name:ident $name:ident $($opt:ident)?:
            $typ:ty { $($options:tt)* } = $val:expr),*
        $(,)?
    ) => {
        $(
            prop!(pub $type_name: $typ { $($options)* } = $val);
        )*
        impl Style {
            $(
                define_builtin_props!(decl: $type_name $name $($opt)?: $typ = $val);
            )*
        }

        impl BuiltinStyle<'_> {
            $(
                pub fn $name(&self) -> $typ {
                    self.style.get($type_name)
                }
            )*
        }
    };
    (decl: $type_name:ident $name:ident nocb: $typ:ty = $val:expr) => {};
    (decl: $type_name:ident $name:ident: $typ:ty = $val:expr) => {
        pub fn $name(self, v: impl Into<$typ>) -> Self {
            self.set($type_name, v.into())
        }
    }
}

pub struct BuiltinStyle<'a> {
    style: &'a Style,
}

define_builtin_props!(
    DisplayProp display: Display {} = Display::Flex,
    PositionProp position: Position {} = Position::Relative,
    Width width: PxPctAuto {} = PxPctAuto::Auto,
    Height height: PxPctAuto {} = PxPctAuto::Auto,
    MinWidth min_width: PxPctAuto {} = PxPctAuto::Auto,
    MinHeight min_height: PxPctAuto {} = PxPctAuto::Auto,
    MaxWidth max_width: PxPctAuto {} = PxPctAuto::Auto,
    MaxHeight max_height: PxPctAuto {} = PxPctAuto::Auto,
    FlexDirectionProp flex_direction: FlexDirection {} = FlexDirection::Row,
    FlexWrapProp flex_wrap: FlexWrap {} = FlexWrap::NoWrap,
    FlexGrow flex_grow: f32 {} = 0.0,
    FlexShrink flex_shrink: f32 {} = 1.0,
    FlexBasis flex_basis: PxPctAuto {} = PxPctAuto::Auto,
    JustifyContentProp justify_content: Option<JustifyContent> {} = None,
    JustifyItemsProp justify_items: Option<JustifyItems> {} = None,
    BoxSizingProp box_sizing: Option<BoxSizing> {} = None,
    JustifySelf justify_self: Option<AlignItems> {} = None,
    AlignItemsProp align_items: Option<AlignItems> {} = None,
    AlignContentProp align_content: Option<AlignContent> {} = None,
    GridTemplateRows grid_template_rows: Vec<GridTemplateComponent<String>> {} = Vec::new(),
    GridTemplateColumns grid_template_columns: Vec<GridTemplateComponent<String>> {} = Vec::new(),
    GridAutoRows grid_auto_rows: Vec<MinMax<MinTrackSizingFunction, MaxTrackSizingFunction>> {} = Vec::new(),
    GridAutoColumns grid_auto_columns: Vec<MinMax<MinTrackSizingFunction, MaxTrackSizingFunction>> {} = Vec::new(),
    GridAutoFlow grid_auto_flow: taffy::GridAutoFlow {} = taffy::GridAutoFlow::Row,
    GridRow grid_row: Line<GridPlacement> {} = Line::default(),
    GridColumn grid_column: Line<GridPlacement> {} = Line::default(),
    AlignSelf align_self: Option<AlignItems> {} = None,
    OutlineColor outline_color: Brush {} = Brush::Solid(palette::css::TRANSPARENT),
    Outline outline nocb: StrokeWrap {} = StrokeWrap::new(0.),
    OutlineProgress outline_progress: Pct {} = Pct(100.),
    BorderProgress border_progress: Pct {} = Pct(100.),
    BorderProp border_combined nocb: Border {} = Border::default(),
    BorderColorProp border_color_combined nocb: BorderColor {} = BorderColor::default(),
    BorderRadiusProp border_radius_combined nocb: BorderRadius {} = BorderRadius::default(),
    PaddingProp padding_combined nocb: Padding {} = Padding::default(),
    MarginProp margin_combined nocb: Margin {} = Margin::default(),
    InsetLeft inset_left: PxPctAuto {} = PxPctAuto::Auto,
    InsetTop inset_top: PxPctAuto {} = PxPctAuto::Auto,
    InsetRight inset_right: PxPctAuto {} = PxPctAuto::Auto,
    InsetBottom inset_bottom: PxPctAuto {} = PxPctAuto::Auto,
    PointerEventsProp pointer_events: Option<PointerEvents> { inherited } = None,
    ZIndex z_index nocb: Option<i32> {} = None,
    Cursor cursor nocb: Option<CursorStyle> {} = None,
    TextColor color nocb: Option<Color> { inherited } = None,
    Background background nocb: Option<Brush> {} = None,
    Foreground foreground nocb: Option<Brush> {} = None,
    BoxShadowProp box_shadow nocb: SmallVec<[BoxShadow; 3]> {} = SmallVec::new(),
    FontSize font_size nocb: Option<f32> { inherited } = None,
    FontFamily font_family nocb: Option<String> { inherited } = None,
    FontWeight font_weight nocb: Option<Weight> { inherited } = None,
    FontStyle font_style nocb: Option<crate::text::Style> { inherited } = None,
    CursorColor cursor_color nocb: Brush {} = Brush::Solid(palette::css::BLACK.with_alpha(0.3)),
    SelectionCornerRadius selection_corer_radius nocb: f64 {} = 1.,
    Selectable selectable: bool { inherited } = true,
    TextOverflowProp text_overflow: TextOverflow {} = TextOverflow::Wrap,
    TextAlignProp text_align: Option<crate::text::Align> {} = None,
    LineHeight line_height nocb: Option<LineHeightValue> { inherited } = None,
    AspectRatio aspect_ratio: Option<f32> {} = None,
    ColGap col_gap nocb: PxPct {} = PxPct::Px(0.),
    RowGap row_gap nocb: PxPct {} = PxPct::Px(0.),
    ScaleX scale_x: Pct {} = Pct(100.),
    ScaleY scale_y: Pct {} = Pct(100.),
    TranslateX translate_x: PxPct {} = PxPct::Px(0.),
    TranslateY translate_y: PxPct {} = PxPct::Px(0.),
    Rotation rotate: Px {} = Px(0.),
    Selected set_selected: bool { inherited } = false,
    Disabled set_disabled: bool { inherited } = false,
    Hidden set_hidden: bool { inherited } = false,
    Focusable focusable: bool { } = false,
    Draggable draggable: bool { } = false,
);

impl BuiltinStyle<'_> {
    // Individual padding accessors
    pub fn padding_left(&self) -> PxPct {
        self.style.get(PaddingProp).left.unwrap_or(PxPct::Px(0.0))
    }
    pub fn padding_top(&self) -> PxPct {
        self.style.get(PaddingProp).top.unwrap_or(PxPct::Px(0.0))
    }
    pub fn padding_right(&self) -> PxPct {
        self.style.get(PaddingProp).right.unwrap_or(PxPct::Px(0.0))
    }
    pub fn padding_bottom(&self) -> PxPct {
        self.style.get(PaddingProp).bottom.unwrap_or(PxPct::Px(0.0))
    }

    // Individual margin accessors
    pub fn margin_left(&self) -> PxPctAuto {
        self.style
            .get(MarginProp)
            .left
            .unwrap_or(PxPctAuto::Px(0.0))
    }
    pub fn margin_top(&self) -> PxPctAuto {
        self.style.get(MarginProp).top.unwrap_or(PxPctAuto::Px(0.0))
    }
    pub fn margin_right(&self) -> PxPctAuto {
        self.style
            .get(MarginProp)
            .right
            .unwrap_or(PxPctAuto::Px(0.0))
    }
    pub fn margin_bottom(&self) -> PxPctAuto {
        self.style
            .get(MarginProp)
            .bottom
            .unwrap_or(PxPctAuto::Px(0.0))
    }
}

prop!(
    /// How children overflowing their container in Y axis should affect layout
    pub OverflowX: Overflow {} = Overflow::default()
);

prop!(
    /// How children overflowing their container in X axis should affect layout
    pub OverflowY: Overflow {} = Overflow::default()
);

prop_extractor! {
    pub FontProps {
        pub size: FontSize,
        pub family: FontFamily,
        pub weight: FontWeight,
        pub style: FontStyle,
    }
}

prop_extractor! {
    pub(crate) LayoutProps {
        pub border: BorderProp,
        pub padding: PaddingProp,
        pub margin: MarginProp,

        pub width: Width,
        pub height: Height,

        pub min_width: MinWidth,
        pub min_height: MinHeight,

        pub max_width: MaxWidth,
        pub max_height: MaxHeight,

        pub flex_grow: FlexGrow,
        pub flex_shrink: FlexShrink,
        pub flex_basis: FlexBasis ,

        pub inset_left: InsetLeft,
        pub inset_top: InsetTop,
        pub inset_right: InsetRight,
        pub inset_bottom: InsetBottom,

        pub row_gap: RowGap,
        pub col_gap: ColGap,

        pub scale_x: ScaleX,
        pub scale_y: ScaleY,

        pub translate_x: TranslateX,
        pub translate_y: TranslateY,

        pub rotation: Rotation,
    }
}

impl LayoutProps {
    pub fn to_style(&self) -> Style {
        let border = self.border();
        let padding = self.padding();
        let margin = self.margin();
        Style::new()
            .width(self.width())
            .height(self.height())
            .apply_border(border)
            .apply_padding(padding)
            .min_width(self.min_width())
            .min_height(self.min_height())
            .max_width(self.max_width())
            .max_height(self.max_height())
            .flex_grow(self.flex_grow())
            .flex_shrink(self.flex_shrink())
            .flex_basis(self.flex_basis())
            .inset_left(self.inset_left())
            .inset_top(self.inset_top())
            .inset_right(self.inset_right())
            .inset_bottom(self.inset_bottom())
            .apply_margin(margin)
            .col_gap(self.col_gap())
            .row_gap(self.row_gap())
    }
}

prop_extractor! {
    pub SelectionStyle {
        pub corner_radius: SelectionCornerRadius,
        pub selection_color: CursorColor,
    }
}

impl Style {
    pub fn get<P: StyleProp>(&self, _prop: P) -> P::Type {
        self.get_prop_or_default::<P>()
    }

    pub fn get_style_value<P: StyleProp>(&self, _prop: P) -> StyleValue<P::Type> {
        self.get_prop_style_value::<P>()
    }

    pub fn set<P: StyleProp>(self, prop: P, value: impl Into<P::Type>) -> Self {
        self.set_style_value(prop, StyleValue::Val(value.into()))
    }

    pub fn set_style_value<P: StyleProp>(mut self, _prop: P, value: StyleValue<P::Type>) -> Self {
        let insert = match value {
            StyleValue::Val(value) => StyleMapValue::Val(value),
            StyleValue::Animated(value) => StyleMapValue::Animated(value),
            StyleValue::Unset => StyleMapValue::Unset,
            StyleValue::Base => {
                self.map.remove(&P::key());
                return self;
            }
        };
        self.map.insert(P::key(), Rc::new(insert));
        self
    }

    pub fn transition<P: StyleProp>(mut self, _prop: P, transition: Transition) -> Self {
        self.map
            .insert(P::prop_ref().info().transition_key, Rc::new(transition));
        self
    }

    fn selector(mut self, selector: StyleSelector, style: impl FnOnce(Style) -> Style) -> Self {
        let over = style(Style::default());
        self.set_selector(selector, over);
        self
    }

    /// The visual style to apply when the mouse hovers over the element
    pub fn hover(self, style: impl FnOnce(Style) -> Style) -> Self {
        self.selector(StyleSelector::Hover, style)
    }

    pub fn focus(self, style: impl FnOnce(Style) -> Style) -> Self {
        self.selector(StyleSelector::Focus, style)
    }

    /// Similar to the `:focus-visible` css selector, this style only activates when tab navigation is used.
    pub fn focus_visible(self, style: impl FnOnce(Style) -> Style) -> Self {
        self.selector(StyleSelector::FocusVisible, style)
    }

    pub fn selected(self, style: impl FnOnce(Style) -> Style) -> Self {
        self.selector(StyleSelector::Selected, style)
    }

    /// The visual style to apply when the view is being dragged
    pub fn drag(self, style: impl FnOnce(Style) -> Style) -> Self {
        self.selector(StyleSelector::Dragging, style)
    }

    pub fn disabled(self, style: impl FnOnce(Style) -> Style) -> Self {
        self.selector(StyleSelector::Disabled, style)
    }

    pub fn dark_mode(self, style: impl FnOnce(Style) -> Style) -> Self {
        self.selector(StyleSelector::DarkMode, style)
    }

    pub fn file_hover(self, style: impl FnOnce(Style) -> Style) -> Self {
        self.selector(StyleSelector::FileHover, style)
    }

    pub fn active(self, style: impl FnOnce(Style) -> Style) -> Self {
        self.selector(StyleSelector::Active, style)
    }

    pub fn responsive(mut self, size: ScreenSize, style: impl FnOnce(Style) -> Style) -> Self {
        let over = style(Style::default());
        for breakpoint in size.breakpoints() {
            self.set_breakpoint(breakpoint, over.clone());
        }
        self
    }

    pub fn class<C: StyleClass>(mut self, _class: C, style: impl FnOnce(Style) -> Style) -> Self {
        let over = style(Style::default());
        self.set_class(C::class_ref(), over);
        self
    }

    /// Applies a `CustomStyle` type to the `CustomStyle`'s associated style class.
    ///
    /// For example: if the `CustomStyle` you use is `DropdownCustomStyle` then it
    /// will apply the custom style to that custom style type's associated style class
    /// which, in this example, is `DropdownClass`.
    ///
    /// This is especially useful when building a stylesheet or targeting a child view.
    ///
    /// # Examples
    /// ```
    /// // In a style sheet or on a parent view
    /// use floem::prelude::*;
    /// use floem::style::Style;
    /// Style::new().custom_style_class(|s: dropdown::DropdownCustomStyle| s.close_on_accept(false));
    /// // This property is now set on the `DropdownClass` class and will be applied to any dropdowns that are children of this view.
    /// ```
    ///
    /// See also: [`Style::custom`](Self::custom) and [`Style::apply_custom`](Self::apply_custom).
    pub fn custom_style_class<CS: CustomStyle>(mut self, style: impl FnOnce(CS) -> CS) -> Self {
        let over = style(CS::default());
        self.set_class(CS::StyleClass::class_ref(), over.into());
        self
    }

    pub fn width_full(self) -> Self {
        self.width_pct(100.0)
    }

    pub fn width_pct(self, width: f64) -> Self {
        self.width(width.pct())
    }

    pub fn height_full(self) -> Self {
        self.height_pct(100.0)
    }

    pub fn height_pct(self, height: f64) -> Self {
        self.height(height.pct())
    }

    pub fn col_gap(self, width: impl Into<PxPct>) -> Self {
        self.set(ColGap, width.into())
    }

    pub fn row_gap(self, height: impl Into<PxPct>) -> Self {
        self.set(RowGap, height.into())
    }

    pub fn row_col_gap(self, width: impl Into<PxPct>, height: impl Into<PxPct>) -> Self {
        self.col_gap(width).row_gap(height)
    }

    pub fn gap(self, gap: impl Into<PxPct>) -> Self {
        let gap = gap.into();
        self.col_gap(gap).row_gap(gap)
    }

    pub fn size(self, width: impl Into<PxPctAuto>, height: impl Into<PxPctAuto>) -> Self {
        self.width(width).height(height)
    }

    pub fn size_full(self) -> Self {
        self.size_pct(100.0, 100.0)
    }

    pub fn size_pct(self, width: f64, height: f64) -> Self {
        self.width(width.pct()).height(height.pct())
    }

    pub fn min_width_full(self) -> Self {
        self.min_width_pct(100.0)
    }

    pub fn min_width_pct(self, min_width: f64) -> Self {
        self.min_width(min_width.pct())
    }

    pub fn min_height_full(self) -> Self {
        self.min_height_pct(100.0)
    }

    pub fn min_height_pct(self, min_height: f64) -> Self {
        self.min_height(min_height.pct())
    }

    pub fn min_size_full(self) -> Self {
        self.min_size_pct(100.0, 100.0)
    }

    pub fn min_size(
        self,
        min_width: impl Into<PxPctAuto>,
        min_height: impl Into<PxPctAuto>,
    ) -> Self {
        self.min_width(min_width).min_height(min_height)
    }

    pub fn min_size_pct(self, min_width: f64, min_height: f64) -> Self {
        self.min_size(min_width.pct(), min_height.pct())
    }

    pub fn max_width_full(self) -> Self {
        self.max_width_pct(100.0)
    }

    pub fn max_width_pct(self, max_width: f64) -> Self {
        self.max_width(max_width.pct())
    }

    pub fn max_height_full(self) -> Self {
        self.max_height_pct(100.0)
    }

    pub fn max_height_pct(self, max_height: f64) -> Self {
        self.max_height(max_height.pct())
    }

    pub fn max_size(
        self,
        max_width: impl Into<PxPctAuto>,
        max_height: impl Into<PxPctAuto>,
    ) -> Self {
        self.max_width(max_width).max_height(max_height)
    }

    pub fn max_size_full(self) -> Self {
        self.max_size_pct(100.0, 100.0)
    }

    pub fn max_size_pct(self, max_width: f64, max_height: f64) -> Self {
        self.max_size(max_width.pct(), max_height.pct())
    }

    pub fn border_color(self, color: impl Into<Brush>) -> Self {
        self.set(BorderColorProp, BorderColor::all(color))
    }

    pub fn border(self, border: impl Into<StrokeWrap>) -> Self {
        self.set(BorderProp, Border::all(border))
    }

    pub fn outline(self, outline: impl Into<StrokeWrap>) -> Self {
        self.set_style_value(Outline, StyleValue::Val(outline.into()))
    }

    /// Sets `border_left` and `border_right` to `border`
    pub fn border_horiz(self, border: impl Into<StrokeWrap>) -> Self {
        let mut current = self.get(BorderProp);
        let border = border.into();
        current.left = Some(border.clone());
        current.right = Some(border);
        self.set(BorderProp, current)
    }

    /// Sets `border_top` and `border_bottom` to `border`
    pub fn border_vert(self, border: impl Into<StrokeWrap>) -> Self {
        let mut current = self.get(BorderProp);
        let border = border.into();
        current.top = Some(border.clone());
        current.bottom = Some(border);
        self.set(BorderProp, current)
    }

    pub fn padding_left_pct(self, padding: f64) -> Self {
        self.padding_left(padding.pct())
    }

    pub fn padding_right_pct(self, padding: f64) -> Self {
        self.padding_right(padding.pct())
    }

    pub fn padding_top_pct(self, padding: f64) -> Self {
        self.padding_top(padding.pct())
    }

    pub fn padding_bottom_pct(self, padding: f64) -> Self {
        self.padding_bottom(padding.pct())
    }

    /// Set padding on all directions
    pub fn padding(self, padding: impl Into<PxPct>) -> Self {
        self.set(PaddingProp, Padding::all(padding))
    }

    pub fn padding_pct(self, padding: f64) -> Self {
        self.set(PaddingProp, Padding::all(padding.pct()))
    }

    /// Sets `padding_left` and `padding_right` to `padding`
    pub fn padding_horiz(self, padding: impl Into<PxPct>) -> Self {
        let mut current = self.get(PaddingProp);
        let padding = padding.into();
        current.left = Some(padding);
        current.right = Some(padding);
        self.set(PaddingProp, current)
    }

    pub fn padding_horiz_pct(self, padding: f64) -> Self {
        self.padding_horiz(padding.pct())
    }

    /// Sets `padding_top` and `padding_bottom` to `padding`
    pub fn padding_vert(self, padding: impl Into<PxPct>) -> Self {
        let mut current = self.get(PaddingProp);
        let padding = padding.into();
        current.top = Some(padding);
        current.bottom = Some(padding);
        self.set(PaddingProp, current)
    }

    pub fn padding_vert_pct(self, padding: f64) -> Self {
        self.padding_vert(padding.pct())
    }

    pub fn margin_left_pct(self, margin: f64) -> Self {
        self.margin_left(margin.pct())
    }

    pub fn margin_right_pct(self, margin: f64) -> Self {
        self.margin_right(margin.pct())
    }

    pub fn margin_top_pct(self, margin: f64) -> Self {
        self.margin_top(margin.pct())
    }

    pub fn margin_bottom_pct(self, margin: f64) -> Self {
        self.margin_bottom(margin.pct())
    }

    pub fn margin(self, margin: impl Into<PxPctAuto>) -> Self {
        self.set(MarginProp, Margin::all(margin))
    }

    pub fn margin_pct(self, margin: f64) -> Self {
        self.set(MarginProp, Margin::all(margin.pct()))
    }

    /// Sets `margin_left` and `margin_right` to `margin`
    pub fn margin_horiz(self, margin: impl Into<PxPctAuto>) -> Self {
        let mut current = self.get(MarginProp);
        let margin = margin.into();
        current.left = Some(margin);
        current.right = Some(margin);
        self.set(MarginProp, current)
    }

    pub fn margin_horiz_pct(self, margin: f64) -> Self {
        self.margin_horiz(margin.pct())
    }

    /// Sets `margin_top` and `margin_bottom` to `margin`
    pub fn margin_vert(self, margin: impl Into<PxPctAuto>) -> Self {
        let mut current = self.get(MarginProp);
        let margin = margin.into();
        current.top = Some(margin);
        current.bottom = Some(margin);
        self.set(MarginProp, current)
    }

    pub fn margin_vert_pct(self, margin: f64) -> Self {
        self.margin_vert(margin.pct())
    }

    // Individual padding methods using the combined struct
    pub fn padding_left(self, padding: impl Into<PxPct>) -> Self {
        let mut current = self.get(PaddingProp);
        current.left = Some(padding.into());
        self.set(PaddingProp, current)
    }
    pub fn padding_right(self, padding: impl Into<PxPct>) -> Self {
        let mut current = self.get(PaddingProp);
        current.right = Some(padding.into());
        self.set(PaddingProp, current)
    }
    pub fn padding_top(self, padding: impl Into<PxPct>) -> Self {
        let mut current = self.get(PaddingProp);
        current.top = Some(padding.into());
        self.set(PaddingProp, current)
    }
    pub fn padding_bottom(self, padding: impl Into<PxPct>) -> Self {
        let mut current = self.get(PaddingProp);
        current.bottom = Some(padding.into());
        self.set(PaddingProp, current)
    }

    // Individual margin methods using the combined struct
    pub fn margin_left(self, margin: impl Into<PxPctAuto>) -> Self {
        let mut current = self.get(MarginProp);
        current.left = Some(margin.into());
        self.set(MarginProp, current)
    }
    pub fn margin_right(self, margin: impl Into<PxPctAuto>) -> Self {
        let mut current = self.get(MarginProp);
        current.right = Some(margin.into());
        self.set(MarginProp, current)
    }
    pub fn margin_top(self, margin: impl Into<PxPctAuto>) -> Self {
        let mut current = self.get(MarginProp);
        current.top = Some(margin.into());
        self.set(MarginProp, current)
    }
    pub fn margin_bottom(self, margin: impl Into<PxPctAuto>) -> Self {
        let mut current = self.get(MarginProp);
        current.bottom = Some(margin.into());
        self.set(MarginProp, current)
    }

    // Convenience methods for combined padding and margin properties
    pub fn apply_padding(self, padding: Padding) -> Self {
        self.set(PaddingProp, padding)
    }
    pub fn apply_margin(self, margin: Margin) -> Self {
        self.set(MarginProp, margin)
    }

    pub fn border_radius(self, radius: impl Into<PxPct>) -> Self {
        self.set(BorderRadiusProp, BorderRadius::all(radius))
    }

    // Individual border methods using the combined structs
    pub fn border_left(self, border: impl Into<StrokeWrap>) -> Self {
        let mut current = self.get(BorderProp);
        current.left = Some(border.into());
        self.set(BorderProp, current)
    }
    pub fn border_right(self, border: impl Into<StrokeWrap>) -> Self {
        let mut current = self.get(BorderProp);
        current.right = Some(border.into());
        self.set(BorderProp, current)
    }
    pub fn border_top(self, border: impl Into<StrokeWrap>) -> Self {
        let mut current = self.get(BorderProp);
        current.top = Some(border.into());
        self.set(BorderProp, current)
    }
    pub fn border_bottom(self, border: impl Into<StrokeWrap>) -> Self {
        let mut current = self.get(BorderProp);
        current.bottom = Some(border.into());
        self.set(BorderProp, current)
    }

    // Individual border color methods
    pub fn border_left_color(self, color: impl Into<Brush>) -> Self {
        let mut current = self.get(BorderColorProp);
        current.left = Some(color.into());
        self.set(BorderColorProp, current)
    }
    pub fn border_right_color(self, color: impl Into<Brush>) -> Self {
        let mut current = self.get(BorderColorProp);
        current.right = Some(color.into());
        self.set(BorderColorProp, current)
    }
    pub fn border_top_color(self, color: impl Into<Brush>) -> Self {
        let mut current = self.get(BorderColorProp);
        current.top = Some(color.into());
        self.set(BorderColorProp, current)
    }
    pub fn border_bottom_color(self, color: impl Into<Brush>) -> Self {
        let mut current = self.get(BorderColorProp);
        current.bottom = Some(color.into());
        self.set(BorderColorProp, current)
    }

    // Individual border radius methods
    pub fn border_top_left_radius(self, radius: impl Into<PxPct>) -> Self {
        let mut current = self.get(BorderRadiusProp);
        current.top_left = Some(radius.into());
        self.set(BorderRadiusProp, current)
    }
    pub fn border_top_right_radius(self, radius: impl Into<PxPct>) -> Self {
        let mut current = self.get(BorderRadiusProp);
        current.top_right = Some(radius.into());
        self.set(BorderRadiusProp, current)
    }
    pub fn border_bottom_left_radius(self, radius: impl Into<PxPct>) -> Self {
        let mut current = self.get(BorderRadiusProp);
        current.bottom_left = Some(radius.into());
        self.set(BorderRadiusProp, current)
    }
    pub fn border_bottom_right_radius(self, radius: impl Into<PxPct>) -> Self {
        let mut current = self.get(BorderRadiusProp);
        current.bottom_right = Some(radius.into());
        self.set(BorderRadiusProp, current)
    }

    // Convenience methods for combined border properties
    pub fn apply_border(self, border: Border) -> Self {
        self.set(BorderProp, border)
    }
    pub fn apply_border_color(self, border_color: BorderColor) -> Self {
        self.set(BorderColorProp, border_color)
    }
    pub fn apply_border_radius(self, border_radius: BorderRadius) -> Self {
        self.set(BorderRadiusProp, border_radius)
    }

    pub fn inset_left_pct(self, inset: f64) -> Self {
        self.inset_left(inset.pct())
    }

    pub fn inset_right_pct(self, inset: f64) -> Self {
        self.inset_right(inset.pct())
    }

    pub fn inset_top_pct(self, inset: f64) -> Self {
        self.inset_top(inset.pct())
    }

    pub fn inset_bottom_pct(self, inset: f64) -> Self {
        self.inset_bottom(inset.pct())
    }

    pub fn inset(self, inset: impl Into<PxPctAuto>) -> Self {
        let inset = inset.into();
        self.inset_left(inset)
            .inset_top(inset)
            .inset_right(inset)
            .inset_bottom(inset)
    }

    pub fn inset_pct(self, inset: f64) -> Self {
        let inset = inset.pct();
        self.inset_left(inset)
            .inset_top(inset)
            .inset_right(inset)
            .inset_bottom(inset)
    }

    pub fn cursor(self, cursor: impl Into<StyleValue<CursorStyle>>) -> Self {
        self.set_style_value(Cursor, cursor.into().map(Some))
    }

    /// Specifies text color for the element.
    pub fn color(self, color: impl Into<StyleValue<Color>>) -> Self {
        self.set_style_value(TextColor, color.into().map(Some))
    }

    pub fn background(self, color: impl Into<Brush>) -> Self {
        let brush = StyleValue::Val(Some(color.into()));
        self.set_style_value(Background, brush)
    }

    /// Specifies shadow blur. The larger this value, the bigger the blur,
    /// so the shadow becomes bigger and lighter.
    pub fn box_shadow_blur(self, blur_radius: impl Into<PxPct>) -> Self {
        let mut value = self.get(BoxShadowProp);
        if let Some(v) = value.first_mut() {
            v.blur_radius = blur_radius.into();
        } else {
            value.push(BoxShadow {
                blur_radius: blur_radius.into(),
                ..Default::default()
            });
        }
        self.set(BoxShadowProp, value)
    }

    /// Specifies color for the shadow.
    pub fn box_shadow_color(self, color: Color) -> Self {
        let mut value = self.get(BoxShadowProp);
        if let Some(v) = value.first_mut() {
            v.color = color;
        } else {
            value.push(BoxShadow {
                color,
                ..Default::default()
            });
        }
        self.set(BoxShadowProp, value)
    }

    /// Specifies shadow blur spread. Positive values will cause the shadow
    /// to expand and grow bigger, negative values will cause the shadow to shrink.
    pub fn box_shadow_spread(self, spread: impl Into<PxPct>) -> Self {
        let mut value = self.get(BoxShadowProp);
        if let Some(v) = value.first_mut() {
            v.spread = spread.into();
        } else {
            value.push(BoxShadow {
                spread: spread.into(),
                ..Default::default()
            });
        }
        self.set(BoxShadowProp, value)
    }

    /// Applies a shadow for the stylized element. Use [BoxShadow] builder
    /// to construct each shadow.
    /// ```rust
    /// use floem::prelude::*;
    /// use floem::prelude::palette::css;
    /// use floem::style::BoxShadow;
    ///
    /// empty().style(|s| s.apply_box_shadows(vec![
    ///    BoxShadow::new()
    ///        .color(css::BLACK)
    ///        .top_offset(5.)
    ///        .bottom_offset(-30.)
    ///        .right_offset(-20.)
    ///        .left_offset(10.)
    ///        .blur_radius(5.)
    ///        .spread(10.)
    /// ]));
    /// ```
    /// ### Info
    /// If you only specify one shadow on the element, use standard style methods directly
    /// on [Style] struct:
    /// ```rust
    /// use floem::prelude::*;
    /// empty().style(|s| s
    ///     .box_shadow_top_offset(-5.)
    ///     .box_shadow_bottom_offset(30.)
    ///     .box_shadow_right_offset(20.)
    ///     .box_shadow_left_offset(-10.)
    ///     .box_shadow_spread(1.)
    ///     .box_shadow_blur(3.)
    /// );
    /// ```
    pub fn apply_box_shadows(self, shadow: impl Into<SmallVec<[BoxShadow; 3]>>) -> Self {
        self.set(BoxShadowProp, shadow.into())
    }

    /// Specifies the offset on horizontal axis.
    /// Negative offset value places the shadow to the left of the element.
    pub fn box_shadow_h_offset(self, h_offset: impl Into<PxPct>) -> Self {
        let mut value = self.get(BoxShadowProp);
        let offset = h_offset.into();
        if let Some(v) = value.first_mut() {
            v.left_offset = -offset;
            v.right_offset = offset;
        } else {
            value.push(BoxShadow {
                left_offset: -offset,
                right_offset: offset,
                ..Default::default()
            });
        }
        self.set(BoxShadowProp, value)
    }

    /// Specifies the offset on vertical axis.
    /// Negative offset value places the shadow above the element.
    pub fn box_shadow_v_offset(self, v_offset: impl Into<PxPct>) -> Self {
        let mut value = self.get(BoxShadowProp);
        let offset = v_offset.into();
        if let Some(v) = value.first_mut() {
            v.top_offset = -offset;
            v.bottom_offset = offset;
        } else {
            value.push(BoxShadow {
                top_offset: -offset,
                bottom_offset: offset,
                ..Default::default()
            });
        }
        self.set(BoxShadowProp, value)
    }

    /// Specifies the offset of the left edge.
    pub fn box_shadow_left_offset(self, left_offset: impl Into<PxPct>) -> Self {
        let mut value = self.get(BoxShadowProp);
        if let Some(v) = value.first_mut() {
            v.left_offset = left_offset.into();
        } else {
            value.push(BoxShadow {
                left_offset: left_offset.into(),
                ..Default::default()
            });
        }
        self.set(BoxShadowProp, value)
    }

    /// Specifies the offset of the right edge.
    pub fn box_shadow_right_offset(self, right_offset: impl Into<PxPct>) -> Self {
        let mut value = self.get(BoxShadowProp);
        if let Some(v) = value.first_mut() {
            v.right_offset = right_offset.into();
        } else {
            value.push(BoxShadow {
                right_offset: right_offset.into(),
                ..Default::default()
            });
        }
        self.set(BoxShadowProp, value)
    }

    /// Specifies the offset of the top edge.
    pub fn box_shadow_top_offset(self, top_offset: impl Into<PxPct>) -> Self {
        let mut value = self.get(BoxShadowProp);
        if let Some(v) = value.first_mut() {
            v.top_offset = top_offset.into();
        } else {
            value.push(BoxShadow {
                top_offset: top_offset.into(),
                ..Default::default()
            });
        }
        self.set(BoxShadowProp, value)
    }

    /// Specifies the offset of the bottom edge.
    pub fn box_shadow_bottom_offset(self, bottom_offset: impl Into<PxPct>) -> Self {
        let mut value = self.get(BoxShadowProp);
        if let Some(v) = value.first_mut() {
            v.bottom_offset = bottom_offset.into();
        } else {
            value.push(BoxShadow {
                bottom_offset: bottom_offset.into(),
                ..Default::default()
            });
        }
        self.set(BoxShadowProp, value)
    }

    pub fn font_size(self, size: impl Into<Px>) -> Self {
        let px = size.into();
        self.set_style_value(FontSize, StyleValue::Val(Some(px.0 as f32)))
    }

    pub fn font_family(self, family: impl Into<StyleValue<String>>) -> Self {
        self.set_style_value(FontFamily, family.into().map(Some))
    }

    pub fn font_weight(self, weight: impl Into<StyleValue<Weight>>) -> Self {
        self.set_style_value(FontWeight, weight.into().map(Some))
    }

    pub fn font_bold(self) -> Self {
        self.font_weight(Weight::BOLD)
    }

    pub fn font_style(self, style: impl Into<StyleValue<crate::text::Style>>) -> Self {
        self.set_style_value(FontStyle, style.into().map(Some))
    }

    pub fn cursor_color(self, color: impl Into<Brush>) -> Self {
        let brush = StyleValue::Val(color.into());
        self.set_style_value(CursorColor, brush)
    }

    pub fn line_height(self, normal: f32) -> Self {
        self.set(LineHeight, Some(LineHeightValue::Normal(normal)))
    }

    pub fn pointer_events_auto(self) -> Self {
        self.pointer_events(PointerEvents::Auto)
    }

    pub fn pointer_events_none(self) -> Self {
        self.pointer_events(PointerEvents::None)
    }

    pub fn text_ellipsis(self) -> Self {
        self.text_overflow(TextOverflow::Ellipsis)
    }

    pub fn text_clip(self) -> Self {
        self.text_overflow(TextOverflow::Clip)
    }

    pub fn absolute(self) -> Self {
        self.position(taffy::style::Position::Absolute)
    }

    pub fn items_stretch(self) -> Self {
        self.align_items(Some(taffy::style::AlignItems::Stretch))
    }

    pub fn items_start(self) -> Self {
        self.align_items(Some(taffy::style::AlignItems::FlexStart))
    }

    /// Defines the alignment along the cross axis as Centered
    pub fn items_center(self) -> Self {
        self.align_items(Some(taffy::style::AlignItems::Center))
    }

    pub fn items_end(self) -> Self {
        self.align_items(Some(taffy::style::AlignItems::FlexEnd))
    }

    pub fn items_baseline(self) -> Self {
        self.align_items(Some(taffy::style::AlignItems::Baseline))
    }

    pub fn justify_start(self) -> Self {
        self.justify_content(Some(taffy::style::JustifyContent::FlexStart))
    }

    pub fn justify_end(self) -> Self {
        self.justify_content(Some(taffy::style::JustifyContent::FlexEnd))
    }

    /// Defines the alignment along the main axis as Centered
    pub fn justify_center(self) -> Self {
        self.justify_content(Some(taffy::style::JustifyContent::Center))
    }

    pub fn justify_between(self) -> Self {
        self.justify_content(Some(taffy::style::JustifyContent::SpaceBetween))
    }

    pub fn justify_around(self) -> Self {
        self.justify_content(Some(taffy::style::JustifyContent::SpaceAround))
    }

    pub fn justify_evenly(self) -> Self {
        self.justify_content(Some(taffy::style::JustifyContent::SpaceEvenly))
    }

    pub fn hide(self) -> Self {
        self.set(Hidden, true).set(DisplayProp, Display::None)
    }

    pub fn flex(self) -> Self {
        self.display(taffy::style::Display::Flex)
    }

    pub fn grid(self) -> Self {
        self.display(taffy::style::Display::Grid)
    }

    pub fn flex_row(self) -> Self {
        self.flex_direction(taffy::style::FlexDirection::Row)
    }

    pub fn flex_col(self) -> Self {
        self.flex_direction(taffy::style::FlexDirection::Column)
    }

    pub fn z_index(self, z_index: i32) -> Self {
        self.set(ZIndex, Some(z_index))
    }

    pub fn scale(self, scale: impl Into<Pct>) -> Self {
        let val = scale.into();
        self.scale_x(val).scale_y(val)
    }

    /// Allow the application of a function if the option exists.
    /// This is useful for chaining together a bunch of optional style changes.
    /// ```rust
    /// use floem::style::Style;
    /// let maybe_none: Option<i32> = None;
    /// let style = Style::default()
    ///     .apply_opt(Some(5.0), Style::padding) // ran
    ///     .apply_opt(maybe_none, Style::margin) // not ran
    ///     .apply_opt(Some(5.0), |s, v| s.border_right(v * 2.0))
    ///     .border_left(5.0); // ran, obviously
    /// ```
    pub fn apply_opt<T>(self, opt: Option<T>, f: impl FnOnce(Self, T) -> Self) -> Self {
        if let Some(t) = opt { f(self, t) } else { self }
    }

    /// Allow the application of a function if the condition holds.
    /// This is useful for chaining together optional style changes.
    /// ```rust
    /// use floem::style::Style;
    /// let style = Style::default()
    ///     .apply_if(true, |s| s.padding(5.0)) // ran
    ///     .apply_if(false, |s| s.margin(5.0)); // not ran
    /// ```
    pub fn apply_if(self, cond: bool, f: impl FnOnce(Self) -> Self) -> Self {
        if cond { f(self) } else { self }
    }

    /// Applies a `CustomStyle` type into this style.
    ///
    /// # Examples
    /// ```
    /// use floem::prelude::*;
    /// text("test").style(|s| s.custom(|s: LabelCustomStyle| s.selectable(false)));
    /// ```
    ///
    /// See also: [`apply_custom`](Self::apply_custom), [`custom_style_class`](Self::custom_style_class)
    pub fn custom<CS: CustomStyle>(self, custom: impl FnOnce(CS) -> CS) -> Self {
        self.apply(custom(CS::default()).into())
    }

    /// Applies a `CustomStyle` type into this style.
    ///
    /// # Examples
    /// ```
    /// use floem::prelude::*;
    /// text("test").style(|s| s.apply_custom(LabelCustomStyle::new().selectable(false)));
    /// ```
    ///
    /// See also: [`custom`](Self::custom), [`custom_style_class`](Self::custom_style_class)
    pub fn apply_custom<CS: Into<Style>>(self, custom_style: CS) -> Self {
        self.apply(custom_style.into())
    }

    pub fn transition_width(self, transition: Transition) -> Self {
        self.transition(Width, transition)
    }

    pub fn transition_height(self, transition: Transition) -> Self {
        self.transition(Height, transition)
    }

    pub fn transition_size(self, transition: Transition) -> Self {
        self.transition_width(transition.clone())
            .transition_height(transition)
    }

    pub fn transition_color(self, transition: Transition) -> Self {
        self.transition(TextColor, transition)
    }
    pub fn transition_background(self, transition: Transition) -> Self {
        self.transition(Background, transition)
    }
}

impl Style {
    pub fn to_taffy_style(&self) -> TaffyStyle {
        let style = self.builtin();
        TaffyStyle {
            display: style.display(),
            overflow: taffy::Point {
                x: self.get(OverflowX),
                y: self.get(OverflowY),
            },
            position: style.position(),
            size: taffy::prelude::Size {
                width: style.width().into(),
                height: style.height().into(),
            },
            min_size: taffy::prelude::Size {
                width: style.min_width().into(),
                height: style.min_height().into(),
            },
            max_size: taffy::prelude::Size {
                width: style.max_width().into(),
                height: style.max_height().into(),
            },
            flex_direction: style.flex_direction(),
            flex_grow: style.flex_grow(),
            flex_shrink: style.flex_shrink(),
            flex_basis: style.flex_basis().into(),
            flex_wrap: style.flex_wrap(),
            justify_content: style.justify_content(),
            justify_self: style.justify_self(),
            justify_items: style.justify_items(),
            align_items: style.align_items(),
            align_content: style.align_content(),
            align_self: style.align_self(),
            aspect_ratio: style.aspect_ratio(),
            border: {
                let border = style.style.get(BorderProp);
                Rect {
                    left: LengthPercentage::length(border.left.map_or(0.0, |b| b.0.width) as f32),
                    top: LengthPercentage::length(border.top.map_or(0.0, |b| b.0.width) as f32),
                    right: LengthPercentage::length(border.right.map_or(0.0, |b| b.0.width) as f32),
                    bottom: LengthPercentage::length(
                        border.bottom.map_or(0.0, |b| b.0.width) as f32
                    ),
                }
            },
            padding: {
                let padding = style.style.get(PaddingProp);
                Rect {
                    left: padding.left.unwrap_or(PxPct::Px(0.0)).into(),
                    top: padding.top.unwrap_or(PxPct::Px(0.0)).into(),
                    right: padding.right.unwrap_or(PxPct::Px(0.0)).into(),
                    bottom: padding.bottom.unwrap_or(PxPct::Px(0.0)).into(),
                }
            },
            margin: {
                let margin = style.style.get(MarginProp);
                Rect {
                    left: margin.left.unwrap_or(PxPctAuto::Px(0.0)).into(),
                    top: margin.top.unwrap_or(PxPctAuto::Px(0.0)).into(),
                    right: margin.right.unwrap_or(PxPctAuto::Px(0.0)).into(),
                    bottom: margin.bottom.unwrap_or(PxPctAuto::Px(0.0)).into(),
                }
            },
            inset: Rect {
                left: style.inset_left().into(),
                top: style.inset_top().into(),
                right: style.inset_right().into(),
                bottom: style.inset_bottom().into(),
            },
            gap: Size {
                width: style.col_gap().into(),
                height: style.row_gap().into(),
            },
            grid_template_rows: style.grid_template_rows(),
            grid_template_columns: style.grid_template_columns(),
            grid_row: style.grid_row(),
            grid_column: style.grid_column(),
            grid_auto_rows: style.grid_auto_rows(),
            grid_auto_columns: style.grid_auto_columns(),
            grid_auto_flow: style.grid_auto_flow(),
            ..Default::default()
        }
    }
}

pub trait CustomStyle: Default + Clone + Into<Style> + From<Style> {
    type StyleClass: StyleClass;

    /// Get access to a normal style
    fn style(self, style: impl FnOnce(Style) -> Style) -> Self {
        let self_style = self.into();
        let new = style(self_style);
        new.into()
    }

    fn hover(self, style: impl FnOnce(Self) -> Self) -> Self {
        let self_style: Style = self.into();
        let new = self_style.selector(StyleSelector::Hover, |_| style(Self::default()).into());
        new.into()
    }

    fn focus(self, style: impl FnOnce(Self) -> Self) -> Self {
        let self_style: Style = self.into();
        let new = self_style.selector(StyleSelector::Focus, |_| style(Self::default()).into());
        new.into()
    }

    /// Similar to the `:focus-visible` css selector, this style only activates when tab navigation is used.
    fn focus_visible(self, style: impl FnOnce(Self) -> Self) -> Self {
        let self_style: Style = self.into();
        let new = self_style.selector(StyleSelector::FocusVisible, |_| {
            style(Self::default()).into()
        });
        new.into()
    }

    fn selected(self, style: impl FnOnce(Self) -> Self) -> Self {
        let self_style: Style = self.into();
        let new = self_style.selector(StyleSelector::Selected, |_| style(Self::default()).into());
        new.into()
    }

    fn disabled(self, style: impl FnOnce(Self) -> Self) -> Self {
        let self_style: Style = self.into();
        let new = self_style.selector(StyleSelector::Disabled, |_| style(Self::default()).into());
        new.into()
    }

    fn dark_mode(self, style: impl FnOnce(Self) -> Self) -> Self {
        let self_style: Style = self.into();
        let new = self_style.selector(StyleSelector::DarkMode, |_| style(Self::default()).into());
        new.into()
    }

    fn active(self, style: impl FnOnce(Self) -> Self) -> Self {
        let self_style: Style = self.into();
        let new = self_style.selector(StyleSelector::Active, |_| style(Self::default()).into());
        new.into()
    }

    fn responsive(self, size: ScreenSize, style: impl FnOnce(Self) -> Self) -> Self {
        let over = style(Self::default());
        let over_style: Style = over.into();
        let mut self_style: Style = self.into();
        for breakpoint in size.breakpoints() {
            self_style.set_breakpoint(breakpoint, over_style.clone());
        }
        self_style.into()
    }

    fn apply_if(self, cond: bool, style: impl FnOnce(Self) -> Self) -> Self {
        if cond { style(self) } else { self }
    }
    fn transition<P: StyleProp>(self, _prop: P, transition: Transition) -> Self {
        let mut self_style: Style = self.into();
        self_style
            .map
            .insert(P::prop_ref().info().transition_key, Rc::new(transition));
        self_style.into()
    }
}

pub trait CustomStylable<S: CustomStyle + 'static>: IntoView<V = Self::DV> + Sized {
    type DV: View;

    /// #  Add a custom style to the view with access to this view's specialized custom style.
    ///
    /// A note for implementors of the trait:
    ///
    /// _Don't try to implement this method yourself, just use the trait's default implementation._
    fn custom_style(self, style: impl Fn(S) -> S + 'static) -> Self::DV {
        let view = self.into_view();
        let id = view.id();
        let view_state = id.state();
        let offset = view_state.borrow_mut().style.next_offset();
        let style = create_updater(
            move || style(S::default()),
            move |style| id.update_style(offset, style.into()),
        );
        view_state.borrow_mut().style.push(style.into());
        view
    }
}

#[cfg(test)]
mod tests {
    use super::{Padding, Style, StyleValue};
    use crate::{style::PaddingProp, unit::PxPct};

    #[test]
    fn style_override() {
        let style1 = Style::new().padding_left(32.0);
        let style2 = Style::new().padding_left(64.0);

        let style = style1.apply(style2);

        // Check that the combined padding has the expected left value
        let padding = style.get(PaddingProp);
        assert_eq!(padding.left, Some(PxPct::Px(64.0)));

        let style1 = Style::new().padding_left(32.0).padding_bottom(45.0);
        let style2 = Style::new().padding_left(64.0);

        let style = style1.apply(style2);

        let padding = style.get(PaddingProp);
        assert_eq!(padding.left, Some(PxPct::Px(64.0)));
        assert_eq!(padding.bottom, Some(PxPct::Px(45.0))); // Should be preserved from style1

        // Test with explicit combined padding struct
        let style1 = Style::new().apply_padding(Padding::new().left(32.0).bottom(45.0));
        let style2 = Style::new().apply_padding(Padding::new().left(64.0));

        let style = style1.apply(style2);

        let padding = style.get(PaddingProp);
        assert_eq!(padding.left, Some(PxPct::Px(64.0)));
        assert_eq!(padding.bottom, Some(PxPct::Px(45.)));

        // Test that individual methods work correctly within a single style
        let style1 = Style::new().padding_left(32.0).padding_bottom(45.0);

        let padding = style1.get(PaddingProp);
        assert_eq!(padding.left, Some(PxPct::Px(32.0)));
        assert_eq!(padding.bottom, Some(PxPct::Px(45.0))); // Both values are preserved in same style

        // Test with StyleValue manipulation on combined struct
        let custom_padding = Padding::new().left(100.0).right(200.0);
        let style1 = Style::new().set_style_value(PaddingProp, StyleValue::Val(custom_padding));

        let padding = style1.get(PaddingProp);
        assert_eq!(padding.left, Some(PxPct::Px(100.0)));
        assert_eq!(padding.right, Some(PxPct::Px(200.0)));
        assert_eq!(padding.top, None);
        assert_eq!(padding.bottom, None);
    }
}
