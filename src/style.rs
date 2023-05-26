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
pub use taffy::style::{
    AlignContent, AlignItems, Dimension, Display, FlexDirection, JustifyContent, Position,
};
use taffy::{
    prelude::Rect,
    style::{LengthPercentage, LengthPercentageAuto, Style as TaffyStyle},
    style_helpers::TaffyZero,
};
use vello::peniko::Color;

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
        $($name:ident $($opt:ident)?: $typ:ty = $val:expr),* $(,)?
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
                define_styles!(decl: $name $($opt)?: $typ = $val);
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
    (decl: $name:ident nocb: $typ:ty = $val:expr) => {};
    (decl: $name:ident: $typ:ty = $val:expr) => {
        pub fn $name(mut self, v: impl Into<StyleValue<$typ>>) -> Self {
            self.$name = v.into();
            self
        }
    }
}

define_styles!(
    display: Display = Display::Flex,
    position: Position = Position::Relative,
    width: Dimension = Dimension::Auto,
    height: Dimension = Dimension::Auto,
    min_width: Dimension = Dimension::Auto,
    min_height: Dimension = Dimension::Auto,
    max_width: Dimension = Dimension::Auto,
    max_height: Dimension = Dimension::Auto,
    flex_direction: FlexDirection = FlexDirection::Row,
    flex_grow: f32 = 0.0,
    flex_shrink: f32 = 1.0,
    flex_basis: Dimension = Dimension::Auto,
    justify_content: Option<JustifyContent> = None,
    align_items: Option<AlignItems> = None,
    align_content: Option<AlignContent> = None,
    border_left: f32 = 0.0,
    border_top: f32 = 0.0,
    border_right: f32 = 0.0,
    border_bottom: f32 = 0.0,
    border_radius: f32 = 0.0,
    border_color: Color = Color::BLACK,
    padding_left: LengthPercentage = LengthPercentage::ZERO,
    padding_top: LengthPercentage = LengthPercentage::ZERO,
    padding_right: LengthPercentage = LengthPercentage::ZERO,
    padding_bottom: LengthPercentage = LengthPercentage::ZERO,
    margin_left: LengthPercentageAuto = LengthPercentageAuto::ZERO,
    margin_top: LengthPercentageAuto = LengthPercentageAuto::ZERO,
    margin_right: LengthPercentageAuto = LengthPercentageAuto::ZERO,
    margin_bottom: LengthPercentageAuto = LengthPercentageAuto::ZERO,
    inset_left: LengthPercentageAuto = LengthPercentageAuto::Auto,
    inset_top: LengthPercentageAuto = LengthPercentageAuto::Auto,
    inset_right: LengthPercentageAuto = LengthPercentageAuto::Auto,
    inset_bottom: LengthPercentageAuto = LengthPercentageAuto::Auto,
    cursor nocb: Option<CursorStyle> = None,
    color nocb: Option<Color> = None,
    background nocb: Option<Color> = None,
    font_size nocb: Option<f32> = None,
    font_family nocb: Option<String> = None,
    font_weight nocb: Option<Weight> = None,
    font_style nocb: Option<FontStyle> = None,
    cursor_color nocb: Option<Color> = None,
    text_overflow: TextOverflow = TextOverflow::Wrap,
    line_height nocb: Option<LineHeightValue> = None,
);

impl Style {
    pub fn width_px(self, width: f32) -> Self {
        self.width(Dimension::Points(width))
    }

    pub fn width_pct(self, width: f32) -> Self {
        self.width(Dimension::Percent(width / 100.0))
    }

    pub fn height_px(self, height: f32) -> Self {
        self.height(Dimension::Points(height))
    }

    pub fn height_pct(self, height: f32) -> Self {
        self.height(Dimension::Percent(height / 100.0))
    }

    pub fn size(
        self,
        width: impl Into<StyleValue<Dimension>>,
        height: impl Into<StyleValue<Dimension>>,
    ) -> Self {
        self.width(width).height(height)
    }

    pub fn size_px(self, width: f32, height: f32) -> Self {
        self.width_px(width).height_px(height)
    }

