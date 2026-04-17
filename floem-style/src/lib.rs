//! The Floem style engine.
//!
//! This crate holds the view-agnostic parts of Floem's style system: unit
//! types, pseudo-class selectors, the cascade, built-in property definitions,
//! transitions, and per-node style storage. It is consumed by the `floem`
//! crate, and is designed so a second host (e.g. a native-widget backend)
//! can run the same engine over its own node type by implementing the
//! traits this crate exposes.

pub mod builtin_props;
pub mod cache;
pub mod cascade;
pub mod components;
pub mod context_value;
pub mod debug_view;
pub mod design_system;
pub mod easing;
pub mod element_id;
pub mod inspector_render;
pub mod interaction;
pub mod merge_id;
pub mod prop_reader;
pub mod prop_value;
pub mod props;
pub mod recalc;
pub mod responsive;
pub mod selectors;
pub mod sink;
pub mod style;
pub mod style_macros;
pub mod style_value;
pub mod transition;
pub mod unit;
pub(crate) mod value_impls;
pub mod values;
pub mod visibility;

pub use builtin_props::{
    AlignContent, AlignContentProp, AlignItems, AlignItemsProp, AlignSelf, AspectRatio,
    Background, BorderBottom, BorderBottomColor, BorderBottomLeftRadius, BorderBottomRightRadius,
    BorderLeft, BorderLeftColor, BorderProgress, BorderRight, BorderRightColor, BorderTop,
    BorderTopColor, BorderTopLeftRadius, BorderTopRightRadius, BoxShadowProp, BoxSizing,
    BoxSizingProp, ColGap, Cursor, CursorColor, Dimension, Disabled, Display, DisplayProp,
    FlexBasis, FlexDirection, FlexDirectionProp, FlexGrow, FlexShrink, FlexWrap, FlexWrapProp,
    Focusable, FontFamily, FontSize, FontStyle, FontWeight, Foreground, GridAutoColumns,
    GridAutoFlow, GridAutoRows, GridColumn, GridRow, GridTemplateColumns, GridTemplateRows, Height,
    InsetBottom, InsetLeft, InsetRight, InsetTop, IsFixed, JustifyContent, JustifyContentProp,
    JustifyItems, JustifyItemsProp, JustifySelf, LineHeight, MarginBottom, MarginLeft, MarginRight,
    MarginTop, MaxHeight, MaxWidth, MinHeight, MinWidth, ObjectFitProp, ObjectPositionProp,
    Opacity, Outline, OutlineColor, OutlineProgress, OverflowX, OverflowY, PaddingBottom,
    PaddingLeft, PaddingRight, PaddingTop, PointerEventsProp, Position, PositionProp, RotateAbout,
    Rotation, RowGap, ScaleAbout, ScaleX, ScaleY, ScrollbarWidth, Selectable, Selected,
    SelectionCornerRadius, TextAlignProp, TextColor, TextOverflowProp, Transform, TranslateX,
    TranslateY, Width, ZIndex,
};
pub use cache::{CacheHit, CacheStats, StyleCache, StyleCacheKey};
pub use cascade::resolve_nested_maps;
pub use components::{Border, BorderColor, BorderRadius, BoxShadow, Margin, Padding};
pub use context_value::ContextValue;
pub use debug_view::PropDebugView;
pub use design_system::DesignSystem;
pub use easing::{Bezier, Easing, Linear, Spring, Step, StepPosition};
pub use element_id::ElementId;
pub use inspector_render::InspectorRender;
pub use interaction::{InheritedInteractionCx, InteractionState};
pub use merge_id::{
    combine_merge_ids, next_style_merge_id, DEFERRED_EFFECTS_INFO, DEFERRED_EFFECTS_KEY,
};
pub use prop_reader::{ExtractorField, StylePropReader};
pub use prop_value::StylePropValue;
pub use props::{
    EqAnyFn, HashAnyFn, InterpolateFn, ResolveInheritedAnyFn, StyleClass, StyleClassInfo,
    StyleClassRef, StyleDebugGroup, StyleDebugGroupInfo, StyleDebugGroupRef, StyleKey,
    StyleKeyInfo, StyleProp, StylePropInfo, StylePropRef, RESPONSIVE_SELECTORS_INFO,
    STRUCTURAL_SELECTORS_INFO,
};
pub use selectors::StyleSelectorKey;
pub use sink::StyleSink;
pub use style::{BuiltinStyle, ContextRef, DeferredStyleEffect, ExprStyle, Style};
pub use style_value::{StyleMapValue, StyleValue};
pub use transition::{ActiveTransition, DirectTransition, Transition, TransitionState};
pub use value_impls::AffineLerp;
pub use values::{
    CursorStyle, Focus, NoWrapOverflow, ObjectFit, ObjectPosition, PointerEvents, StrokeWrap,
    TextOverflow,
};
pub use visibility::{Visibility, VisibilityPhase};
