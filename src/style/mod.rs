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

pub use taffy::style::{
    AlignContent, AlignItems, BoxSizing, Dimension, Display, FlexDirection, FlexWrap,
    JustifyContent, JustifyItems, Position,
};

// Import macros used by the prop_extractor blocks below.
use crate::prop_extractor;

mod custom;
mod cx;
mod inspector_render_impl;
mod sink;
mod storage;
mod style_debug_ext;
pub mod theme;
mod debug_view_impl;
mod transition_ext;

// Re-export modules moved to the `floem_style` crate so the `floem::style::*`
// API surface remains stable for downstream users.
pub use floem_style::{recalc, selectors, unit};

pub use floem_style::{CursorStyle, Focus, NoWrapOverflow, PointerEvents, TextOverflow};
pub use floem_style::{Border, BorderColor, BorderRadius, BoxShadow, Margin, Padding};
pub use custom::{CustomStylable, CustomStyle, StyleCustomExt};
pub use cx::{InheritedInteractionCx, InteractionState, StyleCx};
pub use floem_style::{InspectorRender, PropDebugView};
pub use inspector_render_impl::FloemInspectorRender;
pub use floem_style::{
    ExtractorField, StyleClass, StyleClassInfo, StyleClassRef, StyleDebugGroup,
    StyleDebugGroupInfo, StyleDebugGroupRef, StyleKey, StyleKeyInfo, StyleProp, StylePropInfo,
    StylePropRef,
};
pub use floem_style::selectors::{NthChild, StructuralSelector, StyleSelector, StyleSelectors};
pub use theme::{DesignSystem, StyleThemeExt};
pub use floem_style::{DirectTransition, Transition, TransitionState};
pub use style_debug_ext::StyleDebugViewExt;
pub use transition_ext::TransitionDebugViewExt;
pub use floem_style::unit::{
    AnchorAbout, Angle, Auto, DurationUnitExt, Em, FontSizeCx, Length, LengthAuto, Lh,
    LineHeightValue, Pct, Pt, UnitExt,
};
pub use floem_style::{
    ContextValue, ObjectFit, ObjectPosition, StrokeWrap, StyleMapValue, StylePropValue, StyleValue,
};

pub use floem_style::{CacheHit, CacheStats, StyleCache, StyleCacheKey};

pub(crate) use storage::StyleStorage;


// ============================================================================
// Style struct, cascade, built-in props — all moved to the `floem_style` crate
// ============================================================================

pub use floem_style::{
    resolve_nested_maps, BuiltinStyle, ContextRef, DeferredStyleEffect, ExprStyle, Style,
};
pub(crate) use floem_style::cascade::{ResponsiveSelectors, StructuralSelectors};

pub use floem_style::{
    AlignContentProp, AlignItemsProp, AlignSelf, AspectRatio, Background, BorderBottom,
    BorderBottomColor, BorderBottomLeftRadius, BorderBottomRightRadius, BorderLeft,
    BorderLeftColor, BorderProgress, BorderRight, BorderRightColor, BorderTop, BorderTopColor,
    BorderTopLeftRadius, BorderTopRightRadius, BoxShadowProp, BoxSizingProp, ColGap, Cursor,
    CursorColor, Disabled, DisplayProp, FlexBasis, FlexDirectionProp, FlexGrow, FlexShrink,
    FlexWrapProp, Focusable, FontFamily, FontSize, FontStyle, FontWeight, Foreground,
    GridAutoColumns, GridAutoFlow, GridAutoRows, GridColumn, GridRow, GridTemplateColumns,
    GridTemplateRows, Height, InsetBottom, InsetLeft, InsetRight, InsetTop, IsFixed,
    JustifyContentProp, JustifyItemsProp, JustifySelf, LineHeight, MarginBottom, MarginLeft,
    MarginRight, MarginTop, MaxHeight, MaxWidth, MinHeight, MinWidth, ObjectFitProp,
    ObjectPositionProp, Opacity, Outline, OutlineColor, OutlineProgress, OverflowX, OverflowY,
    PaddingBottom, PaddingLeft, PaddingRight, PaddingTop, PointerEventsProp, PositionProp,
    RotateAbout, Rotation, RowGap, ScaleAbout, ScaleX, ScaleY, ScrollbarWidth, Selectable,
    Selected, SelectionCornerRadius, TextAlignProp, TextColor, TextOverflowProp, Transform,
    TranslateX, TranslateY, Width, ZIndex,
};

use crate::views::editor::SelectionColor;

pub use floem_style::{FontProps, LayoutProps, TransformProps};

prop_extractor! {
    pub SelectionStyle {
        pub corner_radius: SelectionCornerRadius,
        pub selection_color: SelectionColor,
    }
}
