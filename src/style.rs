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

use dyn_clone::DynClone;
use floem_renderer::cosmic_text::{LineHeightValue, Style as FontStyle, Weight};
use peniko::Color;
pub use taffy::style::{
    AlignContent, AlignItems, Dimension, Display, FlexDirection, JustifyContent, Position,
};
use taffy::{
    geometry::Size,
    prelude::Rect,
    style::{FlexWrap, LengthPercentage, LengthPercentageAuto, Style as TaffyStyle},
    style_helpers::TaffyZero,
};

use crate::unit::{Pct, Px, PxOrPct};

pub trait StyleFn: DynClone {
    fn call(&self, style: Style) -> Style;
}

impl<F> StyleFn for F
where
    F: Fn(Style) -> Style + Clone,
{
    fn call(&self, style: Style) -> Style {
        self(style)
    }
}

pub enum StyleSelector {
    Hover,
    Focus,
    FocusVisible,
    Disabled,
    Active,
    Dragging,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextOverflow {
    Wrap,
    Clip,
    Ellipsis,
}

#[derive(Debug, Clone, Copy)]
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

macro_rules! define_styles {
    (
        $struct_name:ident with: $($name:ident $($opt:ident)?: $typ:ty = $val:expr),* $(,)?
    ) => {
        #[derive(Debug, Clone)]
        pub struct $struct_name {
            $(
                pub $name: $typ,
            )*
        }
        impl Default for $struct_name {
            fn default() -> Self {
                Self {
                    $(
                        $name: $val,
                    )*
                }
            }
        }

        impl $struct_name {
            $(
                define_styles!(decl: $name $($opt)?: $typ = $val);
            )*
        }
    };

    // internal submacro

    // 'nocb' doesn't add a builder function
    (decl: $name:ident nocb: $typ:ty = $val:expr) => {};
    (decl: $name:ident: $typ:ty = $val:expr) => {
        pub fn $name(mut self, v: impl Into<$typ>) -> Self {
            self.$name = v.into();
            self
        }
    }
}

use super::*;

define_styles!(
    BoxShadow with:
    h_offset: Px = Px(0.0),
    v_offset: Px = Px(0.0),
    blur_radius: Px = Px(0.0),
    spread: Px = Px(0.0),
    color: Color = Color::BLACK,
);

pub fn box_shadow(
    h_offset: impl Into<Px>,
    v_offset: impl Into<Px>,
    blur_radius: impl Into<Px>,
    spread: impl Into<Px>,
    color: impl Into<Color>,
) -> BoxShadow {
    BoxShadow {
        h_offset: h_offset.into(),
        v_offset: v_offset.into(),
        blur_radius: blur_radius.into(),
        spread: spread.into(),
        color: color.into(),
    }
}

define_styles!(
    Style with:
    display: Display = Display::Flex,
    position: Position = Position::Relative,
    width nocb: Dimension = Dimension::Auto,
    height nocb: Dimension = Dimension::Auto,
    min_width nocb: Dimension = Dimension::Auto,
    min_height nocb: Dimension = Dimension::Auto,
    max_width nocb: Dimension = Dimension::Auto,
    max_height nocb: Dimension = Dimension::Auto,
    flex_direction: FlexDirection = FlexDirection::Row,
    flex_wrap: FlexWrap = FlexWrap::NoWrap,
    flex_grow: f32 = 0.0,
    flex_shrink: f32 = 1.0,
    flex_basis nocb: Dimension = Dimension::Auto,
    justify_content: Option<JustifyContent> = None,
    justify_self: Option<AlignItems> = None,
    align_items: Option<AlignItems> = None,
    align_content: Option<AlignContent> = None,
    align_self: Option<AlignItems> = None,
    border_left: Px = Px(0.0),
    border_top: Px = Px(0.0),
    border_right: Px = Px(0.0),
    border_bottom: Px = Px(0.0),
    border_radius: Px = Px(0.0),
    outline_color: Color = Color::TRANSPARENT,
    outline: Px = Px(0.0),
    border_color: Color = Color::BLACK,
    padding_left nocb: LengthPercentage = LengthPercentage::ZERO,
    padding_top nocb: LengthPercentage = LengthPercentage::ZERO,
    padding_right nocb: LengthPercentage = LengthPercentage::ZERO,
    padding_bottom nocb: LengthPercentage = LengthPercentage::ZERO,
    margin_left nocb: LengthPercentageAuto = LengthPercentageAuto::ZERO,
    margin_top nocb: LengthPercentageAuto = LengthPercentageAuto::ZERO,
    margin_right nocb: LengthPercentageAuto = LengthPercentageAuto::ZERO,
    margin_bottom nocb: LengthPercentageAuto = LengthPercentageAuto::ZERO,
    inset_left nocb: LengthPercentageAuto = LengthPercentageAuto::Auto,
    inset_top nocb: LengthPercentageAuto = LengthPercentageAuto::Auto,
    inset_right nocb: LengthPercentageAuto = LengthPercentageAuto::Auto,
    inset_bottom nocb: LengthPercentageAuto = LengthPercentageAuto::Auto,
    z_index nocb: Option<i32> = None,
    cursor nocb: Option<CursorStyle> = None,
    color nocb: Option<Color> = None,
    background nocb: Option<Color> = None,
    box_shadows: Vec<BoxShadow> = Vec::new(),
    scroll_bar_color nocb: Option<Color> = None,
    scroll_bar_rounded nocb: Option<bool> = None,
    scroll_bar_thickness nocb: Option<Px> = None,
    scroll_bar_edge_width nocb: Option<Px> = None,
    font_size nocb: Option<f32> = None,
    font_family nocb: Option<String> = None,
    font_weight nocb: Option<Weight> = None,
    font_style nocb: Option<FontStyle> = None,
    cursor_color nocb: Option<Color> = None,
    text_overflow: TextOverflow = TextOverflow::Wrap,
    line_height nocb: Option<LineHeightValue> = None,
    aspect_ratio: Option<f32> = None,
    gap: Size<LengthPercentage> = Size::zero(),
);

impl Style {
    pub fn width(mut self, width: impl Into<PxOrPct>) -> Self {
        self.width = match width.into() {
            PxOrPct::Px(Px(px)) => Dimension::Points(px as f32),
            PxOrPct::Pct(Pct(pct)) => Dimension::Percent(pct as f32 / 100.0),
        };
        self
    }

