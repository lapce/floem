//! # Style
//!
//!

use floem_reactive::create_updater;
use floem_renderer::text::{LineHeightValue, Weight};
use im_rc::hashmap::Entry;
use peniko::kurbo::Point;
use peniko::{Brush, Color, ColorStop, ColorStops, Gradient, GradientKind};
use rustc_hash::FxHasher;
use std::any::{type_name, Any};
use std::collections::HashMap;
use std::fmt::{self, Debug};
use std::hash::Hasher;
use std::hash::{BuildHasherDefault, Hash};
use std::ptr;
use std::rc::Rc;

#[cfg(not(target_arch = "wasm32"))]
use std::time::{Duration, Instant};
#[cfg(target_arch = "wasm32")]
use web_time::{Duration, Instant};

pub use taffy::style::{
    AlignContent, AlignItems, Dimension, Display, FlexDirection, FlexWrap, JustifyContent, Position,
};
use taffy::{
    geometry::{MinMax, Size},
    prelude::{GridPlacement, Line, Rect},
    style::{
        LengthPercentage, MaxTrackSizingFunction, MinTrackSizingFunction, Style as TaffyStyle,
        TrackSizingFunction,
    },
};

use crate::animate::{Bezier, Easing};
use crate::context::InteractionState;
use crate::responsive::{ScreenSize, ScreenSizeBp};
use crate::unit::{Px, PxPct, PxPctAuto, UnitExt};
use crate::view::{IntoView, View};
use crate::views::{empty, stack, text, Decorators};

pub trait StylePropValue: Clone + PartialEq + Debug {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        None
    }

    fn interpolate(&self, _other: &Self, _value: f64) -> Option<Self> {
        None
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
impl StylePropValue for Display {}
impl StylePropValue for Position {}
impl StylePropValue for FlexDirection {}
impl StylePropValue for FlexWrap {}
impl StylePropValue for AlignItems {}
impl StylePropValue for AlignContent {}
impl StylePropValue for TrackSizingFunction {}
impl StylePropValue for MinTrackSizingFunction {}
impl StylePropValue for MaxTrackSizingFunction {}
impl<T: StylePropValue, M: StylePropValue> StylePropValue for MinMax<T, M> {}
impl<T: StylePropValue> StylePropValue for Line<T> {}
impl StylePropValue for GridPlacement {}
impl StylePropValue for CursorStyle {}
impl StylePropValue for BoxShadow {
    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        Some(Self {
            blur_radius: self
                .blur_radius
                .interpolate(&other.blur_radius, value)
                .unwrap(),
            color: self.color.interpolate(&other.color, value).unwrap(),
            spread: self.spread.interpolate(&other.spread, value).unwrap(),
            h_offset: self.h_offset.interpolate(&other.h_offset, value).unwrap(),
            v_offset: self.v_offset.interpolate(&other.v_offset, value).unwrap(),
        })
    }
}
impl StylePropValue for String {}
impl StylePropValue for Weight {
    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        self.0.interpolate(&other.0, value).map(Weight)
    }
}
impl StylePropValue for crate::text::Style {}
impl StylePropValue for TextOverflow {}
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
impl<T: StylePropValue> StylePropValue for Vec<T> {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        None
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
impl StylePropValue for PxPctAuto {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        let label = match self {
            Self::Px(v) => format!("{} px", v),
            Self::Pct(v) => format!("{}%", v),
            Self::Auto => "auto".to_string(),
        };
        Some(text(label).into_any())
    }
    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        match (self, other) {
            (Self::Px(v1), Self::Px(v2)) => Some(Self::Px(v1 + (v2 - v1) * value)),
            (Self::Pct(v1), Self::Pct(v2)) => Some(Self::Pct(v1 + (v2 - v1) * value)),
            (Self::Auto, Self::Auto) => Some(Self::Auto),
            // TODO: Figure out some way to get in the relevent layout information in order to interpolate betweeen pixels and percent
            _ => None,
        }
    }
}
impl StylePropValue for PxPct {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        let label = match self {
            Self::Px(v) => format!("{} px", v),
            Self::Pct(v) => format!("{}%", v),
        };
        Some(text(label).into_any())
    }

    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        match (self, other) {
            (Self::Px(v1), Self::Px(v2)) => Some(Self::Px(v1 + (v2 - v1) * value)),
            (Self::Pct(v1), Self::Pct(v2)) => Some(Self::Pct(v1 + (v2 - v1) * value)),
            // TODO: Figure out some way to get in the relevent layout information in order to interpolate betweeen pixels and percent
            _ => None,
        }
    }
}
impl StylePropValue for Color {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        let color = *self;
        let color = empty().style(move |s| {
            s.background(color)
                .width(22.0)
                .height(14.0)
                .border(1)
                .border_color(Color::WHITE.with_alpha_factor(0.5))
                .border_radius(5.0)
        });
        let color = stack((color,)).style(|s| {
            s.border(1)
                .border_color(Color::BLACK.with_alpha_factor(0.5))
                .border_radius(5.0)
                .margin_left(6.0)
        });
        Some(
            stack((text(format!("{self:?}")), color))
                .style(|s| s.items_center())
                .into_any(),
        )
    }

    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
        let r = f64::interpolate(&(self.r as f64), &(other.r as f64), value)
            .unwrap()
            .round() as u8;
        let g = f64::interpolate(&(self.g as f64), &(other.g as f64), value)
            .unwrap()
            .round() as u8;
        let b = f64::interpolate(&(self.b as f64), &(other.b as f64), value)
            .unwrap()
            .round() as u8;
        let a = f64::interpolate(&(self.a as f64), &(other.a as f64), value)
            .unwrap()
            .round() as u8;
        Some(Color { r, g, b, a })
    }
}

