//! # Style  
//! Styles are divided into two parts:
//! [`ComputedStyle`]: A style with definite values for most fields.  
//!
//! [`Style`]: A style with [`StyleValue`]s for the fields, where `Unset` falls back to the relevant
//! field in the [`ComputedStyle`] and `Base` falls back to the underlying [`Style`] or the
//! [`ComputedStyle`].
//!
//!
//! A loose analogy with CSS might be:  
//! [`ComputedStyle`] is like the browser's default style sheet for any given element (view).  
//!   
//! [`Style`] is like the styling associated with a *specific* element (view):
//! ```html
//! <div style="color: red; font-size: 12px;">
//! ```
//!   
//! An override [`Style`] is perhaps closest to classes that can be applied to an element, like
//! `div:hover { color: blue; }`.  
//! However, we do not actually have 'classes' where you can define a separate collection of styles
//! in the same way. So, the hover styling is still defined with the view as you construct it, so
//! perhaps a closer pseudocode analogy is:
//! ```html
//! <div hover_style="color: blue;" style="color: red; font-size: 12px;">
//! ```
//!

use floem_renderer::cosmic_text;
use floem_renderer::cosmic_text::{LineHeightValue, Weight};
use peniko::Color;
use std::any::{type_name, Any};
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::fmt::{self, Debug};
use std::hash::Hash;
use std::hash::Hasher;
use std::ptr;
use std::rc::Rc;
pub use taffy::style::{
    AlignContent, AlignItems, Dimension, Display, FlexDirection, FlexWrap, JustifyContent, Position,
};
use taffy::{
    geometry::Size,
    prelude::Rect,
    style::{LengthPercentage, Style as TaffyStyle},
};

use crate::context::InteractionState;
use crate::context::LayoutCx;
use crate::responsive::{ScreenSize, ScreenSizeBp};
use crate::unit::{Px, PxPct, PxPctAuto, UnitExt};
use crate::view::View;
use crate::views::{empty, stack, text, Decorators};

pub trait StylePropValue: Clone + PartialEq + Debug {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        None
    }
}

impl StylePropValue for i32 {}
impl StylePropValue for bool {}
impl StylePropValue for f32 {}
impl StylePropValue for f64 {}
impl StylePropValue for Display {}
impl StylePropValue for Position {}
impl StylePropValue for FlexDirection {}
impl StylePropValue for FlexWrap {}
impl StylePropValue for AlignItems {}
impl StylePropValue for AlignContent {}
impl StylePropValue for CursorStyle {}
impl StylePropValue for BoxShadow {}
impl StylePropValue for String {}
impl StylePropValue for Weight {}
impl StylePropValue for cosmic_text::Style {}
impl StylePropValue for TextOverflow {}
impl StylePropValue for LineHeightValue {}
impl StylePropValue for Size<LengthPercentage> {}

impl<T: StylePropValue> StylePropValue for Option<T> {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        self.as_ref().and_then(|v| v.debug_view())
    }
}
impl StylePropValue for Px {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        Some(Box::new(text(format!("{} px", self.0))))
    }
}
impl StylePropValue for PxPctAuto {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        let label = match self {
            Self::Px(v) => format!("{} px", v),
            Self::Pct(v) => format!("{}%", v),
            Self::Auto => "auto".to_string(),
        };
        Some(Box::new(text(label)))
    }
}
impl StylePropValue for PxPct {
    fn debug_view(&self) -> Option<Box<dyn View>> {
        let label = match self {
            Self::Px(v) => format!("{} px", v),
            Self::Pct(v) => format!("{}%", v),
        };
        Some(Box::new(text(label)))
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
        Some(Box::new(
            stack((text(format!("{self:?}")), color)).style(|s| s.align_items(AlignItems::Center)),
        ))
    }
}

pub trait StyleProp: Default + Copy + 'static {
    type Type: StylePropValue;
    fn prop_ref() -> StylePropRef;
    fn default_value() -> Self::Type;
}

#[derive(Debug)]
pub struct StylePropInfo {
    pub(crate) name: fn() -> &'static str,
    pub(crate) inherited: bool,
    pub(crate) default_as_any: fn() -> Rc<dyn Any>,
    pub(crate) debug_any: fn(val: &dyn Any) -> String,
    pub(crate) debug_view: fn(val: &dyn Any) -> Option<Box<dyn View>>,
}