    pub fn height(mut self, height: impl Into<PxOrPct>) -> Self {
        self.height = match height.into() {
            PxOrPct::Px(Px(px)) => Dimension::Points(px as f32),
            PxOrPct::Pct(Pct(pct)) => Dimension::Percent(pct as f32 / 100.0),
        };
        self
    }

    pub fn size(self, width: impl Into<PxOrPct>, height: impl Into<PxOrPct>) -> Self {
        self.width(width).height(height)
    }

    pub fn min_width(mut self, min_width: impl Into<PxOrPct>) -> Self {
        self.min_width = match min_width.into() {
            PxOrPct::Px(Px(px)) => Dimension::Points(px as f32),
            PxOrPct::Pct(Pct(pct)) => Dimension::Percent(pct as f32 / 100.0),
        };
        self
    }

    pub fn min_height(mut self, min_height: impl Into<PxOrPct>) -> Self {
        self.min_height = match min_height.into() {
            PxOrPct::Px(Px(px)) => Dimension::Points(px as f32),
            PxOrPct::Pct(Pct(pct)) => Dimension::Percent(pct as f32 / 100.0),
        };
        self
    }

    pub fn min_size(self, min_width: impl Into<PxOrPct>, min_height: impl Into<PxOrPct>) -> Self {
        self.min_width(min_width).min_height(min_height)
    }

    pub fn max_width(mut self, max_width: impl Into<PxOrPct>) -> Self {
        self.max_width = match max_width.into() {
            PxOrPct::Px(Px(px)) => Dimension::Points(px as f32),
            PxOrPct::Pct(Pct(pct)) => Dimension::Percent(pct as f32 / 100.0),
        };
        self
    }

    pub fn max_height(mut self, max_height: impl Into<PxOrPct>) -> Self {
        self.max_height = match max_height.into() {
            PxOrPct::Px(Px(px)) => Dimension::Points(px as f32),
            PxOrPct::Pct(Pct(pct)) => Dimension::Percent(pct as f32 / 100.0),
        };
        self
    }

    pub fn max_size(self, max_width: impl Into<PxOrPct>, max_height: impl Into<PxOrPct>) -> Self {
        self.max_width(max_width).max_height(max_height)
    }