impl StylePropValue for Gradient {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        let box_width = 22.;
        let box_height = 14.;
        let mut grad = self.clone();
        grad.kind = match grad.kind {
            GradientKind::Linear { start, end } => {
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

                GradientKind::Linear {
                    start: new_start,
                    end: new_end,
                }
            }
            _ => grad.kind,
        };
        let color = empty().style(move |s| {
            s.background(grad.clone())
                .width(box_width)
                .height(box_height)
                .border(1)
                .border_color(Color::WHITE.with_alpha_factor(0.5))
                .border_radius(5.0)
        });
        let color = stack((color,)).style(|s| {
            s.border(1)
                .border_color(Color::BLACK.with_alpha_factor(0.5))
                .border_radius(5.0)
                .margin_left(6.0)
        });
        Some(
            stack((text(format!("{self:?}")), color))
                .style(|s| s.items_center())
                .into_any(),
        )
    }

    fn interpolate(&self, other: &Self, value: f64) -> Option<Self> {
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
            (Brush::Solid(color), Brush::Solid(other)) => {
                color.interpolate(other, value).map(Self::Solid)
            }
            (Brush::Gradient(gradient), Brush::Solid(solid)) => {
                let interpolated_stops: ColorStops = gradient
                    .stops
                    .iter()
                    .map(|stop| {
                        let interpolated_color = stop.color.interpolate(solid, value).unwrap();
                        ColorStop::from((stop.offset, interpolated_color))
                    })
                    .collect();
                Some(Brush::Gradient(Gradient {
                    kind: gradient.kind,
                    extend: gradient.extend,
                    stops: interpolated_stops,
                }))
            }
            (Brush::Solid(solid), Brush::Gradient(gradient)) => {
                let interpolated_stops: ColorStops = gradient
                    .stops
                    .iter()
                    .map(|stop| {
                        let interpolated_color = solid.interpolate(&stop.color, value).unwrap();
                        ColorStop::from((stop.offset, interpolated_color))
                    })
                    .collect();
                Some(Brush::Gradient(Gradient {
                    kind: gradient.kind,
                    extend: gradient.extend,
                    stops: interpolated_stops,
                }))
            }

            (Brush::Gradient(gradient1), Brush::Gradient(gradient2)) => {
                gradient1.interpolate(gradient2, value).map(Brush::Gradient)
            }

            _ => None,
        }
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
    ($v:vis $name:ident) => {
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

type InterpolateFn = fn(val1: &dyn Any, val2: &dyn Any, time: f64) -> Option<Rc<dyn Any>>;

#[derive(Debug)]
pub struct StylePropInfo {
    pub(crate) name: fn() -> &'static str,
    pub(crate) inherited: bool,
    pub(crate) default_as_any: fn() -> Rc<dyn Any>,
    pub(crate) interpolate: InterpolateFn,
    pub(crate) debug_any: fn(val: &dyn Any) -> String,
    pub(crate) debug_view: fn(val: &dyn Any) -> Option<Box<dyn View>>,
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
                        StyleMapValue::Val(v) => format!("{:?}", v),
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
                    if let (StyleMapValue::Val(v1), StyleMapValue::Val(v2)) = (v1, v2) {
                        v1.interpolate(v2, time)
                            .map(|val| Rc::new(StyleMapValue::Val(val)) as Rc<dyn Any>)
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
                        StyleMapValue::Val(v) => v.debug_view(),

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
    fn read(
        state: &mut Self::State,
        style: &Style,
        fallback: &Style,
        now: &Instant,
        request_transition: &mut bool,
    ) -> bool {
        let new = style
            .get_prop::<P>()
            .or_else(|| fallback.get_prop::<P>())
            .unwrap_or_else(|| P::default_value());
        state.1.read(
            style
                .get_transition::<P>()
                .or_else(|| fallback.get_transition::<P>()),
        );
        let changed = new != state.0;
        if changed {
            state.1.transition(&Self::get(state), &new);
            state.0 = new;
        }
        changed | state.1.step(now, request_transition)
    }
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
    ($v:vis $name:ident: $ty:ty { $($options:tt)* } = $default:expr
    ) => {
        #[derive(Default, Copy, Clone)]
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
    Val(T),
    /// Use the default value for the style, typically from the underlying `ComputedStyle`
    Unset,
}

impl<T> StyleMapValue<T> {
    pub(crate) fn as_ref(&self) -> Option<&T> {
        match self {
            Self::Val(v) => Some(v),
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

    fn step(&mut self, now: &Instant, request_transition: &mut bool) -> bool {
        if !self.initial {
            // We have observed the initial value. Any further changes may trigger animations.
            self.initial = true;
        }
        if let Some(active) = &mut self.active {
            if let Some(transition) = &self.transition {
                let time = now.saturating_duration_since(active.start);
                if time < transition.duration {
                    if let Some(i) = T::interpolate(
                        &active.before,
                        &active.after,
                        transition.easing.apply_easing_fn(
                            time.as_secs_f64() / transition.duration.as_secs_f64(),
                        ),
                    ) {
                        active.current = i;
                        *request_transition = true;
                        return true;
                    }
                }
            }
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
    pub easing: Easing,
}

impl Transition {
    pub fn linear(duration: Duration) -> Self {
        Self {
            duration,
            easing: Easing::Linear,
        }
    }

    pub fn ease_in_out(duration: Duration) -> Self {
        Self {
            duration,
            easing: Easing::CubicBezier(Bezier::EASE_IN_OUT),
        }
    }
}

#[derive(Debug)]
pub enum StyleKeyInfo {
    Transition,
    Prop(StylePropInfo),
    Selector(StyleSelectors),
    Class(StyleClassInfo),
}

#[derive(Copy, Clone)]
pub struct StyleKey {
    pub info: &'static StyleKeyInfo,
}
impl StyleKey {
    pub(crate) fn debug_any(&self, value: &dyn Any) -> String {
        match self.info {
            StyleKeyInfo::Selector(..) | StyleKeyInfo::Transition => String::new(),
            StyleKeyInfo::Class(info) => (info.name)().to_string(),
            StyleKeyInfo::Prop(v) => (v.debug_any)(value),
        }
    }
    fn inherited(&self) -> bool {
        match self.info {
            StyleKeyInfo::Selector(..) | StyleKeyInfo::Transition => false,
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
            StyleKeyInfo::Selector(..) => write!(f, "selector"),
            StyleKeyInfo::Transition => write!(f, "transition"),
            StyleKeyInfo::Class(v) => write!(f, "{}", (v.name)()),
            StyleKeyInfo::Prop(v) => write!(f, "{}", (v.name)()),
        }
    }
}

type ImHashMap<K, V> = im_rc::HashMap<K, V, BuildHasherDefault<FxHasher>>;

style_key_selector!(selector_xs, StyleSelectors::new().responsive());
style_key_selector!(selector_sm, StyleSelectors::new().responsive());
style_key_selector!(selector_md, StyleSelectors::new().responsive());
style_key_selector!(selector_lg, StyleSelectors::new().responsive());
style_key_selector!(selector_xl, StyleSelectors::new().responsive());
style_key_selector!(selector_xxl, StyleSelectors::new().responsive());

fn screen_size_bp_to_key(breakpoint: ScreenSizeBp) -> StyleKey {
    match breakpoint {
        ScreenSizeBp::Xs => selector_xs(),
        ScreenSizeBp::Sm => selector_sm(),
        ScreenSizeBp::Md => selector_md(),
        ScreenSizeBp::Lg => selector_lg(),
        ScreenSizeBp::Xl => selector_xl(),
        ScreenSizeBp::Xxl => selector_xxl(),
    }
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
                    StyleMapValue::Unset => StyleValue::Unset,
                },
            )
            .unwrap_or(StyleValue::Base)
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

    pub fn apply_classes_from_context(
        mut self,
        classes: &[StyleClassRef],
        context: &Style,
    ) -> Style {
        for class in classes {
            if let Some(map) = context.get_nested_map(class.key) {
                self.apply_mut(map);
            }
        }
        self
    }

    pub fn apply_class<C: StyleClass>(mut self, _class: C) -> Style {
        if let Some(map) = self.map.get(&C::key()) {
            self.apply_mut(map.downcast_ref::<Style>().unwrap().clone());
        }
        self
    }

    pub fn apply_selectors(mut self, selectors: &[StyleSelector]) -> Style {
        for selector in selectors {
            if let Some(map) = self.get_nested_map(selector.to_key()) {
                self.apply_mut(map.apply_selectors(selectors));
            }
        }
        self
    }

    fn get_nested_map(&self, key: StyleKey) -> Option<Style> {
        self.map
            .get(&key)
            .map(|map| map.downcast_ref::<Style>().unwrap().clone())
    }

    pub(crate) fn apply_interact_state(
        &mut self,
        interact_state: &InteractionState,
        screen_size_bp: ScreenSizeBp,
    ) {
        if let Some(mut map) = self.get_nested_map(screen_size_bp_to_key(screen_size_bp)) {
            map.apply_interact_state(interact_state, screen_size_bp);
            self.apply_mut(map);
        }

        if interact_state.is_hovered && !interact_state.is_disabled {
            if let Some(mut map) = self.get_nested_map(StyleSelector::Hover.to_key()) {
                map.apply_interact_state(interact_state, screen_size_bp);
                self.apply_mut(map);
            }
        }
        if interact_state.is_focused {
            if let Some(mut map) = self.get_nested_map(StyleSelector::Focus.to_key()) {
                map.apply_interact_state(interact_state, screen_size_bp);
                self.apply_mut(map);
            }
        }
        if interact_state.is_selected {
            if let Some(mut map) = self.get_nested_map(StyleSelector::Selected.to_key()) {
                map.apply_interact_state(interact_state, screen_size_bp);
                self.apply_mut(map);
            }
        }
        if interact_state.is_disabled {
            if let Some(mut map) = self.get_nested_map(StyleSelector::Disabled.to_key()) {
                map.apply_interact_state(interact_state, screen_size_bp);
                self.apply_mut(map);
            }
        }

        let focused_keyboard =
            interact_state.using_keyboard_navigation && interact_state.is_focused;

        if focused_keyboard {
            if let Some(mut map) = self.get_nested_map(StyleSelector::FocusVisible.to_key()) {
                map.apply_interact_state(interact_state, screen_size_bp);
                self.apply_mut(map);
            }
        }

        let active_mouse = interact_state.is_hovered && !interact_state.using_keyboard_navigation;
        if interact_state.is_clicking && (active_mouse || focused_keyboard) {
            if let Some(mut map) = self.get_nested_map(StyleSelector::Active.to_key()) {
                map.apply_interact_state(interact_state, screen_size_bp);
                self.apply_mut(map);
            }
        }
    }

    pub(crate) fn any_inherited(&self) -> bool {
        self.map.iter().any(|(p, _)| p.inherited())
    }

    pub(crate) fn apply_only_inherited(this: &mut Rc<Style>, over: &Style) {
        if over.any_inherited() {
            let inherited = over
                .map
                .iter()
                .filter(|(p, _)| p.inherited())
                .map(|(p, v)| (*p, v.clone()));

            let this = Rc::make_mut(this);
            this.apply_iter(inherited);
        }
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

    fn apply_iter(&mut self, iter: impl Iterator<Item = (StyleKey, Rc<dyn Any>)>) {
        for (k, v) in iter {
            match k.info {
                StyleKeyInfo::Class(..) | StyleKeyInfo::Selector(..) => match self.map.entry(k) {
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
                        e.insert(v);
                    }
                },
                StyleKeyInfo::Transition | StyleKeyInfo::Prop(..) => {
                    self.map.insert(k, v);
                }
            }
        }
    }

    pub(crate) fn apply_mut(&mut self, over: Style) {
        self.apply_iter(over.map.into_iter());
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

    /// Apply multiple `Style`s to this style, returning a new `Style` with the overrides.
    /// Later styles take precedence over earlier styles.
    pub fn apply_overriding_styles(self, overrides: impl Iterator<Item = Style>) -> Style {
        overrides.fold(self, |acc, x| acc.apply(x))
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
    Active,
    Dragging,
    Selected,
}

style_key_selector!(hover, StyleSelectors::new().set(StyleSelector::Hover, true));
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
        let v = selector as isize as u8;
        let bit = 1 << v;
        self.selectors = (self.selectors & !bit) | ((value as u8) << v);
        self
    }
    pub(crate) fn has(self, selector: StyleSelector) -> bool {
        let v = (selector as isize).try_into().unwrap();
        let bit = 1_u8.checked_shl(v).unwrap();
        self.selectors & bit != 0
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextOverflow {
    Wrap,
    Clip,
    Ellipsis,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CursorStyle {
    Default,
    Pointer,
    Text,
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxShadow {
    pub blur_radius: PxPct,
    pub color: Color,
    pub spread: PxPct,
    pub h_offset: PxPct,
    pub v_offset: PxPct,
}

impl Default for BoxShadow {
    fn default() -> Self {
        Self {
            blur_radius: PxPct::Px(0.),
            color: Color::BLACK,
            spread: PxPct::Px(0.),
            h_offset: PxPct::Px(0.),
            v_offset: PxPct::Px(0.),
        }
    }
}

/// The value for a [`Style`] property
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleValue<T> {
    Val(T),
    /// Use the default value for the style, typically from the underlying `ComputedStyle`
    Unset,
    /// Use whatever the base style is. For an overriding style like hover, this uses the base
    /// style. For the base style, this is equivalent to `Unset`
    Base,
}

impl<T> StyleValue<T> {
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> StyleValue<U> {
        match self {
            Self::Val(x) => StyleValue::Val(f(x)),
            Self::Unset => StyleValue::Unset,
            Self::Base => StyleValue::Base,
        }
    }

    pub fn unwrap_or(self, default: T) -> T {
        match self {
            Self::Val(x) => x,
            Self::Unset => default,
            Self::Base => default,
        }
    }

    pub fn unwrap_or_else(self, f: impl FnOnce() -> T) -> T {
        match self {
            Self::Val(x) => x,
            Self::Unset => f(),
            Self::Base => f(),
        }
    }

    pub fn as_mut(&mut self) -> Option<&mut T> {
        match self {
            Self::Val(x) => Some(x),
            Self::Unset => None,
            Self::Base => None,
        }
    }
}

impl<T> Default for StyleValue<T> {
    fn default() -> Self {
        // By default we let the `Style` decide what to do.
        Self::Base
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
    JustifySelf justify_self: Option<AlignItems> {} = None,
    AlignItemsProp align_items: Option<AlignItems> {} = None,
    AlignContentProp align_content: Option<AlignContent> {} = None,
    GridTemplateRows grid_template_rows: Vec<TrackSizingFunction> {} = Vec::new(),
    GridTemplateColumns grid_template_columns: Vec<TrackSizingFunction> {} = Vec::new(),
    GridAutoRows grid_auto_rows: Vec<MinMax<MinTrackSizingFunction, MaxTrackSizingFunction>> {} = Vec::new(),
    GridAutoColumns grid_auto_columns: Vec<MinMax<MinTrackSizingFunction, MaxTrackSizingFunction>> {} = Vec::new(),
    GridRow grid_row: Line<GridPlacement> {} = Line::default(),
    GridColumn grid_column: Line<GridPlacement> {} = Line::default(),
    AlignSelf align_self: Option<AlignItems> {} = None,
    BorderLeft border_left: Px {} = Px(0.0),
    BorderTop border_top: Px {} = Px(0.0),
    BorderRight border_right: Px {} = Px(0.0),
    BorderBottom border_bottom: Px {} = Px(0.0),
    BorderRadius border_radius: PxPct {} = PxPct::Px(0.0),
    OutlineColor outline_color: Brush {} = Brush::Solid(Color::TRANSPARENT),
    Outline outline: Px {} = Px(0.0),
    BorderColor border_color: Brush {} = Brush::Solid(Color::BLACK),
    PaddingLeft padding_left: PxPct {} = PxPct::Px(0.0),
    PaddingTop padding_top: PxPct {} = PxPct::Px(0.0),
    PaddingRight padding_right: PxPct {} = PxPct::Px(0.0),
    PaddingBottom padding_bottom: PxPct {} = PxPct::Px(0.0),
    MarginLeft margin_left: PxPctAuto {} = PxPctAuto::Px(0.0),
    MarginTop margin_top: PxPctAuto {} = PxPctAuto::Px(0.0),
    MarginRight margin_right: PxPctAuto {} = PxPctAuto::Px(0.0),
    MarginBottom margin_bottom: PxPctAuto {} = PxPctAuto::Px(0.0),
    InsetLeft inset_left: PxPctAuto {} = PxPctAuto::Auto,
    InsetTop inset_top: PxPctAuto {} = PxPctAuto::Auto,
    InsetRight inset_right: PxPctAuto {} = PxPctAuto::Auto,
    InsetBottom inset_bottom: PxPctAuto {} = PxPctAuto::Auto,
    ZIndex z_index nocb: Option<i32> {} = None,
    Cursor cursor nocb: Option<CursorStyle> {} = None,
    TextColor color nocb: Option<Color> { inherited } = None,
    Background background nocb: Option<Brush> {} = None,
    Foreground foreground nocb: Option<Brush> {} = None,
    BoxShadowProp box_shadow nocb: Option<BoxShadow> {} = None,
    FontSize font_size nocb: Option<f32> { inherited } = None,
    FontFamily font_family nocb: Option<String> { inherited } = None,
    FontWeight font_weight nocb: Option<Weight> { inherited } = None,
    FontStyle font_style nocb: Option<crate::text::Style> { inherited } = None,
    CursorColor cursor_color nocb: Brush {} = Brush::Solid(Color::BLACK.with_alpha_factor(0.3)),
    SelectionCornerRadius selection_corer_radius nocb: f64 {} = 1.,
    Selectable selectable: bool {} = true,
    TextOverflowProp text_overflow: TextOverflow {} = TextOverflow::Wrap,
    LineHeight line_height nocb: Option<LineHeightValue> { inherited } = None,
    AspectRatio aspect_ratio: Option<f32> {} = None,
    ColGap col_gap nocb: PxPct {} = PxPct::Px(0.),
    RowGap row_gap nocb: PxPct {} = PxPct::Px(0.),
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
        pub border_left: BorderLeft,
        pub border_top: BorderTop,
        pub border_right: BorderRight,
        pub border_bottom: BorderBottom,

        pub padding_left: PaddingLeft,
        pub padding_top: PaddingTop,
        pub padding_right: PaddingRight,
        pub padding_bottom: PaddingBottom,

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

        pub margin_left: MarginLeft,
        pub margin_top: MarginTop,
        pub margin_right: MarginRight,
        pub margin_bottom: MarginBottom,

        pub row_gap: RowGap,
        pub col_gap: ColGap,
    }
}
impl LayoutProps {
    pub fn to_style(&self) -> Style {
        Style::new()
            .width(self.width())
            .height(self.height())
            .border_left(self.border_left())
            .border_top(self.border_top())
            .border_right(self.border_right())
            .border_bottom(self.border_bottom())
            .padding_left(self.padding_left())
            .padding_top(self.padding_top())
            .padding_right(self.padding_right())
            .padding_bottom(self.padding_bottom())
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
            .margin_left(self.margin_left())
            .margin_top(self.margin_top())
            .margin_right(self.margin_right())
            .margin_bottom(self.margin_bottom())
            .row_gap(self.row_gap())
            .column_gap(self.col_gap())
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

    pub fn disabled(self, style: impl FnOnce(Style) -> Style) -> Self {
        self.selector(StyleSelector::Disabled, style)
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

    pub fn row_gap(self, width: impl Into<PxPct>) -> Self {
        self.set(RowGap, width.into())
    }

    pub fn column_gap(self, height: impl Into<PxPct>) -> Self {
        self.set(ColGap, height.into())
    }

    pub fn row_col_gap(self, width: impl Into<PxPct>, height: impl Into<PxPct>) -> Self {
        self.row_gap(width).column_gap(height)
    }

    pub fn gap(self, gap: impl Into<PxPct>) -> Self {
        let gap = gap.into();
        self.row_gap(gap).column_gap(gap)
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

    pub fn border(self, border: impl Into<Px>) -> Self {
        let border = border.into();
        self.border_left(border)
            .border_top(border)
            .border_right(border)
            .border_bottom(border)
    }

    /// Sets `border_left` and `border_right` to `border`
    pub fn border_horiz(self, border: impl Into<Px>) -> Self {
        let border = border.into();
        self.border_left(border).border_right(border)
    }

    /// Sets `border_top` and `border_bottom` to `border`
    pub fn border_vert(self, border: impl Into<Px>) -> Self {
        let border = border.into();
        self.border_top(border).border_bottom(border)
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
        let padding = padding.into();
        self.padding_left(padding)
            .padding_top(padding)
            .padding_right(padding)
            .padding_bottom(padding)
    }

    pub fn padding_pct(self, padding: f64) -> Self {
        let padding = padding.pct();
        self.padding_left(padding)
            .padding_top(padding)
            .padding_right(padding)
            .padding_bottom(padding)
    }

    /// Sets `padding_left` and `padding_right` to `padding`
    pub fn padding_horiz(self, padding: impl Into<PxPct>) -> Self {
        let padding = padding.into();
        self.padding_left(padding).padding_right(padding)
    }

    pub fn padding_horiz_pct(self, padding: f64) -> Self {
        let padding = padding.pct();
        self.padding_left(padding).padding_right(padding)
    }

    /// Sets `padding_top` and `padding_bottom` to `padding`
    pub fn padding_vert(self, padding: impl Into<PxPct>) -> Self {
        let padding = padding.into();
        self.padding_top(padding).padding_bottom(padding)
    }

    pub fn padding_vert_pct(self, padding: f64) -> Self {
        let padding = padding.pct();
        self.padding_top(padding).padding_bottom(padding)
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
        let margin = margin.into();
        self.margin_left(margin)
            .margin_top(margin)
            .margin_right(margin)
            .margin_bottom(margin)
    }

    pub fn margin_pct(self, margin: f64) -> Self {
        let margin = margin.pct();
        self.margin_left(margin)
            .margin_top(margin)
            .margin_right(margin)
            .margin_bottom(margin)
    }

    /// Sets `margin_left` and `margin_right` to `margin`
    pub fn margin_horiz(self, margin: impl Into<PxPctAuto>) -> Self {
        let margin = margin.into();
        self.margin_left(margin).margin_right(margin)
    }

    pub fn margin_horiz_pct(self, margin: f64) -> Self {
        let margin = margin.pct();
        self.margin_left(margin).margin_right(margin)
    }

    /// Sets `margin_top` and `margin_bottom` to `margin`
    pub fn margin_vert(self, margin: impl Into<PxPctAuto>) -> Self {
        let margin = margin.into();
        self.margin_top(margin).margin_bottom(margin)
    }

    pub fn margin_vert_pct(self, margin: f64) -> Self {
        let margin = margin.pct();
        self.margin_top(margin).margin_bottom(margin)
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

    pub fn color(self, color: impl Into<StyleValue<Color>>) -> Self {
        self.set_style_value(TextColor, color.into().map(Some))
    }

    pub fn background(self, color: impl Into<Brush>) -> Self {
        let brush = StyleValue::Val(Some(color.into()));
        self.set_style_value(Background, brush)
    }

    pub fn box_shadow_blur(self, blur_radius: impl Into<PxPct>) -> Self {
        let mut value = self.get(BoxShadowProp).unwrap_or_default();
        value.blur_radius = blur_radius.into();
        self.set(BoxShadowProp, Some(value))
    }

    pub fn box_shadow_color(self, color: Color) -> Self {
        let mut value = self.get(BoxShadowProp).unwrap_or_default();
        value.color = color;
        self.set(BoxShadowProp, Some(value))
    }

    pub fn box_shadow_spread(self, spread: impl Into<PxPct>) -> Self {
        let mut value = self.get(BoxShadowProp).unwrap_or_default();
        value.spread = spread.into();
        self.set(BoxShadowProp, Some(value))
    }

    pub fn box_shadow_h_offset(self, h_offset: impl Into<PxPct>) -> Self {
        let mut value = self.get(BoxShadowProp).unwrap_or_default();
        value.h_offset = h_offset.into();
        self.set(BoxShadowProp, Some(value))
    }

    pub fn box_shadow_v_offset(self, v_offset: impl Into<PxPct>) -> Self {
        let mut value = self.get(BoxShadowProp).unwrap_or_default();
        value.v_offset = v_offset.into();
        self.set(BoxShadowProp, Some(value))
    }

    pub fn font_size(self, size: impl Into<StyleValue<f32>>) -> Self {
        self.set_style_value(FontSize, size.into().map(Some))
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

    pub fn cursor_color(self, color: impl Into<StyleValue<Brush>>) -> Self {
        self.set_style_value(CursorColor, color.into())
    }

    pub fn line_height(self, normal: f32) -> Self {
        self.set(LineHeight, Some(LineHeightValue::Normal(normal)))
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

    /// Defines the alignment along the main axis as Centered
    pub fn justify_center(self) -> Self {
        self.justify_content(Some(taffy::style::JustifyContent::Center))
    }

    pub fn justify_end(self) -> Self {
        self.justify_content(Some(taffy::style::JustifyContent::FlexEnd))
    }

    pub fn justify_start(self) -> Self {
        self.justify_content(Some(taffy::style::JustifyContent::FlexStart))
    }

    pub fn justify_between(self) -> Self {
        self.justify_content(Some(taffy::style::JustifyContent::SpaceBetween))
    }

    pub fn hide(self) -> Self {
        self.display(taffy::style::Display::None)
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

    /// Allow the application of a function if the option exists.
    /// This is useful for chaining together a bunch of optional style changes.
    /// ```rust
    /// use floem::style::Style;
    /// let maybe_none: Option<i32> = None;
    /// let style = Style::default()
    ///    .apply_opt(Some(5.0), Style::padding) // ran
    ///    .apply_opt(maybe_none, Style::margin) // not ran
    ///    .apply_opt(Some(5.0), |s, v| s.border_right(v * 2.0))
    ///    .border_left(5.0); // ran, obviously
    /// ```
    pub fn apply_opt<T>(self, opt: Option<T>, f: impl FnOnce(Self, T) -> Self) -> Self {
        if let Some(t) = opt {
            f(self, t)
        } else {
            self
        }
    }

    /// Allow the application of a function if the condition holds.
    /// This is useful for chaining together a bunch of optional style changes.
    /// ```rust
    /// use floem::style::Style;
    /// let style = Style::default()
    ///     .apply_if(true, |s| s.padding(5.0)) // ran
    ///     .apply_if(false, |s| s.margin(5.0)); // not ran
    /// ```
    pub fn apply_if(self, cond: bool, f: impl FnOnce(Self) -> Self) -> Self {
        if cond {
            f(self)
        } else {
            self
        }
    }

    pub fn apply_custom<CS: Into<Style>>(self, custom_style: CS) -> Self {
        self.apply(custom_style.into())
    }
}

impl Style {
    pub fn to_taffy_style(&self) -> TaffyStyle {
        let style = self.builtin();
        TaffyStyle {
            display: style.display(),
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
            align_items: style.align_items(),
            align_content: style.align_content(),
            align_self: style.align_self(),
            aspect_ratio: style.aspect_ratio(),
            border: Rect {
                left: LengthPercentage::Length(style.border_left().0 as f32),
                top: LengthPercentage::Length(style.border_top().0 as f32),
                right: LengthPercentage::Length(style.border_right().0 as f32),
                bottom: LengthPercentage::Length(style.border_bottom().0 as f32),
            },
            padding: Rect {
                left: style.padding_left().into(),
                top: style.padding_top().into(),
                right: style.padding_right().into(),
                bottom: style.padding_bottom().into(),
            },
            margin: Rect {
                left: style.margin_left().into(),
                top: style.margin_top().into(),
                right: style.margin_right().into(),
                bottom: style.margin_bottom().into(),
            },
            inset: Rect {
                left: style.inset_left().into(),
                top: style.inset_top().into(),
                right: style.inset_right().into(),
                bottom: style.inset_bottom().into(),
            },
            gap: Size {
                width: style.row_gap().into(),
                height: style.col_gap().into(),
            },
            grid_template_rows: style.grid_template_rows(),
            grid_template_columns: style.grid_template_columns(),
            grid_row: style.grid_row(),
            grid_column: style.grid_column(),
            grid_auto_rows: style.grid_auto_rows(),
            grid_auto_columns: style.grid_auto_columns(),
            ..Default::default()
        }
    }
}

pub trait CustomStylable<S: Default + Into<Style> + 'static>:
    IntoView<V = Self::DV> + Sized
{
    type DV: View;

    /// #  Add a custom style to the view with acess to this view's specialized custom style.
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
    use super::{Style, StyleValue};
    use crate::{
        style::{PaddingBottom, PaddingLeft},
        unit::PxPct,
    };

    #[test]
    fn style_override() {
        let style1 = Style::new().padding_left(32.0);
        let style2 = Style::new().padding_left(64.0);

        let style = style1.apply(style2);

        assert_eq!(
            style.get_style_value(PaddingLeft),
            StyleValue::Val(PxPct::Px(64.0))
        );

        let style1 = Style::new().padding_left(32.0).padding_bottom(45.0);
        let style2 = Style::new()
            .padding_left(64.0)
            .set_style_value(PaddingBottom, StyleValue::Base);

        let style = style1.apply(style2);

        assert_eq!(
            style.get_style_value(PaddingLeft),
            StyleValue::Val(PxPct::Px(64.0))
        );
        assert_eq!(
            style.get_style_value(PaddingBottom),
            StyleValue::Val(PxPct::Px(45.0))
        );

        let style1 = Style::new().padding_left(32.0).padding_bottom(45.0);
        let style2 = Style::new()
            .padding_left(64.0)
            .set_style_value(PaddingBottom, StyleValue::Unset);

        let style = style1.apply(style2);

        assert_eq!(
            style.get_style_value(PaddingLeft),
            StyleValue::Val(PxPct::Px(64.0))
        );
        assert_eq!(style.get_style_value(PaddingBottom), StyleValue::Unset);

        let style1 = Style::new().padding_left(32.0).padding_bottom(45.0);
        let style2 = Style::new()
            .padding_left(64.0)
            .set_style_value(PaddingBottom, StyleValue::Unset);

        let style3 = Style::new().set_style_value(PaddingBottom, StyleValue::Base);

        let style = style1.apply_overriding_styles([style2, style3].into_iter());

        assert_eq!(
            style.get_style_value(PaddingLeft),
            StyleValue::Val(PxPct::Px(64.0))
        );
        assert_eq!(style.get_style_value(PaddingBottom), StyleValue::Unset);

        let style1 = Style::new().padding_left(32.0).padding_bottom(45.0);
        let style2 = Style::new()
            .padding_left(64.0)
            .set_style_value(PaddingBottom, StyleValue::Unset);
        let style3 = Style::new().padding_bottom(100.0);

        let style = style1.apply_overriding_styles([style2, style3].into_iter());

        assert_eq!(
            style.get_style_value(PaddingLeft),
            StyleValue::Val(PxPct::Px(64.0))
        );
        assert_eq!(
            style.get_style_value(PaddingBottom),
            StyleValue::Val(PxPct::Px(100.0))
        );
    }
}