impl StylePropInfo {
    pub const fn new<Name, T: StylePropValue + 'static>(
        inherited: bool,
        default_as_any: fn() -> Rc<dyn Any>,
    ) -> Self {
        StylePropInfo {
            name: || std::any::type_name::<Name>(),
            inherited,
            default_as_any,
            debug_any: |val| {
                if let Some(v) = val.downcast_ref::<T>() {
                    format!("{:?}", v)
                } else {
                    panic!(
                        "expected type {} for property {}",
                        type_name::<T>(),
                        std::any::type_name::<Name>(),
                    )
                }
            },
            debug_view: |val| {
                if let Some(v) = val.downcast_ref::<T>() {
                    v.debug_view()
                } else {
                    panic!(
                        "expected type {} for property {}",
                        type_name::<T>(),
                        std::any::type_name::<Name>(),
                    )
                }
            },
        }
    }
}

#[derive(Copy, Clone)]
pub struct StylePropRef {
    pub info: &'static StylePropInfo,
}
impl PartialEq for StylePropRef {
    fn eq(&self, other: &Self) -> bool {
        ptr::eq(self.info, other.info)
    }
}
impl Hash for StylePropRef {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_usize(self.info as *const _ as usize)
    }
}
impl Eq for StylePropRef {}
impl Debug for StylePropRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", (self.info.name)())
    }
}

pub trait StylePropReader {
    type State: Debug;
    type Type: Clone;

    /// Reads the property from the current style state.
    /// Returns true if the property changed.
    fn read(state: &mut Self::State, cx: &LayoutCx) -> bool;

    fn get(state: &Self::State) -> Self::Type;
    fn new() -> Self::State;
}

impl<P: StyleProp> StylePropReader for P {
    type State = P::Type;
    type Type = P::Type;
    fn read(state: &mut Self::State, cx: &LayoutCx) -> bool {
        let new = cx
            .get_prop(P::default())
            .unwrap_or_else(|| P::default_value());
        let changed = new != *state;
        *state = new;
        changed
    }
    fn get(state: &Self::State) -> Self::Type {
        state.clone()
    }
    fn new() -> Self::State {
        P::default_value()
    }
}

impl<P: StyleProp> StylePropReader for Option<P> {
    type State = Option<P::Type>;
    type Type = Option<P::Type>;
    fn read(state: &mut Self::State, cx: &LayoutCx) -> bool {
        let new = cx.get_prop(P::default());
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

pub struct ExtratorField<R: StylePropReader> {
    state: R::State,
}

impl<R: StylePropReader> Debug for ExtratorField<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.state.fmt(f)
    }
}

impl<R: StylePropReader> ExtratorField<R> {
    pub fn read(&mut self, cx: &LayoutCx) -> bool {
        R::read(&mut self.state, cx)
    }
    pub fn get(&self) -> R::Type {
        R::get(&self.state)
    }
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self { state: R::new() }
    }
}