    pub fn border(mut self, border: impl Into<Px>) -> Self {
        let border = border.into();
        self.border_left = border;
        self.border_top = border;
        self.border_right = border;
        self.border_bottom = border;
        self
    }

    /// Sets `border_left` and `border_right` to `border`
    pub fn border_horiz(mut self, border: impl Into<Px>) -> Self {
        let border = border.into();
        self.border_left = border;
        self.border_right = border;
        self
    }

    /// Sets `border_top` and `border_bottom` to `border`
    pub fn border_vert(mut self, border: impl Into<Px>) -> Self {
        let border = border.into();
        self.border_top = border;
        self.border_bottom = border;
        self
    }

    pub fn padding_left(mut self, padding_left: impl Into<PxOrPct>) -> Self {
        self.padding_left = match padding_left.into() {
            PxOrPct::Px(Px(px)) => LengthPercentage::Points(px as f32),
            PxOrPct::Pct(Pct(pct)) => LengthPercentage::Percent(pct as f32 / 100.0),
        };
        self
    }

    pub fn padding_right(mut self, padding_right: impl Into<PxOrPct>) -> Self {
        self.padding_right = match padding_right.into() {
            PxOrPct::Px(Px(px)) => LengthPercentage::Points(px as f32),
            PxOrPct::Pct(Pct(pct)) => LengthPercentage::Percent(pct as f32 / 100.0),
        };
        self
    }

    pub fn padding_top(mut self, padding_top: impl Into<PxOrPct>) -> Self {
        self.padding_top = match padding_top.into() {
            PxOrPct::Px(Px(px)) => LengthPercentage::Points(px as f32),
            PxOrPct::Pct(Pct(pct)) => LengthPercentage::Percent(pct as f32 / 100.0),
        };
        self
    }

    pub fn padding_bottom(mut self, padding_bottom: impl Into<PxOrPct>) -> Self {
        self.padding_bottom = match padding_bottom.into() {
            PxOrPct::Px(Px(px)) => LengthPercentage::Points(px as f32),
            PxOrPct::Pct(Pct(pct)) => LengthPercentage::Percent(pct as f32 / 100.0),
        };
        self
    }

    /// Set padding on all directions
    pub fn padding(self, padding: impl Into<PxOrPct>) -> Self {
        let padding = padding.into();
        self.padding_left(padding)
            .padding_top(padding)
            .padding_right(padding)
            .padding_bottom(padding)
    }

    /// Sets `padding_left` and `padding_right` to `padding`
    pub fn padding_horiz(self, padding: impl Into<PxOrPct>) -> Self {
        let padding = padding.into();
        self.padding_left(padding).padding_right(padding)
    }

    /// Sets `padding_top` and `padding_bottom` to `padding`
    pub fn padding_vert(self, padding: impl Into<PxOrPct>) -> Self {
        let padding = padding.into();
        self.padding_top(padding).padding_bottom(padding)
    }

    pub fn margin_left(mut self, margin_left: impl Into<PxOrPct>) -> Self {
        self.margin_left = match margin_left.into() {
            PxOrPct::Px(Px(px)) => LengthPercentageAuto::Points(px as f32),
            PxOrPct::Pct(Pct(pct)) => LengthPercentageAuto::Percent(pct as f32 / 100.0),
        };
        self
    }

    pub fn margin_right(mut self, margin_right: impl Into<PxOrPct>) -> Self {
        self.margin_right = match margin_right.into() {
            PxOrPct::Px(Px(px)) => LengthPercentageAuto::Points(px as f32),
            PxOrPct::Pct(Pct(pct)) => LengthPercentageAuto::Percent(pct as f32 / 100.0),
        };
        self
    }

    pub fn margin_top(mut self, margin_top: impl Into<PxOrPct>) -> Self {
        self.margin_top = match margin_top.into() {
            PxOrPct::Px(Px(px)) => LengthPercentageAuto::Points(px as f32),
            PxOrPct::Pct(Pct(pct)) => LengthPercentageAuto::Percent(pct as f32 / 100.0),
        };
        self
    }

    pub fn margin_bottom(mut self, margin_bottom: impl Into<PxOrPct>) -> Self {
        self.margin_bottom = match margin_bottom.into() {
            PxOrPct::Px(Px(px)) => LengthPercentageAuto::Points(px as f32),
            PxOrPct::Pct(Pct(pct)) => LengthPercentageAuto::Percent(pct as f32 / 100.0),
        };
        self
    }

