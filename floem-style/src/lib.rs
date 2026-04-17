//! The Floem style engine.
//!
//! This crate holds the view-agnostic parts of Floem's style system: unit
//! types, pseudo-class selectors, and (in later phases) the cascade, cache,
//! transitions, and per-node style storage. It is consumed by the `floem`
//! crate, and is designed so a second host (e.g. a native-widget backend)
//! can run the same engine over its own node type by implementing the
//! traits this crate exposes.

pub mod components;
pub mod context_value;
pub mod debug_view;
pub mod design_system;
pub mod easing;
pub mod element_id;
pub mod inspector_render;
pub mod interaction;
pub mod merge_id;
pub mod prop_value;
pub mod props;
pub mod recalc;
pub mod responsive;
pub mod selectors;
pub mod style_value;
pub mod transition;
pub mod unit;
pub mod value_impls;
pub mod values;
pub mod visibility;

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
pub use selectors::StyleSelectorKey;
pub use prop_value::StylePropValue;
pub use style_value::{StyleMapValue, StyleValue};
pub use props::{
    EqAnyFn, HashAnyFn, InterpolateFn, ResolveInheritedAnyFn, StyleClass, StyleClassInfo,
    StyleClassRef, StyleDebugGroup, StyleDebugGroupInfo, StyleDebugGroupRef, StyleKey,
    StyleKeyInfo, StyleProp, StylePropInfo, StylePropRef, RESPONSIVE_SELECTORS_INFO,
    STRUCTURAL_SELECTORS_INFO,
};
pub use transition::{ActiveTransition, DirectTransition, Transition, TransitionState};
pub use value_impls::AffineLerp;
pub use values::{
    CursorStyle, Focus, NoWrapOverflow, ObjectFit, ObjectPosition, PointerEvents, StrokeWrap,
    TextOverflow,
};
pub use visibility::{Visibility, VisibilityPhase};
