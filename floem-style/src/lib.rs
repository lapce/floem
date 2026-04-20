//! The Floem style engine.
//!
//! A CSS-style cascade engine tied to [`taffy`] for layout. Holds the
//! view-agnostic parts of Floem's style system: unit types, pseudo-class
//! selectors, the cascade, built-in property definitions, transitions,
//! per-node style storage, and the [`StyleTree`] that drives it all.
//!
//! # Scope
//!
//! Floem's model follows CSS: layout *is* style. `display`, `flex-*`,
//! `grid-*`, `width`, `padding` etc. are style properties the cascade
//! resolves, and their resolved values are the input to a layout solver.
//! This crate embraces that by bundling [`taffy`] as the layout-input
//! contract: style types in `floem_style` include taffy's enums
//! ([`Display`], [`FlexDirection`], [`AlignItems`], etc.) as first-class
//! values, and [`Style::to_taffy_style`] is the public bridge.
//!
//! The whole `taffy` crate is re-exported as [`crate::taffy`] so consumers
//! don't need a separate dependency.
//!
//! # What this crate does NOT own
//!
//! - **Reactive runtime**: hosts bring their own signal/effect primitives.
//!   The engine itself holds no reactive state.
//! - **View tree / hit-test structure**: hosts own their node type
//!   ([`floem::ViewId`], etc.) and map it to [`StyleNodeId`] via the
//!   [`ElementId`] abstraction.
//! - **Rendering / input**: out of scope.
//!
//! # Integration
//!
//! A host drives the engine by:
//! 1. Implementing the [`StyleSink`] trait on its window/root state.
//! 2. Allocating a [`StyleNodeId`] per element via [`StyleTree::new_node`]
//!    and wiring parent/children edges.
//! 3. Pushing direct styles and classes via
//!    [`StyleTree::set_direct_style`] / [`StyleTree::set_classes`].
//! 4. Calling [`StyleTree::compute_style`] each frame, which runs the
//!    cascade (including animations, selector matching, inherited/class
//!    context propagation, and side-effects via the sink).
//! 5. Reading resolved values from the [`StyleNode`] and converting
//!    layout-relevant ones to a [`taffy::style::Style`] via
//!    [`Style::to_taffy_style`] before passing to taffy.
//!
//! See `tests/mock_sink.rs` and `tests/style_tree_cascade.rs` for
//! end-to-end examples without floem.

pub mod animation;
pub mod builtin_props;
pub mod cache;
pub mod cascade;
pub mod components;
pub mod context_value;
pub mod debug_view;
pub mod design_system;
pub mod easing;
pub mod element_id;
pub mod extractors;
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
// Holds only `#[macro_export]` macros, which surface at the crate root;
// no user needs to name this module directly.
pub(crate) mod style_macros;
pub mod style_value;
pub mod transition;
pub mod tree;
pub mod unit;
pub(crate) mod value_impls;
pub mod values;
pub mod visibility;

/// Re-export of the [`taffy`] crate. floem-style owns the style →
/// layout-input bridge for taffy (see crate-level docs), so consumers
/// can reach taffy types directly through `floem_style::taffy::...`
/// without adding a separate dependency.
pub use taffy;

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
pub use animation::{
    AnimStateCommand, AnimStateKind, Animation, AnimationEvents, KeyFrame, KeyFrameStyle,
    PropCache, RepeatMode, ReverseOnce,
};
pub use cache::{CacheHit, CacheStats, StyleCache, StyleCacheKey};
pub use cascade::resolve_nested_maps;
pub use components::{Border, BorderColor, BorderRadius, BoxShadow, Margin, Padding};
pub use context_value::ContextValue;
pub use debug_view::PropDebugView;
pub use design_system::DesignSystem;
pub use easing::{Bezier, Easing, Linear, Spring, Step, StepPosition};
pub use element_id::ElementId;
pub use extractors::{FontProps, LayoutProps, TransformProps, ViewStyleProps};
pub use inspector_render::InspectorRender;
pub use interaction::{InheritedInteractionCx, InteractionState};
pub use merge_id::{
    combine_merge_ids, next_style_merge_id, DEFERRED_EFFECTS_INFO, DEFERRED_EFFECTS_KEY,
};
pub use prop_reader::{ExtractorField, PropExtractorCx};
pub use prop_value::StylePropValue;
pub use props::{
    EqAnyFn, HashAnyFn, InterpolateFn, ResolveInheritedAnyFn, StyleClass, StyleClassInfo,
    StyleClassRef, StyleDebugGroup, StyleDebugGroupInfo, StyleDebugGroupRef, StyleKey,
    StyleKeyInfo, StyleProp, StylePropInfo, StylePropRef, RESPONSIVE_SELECTORS_INFO,
    STRUCTURAL_SELECTORS_INFO,
};
pub use sink::{AnimationBackend, CascadeInputs, NoAnimationBackend, PerNodeInteraction};
pub use style::{BuiltinStyle, ContextRef, DeferredStyleEffect, ExprStyle, Style};
pub use style_value::{StyleMapValue, StyleValue};
pub use transition::{ActiveTransition, DirectTransition, Transition, TransitionState};
pub use tree::{StyleNode, StyleNodeId, StyleTree};
pub use unit::LineHeightValue;
pub use value_impls::AffineLerp;
pub use values::{
    CursorStyle, Focus, NoWrapOverflow, ObjectFit, ObjectPosition, PointerEvents, StrokeWrap,
    TextOverflow,
};
pub use visibility::{Visibility, VisibilityPhase};