    /// Set margin on all directions
    pub fn margin(self, margin: impl Into<PxOrPct>) -> Self {
        let margin = margin.into();
        self.margin_left(margin)
            .margin_top(margin)
            .margin_right(margin)
            .margin_bottom(margin)
    }

    /// Sets `margin_left` and `margin_right` to `margin`
    pub fn margin_horiz(self, margin: impl Into<PxOrPct>) -> Self {
        let margin = margin.into();
        self.margin_left(margin).margin_right(margin)
    }

    /// Sets `margin_top` and `margin_bottom` to `margin`
    pub fn margin_vert(self, margin: impl Into<PxOrPct>) -> Self {
        let margin = margin.into();
        self.margin_top(margin).margin_bottom(margin)
    }

    pub fn inset_left(mut self, inset_left: impl Into<PxOrPct>) -> Self {
        self.inset_left = match inset_left.into() {
            PxOrPct::Px(Px(px)) => LengthPercentageAuto::Points(px as f32),
            PxOrPct::Pct(Pct(pct)) => LengthPercentageAuto::Percent(pct as f32 / 100.0),
        };
        self
    }

    pub fn inset_right(mut self, inset_right: impl Into<PxOrPct>) -> Self {
        self.inset_right = match inset_right.into() {
            PxOrPct::Px(Px(px)) => LengthPercentageAuto::Points(px as f32),
            PxOrPct::Pct(Pct(pct)) => LengthPercentageAuto::Percent(pct as f32 / 100.0),
        };
        self
    }

    pub fn inset_top(mut self, inset_top: impl Into<PxOrPct>) -> Self {
        self.inset_top = match inset_top.into() {
            PxOrPct::Px(Px(px)) => LengthPercentageAuto::Points(px as f32),
            PxOrPct::Pct(Pct(pct)) => LengthPercentageAuto::Percent(pct as f32 / 100.0),
        };
        self
    }

    pub fn inset_bottom(mut self, inset_bottom: impl Into<PxOrPct>) -> Self {
        self.inset_bottom = match inset_bottom.into() {
            PxOrPct::Px(Px(px)) => LengthPercentageAuto::Points(px as f32),
            PxOrPct::Pct(Pct(pct)) => LengthPercentageAuto::Percent(pct as f32 / 100.0),
        };
        self
    }

    /// Set inset on all directions
    pub fn inset(self, inset: impl Into<PxOrPct>) -> Self {
        let inset = inset.into();
        self.inset_left(inset)
            .inset_top(inset)
            .inset_right(inset)
            .inset_bottom(inset)
    }

    pub fn cursor(mut self, cursor: CursorStyle) -> Self {
        self.cursor = Some(cursor);
        self
    }

    pub fn color(mut self, color: impl Into<Color>) -> Self {
        self.color = Some(color.into());
        self
    }

    pub fn background(mut self, color: impl Into<Color>) -> Self {
        self.background = Some(color.into());
        self
    }

    pub fn box_shadow(self, box_shadow: BoxShadow) -> Self {
        self.box_shadows(vec![box_shadow])
    }

    pub fn scroll_bar_color(mut self, color: impl Into<Color>) -> Self {
        self.scroll_bar_color = Some(color.into());
        self
    }

    pub fn scroll_bar_rounded(mut self, rounded: bool) -> Self {
        self.scroll_bar_rounded = Some(rounded);
        self
    }

    pub fn scroll_bar_thickness(mut self, thickness: impl Into<Px>) -> Self {
        self.scroll_bar_thickness = Some(thickness.into());
        self
    }

    pub fn scroll_bar_edge_width(mut self, edge_width: impl Into<Px>) -> Self {
        self.scroll_bar_edge_width = Some(edge_width.into());
        self
    }

    pub fn font_size(mut self, size: impl Into<f32>) -> Self {
        self.font_size = Some(size.into());
        self
    }

    pub fn font_family(mut self, family: impl Into<String>) -> Self {
        self.font_family = Some(family.into());
        self
    }

    pub fn font_weight(mut self, weight: impl Into<Weight>) -> Self {
        self.font_weight = Some(weight.into());
        self
    }

    pub fn font_bold(self) -> Self {
        self.font_weight(Weight::BOLD)
    }

    pub fn font_style(mut self, style: impl Into<FontStyle>) -> Self {
        self.font_style = Some(style.into());
        self
    }

