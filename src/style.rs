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

use floem_renderer::cosmic_text::{LineHeightValue, Style as FontStyle, Weight};
use peniko::Color;
pub use taffy::style::{
    AlignContent, AlignItems, Dimension, Display, FlexDirection, JustifyContent, Position,
};
use taffy::{
    geometry::Size,
    prelude::Rect,
    style::{FlexWrap, LengthPercentage, Style as TaffyStyle},
};

use crate::unit::{Px, PxPct, PxPctAuto, UnitExt};

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

#[derive(Debug, Clone, Copy)]
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

// Creates `ComputedStyle` which has definite values for the fields, barring some specific cases.
// Creates `Style` which has `StyleValue<T>`s for the fields
macro_rules! define_styles {
    (
        $($name:ident $name_sv:ident $($opt:ident)?: $typ:ty = $val:expr),* $(,)?
    ) => {
        /// A style with definite values for most fields.
        #[derive(Debug, Clone)]
        pub struct ComputedStyle {
            $(
                pub $name: $typ,
            )*
        }
        impl ComputedStyle {
            $(
                pub fn $name(mut self, v: impl Into<$typ>) -> Self {
                    self.$name = v.into();
                    self
                }
            )*
        }
        impl Default for ComputedStyle {
            fn default() -> Self {
                Self {
                    $(
                        $name: $val,
                    )*
                }
            }
        }

        #[derive(Debug, Clone)]
        pub struct Style {
            $(
                pub $name: StyleValue<$typ>,
            )*
        }
        impl Style {
            pub const BASE: Style = Style{
                $(
                    $name: StyleValue::Base,
                )*
            };

            pub const UNSET: Style = Style{
                $(
                    $name: StyleValue::Unset,
                )*
            };

            $(
                define_styles!(decl: $name $name_sv $($opt)?: $typ = $val);
            )*

            /// Convert this `Style` into a computed style, using the given `ComputedStyle` as a base
            /// for any missing values.
            pub fn compute(self, underlying: &ComputedStyle) -> ComputedStyle {
                ComputedStyle {
                    $(
                        $name: self.$name.unwrap_or_else(|| underlying.$name.clone()),
                    )*
                }
            }

            /// Apply another `Style` to this style, returning a new `Style` with the overrides
            ///
            /// `StyleValue::Val` will override the value with the given value
            /// `StyleValue::Unset` will unset the value, causing it to fall back to the underlying
            /// `ComputedStyle` (aka setting it to `None`)
            /// `StyleValue::Base` will leave the value as-is, whether falling back to the underlying
            /// `ComputedStyle` or using the value in the `Style`.
            pub fn apply(self, over: Style) -> Style {
                Style {
                    $(
                        $name: match (self.$name, over.$name) {
                            (_, StyleValue::Val(x)) => StyleValue::Val(x),
                            (StyleValue::Val(x), StyleValue::Base) => StyleValue::Val(x),
                            (StyleValue::Val(_), StyleValue::Unset) => StyleValue::Unset,
                            (StyleValue::Base, StyleValue::Base) => StyleValue::Base,
                            (StyleValue::Unset, StyleValue::Base) => StyleValue::Unset,
                            (StyleValue::Base, StyleValue::Unset) => StyleValue::Unset,
                            (StyleValue::Unset, StyleValue::Unset) => StyleValue::Unset,
                        },
                    )*
                }
            }

            /// Apply multiple `Style`s to this style, returning a new `Style` with the overrides.
            /// Later styles take precedence over earlier styles.
            pub fn apply_overriding_styles(self, overrides: impl Iterator<Item = Style>) -> Style {
                overrides.fold(self, |acc, x| acc.apply(x))
            }
        }
    };
    // internal submacro

    // 'nocb' doesn't add a builder function
    (decl: $name:ident $name_sv:ident nocb: $typ:ty = $val:expr) => {};
    (decl: $name:ident $name_sv:ident: $typ:ty = $val:expr) => {
        pub fn $name(mut self, v: impl Into<$typ>) -> Self
        {
            self.$name = StyleValue::Val(v.into());
            self
        }

        pub fn $name_sv(mut self, v: StyleValue<$typ>) -> Self
        {
            self.$name = v;
            self
        }
    }
}