    pub fn size_pct(self, width: f32, height: f32) -> Self {
        self.width_pct(width).height_pct(height)
    }

    pub fn min_width_px(self, min_width: f32) -> Self {
        self.min_width(Dimension::Points(min_width))
    }

    pub fn min_width_pct(self, min_width: f32) -> Self {
        self.min_width(Dimension::Percent(min_width / 100.0))
    }

    pub fn min_height_px(self, min_height: f32) -> Self {
        self.min_height(Dimension::Points(min_height))
    }

    pub fn min_height_pct(self, min_height: f32) -> Self {
        self.min_height(Dimension::Percent(min_height / 100.0))
    }

    pub fn min_size(
        self,
        min_width: impl Into<StyleValue<Dimension>>,
        min_height: impl Into<StyleValue<Dimension>>,
    ) -> Self {
        self.min_width(min_width).min_height(min_height)
    }

    pub fn min_size_px(self, min_width: f32, min_height: f32) -> Self {
        self.min_width_px(min_width).min_height_px(min_height)
    }

    pub fn min_size_pct(self, min_width: f32, min_height: f32) -> Self {
        self.min_width_pct(min_width).min_height_pct(min_height)
    }

    pub fn max_width_px(self, max_width: f32) -> Self {
        self.max_width(Dimension::Points(max_width))
    }

    pub fn max_width_pct(self, max_width: f32) -> Self {
        self.max_width(Dimension::Percent(max_width / 100.0))
    }

    pub fn max_height_px(self, max_height: f32) -> Self {
        self.max_height(Dimension::Points(max_height))
    }

    pub fn max_height_pct(self, max_height: f32) -> Self {
        self.max_height(Dimension::Percent(max_height / 100.0))
    }

    pub fn max_size(
        self,
        max_width: impl Into<StyleValue<Dimension>>,
        max_height: impl Into<StyleValue<Dimension>>,
    ) -> Self {
        self.max_width(max_width).max_height(max_height)
    }

    pub fn max_size_px(self, max_width: f32, max_height: f32) -> Self {
        self.max_width_px(max_width).max_height_px(max_height)
    }

    pub fn max_size_pct(self, max_width: f32, max_height: f32) -> Self {
        self.max_width_pct(max_width).max_height_pct(max_height)
    }

    pub fn border(self, border: f32) -> Self {
        self.border_left(border)
            .border_top(border)
            .border_right(border)
            .border_bottom(border)
    }

    /// Sets `border_left` and `border_right` to `border`
    pub fn border_horiz(self, border: f32) -> Self {
        self.border_left(border).border_right(border)
    }

    /// Sets `border_top` and `border_bottom` to `border`
    pub fn border_vert(self, border: f32) -> Self {
        self.border_top(border).border_bottom(border)
    }

    pub fn padding_left_px(self, padding: f32) -> Self {
        self.padding_left(LengthPercentage::Points(padding))
    }

    pub fn padding_right_px(self, padding: f32) -> Self {
        self.padding_right(LengthPercentage::Points(padding))
    }

    pub fn padding_top_px(self, padding: f32) -> Self {
        self.padding_top(LengthPercentage::Points(padding))
    }

    pub fn padding_bottom_px(self, padding: f32) -> Self {
        self.padding_bottom(LengthPercentage::Points(padding))
    }

    pub fn padding_left_pct(self, padding: f32) -> Self {
        self.padding_left(LengthPercentage::Percent(padding / 100.0))
    }

    pub fn padding_right_pct(self, padding: f32) -> Self {
        self.padding_right(LengthPercentage::Percent(padding / 100.0))
    }

    pub fn padding_top_pct(self, padding: f32) -> Self {
        self.padding_top(LengthPercentage::Percent(padding / 100.0))
    }

    pub fn padding_bottom_pct(self, padding: f32) -> Self {
        self.padding_bottom(LengthPercentage::Percent(padding / 100.0))
    }

    /// Set padding on all directions
    pub fn padding_px(self, padding: f32) -> Self {
        self.padding_left_px(padding)
            .padding_top_px(padding)
            .padding_right_px(padding)
            .padding_bottom_px(padding)
    }