    pub fn cursor_color(mut self, color: impl Into<Color>) -> Self {
        self.cursor_color = Some(color.into());
        self
    }

    pub fn line_height(mut self, normal: f32) -> Self {
        self.line_height = Some(LineHeightValue::Normal(normal));
        self
    }

    pub fn text_ellipsis(mut self) -> Self {
        self.text_overflow = TextOverflow::Ellipsis;
        self
    }

    pub fn text_clip(mut self) -> Self {
        self.text_overflow = TextOverflow::Clip;
        self
    }

    pub fn absolute(mut self) -> Self {
        self.position = Position::Absolute;
        self
    }

    pub fn items_start(mut self) -> Self {
        self.align_items = Some(AlignItems::FlexStart);
        self
    }

    /// Defines the alignment along the cross axis as Centered
    pub fn items_center(mut self) -> Self {
        self.align_items = Some(AlignItems::Center);
        self
    }

    pub fn items_end(mut self) -> Self {
        self.align_items = Some(AlignItems::FlexEnd);
        self
    }

    /// Defines the alignment along the main axis as Centered
    pub fn justify_center(mut self) -> Self {
        self.justify_content = Some(JustifyContent::Center);
        self
    }

    pub fn justify_end(mut self) -> Self {
        self.justify_content = Some(JustifyContent::FlexEnd);
        self
    }

    pub fn justify_start(mut self) -> Self {
        self.justify_content = Some(JustifyContent::FlexStart);
        self
    }

    pub fn justify_between(mut self) -> Self {
        self.justify_content = Some(JustifyContent::SpaceBetween);
        self
    }

    pub fn hide(mut self) -> Self {
        self.display = Display::None;
        self
    }

    pub fn flex(mut self) -> Self {
        self.display = Display::Flex;
        self
    }

    pub fn flex_basis(mut self, basis: impl Into<PxOrPct>) -> Self {
        match basis.into() {
            PxOrPct::Px(Px(px)) => self.flex_basis = Dimension::Points(px as f32),
            PxOrPct::Pct(Pct(pct)) => self.flex_basis = Dimension::Percent(pct as f32 / 100.0),
        }
        self
    }

    pub fn flex_row(mut self) -> Self {
        self.flex_direction = FlexDirection::Row;
        self
    }

    pub fn flex_col(mut self) -> Self {
        self.flex_direction = FlexDirection::Column;
        self
    }

    pub fn z_index(mut self, z_index: i32) -> Self {
        self.z_index = Some(z_index);
        self
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
        TaffyStyle {
            display: self.display,
            position: self.position,
            size: taffy::prelude::Size {
                width: self.width,
                height: self.height,
            },
            min_size: taffy::prelude::Size {
                width: self.min_width,
                height: self.min_height,
            },
            max_size: taffy::prelude::Size {
                width: self.max_width,
                height: self.max_height,
            },
            flex_direction: self.flex_direction,
            flex_grow: self.flex_grow,
            flex_shrink: self.flex_shrink,
            flex_basis: self.flex_basis,
            flex_wrap: self.flex_wrap,
            justify_content: self.justify_content,
            justify_self: self.justify_self,
            align_items: self.align_items,
            align_content: self.align_content,
            align_self: self.align_self,
            aspect_ratio: self.aspect_ratio,
            border: Rect {
                left: LengthPercentage::Points(self.border_left.0 as f32),
                top: LengthPercentage::Points(self.border_top.0 as f32),
                right: LengthPercentage::Points(self.border_right.0 as f32),
                bottom: LengthPercentage::Points(self.border_bottom.0 as f32),
            },
            padding: Rect {
                left: self.padding_left,
                top: self.padding_top,
                right: self.padding_right,
                bottom: self.padding_bottom,
            },
            margin: Rect {
                left: self.margin_left,
                top: self.margin_top,
                right: self.margin_right,
                bottom: self.margin_bottom,
            },
            inset: Rect {
                left: self.inset_left,
                top: self.inset_top,
                right: self.inset_right,
                bottom: self.inset_bottom,
            },
            gap: self.gap,
            ..Default::default()
        }
    }
}

pub mod short {
    use peniko::Color;

    use super::Style;
    use crate::unit::PxOrPct;

    impl Style {
        pub fn p(self, p: impl Into<PxOrPct>) -> Self {
            self.padding(p)
        }