define_styles!(
    display display_sv: Display = Display::Flex,
    position position_sv: Position = Position::Relative,
    width width_sv: PxPctAuto = PxPctAuto::Auto,
    height height_sv: PxPctAuto = PxPctAuto::Auto,
    min_width min_width_sv: PxPctAuto = PxPctAuto::Auto,
    min_height min_height_sv: PxPctAuto = PxPctAuto::Auto,
    max_width max_width_sv: PxPctAuto = PxPctAuto::Auto,
    max_height max_height_sv: PxPctAuto = PxPctAuto::Auto,
    flex_direction flex_direction_sv: FlexDirection = FlexDirection::Row,
    flex_wrap flex_wrap_sv: FlexWrap = FlexWrap::NoWrap,
    flex_grow flex_grow_sv: f32 = 0.0,
    flex_shrink flex_shrink_sv: f32 = 1.0,
    flex_basis flex_basis_sv: PxPctAuto = PxPctAuto::Auto,
    justify_content justify_content_sv: Option<JustifyContent> = None,
    justify_self justify_self_sv: Option<AlignItems> = None,
    align_items align_items_sv: Option<AlignItems> = None,
    align_content align_content_sv: Option<AlignContent> = None,
    align_self align_self_sv: Option<AlignItems> = None,
    border_left border_left_sv: Px = Px(0.0),
    border_top border_top_sv: Px = Px(0.0),
    border_right border_right_sv: Px = Px(0.0),
    border_bottom border_bottom_sv: Px = Px(0.0),
    border_radius border_radius_sv: Px = Px(0.0),
    outline_color outline_color_sv: Color = Color::TRANSPARENT,
    outline outline_sv: Px = Px(0.0),
    border_color border_color_sv: Color = Color::BLACK,
    padding_left padding_left_sv: PxPct = PxPct::Px(0.0),
    padding_top padding_top_sv: PxPct = PxPct::Px(0.0),
    padding_right padding_right_sv: PxPct = PxPct::Px(0.0),
    padding_bottom padding_bottom_sv: PxPct = PxPct::Px(0.0),
    margin_left margin_left_sv: PxPctAuto = PxPctAuto::Px(0.0),
    margin_top margin_top_sv: PxPctAuto = PxPctAuto::Px(0.0),
    margin_right margin_right_sv: PxPctAuto = PxPctAuto::Px(0.0),
    margin_bottom margin_bottom_sv: PxPctAuto = PxPctAuto::Px(0.0),
    inset_left inset_left_sv: PxPctAuto = PxPctAuto::Auto,
    inset_top inset_top_sv: PxPctAuto = PxPctAuto::Auto,
    inset_right inset_right_sv: PxPctAuto = PxPctAuto::Auto,
    inset_bottom inset_bottom_sv: PxPctAuto = PxPctAuto::Auto,
    z_index z_index_sv nocb: Option<i32> = None,
    cursor cursor_sv nocb: Option<CursorStyle> = None,
    color color_sv nocb: Option<Color> = None,
    background background_sv nocb: Option<Color> = None,
    box_shadow box_shadow_sv nocb: Option<BoxShadow> = None,
    scroll_bar_color scroll_bar_color_sv nocb: Option<Color> = None,
    scroll_bar_rounded scroll_bar_rounded_sv nocb: Option<bool> = None,
    scroll_bar_thickness scroll_bar_thickness_sv nocb: Option<Px> = None,
    scroll_bar_edge_width scroll_bar_edge_width_sv nocb: Option<Px> = None,
    font_size font_size_sv nocb: Option<f32> = None,
    font_family font_family_sv nocb: Option<String> = None,
    font_weight font_weight_sv nocb: Option<Weight> = None,
    font_style font_style_sv nocb: Option<FontStyle> = None,
    cursor_color cursor_color_sv nocb: Option<Color> = None,
    text_overflow text_overflow_sv: TextOverflow = TextOverflow::Wrap,
    line_height line_height_sv nocb: Option<LineHeightValue> = None,
    aspect_ratio aspect_ratio_sv: Option<f32> = None,
    gap gap_sv: Size<LengthPercentage> = Size::zero(),
);

impl Style {
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

    pub fn cursor(mut self, cursor: impl Into<StyleValue<CursorStyle>>) -> Self {
        self.cursor = cursor.into().map(Some);
        self
    }

    pub fn color(mut self, color: impl Into<StyleValue<Color>>) -> Self {
        self.color = color.into().map(Some);
        self
    }

    pub fn background(mut self, color: impl Into<StyleValue<Color>>) -> Self {
        self.background = color.into().map(Some);
        self
    }

