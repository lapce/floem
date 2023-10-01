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

use crate::{
    animate::{alternating, ease, passes, Blendable, EasingFn, EasingMode},
    unit::{Px, PxPct, PxPctAuto, UnitExt},
};

#[derive(Clone, Copy, Debug)]
pub enum StyleSelector {
    Base,
    Main,
    Hover,
    Focus,
    FocusVisible,
    Disabled,
    Active,
    Dragging,
    Override,
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

macro_rules! define_modifier_fns {
    ($struct_name:ident with: $($name:ident $($opt:ident)?: $typ:ty),* $(,)?) => {
        impl $struct_name {
            $(
                define_modifier_fns!(decl: $name $($opt)?: $typ);
            )*
        }
    };
    ($struct_name:ident nested under $outer_struct:ident $nested_property:ident with: $($name:ident $($opt:ident)?: $typ:ty),* $(,)?) => {
        impl $outer_struct {
            $(
                define_modifier_fns!(decl: $name $($opt)?: $typ, nested under $nested_property);
            )*
        }
    };

    // 'nocb' doesn't add a builder function
    (decl: $name:ident nocb: $typ:ty) => {};
    (decl: $name:ident nocb: $typ:ty, nested under $nested_property:ident) => {};
    (decl: $name:ident: $typ:ty, nested under $nested_property:ident) => {
        pub fn $name(mut self, v: impl Into<$typ>) -> Self {
            self.$nested_property.$name = v.into();
            self
        }
    };
    (decl: $name:ident: $typ:ty) => {
        pub fn $name(mut self, v: impl Into<$typ>) -> Self {
            self.$name = v.into();
            self
        }
    };
    (decl: $name:ident blendable: $typ:ty, nested under $nested_property:ident) => {
        pub fn $name(mut self, v: impl Into<$typ>) -> Self {
            if self.blend_style {
                self.$nested_property.$name = self.$nested_property.$name.blend(v.into(), self.animation_value);
            } else {
                self.$nested_property.$name = v.into();
            }
            self
        }
    };
}

macro_rules! define_styles {
    (
        $struct_name:ident
        $(nested under $outer_struct:ident $nested_property:ident)?
        with:
        $($name:ident $($opt:ident)?: $typ:ty = $val:expr),* $(,)?
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

        define_modifier_fns!($struct_name with:
            $(
                $name $($opt)?: $typ,
            )*
        );
    };
}

use super::*;

pub struct StyleAnimCtx {
    pub style: Style,
    pub blend_style: bool,
    pub animation_value: f64,
}

impl StyleAnimCtx {
    pub fn done(style: Style) -> StyleAnimCtx {
        StyleAnimCtx {
            style,
            blend_style: false,
            animation_value: 1.0,
        }
    }
}

define_styles!(
    BoxShadow with:
    h_offset: Px = Px(0.0),
    v_offset: Px = Px(0.0),
    blur_radius: Px = Px(0.0),
    spread: Px = Px(0.0),
    color: Color = Color::BLACK,
);

pub fn box_shadow() -> BoxShadow {
    BoxShadow::default()
}

define_styles!(
    Style with:
    display: Display = Display::Flex,
    position: Position = Position::Relative,
    width: PxPctAuto = PxPctAuto::Auto,
    height: PxPctAuto = PxPctAuto::Auto,
    min_width: PxPctAuto = PxPctAuto::Auto,
    min_height: PxPctAuto = PxPctAuto::Auto,
    max_width: PxPctAuto = PxPctAuto::Auto,
    max_height: PxPctAuto = PxPctAuto::Auto,
    flex_direction: FlexDirection = FlexDirection::Row,
    flex_wrap: FlexWrap = FlexWrap::NoWrap,
    flex_grow: f32 = 0.0,
    flex_shrink: f32 = 1.0,
    flex_basis: PxPctAuto = PxPctAuto::Auto,
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
    padding_left: PxPct = PxPct::Px(0.0),
    padding_top: PxPct = PxPct::Px(0.0),
    padding_right: PxPct = PxPct::Px(0.0),
    padding_bottom: PxPct = PxPct::Px(0.0),
    margin_left: PxPctAuto = PxPctAuto::Px(0.0),
    margin_top: PxPctAuto = PxPctAuto::Px(0.0),
    margin_right: PxPctAuto = PxPctAuto::Px(0.0),
    margin_bottom: PxPctAuto = PxPctAuto::Px(0.0),
    inset_left: PxPctAuto = PxPctAuto::Auto,
    inset_top: PxPctAuto = PxPctAuto::Auto,
    inset_right: PxPctAuto = PxPctAuto::Auto,
    inset_bottom: PxPctAuto = PxPctAuto::Auto,
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

define_modifier_fns!(
    Style nested under StyleAnimCtx style with:
    display: Display,
    position: Position,
    width blendable: PxPctAuto,
    height blendable: PxPctAuto,
    min_width blendable: PxPctAuto,
    min_height blendable: PxPctAuto,
    max_width blendable: PxPctAuto,
    max_height blendable: PxPctAuto,
    flex_direction: FlexDirection,
    flex_wrap: FlexWrap,
    flex_grow: f32,
    flex_shrink: f32,
    flex_basis: PxPctAuto,
    justify_content: Option<JustifyContent>,
    justify_self: Option<AlignItems>,
    align_items: Option<AlignItems>,
    align_content: Option<AlignContent>,
    align_self: Option<AlignItems>,
    border_left blendable: Px,
    border_top blendable: Px,
    border_right blendable: Px,
    border_bottom blendable: Px,
    border_radius blendable: Px,
    outline_color blendable: Color,
    outline blendable: Px,
    border_color blendable: Color,
    padding_left blendable: PxPct,
    padding_top blendable: PxPct,
    padding_right blendable: PxPct,
    padding_bottom blendable: PxPct,
    margin_left blendable: PxPctAuto,
    margin_top blendable: PxPctAuto,
    margin_right blendable: PxPctAuto,
    margin_bottom blendable: PxPctAuto,
    inset_left blendable: PxPctAuto,
    inset_top blendable: PxPctAuto,
    inset_right blendable: PxPctAuto,
    inset_bottom blendable: PxPctAuto,
    z_index nocb: Option<i32>,
    cursor nocb: Option<CursorStyle>,
    color nocb: Option<Color>,
    background nocb: Option<Color>,
    box_shadows: Vec<BoxShadow>,
    scroll_bar_color nocb: Option<Color>,
    scroll_bar_rounded nocb: Option<bool>,
    scroll_bar_thickness nocb: Option<Px>,
    scroll_bar_edge_width nocb: Option<Px>,
    font_size nocb: Option<f32>,
    font_family nocb: Option<String>,
    font_weight nocb: Option<Weight>,
    font_style nocb: Option<FontStyle>,
    cursor_color nocb: Option<Color>,
    text_overflow: TextOverflow,
    line_height nocb: Option<LineHeightValue>,
    aspect_ratio: Option<f32>,
    gap: Size<LengthPercentage>,
);

impl StyleAnimCtx {
    pub fn blend(mut self) -> Self {
        self.blend_style = true;
        self
    }

    pub fn passes(mut self, count: u16) -> Self {
        self.animation_value = passes(count, self.animation_value);
        self
    }

    pub fn alternating_anim(mut self) -> Self {
        self.animation_value = alternating(self.animation_value);
        self
    }

    pub fn ease(mut self, mode: EasingMode, func: EasingFn) -> Self {
        self.animation_value = ease(self.animation_value, mode, func);
        self
    }

    pub fn rescale_anim(mut self, from: f64, to: f64) -> Self {
        self.animation_value = (self.animation_value - from) / (to - from);
        self
    }

    pub fn animation_value(mut self, v: f64) -> Self {
        self.animation_value = v;
        self
    }

    pub fn clamp(mut self, min: f64, max: f64) -> Self {
        self.animation_value = self.animation_value.clamp(min, max);
        self
    }

    pub fn width_pct(self, width: f64) -> Self {
        self.width(width.pct())
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

    pub fn cursor(mut self, cursor: CursorStyle) -> Self {
        self.style.cursor = Some(cursor);
        self
    }

    pub fn color(mut self, color: impl Into<Color>) -> Self {
        if self.blend_style {
            let current = self.style.color.unwrap_or(Color::BLACK);
            self.style.color = Some(current.blend(color.into(), self.animation_value));
        } else {
            self.style.color = Some(color.into());
        }
        self
    }

    pub fn background(mut self, color: impl Into<Color>) -> Self {
        if self.blend_style {
            let current = self.style.background.unwrap_or(Color::WHITE);
            self.style.background = Some(current.blend(color.into(), self.animation_value));
        } else {
            self.style.background = Some(color.into());
        }
        self
    }

    pub fn box_shadow(self, box_shadow: BoxShadow) -> Self {
        self.box_shadows(vec![box_shadow])
    }

    pub fn scroll_bar_color(mut self, color: impl Into<Color>) -> Self {
        if self.blend_style {
            // TODO: what is the default color?
            let current = self.style.scroll_bar_color.unwrap_or(Color::WHITE);
            self.style.scroll_bar_color = Some(current.blend(color.into(), self.animation_value));
        } else {
            self.style.scroll_bar_color = Some(color.into());
        }
        self
    }

    pub fn scroll_bar_rounded(mut self, rounded: bool) -> Self {
        self.style.scroll_bar_rounded = Some(rounded);
        self
    }

    pub fn scroll_bar_thickness(mut self, thickness: impl Into<Px>) -> Self {
        if self.blend_style {
            let current = self.style.scroll_bar_thickness.unwrap_or(0.px());
            self.style.scroll_bar_thickness =
                Some(current.blend(thickness.into(), self.animation_value));
        } else {
            self.style.scroll_bar_thickness = Some(thickness.into());
        }
        self
    }

    pub fn scroll_bar_edge_width(mut self, edge_width: impl Into<Px>) -> Self {
        if self.blend_style {
            let current = self.style.scroll_bar_thickness.unwrap_or(0.px());
            self.style.scroll_bar_thickness =
                Some(current.blend(edge_width.into(), self.animation_value));
        } else {
            self.style.scroll_bar_thickness = Some(edge_width.into());
        }
        self
    }

    pub fn font_size(mut self, size: impl Into<f32>) -> Self {
        if self.blend_style {
            // TODO: does not quite fit, something is missing for the default font size
            let current = self.style.font_size.unwrap_or(views::DEFAULT_FONT_SIZE);
            self.style.font_size = Some(current.blend(size.into(), self.animation_value));
        } else {
            self.style.font_size = Some(size.into());
        }
        self
    }

    pub fn font_family(mut self, family: impl Into<String>) -> Self {
        self.style.font_family = Some(family.into());
        self
    }

    pub fn font_weight(mut self, weight: impl Into<Weight>) -> Self {
        self.style.font_weight = Some(weight.into());
        self
    }

    pub fn font_bold(self) -> Self {
        self.font_weight(Weight::BOLD)
    }

    pub fn font_style(mut self, style: impl Into<FontStyle>) -> Self {
        self.style.font_style = Some(style.into());
        self
    }

    pub fn cursor_color(mut self, color: impl Into<Color>) -> Self {
        self.style.cursor_color = Some(color.into());
        self
    }

    pub fn line_height(mut self, normal: f32) -> Self {
        self.style.line_height = Some(LineHeightValue::Normal(normal));
        self
    }

    pub fn text_ellipsis(mut self) -> Self {
        self.style.text_overflow = TextOverflow::Ellipsis;
        self
    }

    pub fn text_clip(mut self) -> Self {
        self.style.text_overflow = TextOverflow::Clip;
        self
    }

    pub fn absolute(mut self) -> Self {
        self.style.position = Position::Absolute;
        self
    }

    pub fn items_start(mut self) -> Self {
        self.style.align_items = Some(AlignItems::FlexStart);
        self
    }

    /// Defines the alignment along the cross axis as Centered
    pub fn items_center(mut self) -> Self {
        self.style.align_items = Some(AlignItems::Center);
        self
    }

    pub fn items_end(mut self) -> Self {
        self.style.align_items = Some(AlignItems::FlexEnd);
        self
    }

    /// Defines the alignment along the main axis as Centered
    pub fn justify_center(mut self) -> Self {
        self.style.justify_content = Some(JustifyContent::Center);
        self
    }

    pub fn justify_end(mut self) -> Self {
        self.style.justify_content = Some(JustifyContent::FlexEnd);
        self
    }

    pub fn justify_start(mut self) -> Self {
        self.style.justify_content = Some(JustifyContent::FlexStart);
        self
    }

    pub fn justify_between(mut self) -> Self {
        self.style.justify_content = Some(JustifyContent::SpaceBetween);
        self
    }

    pub fn hide(mut self) -> Self {
        self.style.display = Display::None;
        self
    }

    pub fn flex(self) -> Self {
        self.display(Display::Flex)
    }

    pub fn flex_row(self) -> Self {
        self.flex_direction(FlexDirection::Row)
    }

    pub fn flex_col(mut self) -> Self {
        self.style.flex_direction = FlexDirection::Column;
        self
    }

    pub fn z_index(mut self, z_index: i32) -> Self {
        self.style.z_index = Some(z_index);
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
    use super::Style;
    use crate::unit::PxPct;

    #[test]
    fn style_override() {
        let style_fn1 = |s: Style| s.padding_left(32.0);
        let style_fn2 = |s: Style| s.padding_left(64.0);

        let style = style_fn2(style_fn1(Style::default()));

        assert_eq!(style.padding_left, PxPct::Px(64.0));

        let style_fn1 = |s: Style| s.padding_left(32.0).padding_bottom(45.0);
        let style_fn2 = |s: Style| s.padding_left(64.0);

        let style = style_fn2(style_fn1(Style::default()));

        assert_eq!(style.padding_left, PxPct::Px(64.0));
        assert_eq!(style.padding_bottom, PxPct::Px(45.0));
    }
}