#[macro_export]
macro_rules! prop {
    ($v:vis $name:ident: $ty:ty { $($options:tt)* } = $default:expr
    ) => {
        #[derive(Default, Copy, Clone)]
        #[allow(non_camel_case_types)]
        $v struct $name;
        impl $crate::style::StyleProp for $name {
            type Type = $ty;
            fn prop_ref() -> $crate::style::StylePropRef {
                static INFO: $crate::style::StylePropInfo = $crate::style::StylePropInfo::new::<$name, $ty>(
                    prop!([impl inherited][$($options)*]),
                    || std::rc::Rc::new($name::default_value()),
                );
                $crate::style::StylePropRef { info: &INFO }
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
macro_rules! prop_extracter {
    (
        $vis:vis $name:ident {
            $($prop_vis:vis $prop:ident: $reader:ty),*
            $(,)?
        }
    ) => {
        #[derive(Debug)]
        $vis struct $name {
            $(
                $prop_vis $prop: $crate::style::ExtratorField<$reader>,
            )*
        }

        impl $name {
            $vis fn read(&mut self, cx: &$crate::context::LayoutCx) -> bool {
                false
                $(| self.$prop.read(cx))*
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
                        $prop: $crate::style::ExtratorField::new(),
                    )*
                }
            }
        }
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StyleMapValue<T> {
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

#[derive(Default, Clone)]
pub struct Style {
    pub(crate) map: HashMap<StylePropRef, StyleMapValue<Rc<dyn Any>>>,
    pub(crate) selectors: HashMap<StyleSelector, Style>,
    pub(crate) responsive: HashMap<ScreenSizeBp, Style>,
}

impl Style {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn get_prop_or_default<P: StyleProp>(&self) -> P::Type {
        self.get_prop::<P>().unwrap_or_else(|| P::default_value())
    }

    pub(crate) fn get_prop<P: StyleProp>(&self) -> Option<P::Type> {
        self.map
            .get(&P::prop_ref())
            .and_then(|v| v.as_ref())
            .map(|v| v.downcast_ref::<P::Type>().unwrap().clone())
    }

    pub(crate) fn get_prop_style_value<P: StyleProp>(&self) -> StyleValue<P::Type> {
        self.map
            .get(&P::prop_ref())
            .map(|v| match v {
                StyleMapValue::Val(v) => {
                    StyleValue::Val(v.downcast_ref::<P::Type>().unwrap().clone())
                }
                StyleMapValue::Unset => StyleValue::Unset,
            })
            .unwrap_or(StyleValue::Base)
    }

    pub(crate) fn selectors(&self) -> StyleSelectors {
        let mut result =
            self.selectors
                .iter()
                .fold(StyleSelectors::default(), |mut s, (selector, map)| {
                    s.selectors |= map.selectors().selectors;
                    s.set(*selector, true)
                });
        result.responsive |= !self.responsive.is_empty();
        result
    }

    pub(crate) fn apply_interact_state(
        &mut self,
        interact_state: &InteractionState,
        screen_size_bp: ScreenSizeBp,
    ) {
        if let Some(mut map) = self.responsive.remove(&screen_size_bp) {
            map.apply_interact_state(interact_state, screen_size_bp);
            self.apply_mut(map);
        }

        if interact_state.is_hovered && !interact_state.is_disabled {
            if let Some(mut map) = self.selectors.remove(&StyleSelector::Hover) {
                map.apply_interact_state(interact_state, screen_size_bp);
                self.apply_mut(map);
            }
        }
        if interact_state.is_focused {
            if let Some(mut map) = self.selectors.remove(&StyleSelector::Focus) {
                map.apply_interact_state(interact_state, screen_size_bp);
                self.apply_mut(map);
            }
        }
        if interact_state.is_disabled {
            if let Some(mut map) = self.selectors.remove(&StyleSelector::Disabled) {
                map.apply_interact_state(interact_state, screen_size_bp);
                self.apply_mut(map);
            }
        }

        let focused_keyboard =
            interact_state.using_keyboard_navigation && interact_state.is_focused;

        if focused_keyboard {
            if let Some(mut map) = self.selectors.remove(&StyleSelector::FocusVisible) {
                map.apply_interact_state(interact_state, screen_size_bp);
                self.apply_mut(map);
            }
        }

        let active_mouse = interact_state.is_hovered && !interact_state.using_keyboard_navigation;
        if interact_state.is_clicking && (active_mouse || focused_keyboard) {
            if let Some(mut map) = self.selectors.remove(&StyleSelector::Active) {
                map.apply_interact_state(interact_state, screen_size_bp);
                self.apply_mut(map);
            }
        }
    }

    pub(crate) fn apply_only_inherited(map: &mut Rc<Style>, over: &Style) {
        let any_inherited = over.map.iter().any(|(p, _)| p.info.inherited);

        if any_inherited {
            let inherited = over
                .map
                .iter()
                .filter(|(p, _)| p.info.inherited)
                .map(|(p, v)| (*p, v.clone()));

            Rc::make_mut(map).map.extend(inherited);
        }
    }

    fn set_selector(&mut self, selector: StyleSelector, map: Style) {
        match self.selectors.entry(selector) {
            Entry::Occupied(mut e) => e.get_mut().apply_mut(map),
            Entry::Vacant(e) => {
                e.insert(map);
            }
        }
    }

    fn set_breakpoint(&mut self, breakpoint: ScreenSizeBp, map: Style) {
        match self.responsive.entry(breakpoint) {
            Entry::Occupied(mut e) => e.get_mut().apply_mut(map),
            Entry::Vacant(e) => {
                e.insert(map);
            }
        }
    }

    pub(crate) fn builtin(&self) -> BuiltinStyle<'_> {
        BuiltinStyle { style: self }
    }

    pub(crate) fn apply_mut(&mut self, over: Style) {
        self.map.extend(over.map);
        for (selector, map) in over.selectors {
            self.set_selector(selector, map);
        }
        for (breakpoint, map) in over.responsive {
            self.set_breakpoint(breakpoint, map);
        }
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
                    .map(|(p, v)| {
                        (
                            *p,
                            match v {
                                StyleMapValue::Val(v) => (p.info.debug_any)(&**v),
                                StyleMapValue::Unset => "Unset".to_owned(),
                            },
                        )
                    })
                    .collect::<HashMap<StylePropRef, String>>(),
            )
            .field("selectors", &self.selectors)
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
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Default)]
pub(crate) struct StyleSelectors {
    selectors: u8,
    responsive: bool,
}

impl StyleSelectors {
    pub(crate) fn set(mut self, selector: StyleSelector, value: bool) -> Self {
        let v = (selector as isize).try_into().unwrap();
        let bit = 1_u8.checked_shl(v).unwrap();
        self.selectors = (self.selectors & !bit) | ((value as u8) << v);
        self
    }
    pub(crate) fn has(self, selector: StyleSelector) -> bool {
        let v = (selector as isize).try_into().unwrap();
        let bit = 1_u8.checked_shl(v).unwrap();
        self.selectors & bit != 0
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
    pub blur_radius: f64,
    pub color: Color,
    pub spread: f64,
    pub h_offset: f64,
    pub v_offset: f64,
}

impl Default for BoxShadow {
    fn default() -> Self {
        Self {
            blur_radius: 0.0,
            color: Color::BLACK,
            spread: 0.0,
            h_offset: 0.0,
            v_offset: 0.0,
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
    AlignSelf align_self: Option<AlignItems> {} = None,
    BorderLeft border_left: Px {} = Px(0.0),
    BorderTop border_top: Px {} = Px(0.0),
    BorderRight border_right: Px {} = Px(0.0),
    BorderBottom border_bottom: Px {} = Px(0.0),
    BorderRadius border_radius: Px {} = Px(0.0),
    OutlineColor outline_color: Color {} = Color::TRANSPARENT,
    Outline outline: Px {} = Px(0.0),
    BorderColor border_color: Color {} = Color::BLACK,
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
    TextColor color nocb: Option<Color> {} = None,
    Background background nocb: Option<Color> {} = None,
    BoxShadowProp box_shadow nocb: Option<BoxShadow> {} = None,
    FontSize font_size nocb: Option<f32> { inherited } = None,
    FontFamily font_family nocb: Option<String> { inherited } = None,
    FontWeight font_weight nocb: Option<Weight> { inherited } = None,
    FontStyle font_style nocb: Option<cosmic_text::Style> { inherited } = None,
    CursorColor cursor_color nocb: Option<Color> {} = None,
    TextOverflowProp text_overflow: TextOverflow {} = TextOverflow::Wrap,
    LineHeight line_height nocb: Option<LineHeightValue> { inherited } = None,
    AspectRatio aspect_ratio: Option<f32> {} = None,
    Gap gap: Size<LengthPercentage> {} = Size::zero(),
);

prop_extracter! {
    pub FontProps {
        pub size: FontSize,
        pub family: FontFamily,
        pub weight: FontWeight,
        pub style: FontStyle,
    }
}

prop_extracter! {
    pub(crate) LayoutProps {
        pub border_left: BorderLeft,
        pub border_top: BorderTop,
        pub border_right: BorderRight,
        pub border_bottom: BorderBottom,
        pub padding_left: PaddingLeft,
        pub padding_top: PaddingTop,
        pub padding_right: PaddingRight,
        pub padding_bottom: PaddingBottom,
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
        let insert: StyleMapValue<Rc<dyn Any>> = match value {
            StyleValue::Val(value) => StyleMapValue::Val(Rc::new(value)),
            StyleValue::Unset => StyleMapValue::Unset,
            StyleValue::Base => {
                self.map.remove(&P::prop_ref());
                return self;
            }
        };
        self.map.insert(P::prop_ref(), insert);
        self
    }

    fn selector(
        mut self,
        selector: StyleSelector,
        style: impl Fn(Style) -> Style + 'static,
    ) -> Self {
        let over = style(Style::default());
        self.set_selector(selector, over);
        self
    }

    /// The visual style to apply when the mouse hovers over the element
    pub fn hover(self, style: impl Fn(Style) -> Style + 'static) -> Self {
        self.selector(StyleSelector::Hover, style)
    }

    pub fn focus(self, style: impl Fn(Style) -> Style + 'static) -> Self {
        self.selector(StyleSelector::Focus, style)
    }

    /// Similar to the `:focus-visible` css selector, this style only activates when tab navigation is used.
    pub fn focus_visible(self, style: impl Fn(Style) -> Style + 'static) -> Self {
        self.selector(StyleSelector::FocusVisible, style)
    }

    pub fn disabled(self, style: impl Fn(Style) -> Style + 'static) -> Self {
        self.selector(StyleSelector::Disabled, style)
    }

    pub fn active(self, style: impl Fn(Style) -> Style + 'static) -> Self {
        self.selector(StyleSelector::Active, style)
    }

    pub fn responsive(
        mut self,
        size: ScreenSize,
        style: impl Fn(Style) -> Style + 'static,
    ) -> Self {
        let over = style(Style::default());
        for breakpoint in size.breakpoints() {
            self.set_breakpoint(breakpoint, over.clone());
        }
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

    pub fn background(self, color: impl Into<StyleValue<Color>>) -> Self {
        self.set_style_value(Background, color.into().map(Some))
    }

    pub fn box_shadow_blur(self, blur_radius: f64) -> Self {
        let mut value = self.get(BoxShadowProp).unwrap_or_default();
        value.blur_radius = blur_radius;
        self.set(BoxShadowProp, Some(value))
    }

    pub fn box_shadow_color(self, color: Color) -> Self {
        let mut value = self.get(BoxShadowProp).unwrap_or_default();
        value.color = color;
        self.set(BoxShadowProp, Some(value))
    }

    pub fn box_shadow_spread(self, spread: f64) -> Self {
        let mut value = self.get(BoxShadowProp).unwrap_or_default();
        value.spread = spread;
        self.set(BoxShadowProp, Some(value))
    }

    pub fn box_shadow_h_offset(self, h_offset: f64) -> Self {
        let mut value = self.get(BoxShadowProp).unwrap_or_default();
        value.h_offset = h_offset;
        self.set(BoxShadowProp, Some(value))
    }

    pub fn box_shadow_v_offset(self, v_offset: f64) -> Self {
        let mut value = self.get(BoxShadowProp).unwrap_or_default();
        value.v_offset = v_offset;
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

    pub fn font_style(self, style: impl Into<StyleValue<cosmic_text::Style>>) -> Self {
        self.set_style_value(FontStyle, style.into().map(Some))
    }

    pub fn cursor_color(self, color: impl Into<StyleValue<Color>>) -> Self {
        self.set_style_value(CursorColor, color.into().map(Some))
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
    /// ```rust,ignore
    /// let style = Style::default()
    ///    .apply_opt(Some(5.0), Style::padding) // ran
    ///    .apply_opt(None, Style::margin) // not ran
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
    /// ```rust,ignore
    /// let style = Style::default()
    ///     .apply_if(true, |s| s.padding(5.0)) // ran
    ///     .apply_if(false, |s| s.margin(5.0)) // not ran
    /// ```
    pub fn apply_if(self, cond: bool, f: impl FnOnce(Self) -> Self) -> Self {
        if cond {
            f(self)
        } else {
            self
        }
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
                left: LengthPercentage::Points(style.border_left().0 as f32),
                top: LengthPercentage::Points(style.border_top().0 as f32),
                right: LengthPercentage::Points(style.border_right().0 as f32),
                bottom: LengthPercentage::Points(style.border_bottom().0 as f32),
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
            gap: style.gap(),
            ..Default::default()
        }
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