    pub fn padding_pct(self, padding: f32) -> Self {
        self.padding_left_pct(padding)
            .padding_top_pct(padding)
            .padding_right_pct(padding)
            .padding_bottom_pct(padding)
    }

    /// Sets `padding_left` and `padding_right` to `padding`
    pub fn padding_horiz_px(self, padding: f32) -> Self {
        self.padding_left_px(padding).padding_right_px(padding)
    }

    pub fn padding_horiz_pct(self, padding: f32) -> Self {
        self.padding_left_pct(padding).padding_right_pct(padding)
    }

    /// Sets `padding_top` and `padding_bottom` to `padding`
    pub fn padding_vert_px(self, padding: f32) -> Self {
        self.padding_top_px(padding).padding_bottom_px(padding)
    }

    pub fn padding_vert_pct(self, padding: f32) -> Self {
        self.padding_top_pct(padding).padding_bottom_pct(padding)
    }

    pub fn margin_left_px(self, margin: f32) -> Self {
        self.margin_left(LengthPercentageAuto::Points(margin))
    }

    pub fn margin_right_px(self, margin: f32) -> Self {
        self.margin_right(LengthPercentageAuto::Points(margin))
    }

    pub fn margin_top_px(self, margin: f32) -> Self {
        self.margin_top(LengthPercentageAuto::Points(margin))
    }

    pub fn margin_bottom_px(self, margin: f32) -> Self {
        self.margin_bottom(LengthPercentageAuto::Points(margin))
    }

    pub fn margin_left_pct(self, margin: f32) -> Self {
        self.margin_left(LengthPercentageAuto::Percent(margin / 100.0))
    }

    pub fn margin_right_pct(self, margin: f32) -> Self {
        self.margin_right(LengthPercentageAuto::Percent(margin / 100.0))
    }

    pub fn margin_top_pct(self, margin: f32) -> Self {
        self.margin_top(LengthPercentageAuto::Percent(margin / 100.0))
    }

    pub fn margin_bottom_pct(self, margin: f32) -> Self {
        self.margin_bottom(LengthPercentageAuto::Percent(margin / 100.0))
    }

    pub fn margin_px(self, margin: f32) -> Self {
        self.margin_left_px(margin)
            .margin_top_px(margin)
            .margin_right_px(margin)
            .margin_bottom_px(margin)
    }

    pub fn margin_pct(self, margin: f32) -> Self {
        self.margin_left_pct(margin)
            .margin_top_pct(margin)
            .margin_right_pct(margin)
            .margin_bottom_pct(margin)
    }

    /// Sets `margin_left` and `margin_right` to `margin`
    pub fn margin_horiz_px(self, margin: f32) -> Self {
        self.margin_left_px(margin).margin_right_px(margin)
    }

    pub fn margin_horiz_pct(self, margin: f32) -> Self {
        self.margin_left_pct(margin).margin_right_pct(margin)
    }

    /// Sets `margin_top` and `margin_bottom` to `margin`
    pub fn margin_vert_px(self, margin: f32) -> Self {
        self.margin_top_px(margin).margin_bottom_px(margin)
    }

    pub fn margin_vert_pct(self, margin: f32) -> Self {
        self.margin_top_pct(margin).margin_bottom_pct(margin)
    }

    pub fn inset_left_px(self, inset: f32) -> Self {
        self.inset_left(LengthPercentageAuto::Points(inset))
    }

    pub fn inset_right_px(self, inset: f32) -> Self {
        self.inset_right(LengthPercentageAuto::Points(inset))
    }

    pub fn inset_top_px(self, inset: f32) -> Self {
        self.inset_top(LengthPercentageAuto::Points(inset))
    }

    pub fn inset_bottom_px(self, inset: f32) -> Self {
        self.inset_bottom(LengthPercentageAuto::Points(inset))
    }

    pub fn inset_left_pct(self, inset: f32) -> Self {
        self.inset_left(LengthPercentageAuto::Percent(inset / 100.0))
    }

