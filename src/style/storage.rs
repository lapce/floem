//! Per-node storage for the style engine.
//!
//! Fields the style engine reads and writes for each node live here, separate
//! from `ViewState`'s view-layer concerns (event listeners, menus, layout-id
//! bookkeeping, user cursor, etc.). In Phase 2 this becomes the state exposed
//! by the `StyleNode` trait, letting a second consumer such as `floem-native`
//! run the same engine over its own node type.

use crate::style::{
    CursorStyle, InheritedInteractionCx, LayoutProps, Style, StyleSelectors, TransformProps,
};
use crate::view::state::{ViewStyleProps, Visibility};

/// Engine-owned per-node state produced and consumed by the style pass.
#[derive(Default)]
pub(crate) struct StyleStorage {
    pub has_style_selectors: Option<StyleSelectors>,
    pub layout_props: LayoutProps,
    pub view_style_props: ViewStyleProps,
    pub view_transform_props: TransformProps,
    /// Pre-animation snapshot of `combined_style`; animations re-derive
    /// `combined_style` from this each frame so animated values don't feed back.
    pub combined_pre_animation_style: Style,
    /// The resolved style for this view (base + selectors + classes).
    /// Does NOT include inherited properties from ancestors.
    ///
    /// Use for style resolution logic (what did this view define?):
    /// - Checking if a property is explicitly set on this view
    /// - Computing class context propagation to children
    /// - Building style cache keys
    pub combined_style: Style,
    /// The final computed style including inherited properties from ancestors.
    /// This is combined_style merged with inherited context (font_size, color, etc.).
    ///
    /// Use for rendering and layout (what will the user see?):
    /// - Layout calculations via prop extractors
    /// - Visual properties (background, border, transform)
    /// - Anything that affects what gets rendered
    /// - Converting to taffy style for layout engine
    ///
    /// This DOES NOT have final interpolated values and it DOES NOT resolve properties into points.
    pub computed_style: Style,
    /// The inherited properties context for children.
    /// Contains only properties marked as `inherited` (font-size, color, etc.).
    ///
    /// Derived from this view's computed_style (which includes inherited properties
    /// from ancestors). Children will merge this with their combined_style to produce
    /// their computed_style.
    pub style_cx: Style,
    /// The class context containing class definitions for descendants.
    /// Contains `.class(SomeClass, ...)` nested maps that flow down the tree.
    ///
    /// Derived from this view's combined_style (only explicitly set class definitions).
    /// Children will use this to resolve their class references when computing their
    /// combined_style.
    pub class_cx: Style,
    /// Interaction cx saved after computing the final style; becomes the
    /// inherited interaction for this view's children.
    pub style_interaction_cx: InheritedInteractionCx,
    /// View-local interaction flags derived from this view's resolved combined style.
    ///
    /// This excludes inherited parent interaction and is OR'ed onto StyleCx interaction
    /// state in `style_view`. Populated by
    /// [`WindowState::run_style_cascade`](crate::window::state::WindowState::run_style_cascade)
    /// each pass from the [`floem_style::StyleTree`] cascade outputs.
    pub post_compute_combined_interaction: InheritedInteractionCx,
    /// Interaction cx set by a parent on this view; consumed when building the
    /// StyleCx for this view.
    pub parent_set_style_interaction: InheritedInteractionCx,
    /// Controls view visibility for phase transitions.
    pub visibility: Visibility,
    /// The cursor style set by the style pass on the view.
    /// There is also a user-set cursor (on `ViewState`) which takes precedence.
    pub style_cursor: Option<CursorStyle>,
    /// Number of enter/exit animations still running for this view; when this
    /// hits zero the visibility phase can transition to its final state.
    pub num_waiting_animations: u16,
}