    pub fn box_shadow_blur(mut self, blur_radius: f64) -> Self {
        if let Some(box_shadow) = self.box_shadow.as_mut() {
            if let Some(box_shadow) = box_shadow.as_mut() {
                box_shadow.blur_radius = blur_radius;
                return self;
            }
        }

        self.box_shadow = Some(BoxShadow {
            blur_radius,
            ..Default::default()
        })
        .into();
        self
    }

    pub fn box_shadow_color(mut self, color: Color) -> Self {
        if let Some(box_shadow) = self.box_shadow.as_mut() {
            if let Some(box_shadow) = box_shadow.as_mut() {
                box_shadow.color = color;
                return self;
            }
        }

        self.box_shadow = Some(BoxShadow {
            color,
            ..Default::default()
        })
        .into();
        self
    }

    pub fn box_shadow_spread(mut self, spread: f64) -> Self {
        if let Some(box_shadow) = self.box_shadow.as_mut() {
            if let Some(box_shadow) = box_shadow.as_mut() {
                box_shadow.spread = spread;
                return self;
            }
        }

        self.box_shadow = Some(BoxShadow {
            spread,
            ..Default::default()
        })
        .into();
        self
    }

    pub fn box_shadow_h_offset(mut self, h_offset: f64) -> Self {
        if let Some(box_shadow) = self.box_shadow.as_mut() {
            if let Some(box_shadow) = box_shadow.as_mut() {
                box_shadow.h_offset = h_offset;
                return self;
            }
        }

        self.box_shadow = Some(BoxShadow {
            h_offset,
            ..Default::default()
        })
        .into();
        self
    }

    pub fn box_shadow_v_offset(mut self, v_offset: f64) -> Self {
        if let Some(box_shadow) = self.box_shadow.as_mut() {
            if let Some(box_shadow) = box_shadow.as_mut() {
                box_shadow.v_offset = v_offset;
                return self;
            }
        }

        self.box_shadow = Some(BoxShadow {
            v_offset,
            ..Default::default()
        })
        .into();
        self
    }

    pub fn scroll_bar_color(mut self, color: impl Into<StyleValue<Color>>) -> Self {
        self.scroll_bar_color = color.into().map(Some);
        self
    }

    pub fn scroll_bar_rounded(mut self, rounded: impl Into<StyleValue<bool>>) -> Self {
        self.scroll_bar_rounded = rounded.into().map(Some);
        self
    }

    pub fn scroll_bar_thickness(mut self, thickness: impl Into<Px>) -> Self {
        self.scroll_bar_thickness = StyleValue::Val(Some(thickness.into()));
        self
    }

    pub fn scroll_bar_edge_width(mut self, edge_width: impl Into<Px>) -> Self {
        self.scroll_bar_edge_width = StyleValue::Val(Some(edge_width.into()));
        self
    }

    pub fn font_size(mut self, size: impl Into<StyleValue<f32>>) -> Self {
        self.font_size = size.into().map(Some);
        self
    }

    pub fn font_family(mut self, family: impl Into<StyleValue<String>>) -> Self {
        self.font_family = family.into().map(Some);
        self
    }

    pub fn font_weight(mut self, weight: impl Into<StyleValue<Weight>>) -> Self {
        self.font_weight = weight.into().map(Some);
        self
    }

    pub fn font_bold(self) -> Self {
        self.font_weight(Weight::BOLD)
    }

    pub fn font_style(mut self, style: impl Into<StyleValue<FontStyle>>) -> Self {
        self.font_style = style.into().map(Some);
        self
    }

    pub fn cursor_color(mut self, color: impl Into<StyleValue<Color>>) -> Self {
        self.cursor_color = color.into().map(Some);
        self
    }

    pub fn line_height(mut self, normal: f32) -> Self {
        self.line_height = Some(LineHeightValue::Normal(normal)).into();
        self
    }

    pub fn text_ellipsis(self) -> Self {
        self.text_overflow(TextOverflow::Ellipsis)
    }

    pub fn text_clip(self) -> Self {
        self.text_overflow(TextOverflow::Clip)
    }

    pub fn absolute(self) -> Self {
        self.position(Position::Absolute)
    }

    pub fn items_start(self) -> Self {
        self.align_items(Some(AlignItems::FlexStart))
    }

    /// Defines the alignment along the cross axis as Centered
    pub fn items_center(self) -> Self {
        self.align_items(Some(AlignItems::Center))
    }

    pub fn items_end(self) -> Self {
        self.align_items(Some(AlignItems::FlexEnd))
    }