    pub fn inset_right_pct(self, inset: f32) -> Self {
        self.inset_right(LengthPercentageAuto::Percent(inset / 100.0))
    }

    pub fn inset_top_pct(self, inset: f32) -> Self {
        self.inset_top(LengthPercentageAuto::Percent(inset / 100.0))
    }

    pub fn inset_bottom_pct(self, inset: f32) -> Self {
        self.inset_bottom(LengthPercentageAuto::Percent(inset / 100.0))
    }

    pub fn inset_px(self, inset: f32) -> Self {
        self.inset_left_px(inset)
            .inset_top_px(inset)
            .inset_right_px(inset)
            .inset_bottom_px(inset)
    }

    pub fn inset_pct(self, inset: f32) -> Self {
        self.inset_left_pct(inset)
            .inset_top_pct(inset)
            .inset_right_pct(inset)
            .inset_bottom_pct(inset)
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

    pub fn flex_basis_px(self, pt: f32) -> Self {
        self.flex_basis(Dimension::Points(pt))
    }

    pub fn flex_row(self) -> Self {
        self.flex_direction(FlexDirection::Row)
    }

    pub fn flex_col(self) -> Self {
        self.flex_direction(FlexDirection::Column)
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
            justify_content: self.justify_content,
            align_items: self.align_items,
            align_content: self.align_content,
            border: Rect {
                left: LengthPercentage::Points(self.border_left),
                top: LengthPercentage::Points(self.border_top),
                right: LengthPercentage::Points(self.border_right),
                bottom: LengthPercentage::Points(self.border_bottom),
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
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use taffy::style::LengthPercentage;

    use super::{Style, StyleValue};

    #[test]
    fn style_override() {
        let style1 = Style::BASE.padding_left_px(32.0);
        let style2 = Style::BASE.padding_left_px(64.0);

        let style = style1.apply(style2);

        assert_eq!(
            style.padding_left,
            StyleValue::Val(LengthPercentage::Points(64.0))
        );

        let style1 = Style::BASE.padding_left_px(32.0).padding_bottom_px(45.0);
        let style2 = Style::BASE
            .padding_left_px(64.0)
            .padding_bottom(StyleValue::Base);

        let style = style1.apply(style2);

        assert_eq!(
            style.padding_left,
            StyleValue::Val(LengthPercentage::Points(64.0))
        );
        assert_eq!(
            style.padding_bottom,
            StyleValue::Val(LengthPercentage::Points(45.0))
        );

        let style1 = Style::BASE.padding_left_px(32.0).padding_bottom_px(45.0);
        let style2 = Style::BASE
            .padding_left(LengthPercentage::Points(64.0))
            .padding_bottom(StyleValue::Unset);

        let style = style1.apply(style2);

        assert_eq!(
            style.padding_left,
            StyleValue::Val(LengthPercentage::Points(64.0))
        );
        assert_eq!(style.padding_bottom, StyleValue::Unset);

        let style1 = Style::BASE.padding_left_px(32.0).padding_bottom_px(45.0);
        let style2 = Style::BASE
            .padding_left_px(64.0)
            .padding_bottom(StyleValue::Unset);
        let style3 = Style::BASE.padding_bottom(StyleValue::Base);

        let style = style1.apply_overriding_styles([style2, style3].into_iter());

        assert_eq!(
            style.padding_left,
            StyleValue::Val(LengthPercentage::Points(64.0))
        );
        assert_eq!(style.padding_bottom, StyleValue::Unset);

        let style1 = Style::BASE.padding_left_px(32.0).padding_bottom_px(45.0);
        let style2 = Style::BASE
            .padding_left(LengthPercentage::Points(64.0))
            .padding_bottom(StyleValue::Unset);
        let style3 = Style::BASE.padding_bottom(StyleValue::Val(LengthPercentage::Points(100.0)));

        let style = style1.apply_overriding_styles([style2, style3].into_iter());

        assert_eq!(
            style.padding_left,
            StyleValue::Val(LengthPercentage::Points(64.0))
        );
        assert_eq!(
            style.padding_bottom,
            StyleValue::Val(LengthPercentage::Points(100.0))
        );
    }
}
