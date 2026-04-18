//! Built-in style property declarations.
//!
//! This module defines every built-in prop type (e.g. [`Width`], [`Height`],
//! [`FontSize`]) and generates the convenience setters on [`Style`] /
//! [`ExprStyle`] and getters on [`BuiltinStyle`] via
//! [`define_builtin_props!`].
//!
//! The macro drives three rounds of expansion per prop:
//! - `prop!(..)` to install the zero-sized marker type and its `StyleProp` impl
//! - `decl`/`expr_decl` to emit fluent setters (respecting the `nocb` flag)
//! - `unset`/`expr_unset` plus optional `transition_*` (when the `tr` flag is
//!   present)
//!
//! Most of the ambient layout/positioning vocabulary is re-exported from
//! `taffy::style` so downstream users can keep writing
//! `floem::style::FlexDirection::Row` after the extraction.

use parley::{FontStyle as FontStyleProp, FontWeight as FontWeightProp};
use peniko::color::palette;
use peniko::kurbo::{Affine, Stroke};
use peniko::{Brush, Color};
use smallvec::SmallVec;
use taffy::GridTemplateComponent;

pub use taffy::style::{
    AlignContent, AlignItems, BoxSizing, Dimension, Display, FlexDirection, FlexWrap,
    JustifyContent, JustifyItems, Position,
};
use taffy::{
    geometry::MinMax,
    prelude::{GridPlacement, Line},
    style::{MaxTrackSizingFunction, MinTrackSizingFunction, Overflow},
};

use parley::Alignment;

use crate::components::BoxShadow;
use crate::prop;
use crate::style::{BuiltinStyle, ExprStyle, Style};
use crate::transition::Transition;
use crate::unit::{AnchorAbout, Angle, Length, LengthAuto, LineHeightValue, Pct, Pt};
use crate::values::{CursorStyle, Focus, NoWrapOverflow, ObjectFit, ObjectPosition, PointerEvents, TextOverflow};