    /// Defines the alignment along the main axis as Centered
    pub fn justify_center(self) -> Self {
        self.justify_content(Some(JustifyContent::Center))
    }

    pub fn justify_end(self) -> Self {
        self.justify_content(Some(JustifyContent::FlexEnd))
    }

    pub fn justify_start(self) -> Self {
        self.justify_content(Some(JustifyContent::FlexStart))
    }

    pub fn justify_between(self) -> Self {
        self.justify_content(Some(JustifyContent::SpaceBetween))
    }

    pub fn hide(self) -> Self {
        self.display(Display::None)
    }

    pub fn flex(self) -> Self {
        self.display(Display::Flex)
    }

    pub fn flex_row(self) -> Self {
        self.flex_direction(FlexDirection::Row)
    }

    pub fn flex_col(self) -> Self {
        self.flex_direction(FlexDirection::Column)
    }

    pub fn z_index(mut self, z_index: i32) -> Self {
        self.z_index = Some(z_index).into();
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

impl ComputedStyle {
    pub fn to_taffy_style(&self) -> TaffyStyle {
        TaffyStyle {
            display: self.display,
            position: self.position,
            size: taffy::prelude::Size {
                width: self.width.into(),
                height: self.height.into(),
            },
            min_size: taffy::prelude::Size {
                width: self.min_width.into(),
                height: self.min_height.into(),
            },
            max_size: taffy::prelude::Size {
                width: self.max_width.into(),
                height: self.max_height.into(),
            },
            flex_direction: self.flex_direction,
            flex_grow: self.flex_grow,
            flex_shrink: self.flex_shrink,
            flex_basis: self.flex_basis.into(),
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
                left: self.padding_left.into(),
                top: self.padding_top.into(),
                right: self.padding_right.into(),
                bottom: self.padding_bottom.into(),
            },
            margin: Rect {
                left: self.margin_left.into(),
                top: self.margin_top.into(),
                right: self.margin_right.into(),
                bottom: self.margin_bottom.into(),
            },
            inset: Rect {
                left: self.inset_left.into(),
                top: self.inset_top.into(),
                right: self.inset_right.into(),
                bottom: self.inset_bottom.into(),
            },
            gap: self.gap,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Style, StyleValue};
    use crate::unit::PxPct;

    #[test]
    fn style_override() {
        let style1 = Style::BASE.padding_left(32.0);
        let style2 = Style::BASE.padding_left(64.0);

        let style = style1.apply(style2);

        assert_eq!(style.padding_left, StyleValue::Val(PxPct::Px(64.0)));

        let style1 = Style::BASE.padding_left(32.0).padding_bottom(45.0);
        let style2 = Style::BASE
            .padding_left(64.0)
            .padding_bottom_sv(StyleValue::Base);

        let style = style1.apply(style2);

        assert_eq!(style.padding_left, StyleValue::Val(PxPct::Px(64.0)));
        assert_eq!(style.padding_bottom, StyleValue::Val(PxPct::Px(45.0)));

        let style1 = Style::BASE.padding_left(32.0).padding_bottom(45.0);
        let style2 = Style::BASE
            .padding_left(64.0)
            .padding_bottom_sv(StyleValue::Unset);

        let style = style1.apply(style2);

        assert_eq!(style.padding_left, StyleValue::Val(PxPct::Px(64.0)));
        assert_eq!(style.padding_bottom, StyleValue::Unset);

        let style1 = Style::BASE.padding_left(32.0).padding_bottom(45.0);
        let style2 = Style::BASE
            .padding_left(64.0)
            .padding_bottom_sv(StyleValue::Unset);

        let style3 = Style::BASE.padding_bottom_sv(StyleValue::Base);

        let style = style1.apply_overriding_styles([style2, style3].into_iter());

        assert_eq!(style.padding_left, StyleValue::Val(PxPct::Px(64.0)));
        assert_eq!(style.padding_bottom, StyleValue::Unset);

        let style1 = Style::BASE.padding_left(32.0).padding_bottom(45.0);
        let style2 = Style::BASE
            .padding_left(64.0)
            .padding_bottom_sv(StyleValue::Unset);
        let style3 = Style::BASE.padding_bottom(100.0);

        let style = style1.apply_overriding_styles([style2, style3].into_iter());

        assert_eq!(style.padding_left, StyleValue::Val(PxPct::Px(64.0)));
        assert_eq!(style.padding_bottom, StyleValue::Val(PxPct::Px(100.0)));
    }
}