        pub fn px(self, p: impl Into<PxOrPct>) -> Self {
            self.padding_horiz(p)
        }

        pub fn py(self, p: impl Into<PxOrPct>) -> Self {
            self.padding_vert(p)
        }

        pub fn pl(self, p: impl Into<PxOrPct>) -> Self {
            self.padding_left(p)
        }

        pub fn pr(self, p: impl Into<PxOrPct>) -> Self {
            self.padding_right(p)
        }

        pub fn pt(self, p: impl Into<PxOrPct>) -> Self {
            self.padding_top(p)
        }

        pub fn pb(self, p: impl Into<PxOrPct>) -> Self {
            self.padding_bottom(p)
        }

        pub fn m(self, m: impl Into<PxOrPct>) -> Self {
            self.margin(m)
        }

        pub fn mx(self, m: impl Into<PxOrPct>) -> Self {
            self.margin_horiz(m)
        }

        pub fn my(self, m: impl Into<PxOrPct>) -> Self {
            self.margin_vert(m)
        }

        pub fn ml(self, m: impl Into<PxOrPct>) -> Self {
            self.margin_left(m)
        }

        pub fn mr(self, m: impl Into<PxOrPct>) -> Self {
            self.margin_right(m)
        }

        pub fn mt(self, m: impl Into<PxOrPct>) -> Self {
            self.margin_top(m)
        }

        pub fn mb(self, m: impl Into<PxOrPct>) -> Self {
            self.margin_bottom(m)
        }

        pub fn bg(self: Style, color: impl Into<Color>) -> Style {
            self.background(color)
        }
    }
}

pub use short::*;

// #[cfg(test)]
// mod tests {
//     use taffy::style::LengthPercentage;

//     use super::{Style, StyleValue};

//     #[test]
//     fn style_override() {
//         let style1 = Style::BASE.padding_left(32.0);
//         let style2 = Style::BASE.padding_left(64.0);

//         let style = style1.apply(style2);

//         assert_eq!(
//             style.padding_left,
//             StyleValue::Val(LengthPercentage::Points(64.0))
//         );

//         let style1 = Style::BASE.padding_left(32.0).padding_bottom(45.0);
//         let style2 = Style::BASE
//             .padding_left(64.0)
//             .padding_bottom(StyleValue::Base);

//         let style = style1.apply(style2);

//         assert_eq!(
//             style.padding_left,
//             StyleValue::Val(LengthPercentage::Points(64.0))
//         );
//         assert_eq!(
//             style.padding_bottom,
//             StyleValue::Val(LengthPercentage::Points(45.0))
//         );

//         let style1 = Style::BASE.padding_left(32.0).padding_bottom(45.0);
//         let style2 = Style::BASE
//             .padding_left(LengthPercentage::Points(64.0))
//             .padding_bottom(StyleValue::Unset);

//         let style = style1.apply(style2);

//         assert_eq!(
//             style.padding_left,
//             StyleValue::Val(LengthPercentage::Points(64.0))
//         );
//         assert_eq!(style.padding_bottom, StyleValue::Unset);

//         let style1 = Style::BASE.padding_left(32.0).padding_bottom(45.0);
//         let style2 = Style::BASE
//             .padding_left(64.0)
//             .padding_bottom(StyleValue::Unset);
//         let style3 = Style::BASE.padding_bottom(StyleValue::Base);

//         let style = style1.apply_overriding_styles([style2, style3].into_iter());

//         assert_eq!(
//             style.padding_left,
//             StyleValue::Val(LengthPercentage::Points(64.0))
//         );
//         assert_eq!(style.padding_bottom, StyleValue::Unset);

//         let style1 = Style::BASE.padding_left(32.0).padding_bottom(45.0);
//         let style2 = Style::BASE
//             .padding_left(LengthPercentage::Points(64.0))
//             .padding_bottom(StyleValue::Unset);
//         let style3 = Style::BASE.padding_bottom(StyleValue::Val(LengthPercentage::Points(100.0)));

//         let style = style1.apply_overriding_styles([style2, style3].into_iter());

//         assert_eq!(
//             style.padding_left,
//             StyleValue::Val(LengthPercentage::Points(64.0))
//         );
//         assert_eq!(
//             style.padding_bottom,
//             StyleValue::Val(LengthPercentage::Points(100.0))
//         );
//     }
// }