/// Defines built-in style properties with optional builder methods.
///
/// Properties can be marked with flags in braces:
/// - `nocb` (no callback/no chain builder) - no fluent builder method generated
/// - `tr` (transition) - generates a `transition_property_name()` method
///
/// For `Option<T>` properties, specify the inner type in brackets after the full type:
///
/// ```text
/// Color color { tr }: Option<Color> [Color] { inherited } = None,
/// ```
///
/// This generates a setter that accepts `impl Into<Color>` and wraps in `Some`,
/// rather than the confusing `impl Into<Option<Color>>`. Use `unset_*()` to clear.
macro_rules! define_builtin_props {
    (
        $(
            $(#[$meta:meta])*
            $type_name:ident $name:ident $({ $($flags:ident),* })? :
            $typ:ty $( [$inner:ty] )? { $($options:tt)* } = $val:expr
        ),*
        $(,)?
    ) => {
        $(
            prop!($(#[$meta])* pub $type_name: $typ { $($options)* } = $val);
        )*
        impl Style {
            $(
                define_builtin_props!(decl: $(#[$meta])* $type_name $name $({ $($flags),* })? : $typ $( [$inner] )? = $val);
            )*
            $(
                define_builtin_props!(unset: $(#[$meta])* $type_name $name);
            )*
            $(
                define_builtin_props!(transition: $(#[$meta])* $type_name $name $({ $($flags),* })?);
            )*
        }
        impl BuiltinStyle<'_> {
            $(
                $(#[$meta])*
                pub fn $name(&self) -> $typ {
                    self.style.get($type_name)
                }
            )*
        }
        impl ExprStyle {
            $(
                define_builtin_props!(expr_decl: $(#[$meta])* $type_name $name $({ $($flags),* })? : $typ $( [$inner] )? = $val);
            )*
            $(
                define_builtin_props!(expr_unset: $(#[$meta])* $type_name $name);
            )*
        }
    };

    // Built-in setters for `Option<T> [T]` take `Into<T>` and wrap in `Some`.
    (decl: $(#[$meta:meta])* $type_name:ident $name:ident { $($flags:ident),* } : $typ:ty [$inner:ty] = $val:expr) => {
        define_builtin_props!(@opt_check_nocb $(#[$meta])* $type_name $name [$($flags)*]: $inner);
    };
    (decl: $(#[$meta:meta])* $type_name:ident $name:ident : $typ:ty [$inner:ty] = $val:expr) => {
        $(#[$meta])*
        pub fn $name(self, v: impl Into<$inner>) -> Self {
            self.set($type_name, Some(v.into()))
        }
    };
    (decl: $(#[$meta:meta])* $type_name:ident $name:ident { $($flags:ident),* } : $typ:ty = $val:expr) => {
        define_builtin_props!(@check_nocb $(#[$meta])* $type_name $name [$($flags)*]: $typ);
    };
    (decl: $(#[$meta:meta])* $type_name:ident $name:ident : $typ:ty = $val:expr) => {
        $(#[$meta])*
        pub fn $name(self, v: impl Into<$typ>) -> Self {
            self.set($type_name, v.into())
        }
    };

    (expr_decl: $(#[$meta:meta])* $type_name:ident $name:ident { $($flags:ident),* } : $typ:ty [$inner:ty] = $val:expr) => {
        define_builtin_props!(@opt_check_nocb_expr $(#[$meta])* $type_name $name [$($flags)*]: $inner);
    };
    (expr_decl: $(#[$meta:meta])* $type_name:ident $name:ident : $typ:ty [$inner:ty] = $val:expr) => {
        $(#[$meta])*
        pub fn $name<T>(self, v: $crate::ContextValue<T>) -> Self
        where
            T: Into<$inner> + 'static,
        {
            self.set($type_name, v.map(|x| Some(x.into())))
        }
    };
    (expr_decl: $(#[$meta:meta])* $type_name:ident $name:ident { $($flags:ident),* } : $typ:ty = $val:expr) => {
        define_builtin_props!(@check_nocb_expr $(#[$meta])* $type_name $name [$($flags)*]: $typ);
    };
    (expr_decl: $(#[$meta:meta])* $type_name:ident $name:ident : $typ:ty = $val:expr) => {
        $(#[$meta])*
        pub fn $name<T>(self, v: $crate::ContextValue<T>) -> Self
        where
            T: Into<$typ> + 'static,
        {
            self.set($type_name, v.map(Into::into))
        }
    };

    (@opt_check_nocb $(#[$meta:meta])* $type_name:ident $name:ident [nocb $($rest:ident)*]: $inner:ty) => {};
    (@opt_check_nocb $(#[$meta:meta])* $type_name:ident $name:ident [$first:ident $($rest:ident)*]: $inner:ty) => {
        define_builtin_props!(@opt_check_nocb $(#[$meta])* $type_name $name [$($rest)*]: $inner);
    };
    (@opt_check_nocb $(#[$meta:meta])* $type_name:ident $name:ident []: $inner:ty) => {
        $(#[$meta])*
        pub fn $name(self, v: impl Into<$inner>) -> Self {
            self.set($type_name, Some(v.into()))
        }
    };

    (@opt_check_nocb_expr $(#[$meta:meta])* $type_name:ident $name:ident [nocb $($rest:ident)*]: $inner:ty) => {};
    (@opt_check_nocb_expr $(#[$meta:meta])* $type_name:ident $name:ident [$first:ident $($rest:ident)*]: $inner:ty) => {
        define_builtin_props!(@opt_check_nocb_expr $(#[$meta])* $type_name $name [$($rest)*]: $inner);
    };
    (@opt_check_nocb_expr $(#[$meta:meta])* $type_name:ident $name:ident []: $inner:ty) => {
        $(#[$meta])*
        pub fn $name<T>(self, v: $crate::ContextValue<T>) -> Self
        where
            T: Into<$inner> + 'static,
        {
            self.set($type_name, v.map(|x| Some(x.into())))
        }
    };

    // -------------------------------------------------------------------------
    // @check_nocb — plain (non-Option) setter, respects nocb flag
    // -------------------------------------------------------------------------

    (@check_nocb $(#[$meta:meta])* $type_name:ident $name:ident [nocb $($rest:ident)*]: $typ:ty) => {};
    (@check_nocb $(#[$meta:meta])* $type_name:ident $name:ident [$first:ident $($rest:ident)*]: $typ:ty) => {
        define_builtin_props!(@check_nocb $(#[$meta])* $type_name $name [$($rest)*]: $typ);
    };
    (@check_nocb $(#[$meta:meta])* $type_name:ident $name:ident []: $typ:ty) => {
        $(#[$meta])*
        pub fn $name(self, v: impl Into<$typ>) -> Self {
            self.set($type_name, v.into())
        }
    };

    (@check_nocb_expr $(#[$meta:meta])* $type_name:ident $name:ident [nocb $($rest:ident)*]: $typ:ty) => {};
    (@check_nocb_expr $(#[$meta:meta])* $type_name:ident $name:ident [$first:ident $($rest:ident)*]: $typ:ty) => {
        define_builtin_props!(@check_nocb_expr $(#[$meta])* $type_name $name [$($rest)*]: $typ);
    };
    (@check_nocb_expr $(#[$meta:meta])* $type_name:ident $name:ident []: $typ:ty) => {
        $(#[$meta])*
        pub fn $name<T>(self, v: $crate::ContextValue<T>) -> Self
        where
            T: Into<$typ> + 'static,
        {
            self.set($type_name, v.map(Into::into))
        }
    };

    // -------------------------------------------------------------------------
    // unset — generated for all properties
    // -------------------------------------------------------------------------

    (unset: $(#[$meta:meta])* $type_name:ident $name:ident) => {
        paste::paste! {
            #[doc = "Unsets the `" $name "` property."]
            pub fn [<unset_ $name>](self) -> Self {
                self.set_style_value($type_name, $crate::StyleValue::Unset)
            }
        }
    };

    (expr_unset: $(#[$meta:meta])* $type_name:ident $name:ident) => {
        paste::paste! {
            #[doc = "Unsets the `" $name "` property."]
            pub fn [<unset_ $name>](self) -> Self {
                self.set($type_name, $crate::StyleValue::Unset)
            }
        }
    };

    // -------------------------------------------------------------------------
    // transition — generated when `tr` flag is present
    // -------------------------------------------------------------------------

    // With flags — check for tr
    (transition: $(#[$meta:meta])* $type_name:ident $name:ident { $($flags:ident),* }) => {
        define_builtin_props!(@check_tr $(#[$meta])* $type_name $name [$($flags)*]);
    };
    // Without flags — never generate
    (transition: $(#[$meta:meta])* $type_name:ident $name:ident) => {};

    (@check_tr $(#[$meta:meta])* $type_name:ident $name:ident [tr $($rest:ident)*]) => {
        paste::paste! {
            #[doc = "Sets a transition for the `" $name "` property."]
            $(#[$meta])*
            pub fn [<transition_ $name>](self, transition: impl Into<Transition>) -> Self {
                self.transition($type_name, transition.into())
            }
        }
    };
    (@check_tr $(#[$meta:meta])* $type_name:ident $name:ident [$first:ident $($rest:ident)*]) => {
        define_builtin_props!(@check_tr $(#[$meta])* $type_name $name [$($rest)*]);
    };
    (@check_tr $(#[$meta:meta])* $type_name:ident $name:ident []) => {};
}

define_builtin_props!(
    /// Controls the display type of the view.
    ///
    /// This determines how the view participates in layout.
    DisplayProp display {}: Display {} = Display::Flex,

    /// Sets the positioning scheme for the view.
    ///
    /// This affects how the view is positioned relative to its normal position in the document flow.
    PositionProp position {}: Position {} = Position::Relative,

    /// Enables fixed positioning relative to the viewport.
    ///
    /// When true, the view is positioned relative to the window viewport rather than
    /// its parent. This is similar to CSS `position: fixed`. The view will:
    /// - Use `inset` properties relative to the viewport
    /// - Have percentage sizes relative to the viewport
    /// - Be painted above all other content (like overlays)
    ///
    /// Note: This works in conjunction with `position: absolute` internally.
    IsFixed is_fixed {}: bool {} = false,

    /// Sets the width of the view.
    ///
    /// Can be specified in pixels, percentages, or auto.
    Width width {tr}: LengthAuto {} = LengthAuto::Auto,

    /// Sets the height of the view.
    ///
    /// Can be specified in pixels, percentages, or auto.
    Height height {tr}: LengthAuto {} = LengthAuto::Auto,

    /// Sets the minimum width of the view.
    ///
    /// The view will not shrink below this width.
    MinWidth min_width {tr}: LengthAuto {} = LengthAuto::Auto,

    /// Sets the minimum height of the view.
    ///
    /// The view will not shrink below this height.
    MinHeight min_height {tr}: LengthAuto {} = LengthAuto::Auto,

    /// Sets the maximum width of the view.
    ///
    /// The view will not grow beyond this width.
    MaxWidth max_width {tr}: LengthAuto {} = LengthAuto::Auto,

    /// Sets the maximum height of the view.
    ///
    /// The view will not grow beyond this height.
    MaxHeight max_height {tr}: LengthAuto {} = LengthAuto::Auto,

    /// Sets the direction of the main axis for flex items.
    ///
    /// Determines whether flex items are laid out in rows or columns.
    FlexDirectionProp flex_direction {}: FlexDirection {} = FlexDirection::Row,

    /// Controls whether flex items wrap to new lines.
    ///
    /// When enabled, items that don't fit will wrap to the next line.
    FlexWrapProp flex_wrap {}: FlexWrap {} = FlexWrap::NoWrap,

    /// Sets the flex grow factor for the flex item.
    ///
    /// Determines how much the item should grow relative to other items.
    FlexGrow flex_grow {}: f32 {} = 0.0,

    /// Sets the flex shrink factor for the flex item.
    ///
    /// Determines how much the item should shrink relative to other items.
    FlexShrink flex_shrink {}: f32 {} = 1.0,

    /// Sets the initial main size of a flex item.
    ///
    /// This is the size of the item before free space is distributed.
    FlexBasis flex_basis {tr}: LengthAuto {} = LengthAuto::Auto,

    /// Controls alignment of flex items along the main axis.
    ///
    /// Determines how extra space is distributed between and around items.
    JustifyContentProp justify_content {}: Option<JustifyContent> [JustifyContent] {} = None,

    /// Controls default alignment of grid items along the inline axis.
    ///
    /// Sets the default justify-self value for all items in the container.
    JustifyItemsProp justify_items {}: Option<JustifyItems> [JustifyItems] {} = None,

    /// Controls how the total width and height are calculated.
    ///
    /// Determines whether borders and padding are included in the view's size.
    BoxSizingProp box_sizing {}: Option<BoxSizing> [BoxSizing] {} = None,

    /// Controls individual alignment along the inline axis.
    ///
    /// Overrides the container's justify-items value for this specific item.
    JustifySelf justify_self {}: Option<AlignItems> [AlignItems] {} = None,

    /// Controls alignment of flex items along the cross axis.
    ///
    /// Determines how items are aligned when they don't fill the container's cross axis.
    AlignItemsProp align_items {}: Option<AlignItems> [AlignItems] {} = None,

    /// Controls alignment of wrapped flex lines.
    ///
    /// Only has an effect when flex-wrap is enabled and there are multiple lines.
    AlignContentProp align_content {}: Option<AlignContent> [AlignContent] {} = None,

    /// Defines the line names and track sizing functions of the grid rows.
    ///
    /// Specifies the size and names of the rows in a grid layout.
    GridTemplateRows grid_template_rows {}: Vec<GridTemplateComponent<String>> {} = Vec::new(),

    /// Defines the line names and track sizing functions of the grid columns.
    ///
    /// Specifies the size and names of the columns in a grid layout.
    GridTemplateColumns grid_template_columns {}: Vec<GridTemplateComponent<String>> {} = Vec::new(),

    /// Specifies the size of implicitly-created grid rows.
    ///
    /// Sets the default size for rows that are created automatically.
    GridAutoRows grid_auto_rows {}: Vec<MinMax<MinTrackSizingFunction, MaxTrackSizingFunction>> {} = Vec::new(),

    /// Specifies the size of implicitly-created grid columns.
    ///
    /// Sets the default size for columns that are created automatically.
    GridAutoColumns grid_auto_columns {}: Vec<MinMax<MinTrackSizingFunction, MaxTrackSizingFunction>> {} = Vec::new(),

    /// Controls how auto-placed items get flowed into the grid.
    ///
    /// Determines the direction that grid items are placed when not explicitly positioned.
    GridAutoFlow grid_auto_flow {}: taffy::GridAutoFlow {} = taffy::GridAutoFlow::Row,

    /// Specifies a grid item's location within the grid row.
    ///
    /// Determines which grid rows the item spans.
    GridRow grid_row {}: Line<GridPlacement> {} = Line::default(),

    /// Specifies a grid item's location within the grid column.
    ///
    /// Determines which grid columns the item spans.
    GridColumn grid_column {}: Line<GridPlacement> {} = Line::default(),

    /// Controls individual alignment along the cross axis.
    ///
    /// Overrides the container's align-items value for this specific item.
    AlignSelf align_self {}: Option<AlignItems> [AlignItems] {} = None,

    /// Sets the color of the view's outline.
    ///
    /// The outline is drawn outside the border and doesn't affect layout.
    OutlineColor outline_color {tr}: Brush {} = Brush::Solid(palette::css::TRANSPARENT),

    /// Sets the outline stroke properties.
    ///
    /// Defines the width, style, and other properties of the outline.
    Outline outline {nocb, tr}: Stroke {} = Stroke::new(0.),

    /// Controls the progress/completion of the outline animation.
    ///
    /// Useful for creating animated outline effects.
    OutlineProgress outline_progress {tr}: Pct {} = Pct(100.),

    /// Controls the progress/completion of the border animation.
    ///
    /// Useful for creating animated border effects.
    BorderProgress border_progress {tr}: Pct {} = Pct(100.),

    /// Sets the left border.
    BorderLeft border_left {nocb, tr}: Stroke {} = Stroke::new(0.),
    /// Sets the top border.
    BorderTop border_top {nocb, tr}: Stroke {} = Stroke::new(0.),
    /// Sets the right border.
    BorderRight border_right {nocb, tr}: Stroke {} = Stroke::new(0.),
    /// Sets the bottom border.
    BorderBottom border_bottom {nocb, tr}: Stroke {} = Stroke::new(0.),

    /// Sets the left border color.
    BorderLeftColor border_left_color { tr }: Option<Brush> [Brush] {} = None,
    /// Sets the top border color.
    BorderTopColor border_top_color {  tr }: Option<Brush> [Brush] {} = None,
    /// Sets the right border color.
    BorderRightColor border_right_color { tr }: Option<Brush> [Brush] {} = None,
    /// Sets the bottom border color.
    BorderBottomColor border_bottom_color { tr }: Option<Brush> [Brush] {} = None,

    /// Sets the top-left border radius.
    BorderTopLeftRadius border_top_left_radius { tr }: Length {} = Length::Pt(0.),
    /// Sets the top-right border radius.
    BorderTopRightRadius border_top_right_radius { tr }: Length {} = Length::Pt(0.),
    /// Sets the bottom-left border radius.
    BorderBottomLeftRadius border_bottom_left_radius { tr }: Length {} = Length::Pt(0.),
    /// Sets the bottom-right border radius.
    BorderBottomRightRadius border_bottom_right_radius { tr }: Length {} = Length::Pt(0.),

    /// Sets the left padding.
    PaddingLeft padding_left { tr }: Length {} = Length::Pt(0.),
    /// Sets the top padding.
    PaddingTop padding_top { tr }: Length {} = Length::Pt(0.),
    /// Sets the right padding.
    PaddingRight padding_right { tr }: Length {} = Length::Pt(0.),
    /// Sets the bottom padding.
    PaddingBottom padding_bottom { tr }: Length {} = Length::Pt(0.),

    /// Sets the left margin.
    MarginLeft margin_left { tr }: LengthAuto {} = LengthAuto::Pt(0.),
    /// Sets the top margin.
    MarginTop margin_top { tr }: LengthAuto {} = LengthAuto::Pt(0.),
    /// Sets the right margin.
    MarginRight margin_right { tr }: LengthAuto {} = LengthAuto::Pt(0.),
    /// Sets the bottom margin.
    MarginBottom margin_bottom { tr }: LengthAuto {} = LengthAuto::Pt(0.),

    /// Sets the left offset for positioned views.
    InsetLeft inset_left {tr}: LengthAuto {} = LengthAuto::Auto,

    /// Sets the top offset for positioned views.
    InsetTop inset_top {tr}: LengthAuto {} = LengthAuto::Auto,

    /// Sets the right offset for positioned views.
    InsetRight inset_right {tr}: LengthAuto {} = LengthAuto::Auto,

    /// Sets the bottom offset for positioned views.
    InsetBottom inset_bottom {tr}: LengthAuto {} = LengthAuto::Auto,

    /// Controls whether the view can be the target of mouse events.
    ///
    /// When disabled, mouse events pass through to views behind.
    PointerEventsProp pointer_events {}: Option<PointerEvents> [PointerEvents] { inherited } = None,

    /// Controls the stack order of positioned views.
    ///
    /// This is not a global z-index and will only be used as an override to the sorted order of sibling elements.
    /// If you want a view positioned above others, use an overlay.
    ///
    /// Higher values appear in front of lower values.
    ZIndex z_index {  tr }: Option<i32> [i32] {} = None,

    /// Sets the cursor style when hovering over the view.
    ///
    /// Changes the appearance of the mouse cursor.
    Cursor cursor { }: Option<CursorStyle> [CursorStyle] {} = None,

    /// Sets the text color.
    ///
    /// This property is inherited by child views.
    TextColor color { tr }: Option<Color> [Color] { inherited } = None,

    /// Sets the background color or image.
    ///
    /// Can be a solid color, gradient, or image.
    Background background { tr }: Option<Brush> [Brush] {} = None,

    /// Sets the foreground color or pattern.
    ///
    /// Used for drawing content like icons or shapes.
    Foreground foreground { tr }: Option<Brush> [Brush] {} = None,

    /// Adds one or more drop shadows to the view.
    ///
    /// Can create depth and visual separation effects.
    BoxShadowProp box_shadow {  tr }: SmallVec<[BoxShadow; 3]> {} = SmallVec::new(),

    /// Sets the font size for text content.
    ///
    /// This property is inherited by child views.
    FontSize font_size { nocb, tr }: f64 { inherited } = 14.,

    /// Sets the font family for text content.
    ///
    /// This property is inherited by child views.
    FontFamily font_family { }: Option<String> [String] { inherited } = None,

    /// Sets the font weight (boldness) for text content.
    ///
    /// This property is inherited by child views.
    FontWeight font_weight { }: Option<FontWeightProp> [FontWeightProp] { inherited } = None,

    /// Sets the font style (italic, normal) for text content.
    ///
    /// This property is inherited by child views.
    FontStyle font_style { }: Option<FontStyleProp> [FontStyleProp] { inherited } = None,

    /// Sets the color of the text cursor.
    ///
    /// Visible when text input views have focus.
    CursorColor cursor_color { tr }: Brush {} = Brush::Solid(palette::css::BLACK.with_alpha(0.3)),

    /// Sets the corner radius of text selections.
    ///
    /// Controls how rounded the corners of selected text appear.
    SelectionCornerRadius selection_corer_radius { nocb, tr }: f64 {} = 1.,

    /// Controls whether the view's text can be selected.
    ///
    /// This property is inherited by child views.
    // TODO: rename this TextSelectable
    Selectable selectable {}: bool { inherited } = true,

    /// Controls how overflowed text content is handled.
    ///
    /// Determines whether text wraps or gets clipped.
    TextOverflowProp text_overflow {}: TextOverflow { inherited } = TextOverflow::NoWrap(NoWrapOverflow::Clip),

    /// Sets text alignment within the view.
    ///
    /// Controls horizontal alignment of text content.
    TextAlignProp text_align {}: Option<Alignment> [Alignment] {} = None,

    /// Sets the line height for text content.
    ///
    /// This property is inherited by child views.
    LineHeight line_height { tr }: LineHeightValue { inherited } = LineHeightValue::Normal(1.),

    /// Sets the preferred aspect ratio for the view.
    ///
    /// Maintains width-to-height proportions during layout.
    AspectRatio aspect_ratio {tr}: Option<f32> [f32] {} = None,

    /// Controls how replaced content (like images) should be resized to fit its container.
    ObjectFitProp object_fit {}: ObjectFit {} = ObjectFit::Fill,

    /// Controls where replaced content is anchored inside its content box.
    ObjectPositionProp object_position {}: ObjectPosition {} = ObjectPosition::Center,

    /// Sets the gap between columns in grid or flex layouts.
    ColGap col_gap { tr }: Length {} = Length::Pt(0.),

    /// Sets the gap between rows in grid or flex layouts.
    RowGap row_gap { tr }: Length {} = Length::Pt(0.),

    /// Width of the scrollbar track in pixels.
    ///
    /// This property reserves space for scrollbars when `overflow_x` or `overflow_y` is set to `Scroll`.
    /// The reserved space reduces the available content area but ensures content doesn't flow under the scrollbar.
    ///
    /// **Default:** `8px`
    ScrollbarWidth scrollbar_width {tr}: Pt {} = Pt(8.),

    /// How children overflowing their container in X axis should affect layout
    OverflowX overflow_x {}: Overflow {} = Overflow::default(),

    /// How children overflowing their container in Y axis should affect layout
    OverflowY overflow_y {}: Overflow {} = Overflow::default(),

    /// Sets the horizontal scale transform.
    ScaleX scale_x {tr}: Pct {} = Pct(100.),

    /// Sets the vertical scale transform.
    ScaleY scale_y {tr}: Pct {} = Pct(100.),

    /// Sets the horizontal translation transform.
    TranslateX translate_x {tr}: Length {} = Length::Pt(0.),

    /// Sets the vertical translation transform.
    TranslateY translate_y {tr}: Length {} = Length::Pt(0.),

    /// Sets the rotation transform angle.
    Rotation rotate {tr}: Angle {} = Angle::Rad(0.0),

    /// Sets the anchor point for rotation transformations.
    RotateAbout rotate_about {}: AnchorAbout {} = AnchorAbout::CENTER,

    /// Sets the anchor point for scaling transformations.
    ScaleAbout scale_about {tr}: AnchorAbout {} = AnchorAbout::CENTER,

    /// Sets a custom affine transformation matrix.
    Transform transform {tr}: Affine {} = Affine::IDENTITY,

    /// Sets the opacity of the view.
    Opacity opacity {tr}: f32 {} = 1.0,

    /// Sets the selected state of the view.
    Selected set_selected {}: bool { inherited } = false,

    /// Controls the disabled state of the view.
    Disabled set_disabled {}: bool { inherited } = false,

    /// Controls whether the view can receive focus during navigation such as tab or arrow navigation.
    Focusable set_focus {}: Focus { } = Focus::None,
);


// ============================================================================
// Convenience setters and helpers on Style that build on the built-in props.
// Moved from floem::style::Style's inherent impl blocks.
// ============================================================================

use crate::components::{Border, BorderColor, BorderRadius, Margin, Padding};
use crate::style_value::StyleValue;
use crate::unit::UnitExt;
use crate::values::StrokeWrap;
use parley::style::{OverflowWrap, WordBreakStrength};

impl Style {
    /// Sets the width to 100% of the parent container.
    pub fn width_full(self) -> Self {
        self.width_pct(100.0)
    }

    /// Sets the width as a percentage of the parent container.
    pub fn width_pct(self, width: f64) -> Self {
        self.width(width.pct())
    }

    /// Sets the height to 100% of the parent container.
    pub fn height_full(self) -> Self {
        self.height_pct(100.0)
    }

    /// Sets the height as a percentage of the parent container.
    pub fn height_pct(self, height: f64) -> Self {
        self.height(height.pct())
    }

    /// Makes the view fully keyboard navigable.
    ///
    /// The view can receive focus via Tab/Shift+Tab navigation, arrow keys,
    /// pointer clicks, and programmatic focus calls. This is the recommended
    /// setting for interactive controls like buttons, inputs, and links.
    /// Keyboard navigable is a strict superset of focusable.
    ///
    /// Equivalent to `focus(Focus::Keyboard)`.
    pub fn keyboard_navigable(self) -> Self {
        self.set(Focusable, Focus::Keyboard)
    }

    /// Makes the view focusable by pointer and programmatically, but excludes it
    /// from keyboard navigation. For many elements (especially buttons) you should
    /// probably use [Self::keyboard_navigable].
    ///
    /// The view can be clicked to receive focus or focused via `request_focus()`,
    /// but will not be included in Tab order or arrow key navigation. Useful for
    /// scroll containers, modal backdrops, or roving tabindex patterns.
    /// If you need keyboard traversal, use [Self::keyboard_navigable], which
    /// also enables focusability automatically.
    ///
    /// Equivalent to `focus(Focus::PointerAndProgrammatic)`.
    pub fn focusable(self) -> Self {
        self.set(Focusable, Focus::PointerAndProgrammatic)
    }

    /// Sets the font size for text content.
    pub fn font_size(self, size: impl Into<Pt>) -> Self {
        let px = size.into();
        self.set_style_value(FontSize, StyleValue::Val(px.0))
    }

    /// Makes the view non-focusable through any means.
    ///
    /// The view cannot receive focus via keyboard, pointer, or programmatic calls.
    /// Use this for decorative elements or containers that should never be interactive.
    ///
    /// Equivalent to `focus(Focus::None)`.
    pub fn focus_none(self) -> Self {
        self.set(Focusable, Focus::None)
    }

    /// Sets different gaps for rows and columns in grid or flex layouts.
    pub fn row_col_gap(self, width: impl Into<Length>, height: impl Into<Length>) -> Self {
        self.col_gap(width).row_gap(height)
    }

    /// Sets the same gap for both rows and columns in grid or flex layouts.
    pub fn gap(self, gap: impl Into<Length>) -> Self {
        let gap = gap.into();
        self.col_gap(gap).row_gap(gap)
    }

    /// Sets both width and height of the view.
    pub fn size(self, width: impl Into<LengthAuto>, height: impl Into<LengthAuto>) -> Self {
        self.width(width).height(height)
    }

    /// Sets both width and height to 100% of the parent container.
    pub fn size_full(self) -> Self {
        self.size_pct(100.0, 100.0)
    }

    /// Sets both width and height as percentages of the parent container.
    pub fn size_pct(self, width: f64, height: f64) -> Self {
        self.width(width.pct()).height(height.pct())
    }

    /// Sets the minimum width to 100% of the parent container.
    pub fn min_width_full(self) -> Self {
        self.min_width_pct(100.0)
    }

    /// Sets the minimum width as a percentage of the parent container.
    pub fn min_width_pct(self, min_width: f64) -> Self {
        self.min_width(min_width.pct())
    }

    /// Sets the minimum height to 100% of the parent container.
    pub fn min_height_full(self) -> Self {
        self.min_height_pct(100.0)
    }

    /// Sets the minimum height as a percentage of the parent container.
    pub fn min_height_pct(self, min_height: f64) -> Self {
        self.min_height(min_height.pct())
    }

    /// Sets both minimum width and height to 100% of the parent container.
    pub fn min_size_full(self) -> Self {
        self.min_size_pct(100.0, 100.0)
    }

    /// Sets both minimum width and height of the view.
    pub fn min_size(
        self,
        min_width: impl Into<LengthAuto>,
        min_height: impl Into<LengthAuto>,
    ) -> Self {
        self.min_width(min_width).min_height(min_height)
    }

    /// Sets both minimum width and height as percentages of the parent container.
    pub fn min_size_pct(self, min_width: f64, min_height: f64) -> Self {
        self.min_size(min_width.pct(), min_height.pct())
    }

    /// Sets the maximum width to 100% of the parent container.
    pub fn max_width_full(self) -> Self {
        self.max_width_pct(100.0)
    }

    /// Sets the maximum width as a percentage of the parent container.
    pub fn max_width_pct(self, max_width: f64) -> Self {
        self.max_width(max_width.pct())
    }

    /// Sets the maximum height to 100% of the parent container.
    pub fn max_height_full(self) -> Self {
        self.max_height_pct(100.0)
    }

    /// Sets the maximum height as a percentage of the parent container.
    pub fn max_height_pct(self, max_height: f64) -> Self {
        self.max_height(max_height.pct())
    }

    /// Sets both maximum width and height of the view.
    pub fn max_size(
        self,
        max_width: impl Into<LengthAuto>,
        max_height: impl Into<LengthAuto>,
    ) -> Self {
        self.max_width(max_width).max_height(max_height)
    }

    /// Sets both maximum width and height to 100% of the parent container.
    pub fn max_size_full(self) -> Self {
        self.max_size_pct(100.0, 100.0)
    }

    /// Sets both maximum width and height as percentages of the parent container.
    pub fn max_size_pct(self, max_width: f64, max_height: f64) -> Self {
        self.max_size(max_width.pct(), max_height.pct())
    }

    /// Sets the border color for all sides of the view.
    pub fn border_color(self, color: impl Into<Brush>) -> Self {
        let color = color.into();
        self.set(BorderLeftColor, Some(color.clone()))
            .set(BorderTopColor, Some(color.clone()))
            .set(BorderRightColor, Some(color.clone()))
            .set(BorderBottomColor, Some(color))
    }

    /// Sets the border properties for all sides of the view.
    pub fn border(self, border: impl Into<StrokeWrap>) -> Self {
        let border = border.into();
        self.set(BorderLeft, border.0.clone())
            .set(BorderTop, border.0.clone())
            .set(BorderRight, border.0.clone())
            .set(BorderBottom, border.0)
    }

    /// Sets the outline properties of the view.
    pub fn outline(self, outline: impl Into<StrokeWrap>) -> Self {
        self.set_style_value(Outline, StyleValue::Val(outline.into().0))
    }

    /// Sets the left border.
    pub fn border_left(self, border: impl Into<StrokeWrap>) -> Self {
        self.set(BorderLeft, border.into().0)
    }

    /// Sets the top border.
    pub fn border_top(self, border: impl Into<StrokeWrap>) -> Self {
        self.set(BorderTop, border.into().0)
    }

    /// Sets the right border.
    pub fn border_right(self, border: impl Into<StrokeWrap>) -> Self {
        self.set(BorderRight, border.into().0)
    }

    /// Sets the bottom border.
    pub fn border_bottom(self, border: impl Into<StrokeWrap>) -> Self {
        self.set(BorderBottom, border.into().0)
    }

    /// Sets `border_left` and `border_right` to `border`
    pub fn border_horiz(self, border: impl Into<StrokeWrap>) -> Self {
        let border = border.into();
        self.set(BorderLeft, border.0.clone())
            .set(BorderRight, border.0)
    }

    /// Sets `border_top` and `border_bottom` to `border`
    pub fn border_vert(self, border: impl Into<StrokeWrap>) -> Self {
        let border = border.into();
        self.set(BorderTop, border.0.clone())
            .set(BorderBottom, border.0)
    }

    /// Sets the left padding as a percentage of the parent container width.
    pub fn padding_left_pct(self, padding: f64) -> Self {
        self.padding_left(padding.pct())
    }

    /// Sets the right padding as a percentage of the parent container width.
    pub fn padding_right_pct(self, padding: f64) -> Self {
        self.padding_right(padding.pct())
    }

    /// Sets the top padding as a percentage of the parent container width.
    pub fn padding_top_pct(self, padding: f64) -> Self {
        self.padding_top(padding.pct())
    }

    /// Sets the bottom padding as a percentage of the parent container width.
    pub fn padding_bottom_pct(self, padding: f64) -> Self {
        self.padding_bottom(padding.pct())
    }

    /// Set padding on all directions
    pub fn padding(self, padding: impl Into<Length>) -> Self {
        let padding = padding.into();
        self.set(PaddingLeft, padding)
            .set(PaddingTop, padding)
            .set(PaddingRight, padding)
            .set(PaddingBottom, padding)
    }

    /// Sets padding on all sides as a percentage of the parent container width.
    pub fn padding_pct(self, padding: f64) -> Self {
        self.padding(padding.pct())
    }

    /// Sets `padding_left` and `padding_right` to `padding`
    pub fn padding_horiz(self, padding: impl Into<Length>) -> Self {
        let padding = padding.into();
        self.set(PaddingLeft, padding).set(PaddingRight, padding)
    }

    /// Sets horizontal padding as a percentage of the parent container width.
    pub fn padding_horiz_pct(self, padding: f64) -> Self {
        self.padding_horiz(padding.pct())
    }

    /// Sets `padding_top` and `padding_bottom` to `padding`
    pub fn padding_vert(self, padding: impl Into<Length>) -> Self {
        let padding = padding.into();
        self.set(PaddingTop, padding).set(PaddingBottom, padding)
    }

    /// Sets vertical padding as a percentage of the parent container width.
    pub fn padding_vert_pct(self, padding: f64) -> Self {
        self.padding_vert(padding.pct())
    }

    /// Sets the left margin as a percentage of the parent container width.
    pub fn margin_left_pct(self, margin: f64) -> Self {
        self.margin_left(margin.pct())
    }

    /// Sets the right margin as a percentage of the parent container width.
    pub fn margin_right_pct(self, margin: f64) -> Self {
        self.margin_right(margin.pct())
    }

    /// Sets the top margin as a percentage of the parent container width.
    pub fn margin_top_pct(self, margin: f64) -> Self {
        self.margin_top(margin.pct())
    }

    /// Sets the bottom margin as a percentage of the parent container width.
    pub fn margin_bottom_pct(self, margin: f64) -> Self {
        self.margin_bottom(margin.pct())
    }

    /// Sets margin on all sides of the view.
    pub fn margin(self, margin: impl Into<LengthAuto>) -> Self {
        let margin = margin.into();
        self.set(MarginLeft, margin)
            .set(MarginTop, margin)
            .set(MarginRight, margin)
            .set(MarginBottom, margin)
    }

    /// Sets margin on all sides as a percentage of the parent container width.
    pub fn margin_pct(self, margin: f64) -> Self {
        self.margin(margin.pct())
    }

    /// Sets `margin_left` and `margin_right` to `margin`
    pub fn margin_horiz(self, margin: impl Into<LengthAuto>) -> Self {
        let margin = margin.into();
        self.set(MarginLeft, margin).set(MarginRight, margin)
    }

    /// Sets horizontal margin as a percentage of the parent container width.
    pub fn margin_horiz_pct(self, margin: f64) -> Self {
        self.margin_horiz(margin.pct())
    }

    /// Sets `margin_top` and `margin_bottom` to `margin`
    pub fn margin_vert(self, margin: impl Into<LengthAuto>) -> Self {
        let margin = margin.into();
        self.set(MarginTop, margin).set(MarginBottom, margin)
    }

    /// Sets vertical margin as a percentage of the parent container width.
    pub fn margin_vert_pct(self, margin: f64) -> Self {
        self.margin_vert(margin.pct())
    }

    /// Applies a complete padding configuration to the view.
    pub fn apply_padding(self, padding: Padding) -> Self {
        let mut style = self;
        if let Some(left) = padding.left {
            style = style.set(PaddingLeft, left);
        }
        if let Some(top) = padding.top {
            style = style.set(PaddingTop, top);
        }
        if let Some(right) = padding.right {
            style = style.set(PaddingRight, right);
        }
        if let Some(bottom) = padding.bottom {
            style = style.set(PaddingBottom, bottom);
        }
        style
    }
    /// Applies a complete margin configuration to the view.
    pub fn apply_margin(self, margin: Margin) -> Self {
        let mut style = self;
        if let Some(left) = margin.left {
            style = style.set(MarginLeft, left);
        }
        if let Some(top) = margin.top {
            style = style.set(MarginTop, top);
        }
        if let Some(right) = margin.right {
            style = style.set(MarginRight, right);
        }
        if let Some(bottom) = margin.bottom {
            style = style.set(MarginBottom, bottom);
        }
        style
    }

    /// Sets the border radius for all corners of the view.
    pub fn border_radius(self, radius: impl Into<Length>) -> Self {
        let radius = radius.into();
        self.set(BorderTopLeftRadius, radius)
            .set(BorderTopRightRadius, radius)
            .set(BorderBottomLeftRadius, radius)
            .set(BorderBottomRightRadius, radius)
    }

    /// Applies a complete border configuration to the view.
    pub fn apply_border(self, border: Border) -> Self {
        let mut style = self;
        if let Some(left) = border.left {
            style = style.set(BorderLeft, left);
        }
        if let Some(top) = border.top {
            style = style.set(BorderTop, top);
        }
        if let Some(right) = border.right {
            style = style.set(BorderRight, right);
        }
        if let Some(bottom) = border.bottom {
            style = style.set(BorderBottom, bottom);
        }
        style
    }
    /// Applies a complete border color configuration to the view.
    pub fn apply_border_color(self, border_color: BorderColor) -> Self {
        let mut style = self;
        if let Some(left) = border_color.left {
            style = style.set(BorderLeftColor, Some(left));
        }
        if let Some(top) = border_color.top {
            style = style.set(BorderTopColor, Some(top));
        }
        if let Some(right) = border_color.right {
            style = style.set(BorderRightColor, Some(right));
        }
        if let Some(bottom) = border_color.bottom {
            style = style.set(BorderBottomColor, Some(bottom));
        }
        style
    }
    /// Applies a complete border radius configuration to the view.
    pub fn apply_border_radius(self, border_radius: BorderRadius) -> Self {
        let mut style = self;
        if let Some(top_left) = border_radius.top_left {
            style = style.set(BorderTopLeftRadius, top_left);
        }
        if let Some(top_right) = border_radius.top_right {
            style = style.set(BorderTopRightRadius, top_right);
        }
        if let Some(bottom_left) = border_radius.bottom_left {
            style = style.set(BorderBottomLeftRadius, bottom_left);
        }
        if let Some(bottom_right) = border_radius.bottom_right {
            style = style.set(BorderBottomRightRadius, bottom_right);
        }
        style
    }

    /// Sets the left inset as a percentage of the parent container width.
    pub fn inset_left_pct(self, inset: f64) -> Self {
        self.inset_left(inset.pct())
    }

    /// Sets the right inset as a percentage of the parent container width.
    pub fn inset_right_pct(self, inset: f64) -> Self {
        self.inset_right(inset.pct())
    }

    /// Sets the top inset as a percentage of the parent container height.
    pub fn inset_top_pct(self, inset: f64) -> Self {
        self.inset_top(inset.pct())
    }

    /// Sets the bottom inset as a percentage of the parent container height.
    pub fn inset_bottom_pct(self, inset: f64) -> Self {
        self.inset_bottom(inset.pct())
    }

    /// Sets all insets (left, top, right, bottom) to the same value.
    pub fn inset(self, inset: impl Into<LengthAuto>) -> Self {
        let inset = inset.into();
        self.inset_left(inset)
            .inset_top(inset)
            .inset_right(inset)
            .inset_bottom(inset)
    }

    /// Sets all insets as percentages of the parent container.
    pub fn inset_pct(self, inset: f64) -> Self {
        let inset = inset.pct();
        self.inset_left(inset)
            .inset_top(inset)
            .inset_right(inset)
            .inset_bottom(inset)
    }

    /// Specifies shadow blur. The larger this value, the bigger the blur,
    /// so the shadow becomes bigger and lighter.
    pub fn box_shadow_blur(self, blur_radius: impl Into<Length>) -> Self {
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
    pub fn box_shadow_spread(self, spread: impl Into<Length>) -> Self {
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

    /// Applies a shadow for the stylized view. Use [BoxShadow] builder
    /// to construct each shadow.
    /// ```rust
    /// use floem_style::{BoxShadow, Style};
    /// use peniko::color::palette::css;
    ///
    /// let _ = Style::new().apply_box_shadows(vec![
    ///    BoxShadow::new()
    ///        .color(css::BLACK)
    ///        .top_offset(5.)
    ///        .bottom_offset(-30.)
    ///        .right_offset(-20.)
    ///        .left_offset(10.)
    ///        .blur_radius(5.)
    ///        .spread(10.)
    /// ]);
    /// ```
    /// ### Info
    /// If you only specify one shadow on the view, use standard style methods directly
    /// on [Style] struct:
    /// ```rust
    /// use floem_style::Style;
    ///
    /// let _ = Style::new()
    ///     .box_shadow_top_offset(-5.)
    ///     .box_shadow_bottom_offset(30.)
    ///     .box_shadow_right_offset(20.)
    ///     .box_shadow_left_offset(-10.)
    ///     .box_shadow_spread(1.)
    ///     .box_shadow_blur(3.);
    /// ```
    pub fn apply_box_shadows(self, shadow: impl Into<SmallVec<[BoxShadow; 3]>>) -> Self {
        self.set(BoxShadowProp, shadow.into())
    }

    /// Specifies the offset on horizontal axis.
    /// Negative offset value places the shadow to the left of the view.
    pub fn box_shadow_h_offset(self, h_offset: impl Into<Length>) -> Self {
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
    /// Negative offset value places the shadow above the view.
    pub fn box_shadow_v_offset(self, v_offset: impl Into<Length>) -> Self {
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
    pub fn box_shadow_left_offset(self, left_offset: impl Into<Length>) -> Self {
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
    pub fn box_shadow_right_offset(self, right_offset: impl Into<Length>) -> Self {
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
    pub fn box_shadow_top_offset(self, top_offset: impl Into<Length>) -> Self {
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
    pub fn box_shadow_bottom_offset(self, bottom_offset: impl Into<Length>) -> Self {
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

    /// Sets the font weight to bold.
    pub fn font_bold(self) -> Self {
        self.font_weight(FontWeightProp::BOLD)
    }

    /// Enables pointer events for the view (allows mouse interaction).
    pub fn pointer_events_auto(self) -> Self {
        self.pointer_events(PointerEvents::Auto)
    }

    /// Disables pointer events for the view (mouse events pass through).
    pub fn pointer_events_none(self) -> Self {
        self.pointer_events(PointerEvents::None)
    }

    /// Sets text overflow to show ellipsis (...) when text is clipped.
    pub fn text_ellipsis(self) -> Self {
        self.text_overflow(TextOverflow::NoWrap(NoWrapOverflow::Ellipsis))
    }

    /// Sets text overflow to clip text without showing ellipsis.
    pub fn text_clip(self) -> Self {
        self.text_overflow(TextOverflow::NoWrap(NoWrapOverflow::Clip))
    }

    /// Sets text to wrap using Parley's normal overflow-wrap behavior.
    pub fn text_wrap(self) -> Self {
        self.text_overflow(TextOverflow::Wrap {
            overflow_wrap: OverflowWrap::Normal,
            word_break: WordBreakStrength::Normal,
        })
    }

    /// Sets the view to absolute positioning.
    pub fn absolute(self) -> Self {
        self.position(taffy::style::Position::Absolute)
    }

    /// Sets the view to fixed positioning relative to the viewport.
    ///
    /// This is similar to CSS `position: fixed`. The view will:
    /// - Be positioned relative to the window viewport
    /// - Use `inset` properties relative to the viewport
    /// - Have percentage sizes relative to the viewport
    /// - Be painted above all other content
    ///
    /// # Example
    /// ```rust
    /// use floem_style::Style;
    ///
    /// // Create a full-screen overlay
    /// let _ = Style::new().fixed().inset(0.0);
    /// ```
    pub fn fixed(self) -> Self {
        self.position(taffy::style::Position::Absolute)
            .is_fixed(true)
    }

    /// Aligns flex items to stretch and fill the cross axis.
    pub fn items_stretch(self) -> Self {
        self.align_items(taffy::style::AlignItems::Stretch)
    }

    /// Aligns flex items to the start of the cross axis.
    pub fn items_start(self) -> Self {
        self.align_items(taffy::style::AlignItems::FlexStart)
    }

    /// Defines the alignment along the cross axis as Centered
    pub fn items_center(self) -> Self {
        self.align_items(taffy::style::AlignItems::Center)
    }

    /// Aligns flex items to the end of the cross axis.
    pub fn items_end(self) -> Self {
        self.align_items(taffy::style::AlignItems::FlexEnd)
    }

    /// Aligns flex items along their baselines.
    pub fn items_baseline(self) -> Self {
        self.align_items(taffy::style::AlignItems::Baseline)
    }

    /// Aligns flex items to the start of the main axis.
    pub fn justify_start(self) -> Self {
        self.justify_content(taffy::style::JustifyContent::FlexStart)
    }

    /// Aligns flex items to the end of the main axis.
    pub fn justify_end(self) -> Self {
        self.justify_content(taffy::style::JustifyContent::FlexEnd)
    }

    /// Defines the alignment along the main axis as Centered
    pub fn justify_center(self) -> Self {
        self.justify_content(taffy::style::JustifyContent::Center)
    }

    /// Distributes flex items with space between them.
    pub fn justify_between(self) -> Self {
        self.justify_content(taffy::style::JustifyContent::SpaceBetween)
    }

    /// Distributes flex items with space around them.
    pub fn justify_around(self) -> Self {
        self.justify_content(taffy::style::JustifyContent::SpaceAround)
    }

    /// Distributes flex items with equal space around them.
    pub fn justify_evenly(self) -> Self {
        self.justify_content(taffy::style::JustifyContent::SpaceEvenly)
    }

    /// Hides the view from view and layout.
    pub fn hide(self) -> Self {
        self.set(DisplayProp, Display::None)
    }

    /// Sets the view to use flexbox layout.
    pub fn flex(self) -> Self {
        self.display(taffy::style::Display::Flex)
    }

    /// Sets the view to use grid layout.
    pub fn grid(self) -> Self {
        self.display(taffy::style::Display::Grid)
    }

    /// Sets flex direction to row (horizontal).
    pub fn flex_row(self) -> Self {
        self.flex_direction(taffy::style::FlexDirection::Row)
    }

    /// Sets flex direction to column (vertical).
    pub fn flex_col(self) -> Self {
        self.flex_direction(taffy::style::FlexDirection::Column)
    }

    /// Sets uniform scaling for both X and Y axes.
    pub fn scale(self, scale: impl Into<Pct>) -> Self {
        let val = scale.into();
        self.scale_x(val).scale_y(val)
    }
}

// ============================================================================
// ExprStyle convenience setters that mirror Style's but take ContextValue<T>.
// Moved from floem::style::ExprStyle's inherent impl block.
// ============================================================================

use crate::context_value::ContextValue;

impl ExprStyle {
    /// Sets the font size for text content.
    pub fn font_size<T>(self, size: ContextValue<T>) -> Self
    where
        T: Into<Pt> + 'static,
    {
        let px = size.map(|s| s.into().0);
        self.set_context(FontSize, px)
    }

    pub fn size<W, H>(self, width: ContextValue<W>, height: ContextValue<H>) -> Self
    where
        W: Into<LengthAuto> + 'static,
        H: Into<LengthAuto> + 'static,
    {
        self.width(width).height(height)
    }

    pub fn absolute(self) -> Self {
        self.set(PositionProp, Position::Absolute)
    }

    pub fn flex_row(self) -> Self {
        self.set(FlexDirectionProp, FlexDirection::Row)
    }

    pub fn margin<T>(self, margin: ContextValue<T>) -> Self
    where
        T: Into<LengthAuto> + 'static,
    {
        let margin = margin.map(Into::into);
        self.set(MarginLeft, margin.clone())
            .set(MarginTop, margin.clone())
            .set(MarginRight, margin.clone())
            .set(MarginBottom, margin)
    }

    pub fn border_color<T>(self, color: ContextValue<T>) -> Self
    where
        T: Into<Brush> + 'static,
    {
        let color = color.map(|color| Some(color.into()));
        self.set(BorderLeftColor, color.clone())
            .set(BorderTopColor, color.clone())
            .set(BorderRightColor, color.clone())
            .set(BorderBottomColor, color)
    }

    pub fn border_radius<T>(self, radius: ContextValue<T>) -> Self
    where
        T: Into<Length> + 'static,
    {
        let radius = radius.map(Into::into);
        self.set(BorderTopLeftRadius, radius.clone())
            .set(BorderTopRightRadius, radius.clone())
            .set(BorderBottomLeftRadius, radius.clone())
            .set(BorderBottomRightRadius, radius)
    }

    pub fn border<T>(self, width: ContextValue<T>) -> Self
    where
        T: Into<Pt> + 'static,
    {
        let stroke = width.map(|width| Stroke::new(width.into().0));
        self.set(BorderLeft, stroke.clone())
            .set(BorderTop, stroke.clone())
            .set(BorderRight, stroke.clone())
            .set(BorderBottom, stroke)
    }

    pub fn border_top<T>(self, width: ContextValue<T>) -> Self
    where
        T: Into<Pt> + 'static,
    {
        self.set(BorderTop, width.map(|width| Stroke::new(width.into().0)))
    }

    pub fn items_center(self) -> Self {
        self.set(AlignItemsProp, Some(AlignItems::Center))
    }

    pub fn justify_center(self) -> Self {
        self.set(
            JustifyContentProp,
            Some(taffy::style::JustifyContent::Center),
        )
    }

    pub fn selected(self, style: impl FnOnce(ExprStyle) -> ExprStyle) -> Self {
        self.merge(Style::default().selected(|s| style(s.into()).into()))
    }

    pub fn drag(self, style: impl FnOnce(ExprStyle) -> ExprStyle) -> Self {
        self.merge(Style::default().drag(|s| style(s.into()).into()))
    }

    pub fn file_hover(self, style: impl FnOnce(ExprStyle) -> ExprStyle) -> Self {
        self.merge(Style::default().file_hover(|s| style(s.into()).into()))
    }

    pub fn padding<T>(self, padding: ContextValue<T>) -> Self
    where
        T: Into<Length> + 'static,
    {
        let padding = padding.map(Into::into);
        self.set(PaddingLeft, padding.clone())
            .set(PaddingTop, padding.clone())
            .set(PaddingRight, padding.clone())
            .set(PaddingBottom, padding)
    }

    pub fn padding_horiz<T>(self, padding: ContextValue<T>) -> Self
    where
        T: Into<Length> + 'static,
    {
        let padding = padding.map(Into::into);
        self.set(PaddingLeft, padding.clone())
            .set(PaddingRight, padding)
    }

    pub fn padding_vert<T>(self, padding: ContextValue<T>) -> Self
    where
        T: Into<Length> + 'static,
    {
        let padding = padding.map(Into::into);
        self.set(PaddingTop, padding.clone())
            .set(PaddingBottom, padding)
    }

    pub fn gap<T>(self, gap: ContextValue<T>) -> Self
    where
        T: Into<Length> + 'static,
    {
        let gap = gap.map(Into::into);
        self.set(ColGap, gap.clone()).set(RowGap, gap)
    }

    pub fn custom<CS>(self, custom: impl FnOnce(CS) -> CS) -> Self
    where
        CS: Default + Clone + Into<Style> + From<Style>,
    {
        self.merge(custom(CS::default()).into())
    }

    pub fn apply_border_radius(self, border_radius: BorderRadius) -> Self {
        self.merge(Style::new().apply_border_radius(border_radius))
    }

    pub fn apply_box_shadows(self, shadow: impl Into<SmallVec<[BoxShadow; 3]>>) -> Self {
        self.set(BoxShadowProp, shadow.into())
    }

    pub fn transition_background(self, transition: Transition) -> Self {
        self.merge(Style::new().transition_background(transition))
    }

    pub fn border_bottom(self, width: impl Into<Pt>) -> Self {
        self.set(BorderBottom, Stroke::new(width.into().0))
    }

    pub fn outline(self, width: impl Into<Pt>) -> Self {
        self.set(Outline, Stroke::new(width.into().0))
    }
}
