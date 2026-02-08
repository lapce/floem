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
//! - ContextMappings,
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

use floem_renderer::text::{LineHeightValue, Weight};
use imbl::hashmap::Entry;
use peniko::color::palette;
use peniko::kurbo::{self, Affine, RoundedRect, Vec2};
use peniko::{Brush, Color};
use smallvec::SmallVec;
use std::any::Any;
use std::collections::HashMap;
use std::fmt::{self, Debug};
use std::rc::Rc;
use taffy::GridTemplateComponent;

pub use taffy::style::{
    AlignContent, AlignItems, BoxSizing, Dimension, Display, FlexDirection, FlexWrap,
    JustifyContent, JustifyItems, Position,
};
use taffy::{
    geometry::{MinMax, Size},
    prelude::{GridPlacement, Line, Rect},
    style::{
        LengthPercentage, MaxTrackSizingFunction, MinTrackSizingFunction, Overflow,
        Style as TaffyStyle,
    },
};

use crate::layout::responsive::{ScreenSize, ScreenSizeBp};

// Import macros from crate root (they are #[macro_export] in props.rs)
use crate::{prop, prop_extractor};

mod cache;
mod components;
mod custom;
mod cx;
mod props;
pub mod recalc;
mod selectors;
#[cfg(test)]
mod tests;
pub mod theme;
mod transition;
pub mod unit;
mod values;

pub use components::{
    Border, BorderColor, BorderRadius, BoxShadow, CursorStyle, Margin, Padding, PointerEvents,
    TextOverflow,
};
pub use custom::{CustomStylable, CustomStyle};
pub use cx::{InheritedInteractionCx, InteractionState, StyleCx};
pub use props::{
    ExtractorField, StyleClass, StyleClassInfo, StyleClassRef, StyleKey, StyleKeyInfo, StyleProp,
    StylePropInfo, StylePropReader, StylePropRef,
};
pub use selectors::{StyleSelector, StyleSelectors};
pub use theme::{DesignSystem, StyleThemeExt};
pub use transition::{DirectTransition, Transition, TransitionState};
pub use unit::{AnchorAbout, Angle, Auto, DurationUnitExt, Pct, Px, PxPct, PxPctAuto, UnitExt};
pub use values::{CombineResult, StrokeWrap, StyleMapValue, StylePropValue, StyleValue};

pub use cache::{StyleCache, StyleCacheKey};
pub use recalc::{InheritedChanges, InheritedGroups, Propagate, RecalcFlags, StyleRecalcChange};

pub(crate) use props::{CONTEXT_MAPPINGS_INFO, ImHashMap, style_key_selector};

/// A closure that maps context values to style properties.
type ContextMapFn = Rc<dyn Fn(Style, &Style) -> Style>;

/// Simple storage for context mapping closures.
/// Unlike the old ContextMappings, this only stores the closures themselves -
/// selector and inherited prop discovery happens via immediate evaluation.
///
/// Uses `Rc<Vec>` for O(1) clone - avoids copying the entire Vec when reading.
#[derive(Clone)]
pub(crate) struct ContextMappings(pub Rc<Vec<ContextMapFn>>);

style_key_selector!(selector_xs, StyleSelectors::new().responsive());
style_key_selector!(selector_sm, StyleSelectors::new().responsive());
style_key_selector!(selector_md, StyleSelectors::new().responsive());
style_key_selector!(selector_lg, StyleSelectors::new().responsive());
style_key_selector!(selector_xl, StyleSelectors::new().responsive());
style_key_selector!(selector_xxl, StyleSelectors::new().responsive());

pub(crate) fn screen_size_bp_to_key(breakpoint: ScreenSizeBp) -> StyleKey {
    match breakpoint {
        ScreenSizeBp::Xs => selector_xs(),
        ScreenSizeBp::Sm => selector_sm(),
        ScreenSizeBp::Md => selector_md(),
        ScreenSizeBp::Lg => selector_lg(),
        ScreenSizeBp::Xl => selector_xl(),
        ScreenSizeBp::Xxl => selector_xxl(),
    }
}

/// the bool in the return is a classes_applied flag. if a new class has been applied, we need to do a request_style_recursive
pub fn resolve_nested_maps(
    style: Style,
    interact_state: &InteractionState,
    screen_size_bp: ScreenSizeBp,
    classes: &[StyleClassRef],
    inherited_context: &Style,
    class_context: &Style,
) -> (Style, bool) {
    let mut classes_applied = false;

    // Phase_ 1: Resolve class styles (with selectors, collecting context mappings)
    let (class_style, mut class_context_mappings) = resolve_classes_collecting_mappings(
        classes,
        interact_state,
        screen_size_bp,
        class_context,
        &mut classes_applied,
    );

    // Phase 2: Resolve view's inline style (with selectors, collecting context mappings)
    let (view_style, mut view_context_mappings) =
        resolve_style_collecting_mappings(style, interact_state, screen_size_bp);

    // Phase 3: Apply class context mappings (with recursive resolution)
    // Use class_style as the base, context includes class_style + view_style
    let mut context_result = class_style.clone();
    let mut i = 0;
    while i < class_context_mappings.len() {
        let mapping = class_context_mappings[i].clone();
        let combined_context = inherited_context
            .clone()
            .apply(view_style.clone())
            .apply(context_result.clone());
        let mapped = mapping(context_result.clone(), &combined_context);
        let (resolved, new_mappings) =
            resolve_selectors_collecting_mappings(mapped, interact_state, screen_size_bp);
        context_result.apply_mut_no_mappings(resolved);
        class_context_mappings.splice(i + 1..i + 1, new_mappings);
        i += 1;
    }

    // Apply view style over the context result (view style wins)
    let mut result = context_result.apply(view_style);

    // Phase 4: Apply view context mappings over result (with recursive resolution)
    let mut i = 0;
    while i < view_context_mappings.len() {
        let mapping = view_context_mappings[i].clone();
        let combined_context = inherited_context.clone().apply(result.clone());
        let mapped = mapping(result.clone(), &combined_context);
        let (resolved, new_mappings) =
            resolve_selectors_collecting_mappings(mapped, interact_state, screen_size_bp);
        result.apply_mut_no_mappings(resolved);
        view_context_mappings.splice(i + 1..i + 1, new_mappings);
        i += 1;
    }

    (result, classes_applied)
}

fn resolve_classes_collecting_mappings(
    classes: &[StyleClassRef],
    interact_state: &InteractionState,
    screen_size_bp: ScreenSizeBp,
    class_context: &Style,
    classes_applied: &mut bool,
) -> (Style, Vec<ContextMapFn>) {
    let mut result = Style::new();
    let mut mappings = Vec::new();

    for class in classes {
        if let Some(map) = class_context.get_nested_map(class.key) {
            *classes_applied = true;
            let (resolved, class_mappings) =
                resolve_style_collecting_mappings(map.clone(), interact_state, screen_size_bp);
            result.apply_mut(resolved);
            mappings.extend(class_mappings);
        }
    }

    (result, mappings)
}

fn resolve_style_collecting_mappings(
    style: Style,
    interact_state: &InteractionState,
    screen_size_bp: ScreenSizeBp,
) -> (Style, Vec<ContextMapFn>) {
    let mut mappings = Vec::new();

    // Extract context mappings from style before resolving
    if let Some(style_mappings) = extract_context_mappings(&style) {
        mappings.extend(style_mappings);
    }

    // Resolve all selectors (and collect any new mappings found)
    let (resolved, selector_mappings) =
        resolve_selectors_collecting_mappings(style, interact_state, screen_size_bp);
    mappings.extend(selector_mappings);

    (resolved, mappings)
}

fn resolve_selectors_collecting_mappings(
    mut style: Style,
    interact_state: &InteractionState,
    screen_size_bp: ScreenSizeBp,
) -> (Style, Vec<ContextMapFn>) {
    const MAX_DEPTH: u32 = 20;
    let mut depth = 0;
    let mut all_mappings = Vec::new();

    loop {
        if depth >= MAX_DEPTH {
            break;
        }
        depth += 1;

        let mut changed = false;

        // Helper to apply a nested map and collect any context mappings from it
        let mut apply_nested = |style: &mut Style, key: StyleKey| -> bool {
            if let Some(map) = style.get_nested_map(key) {
                // Extract mappings before applying
                if let Some(mappings) = extract_context_mappings(&map) {
                    all_mappings.extend(mappings);
                }
                style.apply_mut_no_mappings(map);
                style.remove_nested_map(key);
                true
            } else {
                false
            }
        };

        // Apply screen size breakpoints
        if apply_nested(&mut style, screen_size_bp_to_key(screen_size_bp)) {
            changed = true;
        }

        // DarkMode
        if interact_state.is_dark_mode && apply_nested(&mut style, StyleSelector::DarkMode.to_key())
        {
            changed = true;
        }

        // Disabled state
        if interact_state.is_disabled || style.get(Disabled) {
            if apply_nested(&mut style, StyleSelector::Disabled.to_key()) {
                changed = true;
            }
        } else {
            // Selected
            if (interact_state.is_selected || style.get(Selected))
                && apply_nested(&mut style, StyleSelector::Selected.to_key())
            {
                changed = true;
            }

            // Hover
            if interact_state.is_hovered && apply_nested(&mut style, StyleSelector::Hover.to_key())
            {
                changed = true;
            }

            // File Hover
            if interact_state.is_file_hover
                && apply_nested(&mut style, StyleSelector::FileHover.to_key())
            {
                changed = true;
            }

            // Focus states
            if interact_state.is_focused {
                if apply_nested(&mut style, StyleSelector::Focus.to_key()) {
                    changed = true;
                }

                if interact_state.using_keyboard_navigation {
                    if apply_nested(&mut style, StyleSelector::FocusVisible.to_key()) {
                        changed = true;
                    }

                    if interact_state.is_clicking
                        && apply_nested(&mut style, StyleSelector::Active.to_key())
                    {
                        changed = true;
                    }
                }
            }

            // Active (mouse)
            if interact_state.is_clicking
                && !interact_state.using_keyboard_navigation
                && apply_nested(&mut style, StyleSelector::Active.to_key())
            {
                changed = true;
            }
        }

        if !changed {
            break;
        }
    }

    (style, all_mappings)
}

fn extract_context_mappings(style: &Style) -> Option<Vec<ContextMapFn>> {
    let key = StyleKey {
        info: &CONTEXT_MAPPINGS_INFO,
    };
    style.map.get(&key).map(|rc| {
        let mappings = rc.downcast_ref::<ContextMappings>().unwrap();
        mappings.0.iter().cloned().collect()
    })
}

#[derive(Default, Clone)]
pub struct Style {
    pub(crate) map: ImHashMap<StyleKey, Rc<dyn Any>>,
    /// Cached flag indicating whether this style contains any class maps.
    /// This enables O(1) early-exit in `apply_only_class_maps` for the common case
    /// where a view's style has no class definitions.
    has_class_maps: bool,
    /// Cached flag indicating whether this style contains any inherited properties.
    /// This enables O(1) early-exit in `apply_only_inherited` for the common case
    /// where a view's style has no inherited properties.
    has_inherited: bool,
}

impl Style {
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply only inherited properties from `from` style to `to` style.
    /// This is used during style propagation to pass inherited values to children.
    ///
    /// Only properties marked as `inherited: true` in their `StylePropInfo` are applied.
    /// This is more efficient than `apply_mut` when we only need to propagate
    /// inherited properties like font-size, color, etc.
    pub fn apply_only_inherited(to: &mut Rc<Style>, from: &Style) {
        if from.any_inherited() {
            let mut new_style = (**to).clone();
            // Only apply properties marked as inherited, not all properties
            let inherited = from.map.iter().filter(|(p, _)| p.inherited());
            new_style.apply_iter(inherited);
            *to = Rc::new(new_style);
        }
    }

    /// Apply inherited properties and class nested maps from `from` style to `to` style.
    ///
    /// This is used during style propagation to pass both inherited values and
    /// class definitions to children. Class nested maps (like `.class(ListItemClass, ...)`)
    /// need to flow to descendants so they can apply the styling when they have matching classes.
    pub fn apply_inherited_and_class_maps(to: &mut Rc<Style>, from: &Style) {
        let has_inherited = from.any_inherited();
        // O(1) check using cached flag
        let has_class_maps = from.has_class_maps;

        if has_inherited || has_class_maps {
            let mut new_style = (**to).clone();

            // Apply inherited properties
            if has_inherited {
                let inherited = from.map.iter().filter(|(p, _)| p.inherited());
                new_style.apply_iter(inherited);
            }

            // Apply class nested maps so they flow to descendants
            if has_class_maps {
                let class_maps = from
                    .map
                    .iter()
                    .filter(|(k, _)| matches!(k.info, StyleKeyInfo::Class(..)));
                new_style.apply_iter(class_maps);
            }

            *to = Rc::new(new_style);
        }
    }

    /// Apply only class nested maps from `from` style to `to` style.
    /// This is used during style propagation to pass class definitions to children.
    ///
    /// Only class nested maps (`.class(SomeClass, ...)`) are applied, not inherited props.
    pub fn apply_only_class_maps(to: &mut Rc<Style>, from: &Style) {
        // O(1) early exit for the common case where the style has no class maps
        if !from.has_class_maps {
            return;
        }
        let mut new_style = (**to).clone();
        let class_maps = from
            .map
            .iter()
            .filter(|(k, _)| matches!(k.info, StyleKeyInfo::Class(..)));
        new_style.apply_iter(class_maps);
        *to = Rc::new(new_style);
    }

    pub(crate) fn get_transition<P: StyleProp>(&self) -> Option<Transition> {
        self.map
            .get(&P::prop_ref().info().transition_key)
            .map(|v| v.downcast_ref::<Transition>().unwrap().clone())
    }

    pub(crate) fn get_prop_or_default<P: StyleProp>(&self) -> P::Type {
        self.get_prop::<P>().unwrap_or_else(|| P::default_value())
    }

    pub(crate) fn get_prop<P: StyleProp>(&self) -> Option<P::Type> {
        self.map.get(&P::key()).and_then(|v| {
            v.downcast_ref::<StyleMapValue<P::Type>>()
                .unwrap()
                .as_ref()
                .cloned()
        })
    }

    pub(crate) fn get_prop_style_value<P: StyleProp>(&self) -> StyleValue<P::Type> {
        self.map
            .get(&P::key())
            .map(
                |v| match v.downcast_ref::<StyleMapValue<P::Type>>().unwrap() {
                    StyleMapValue::Val(v) => StyleValue::Val(v.clone()),
                    StyleMapValue::Animated(v) => StyleValue::Animated(v.clone()),
                    StyleMapValue::Unset => StyleValue::Unset,
                },
            )
            .unwrap_or(StyleValue::Base)
    }

    pub(crate) fn style_props(&self) -> impl Iterator<Item = StylePropRef> + '_ {
        self.map.keys().filter_map(|p| match p.info {
            StyleKeyInfo::Prop(..) => Some(StylePropRef { key: *p }),
            _ => None,
        })
    }

    pub(crate) fn selectors(&self) -> StyleSelectors {
        let mut result = StyleSelectors::new();

        // Check for direct selectors
        for (k, v) in &self.map {
            if let StyleKeyInfo::Selector(selector) = k.info {
                result = result
                    .union(*selector)
                    .union(v.downcast_ref::<Style>().unwrap().selectors());
            }
        }
        result
    }

    /// Applies class styling from the context (inherited from ancestors) to this style.
    ///
    /// The view's own explicit styles take precedence over context class styles.
    /// This is achieved by applying context class styles first, then applying
    /// the view's own styles on top.
    ///
    /// Context mappings (from `with_context`/`with_theme`) from class styles are
    /// merged with the view's own context mappings. Class mappings run first (as
    /// defaults), then view's own mappings run (allowing overrides). This ensures
    /// that theme class styles can use `with_context` for context-aware styling
    /// (like toggle button height based on font size) while still allowing views
    /// to override via their own `with_context` closures.
    ///
    /// The returned boolean is true if a nested map was applied.
    pub fn apply_classes_from_context(
        self,
        classes: &[StyleClassRef],
        class_context: &std::rc::Rc<Style>,
    ) -> (Style, bool) {
        // Fast path: if no classes or no class maps in context, return unchanged
        if classes.is_empty() || !class_context.has_class_maps {
            return (self, false);
        }

        // Check if any of the classes actually have definitions in the context
        let has_matching_classes = classes
            .iter()
            .any(|class| class_context.get_nested_map(class.key).is_some());
        if !has_matching_classes {
            return (self, false);
        }

        let mut changed = false;

        let context_mappings_key = StyleKey {
            info: &CONTEXT_MAPPINGS_INFO,
        };

        // CSS-like specificity: inline styles (the view's own styles) have higher
        // specificity than class styles from ancestors. We achieve this by:
        // 1. Building up class styles as a base
        // 2. Applying the view's own styles on top
        //
        // Context mappings (from `with_context`/`with_theme`) from class styles are
        // merged with the view's own context mappings. Class mappings run first (as
        // defaults), then view's own mappings run (allowing overrides).

        // Save the view's own styles and context mappings
        let view_style = self;
        let view_ctx_mappings = view_style.map.get(&context_mappings_key).cloned();

        // Start with an empty style and build up from class styles
        let mut result = Style::new();
        let mut all_class_mappings: Vec<ContextMapFn> = Vec::new();

        for class in classes {
            if let Some(map) = class_context.get_nested_map(class.key) {
                let mut class_style = map.clone();
                // Extract class style's context mappings before applying other props
                if let Some(class_mappings_rc) = class_style.map.remove(&context_mappings_key) {
                    let class_mappings =
                        class_mappings_rc.downcast_ref::<ContextMappings>().unwrap();
                    all_class_mappings.extend(class_mappings.0.iter().cloned());
                }
                // Apply class style to result (later classes override earlier ones)
                result.apply_mut(class_style);
                changed = true;
            }
        }

        // Now apply view's own styles ON TOP of class styles (inline styles win)
        // We move the view_style into result to avoid cloning when possible
        result.apply_mut(view_style.clone());

        // Merge context mappings: class mappings FIRST (defaults), view's SECOND (overrides)
        // Only do selector extraction if there are class context mappings that could override them
        if !all_class_mappings.is_empty() {
            // CSS-like specificity fix: The view's inline selector styles (like .selected())
            // should override class styles, but class context mappings (like with_theme) run
            // AFTER this and might override them. To fix this, we wrap the view's selector
            // nested maps in a context mapping that runs LAST, after all class context mappings.
            let view_selector_keys: Vec<_> = view_style
                .map
                .keys()
                .filter(|k| matches!(k.info, StyleKeyInfo::Selector(..)))
                .cloned()
                .collect();

            if !view_selector_keys.is_empty() {
                let view_selectors: HashMap<StyleKey, Rc<dyn Any>> = view_selector_keys
                    .iter()
                    .filter_map(|k| view_style.map.get(k).map(|v| (*k, v.clone())))
                    .collect();

                // Add a context mapping that re-applies the view's selector styles LAST
                let restore_selectors: ContextMapFn = Rc::new(move |mut s: Style, _ctx: &Style| {
                    for (k, v) in view_selectors.iter() {
                        s.apply_iter(std::iter::once((k, v)));
                    }
                    s
                });
                all_class_mappings.push(restore_selectors);
            }

            // Add view's own context mappings AFTER class mappings (so view's override)
            if let Some(view_mappings_rc) = view_ctx_mappings {
                let view_mappings = view_mappings_rc.downcast_ref::<ContextMappings>().unwrap();
                all_class_mappings.extend(view_mappings.0.iter().cloned());
            }
            result.map.insert(
                context_mappings_key,
                Rc::new(ContextMappings(Rc::new(all_class_mappings))),
            );
        } else if let Some(view_mappings_rc) = view_ctx_mappings {
            // No class mappings, just preserve view's own context mappings
            result.map.insert(context_mappings_key, view_mappings_rc);
        }

        (result, changed)
    }

    pub fn apply_class<C: StyleClass>(mut self, _class: C) -> Style {
        if let Some(map) = self.map.get(&C::key()) {
            self.apply_mut(map.downcast_ref::<Style>().unwrap().clone());
        }
        self
    }

    pub fn apply_selectors(mut self, selectors: &[StyleSelector]) -> Style {
        for selector in selectors {
            if let Some(map) = self.get_nested_map(selector.to_key()) {
                self.apply_mut(map.apply_selectors(selectors));
            }
        }
        if self.get(Selected)
            && let Some(map) = self.get_nested_map(StyleSelector::Selected.to_key())
        {
            self.apply_mut(map.apply_selectors(&[StyleSelector::Selected]));
        }
        self
    }

    /// Apply a context-based style transformation.
    ///
    /// This evaluates the closure immediately with the context prop's default value
    /// (for selector and inherited prop discovery), and also stores the closure to
    /// be re-evaluated with actual ancestor context values during style resolution.
    ///
    /// Signal reactivity works because the outer style closure re-runs when signals change.
    pub fn with_context<P: StyleProp>(self, f: impl Fn(Self, &P::Type) -> Self + 'static) -> Self {
        // Evaluate immediately with default value for selector/inherited discovery.
        // This ensures selectors defined inside with_context are visible.
        let default_value = P::default_value();
        let result = f(self.clone(), &default_value);

        // Create a closure for context-aware re-evaluation during style resolution.
        // Cache default_value inside closure to avoid repeated allocations.
        let mapper: ContextMapFn = Rc::new(move |style: Style, context: &Style| {
            // Try getting the property from style first, then from context if not found
            let value = style.get_prop::<P>().or_else(|| {
                let prop_key = P::key();
                if let StyleKeyInfo::Prop(_) = prop_key.info {
                    context.get_prop::<P>()
                } else {
                    None
                }
            });

            if let Some(value) = value {
                f(style, &value)
            } else {
                f(style, &P::default_value())
            }
        });

        // Store the closure for later context resolution
        let key = StyleKey {
            info: &CONTEXT_MAPPINGS_INFO,
        };

        // Build new mappings vec - use Rc::make_mut for efficient copy-on-write
        let mut mappings_vec = result
            .map
            .get(&key)
            .and_then(|v| v.downcast_ref::<ContextMappings>())
            .map(|cm| (*cm.0).clone())
            .unwrap_or_default();
        mappings_vec.push(mapper);

        // Start with the immediate result (has selectors/inherited props)
        // but add our closure storage
        let mut final_result = result;
        final_result
            .map
            .insert(key, Rc::new(ContextMappings(Rc::new(mappings_vec))));
        final_result
    }

    /// Apply a context-based style transformation for optional props.
    ///
    /// Like `with_context`, this evaluates immediately with defaults for discovery,
    /// and stores the closure for context-aware re-evaluation.
    pub fn with_context_opt<P: StyleProp<Type = Option<T>>, T: 'static + Default>(
        self,
        f: impl Fn(Self, T) -> Self + 'static,
    ) -> Self {
        // Evaluate immediately with default T value for selector/inherited discovery
        let result = f(self.clone(), T::default());

        // Create a closure for context-aware re-evaluation
        let mapper: ContextMapFn = Rc::new(move |style: Style, context: &Style| {
            let value = style.get_prop::<P>().or_else(|| {
                let prop_key = P::key();
                if let StyleKeyInfo::Prop(_) = prop_key.info {
                    context.get_prop::<P>()
                } else {
                    None
                }
            });

            match value {
                Some(Some(value)) => f(style, value),
                _ => style,
            }
        });

        // Store the closure
        let key = StyleKey {
            info: &CONTEXT_MAPPINGS_INFO,
        };

        // Build new mappings vec efficiently
        let mut mappings_vec = result
            .map
            .get(&key)
            .and_then(|v| v.downcast_ref::<ContextMappings>())
            .map(|cm| (*cm.0).clone())
            .unwrap_or_default();
        mappings_vec.push(mapper);

        let mut final_result = result;
        final_result
            .map
            .insert(key, Rc::new(ContextMappings(Rc::new(mappings_vec))));
        final_result
    }

    pub(crate) fn get_nested_map(&self, key: StyleKey) -> Option<Style> {
        self.map
            .get(&key)
            .map(|map| map.downcast_ref::<Style>().unwrap().clone())
    }

    pub(crate) fn remove_nested_map(&mut self, key: StyleKey) -> Option<Style> {
        self.map
            .remove(&key)
            .map(|map| map.downcast_ref::<Style>().unwrap().clone())
    }

    /// Check if this style has any inherited properties.
    /// Used to determine if children should be re-styled when this view's style changes.
    /// O(1) using cached flag.
    pub(crate) fn any_inherited(&self) -> bool {
        self.has_inherited
    }

    pub(crate) fn inherited(&self) -> Style {
        let mut new = Style::new();
        if self.any_inherited() {
            let inherited = self.map.iter().filter(|(p, _)| p.inherited());

            new.apply_iter(inherited);
        }
        new
    }

    fn set_selector(&mut self, selector: StyleSelector, map: Style) {
        self.set_map_selector(selector.to_key(), map)
    }

    fn set_map_selector(&mut self, key: StyleKey, map: Style) {
        match self.map.entry(key) {
            Entry::Occupied(mut e) => {
                let mut current = e.get_mut().downcast_ref::<Style>().unwrap().clone();
                current.apply_mut(map);
                *e.get_mut() = Rc::new(current);
            }
            Entry::Vacant(e) => {
                e.insert(Rc::new(map));
            }
        }
    }

    fn set_breakpoint(&mut self, breakpoint: ScreenSizeBp, map: Style) {
        self.set_map_selector(screen_size_bp_to_key(breakpoint), map)
    }

    fn set_class(&mut self, class: StyleClassRef, map: Style) {
        self.has_class_maps = true;
        self.set_map_selector(class.key, map)
    }

    pub fn builtin(&self) -> BuiltinStyle<'_> {
        BuiltinStyle { style: self }
    }

    pub(crate) fn apply_iter<'a>(
        &mut self,
        iter: impl Iterator<Item = (&'a StyleKey, &'a Rc<dyn Any>)>,
    ) {
        for (k, v) in iter {
            match k.info {
                StyleKeyInfo::Class(..) | StyleKeyInfo::Selector(..) => {
                    // Track class maps for O(1) early-exit in apply_only_class_maps
                    if matches!(k.info, StyleKeyInfo::Class(..)) {
                        self.has_class_maps = true;
                    }
                    match self.map.entry(*k) {
                        Entry::Occupied(mut e) => {
                            // We need to merge the new map with the existing map.

                            let v = v.downcast_ref::<Style>().unwrap();
                            match Rc::get_mut(e.get_mut()) {
                                Some(current) => {
                                    current
                                        .downcast_mut::<Style>()
                                        .unwrap()
                                        .apply_mut(v.clone());
                                }
                                None => {
                                    let mut current =
                                        e.get_mut().downcast_ref::<Style>().unwrap().clone();
                                    current.apply_mut(v.clone());
                                    *e.get_mut() = Rc::new(current);
                                }
                            }
                        }
                        Entry::Vacant(e) => {
                            e.insert(v.clone());
                        }
                    }
                }
                StyleKeyInfo::ContextMappings => match self.map.entry(*k) {
                    Entry::Occupied(mut e) => {
                        // Merge the new ContextMappings with existing ones
                        let new_ctx = v.downcast_ref::<ContextMappings>().unwrap();
                        let current = e.get().downcast_ref::<ContextMappings>().unwrap();
                        // Build merged Vec - can't mutate through Rc, so create new
                        let mut merged: Vec<_> = (*current.0).clone();
                        merged.extend(new_ctx.0.iter().cloned());
                        *e.get_mut() = Rc::new(ContextMappings(Rc::new(merged)));
                    }
                    Entry::Vacant(e) => {
                        e.insert(v.clone());
                    }
                },
                StyleKeyInfo::Transition => {
                    self.map.insert(*k, v.clone());
                }
                StyleKeyInfo::Prop(info) => {
                    // Track inherited props for O(1) early-exit in apply_only_inherited
                    if info.inherited {
                        self.has_inherited = true;
                    }
                    match self.map.entry(*k) {
                        Entry::Occupied(mut e) => {
                            // We need to merge the new map with the existing map.
                            e.insert((info.combine)(e.get().clone(), v.clone()));
                        }
                        Entry::Vacant(e) => {
                            e.insert(v.clone());
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn apply_iter_no_mappings<'a>(
        &mut self,
        iter: impl Iterator<Item = (&'a StyleKey, &'a Rc<dyn Any>)>,
    ) {
        for (k, v) in iter {
            match k.info {
                StyleKeyInfo::Class(..) | StyleKeyInfo::Selector(..) => {
                    // Track class maps for O(1) early-exit in apply_only_class_maps
                    if matches!(k.info, StyleKeyInfo::Class(..)) {
                        self.has_class_maps = true;
                    }
                    match self.map.entry(*k) {
                        Entry::Occupied(mut e) => {
                            // We need to merge the new map with the existing map.

                            let v = v.downcast_ref::<Style>().unwrap();
                            match Rc::get_mut(e.get_mut()) {
                                Some(current) => {
                                    current
                                        .downcast_mut::<Style>()
                                        .unwrap()
                                        .apply_mut(v.clone());
                                }
                                None => {
                                    let mut current =
                                        e.get_mut().downcast_ref::<Style>().unwrap().clone();
                                    current.apply_mut(v.clone());
                                    *e.get_mut() = Rc::new(current);
                                }
                            }
                        }
                        Entry::Vacant(e) => {
                            e.insert(v.clone());
                        }
                    }
                }
                StyleKeyInfo::Transition => {
                    self.map.insert(*k, v.clone());
                }
                StyleKeyInfo::Prop(info) => {
                    // Track inherited props for O(1) early-exit in apply_only_inherited
                    if info.inherited {
                        self.has_inherited = true;
                    }
                    match self.map.entry(*k) {
                        Entry::Occupied(mut e) => {
                            // We need to merge the new map with the existing map.
                            e.insert((info.combine)(e.get().clone(), v.clone()));
                        }
                        Entry::Vacant(e) => {
                            e.insert(v.clone());
                        }
                    }
                }
                _ => {}
            }
        }
    }

    pub(crate) fn apply_mut(&mut self, over: Style) {
        self.apply_iter(over.map.iter());
    }

    pub(crate) fn apply_mut_no_mappings(&mut self, over: Style) {
        self.apply_iter_no_mappings(over.map.iter());
    }

    /// Apply another `Style` to this style, returning a new `Style` with the overrides
    ///
    /// `StyleValue::Val` will override the value with the given value
    /// `StyleValue::Unset` will unset the value, causing it to fall back to the default.
    /// `StyleValue::Base` will leave the value as-is, whether falling back to the default
    /// or using the value in the `Style`.
    pub fn apply(mut self, over: Style) -> Style {
        self.apply_mut(over);
        self
    }

    pub fn map(self, over: impl FnOnce(Self) -> Self) -> Self {
        over(self)
    }

    /// Apply multiple `Style`s to this style, returning a new `Style` with the overrides.
    /// Later styles take precedence over earlier styles.
    pub fn apply_overriding_styles(self, overrides: impl Iterator<Item = Style>) -> Style {
        overrides.fold(self, |acc, x| acc.apply(x))
    }

    // /// Apply context mappings with the given context (inherited props from ancestors).
    // /// Returns the style with context values applied and a flag indicating if changes were made.
    // ///
    // /// Uses iterative approach instead of recursion for better performance.
    // pub(crate) fn apply_context_mappings(mut self, context: &Style) -> (Self, bool) {
    //     let key = StyleKey {
    //         info: &CONTEXT_MAPPINGS_INFO,
    //     };
    //     let mut changed = false;

    //     // Iterative approach: keep processing until no more context mappings
    //     loop {
    //         // Single lookup: use remove directly instead of get + remove
    //         let ctx_mappings = self
    //             .map
    //             .remove(&key)
    //             .and_then(|v| v.downcast_ref::<ContextMappings>().cloned());

    //         match ctx_mappings {
    //             Some(mappings) => {
    //                 changed = true;
    //                 // Iterate over Rc<Vec> - no allocation needed
    //                 for mapping in mappings.0.iter() {
    //                     self = mapping(self, context);
    //                 }
    //             }
    //             None => break,
    //         }
    //     }

    //     (self, changed)
    // }
}

impl Debug for Style {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Style")
            .field(
                "map",
                &self
                    .map
                    .iter()
                    .map(|(p, v)| (*p, (p.debug_any(&**v))))
                    .collect::<HashMap<StyleKey, String>>(),
            )
            .finish()
    }
}

style_key_selector!(hover, StyleSelectors::new().set(StyleSelector::Hover, true));
style_key_selector!(
    file_hover,
    StyleSelectors::new().set(StyleSelector::FileHover, true)
);
style_key_selector!(focus, StyleSelectors::new().set(StyleSelector::Focus, true));
style_key_selector!(
    focus_visible,
    StyleSelectors::new().set(StyleSelector::FocusVisible, true)
);
style_key_selector!(
    disabled,
    StyleSelectors::new().set(StyleSelector::Disabled, true)
);
style_key_selector!(
    active,
    StyleSelectors::new().set(StyleSelector::Active, true)
);
style_key_selector!(
    dragging,
    StyleSelectors::new().set(StyleSelector::Dragging, true)
);
style_key_selector!(
    selected,
    StyleSelectors::new().set(StyleSelector::Selected, true)
);
style_key_selector!(
    darkmode,
    StyleSelectors::new().set(StyleSelector::DarkMode, true)
);

impl StyleSelector {
    fn to_key(self) -> StyleKey {
        match self {
            StyleSelector::Hover => hover(),
            StyleSelector::Focus => focus(),
            StyleSelector::FocusVisible => focus_visible(),
            StyleSelector::Disabled => disabled(),
            StyleSelector::Active => active(),
            StyleSelector::Dragging => dragging(),
            StyleSelector::Selected => selected(),
            StyleSelector::DarkMode => darkmode(),
            StyleSelector::FileHover => file_hover(),
        }
    }
}

/// Defines built-in style properties with optional builder methods.
///
/// Properties can be marked with flags in braces:
/// - `nocb` (no callback/no chain builder) - no fluent builder method generated
/// - `tr` (transition) - generates a `transition_property_name()` method
///
/// Examples: `name: Type {}`, `name {nocb}: Type {}`, `name {tr}: Type {}`, `name {nocb, tr}: Type {}`
///
/// All properties get:
/// - A getter method in `BuiltinStyle`
/// - An `unset_property_name()` method
macro_rules! define_builtin_props {
    (
        $(
            $(#[$meta:meta])*
            $type_name:ident $name:ident $({ $($flags:ident),* })? :
            $typ:ty { $($options:tt)* } = $val:expr
        ),*
        $(,)?
    ) => {
        $(
            prop!($(#[$meta])* pub $type_name: $typ { $($options)* } = $val);
        )*
        impl Style {
            $(
                define_builtin_props!(decl: $(#[$meta])* $type_name $name $({ $($flags),* })?: $typ = $val);
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
    };

    // With flags - check if nocb is present
    (decl: $(#[$meta:meta])* $type_name:ident $name:ident { $($flags:ident),* }: $typ:ty = $val:expr) => {
        define_builtin_props!(@check_nocb $(#[$meta])* $type_name $name [$($flags)*]: $typ);
    };

    // Without flags - always generate setter
    (decl: $(#[$meta:meta])* $type_name:ident $name:ident: $typ:ty = $val:expr) => {
        $(#[$meta])*
        pub fn $name(self, v: impl Into<$typ>) -> Self {
            self.set($type_name, v.into())
        }
    };

    // Helper: if nocb found, don't generate setter
    (@check_nocb $(#[$meta:meta])* $type_name:ident $name:ident [nocb $($rest:ident)*]: $typ:ty) => {};
    (@check_nocb $(#[$meta:meta])* $type_name:ident $name:ident [$first:ident $($rest:ident)*]: $typ:ty) => {
        define_builtin_props!(@check_nocb $(#[$meta])* $type_name $name [$($rest)*]: $typ);
    };
    (@check_nocb $(#[$meta:meta])* $type_name:ident $name:ident []: $typ:ty) => {
        // No nocb found, generate the setter
        $(#[$meta])*
        pub fn $name(self, v: impl Into<$typ>) -> Self {
            self.set($type_name, v.into())
        }
    };

    // Unset method - generated for all properties
    (unset: $(#[$meta:meta])* $type_name:ident $name:ident) => {
        paste::paste! {
            #[doc = "Unsets the `" $name "` property."]
            pub fn [<unset_ $name>](self) -> Self {
                self.set_style_value($type_name, $crate::style::StyleValue::Unset)
            }
        }
    };

    // Transition method - with flags, check if 'tr' is present
    (transition: $(#[$meta:meta])* $type_name:ident $name:ident { $($flags:ident),* }) => {
        define_builtin_props!(@check_tr $(#[$meta])* $type_name $name [$($flags)*]);
    };

    // Transition method - without flags, don't generate
    (transition: $(#[$meta:meta])* $type_name:ident $name:ident) => {};

    // Helper: if tr found, generate transition method
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
    (@check_tr $(#[$meta:meta])* $type_name:ident $name:ident []) => {
        // No tr flag found, don't generate transition method
    };
}

pub struct BuiltinStyle<'a> {
    style: &'a Style,
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
    Width width {tr}: PxPctAuto {} = PxPctAuto::Auto,

    /// Sets the height of the view.
    ///
    /// Can be specified in pixels, percentages, or auto.
    Height height {tr}: PxPctAuto {} = PxPctAuto::Auto,

    /// Sets the minimum width of the view.
    ///
    /// The view will not shrink below this width.
    MinWidth min_width {tr}: PxPctAuto {} = PxPctAuto::Auto,

    /// Sets the minimum height of the view.
    ///
    /// The view will not shrink below this height.
    MinHeight min_height {tr}: PxPctAuto {} = PxPctAuto::Auto,

    /// Sets the maximum width of the view.
    ///
    /// The view will not grow beyond this width.
    MaxWidth max_width {tr}: PxPctAuto {} = PxPctAuto::Auto,

    /// Sets the maximum height of the view.
    ///
    /// The view will not grow beyond this height.
    MaxHeight max_height {tr}: PxPctAuto {} = PxPctAuto::Auto,

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
    FlexBasis flex_basis {tr}: PxPctAuto {} = PxPctAuto::Auto,

    /// Controls alignment of flex items along the main axis.
    ///
    /// Determines how extra space is distributed between and around items.
    JustifyContentProp justify_content {}: Option<JustifyContent> {} = None,

    /// Controls default alignment of grid items along the inline axis.
    ///
    /// Sets the default justify-self value for all items in the container.
    JustifyItemsProp justify_items {}: Option<JustifyItems> {} = None,

    /// Controls how the total width and height are calculated.
    ///
    /// Determines whether borders and padding are included in the view's size.
    BoxSizingProp box_sizing {}: Option<BoxSizing> {} = None,

    /// Controls individual alignment along the inline axis.
    ///
    /// Overrides the container's justify-items value for this specific item.
    JustifySelf justify_self {}: Option<AlignItems> {} = None,

    /// Controls alignment of flex items along the cross axis.
    ///
    /// Determines how items are aligned when they don't fill the container's cross axis.
    AlignItemsProp align_items {}: Option<AlignItems> {} = None,

    /// Controls alignment of wrapped flex lines.
    ///
    /// Only has an effect when flex-wrap is enabled and there are multiple lines.
    AlignContentProp align_content {}: Option<AlignContent> {} = None,

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
    AlignSelf align_self {}: Option<AlignItems> {} = None,

    /// Sets the color of the view's outline.
    ///
    /// The outline is drawn outside the border and doesn't affect layout.
    OutlineColor outline_color {tr}: Brush {} = Brush::Solid(palette::css::TRANSPARENT),

    /// Sets the outline stroke properties.
    ///
    /// Defines the width, style, and other properties of the outline.
    Outline outline {nocb, tr}: StrokeWrap {} = StrokeWrap::new(0.),

    /// Controls the progress/completion of the outline animation.
    ///
    /// Useful for creating animated outline effects.
    OutlineProgress outline_progress {tr}: Pct {} = Pct(100.),

    /// Controls the progress/completion of the border animation.
    ///
    /// Useful for creating animated border effects.
    BorderProgress border_progress {tr}: Pct {} = Pct(100.),

    /// Sets the border properties for all sides.
    ///
    /// Defines width, style, and other border characteristics.
    BorderProp border_combined {nocb, tr}: Border {} = Border::default(),

    /// Sets the border color for all sides.
    ///
    /// Can be set individually for each side or all at once.
    BorderColorProp border_color_combined { nocb, tr }: BorderColor {} = BorderColor::default(),

    /// Sets the border radius for all corners.
    ///
    /// Controls how rounded the corners of the view are.
    BorderRadiusProp border_radius_combined { nocb, tr }: BorderRadius {} = BorderRadius::default(),

    /// Sets the padding for all sides.
    ///
    /// Padding is the space between the view's content and its border.
    PaddingProp padding_combined { nocb, tr }: Padding {} = Padding::default(),

    /// Sets the margin for all sides.
    ///
    /// Margin is the space outside the view's border.
    MarginProp margin_combined { nocb, tr }: Margin {} = Margin::default(),

    /// Sets the left offset for positioned views.
    InsetLeft inset_left {tr}: PxPctAuto {} = PxPctAuto::Auto,

    /// Sets the top offset for positioned views.
    InsetTop inset_top {tr}: PxPctAuto {} = PxPctAuto::Auto,

    /// Sets the right offset for positioned views.
    InsetRight inset_right {tr}: PxPctAuto {} = PxPctAuto::Auto,

    /// Sets the bottom offset for positioned views.
    InsetBottom inset_bottom {tr}: PxPctAuto {} = PxPctAuto::Auto,

    /// Controls whether the view can be the target of mouse events.
    ///
    /// When disabled, mouse events pass through to views behind.
    PointerEventsProp pointer_events {}: Option<PointerEvents> { inherited } = None,

    /// Controls the stack order of positioned views.
    ///
    /// Higher values appear in front of lower values.
    ZIndex z_index { nocb, tr }: Option<i32> {} = None,

    /// Sets the cursor style when hovering over the view.
    ///
    /// Changes the appearance of the mouse cursor.
    Cursor cursor { nocb }: Option<CursorStyle> {} = None,

    /// Sets the text color.
    ///
    /// This property is inherited by child views.
    TextColor color { nocb, tr }: Option<Color> { inherited } = None,

    /// Sets the background color or image.
    ///
    /// Can be a solid color, gradient, or image.
    Background background { nocb, tr }: Option<Brush> {} = None,

    /// Sets the foreground color or pattern.
    ///
    /// Used for drawing content like icons or shapes.
    Foreground foreground { nocb, tr }: Option<Brush> {} = None,

    /// Adds one or more drop shadows to the view.
    ///
    /// Can create depth and visual separation effects.
    BoxShadowProp box_shadow { nocb, tr }: SmallVec<[BoxShadow; 3]> {} = SmallVec::new(),

    /// Sets the font size for text content.
    ///
    /// This property is inherited by child views.
    FontSize font_size { nocb, tr }: Option<f32> { inherited } = None,

    /// Sets the font family for text content.
    ///
    /// This property is inherited by child views.
    FontFamily font_family { nocb }: Option<String> { inherited } = None,

    /// Sets the font weight (boldness) for text content.
    ///
    /// This property is inherited by child views.
    FontWeight font_weight { nocb }: Option<Weight> { inherited } = None,

    /// Sets the font style (italic, normal) for text content.
    ///
    /// This property is inherited by child views.
    FontStyle font_style { nocb }: Option<crate::text::Style> { inherited } = None,

    /// Sets the color of the text cursor.
    ///
    /// Visible when text input views have focus.
    CursorColor cursor_color { nocb, tr }: Brush {} = Brush::Solid(palette::css::BLACK.with_alpha(0.3)),

    /// Sets the corner radius of text selections.
    ///
    /// Controls how rounded the corners of selected text appear.
    SelectionCornerRadius selection_corer_radius { nocb, tr }: f64 {} = 1.,

    /// Controls whether the view's text can be selected.
    ///
    /// This property is inherited by child views.
    Selectable selectable {}: bool { inherited } = true,

    /// Controls how overflowed text content is handled.
    ///
    /// Determines whether text wraps or gets clipped.
    TextOverflowProp text_overflow {}: TextOverflow {} = TextOverflow::Wrap,

    /// Sets text alignment within the view.
    ///
    /// Controls horizontal alignment of text content.
    TextAlignProp text_align {}: Option<crate::text::Align> {} = None,

    /// Sets the line height for text content.
    ///
    /// This property is inherited by child views.
    LineHeight line_height { nocb, tr }: Option<LineHeightValue> { inherited } = None,

    /// Sets the preferred aspect ratio for the view.
    ///
    /// Maintains width-to-height proportions during layout.
    AspectRatio aspect_ratio {tr}: Option<f32> {} = None,

    /// Sets the gap between columns in grid or flex layouts.
    ///
    /// Creates space between items in the horizontal direction.
    ColGap col_gap { nocb, tr }: PxPct {} = PxPct::Px(0.),

    /// Sets the gap between rows in grid or flex layouts.
    ///
    /// Creates space between items in the vertical direction.
    RowGap row_gap { nocb, tr }: PxPct {} = PxPct::Px(0.),

    /// Sets the horizontal scale transform.
    ///
    /// Values less than 100% shrink the view, greater than 100% enlarge it.
    ScaleX scale_x {tr}: Pct {} = Pct(100.),

    /// Sets the vertical scale transform.
    ///
    /// Values less than 100% shrink the view, greater than 100% enlarge it.
    ScaleY scale_y {tr}: Pct {} = Pct(100.),

    /// Sets the horizontal translation transform.
    ///
    /// Moves the view left (negative) or right (positive).
    TranslateX translate_x {tr}: PxPct {} = PxPct::Px(0.),

    /// Sets the vertical translation transform.
    ///
    /// Moves the view up (negative) or down (positive).
    TranslateY translate_y {tr}: PxPct {} = PxPct::Px(0.),

    /// Sets the rotation transform angle.
    ///
    /// Positive values rotate clockwise, negative values rotate counter-clockwise.
    /// Use `.deg()` or `.rad()` methods to specify the angle unit.
    Rotation rotate {tr}: Angle {} = Angle::Rad(0.0),

    /// Sets the anchor point for rotation transformations.
    ///
    /// Determines the point around which the view rotates. Use predefined constants
    /// like `AnchorAbout::CENTER` or create custom anchor points with pixel or percentage values.
    RotateAbout rotate_about {}: AnchorAbout {} = AnchorAbout::CENTER,

    /// Sets the anchor point for scaling transformations.
    ///
    /// Determines the point around which the view scales. Use predefined constants
    /// like `AnchorAbout::CENTER` or create custom anchor points with pixel or percentage values.
    ScaleAbout scale_about {tr}: AnchorAbout {} = AnchorAbout::CENTER,

    /// Sets the opacity of the view.
    ///
    /// Values range from 0.0 (fully transparent) to 1.0 (fully opaque).
    /// This affects the entire view including its children.
    Opacity opacity {tr}: f32 {} = 1.0,

    /// Sets the selected state of the view.
    ///
    /// This property is inherited by child views.
    Selected set_selected {}: bool { inherited } = false,

    /// Controls the disabled state of the view.
    ///
    /// This property is inherited by child views.
    Disabled set_disabled {}: bool { inherited } = false,

    /// Controls whether the view can receive focus.
    ///
    /// Focus is necessary for keyboard interaction.
    Focusable focusable {}: bool { } = false,

    /// Controls whether the view can be dragged.
    ///
    /// Enables drag-and-drop functionality for the view.
    Draggable draggable {}: bool { } = false,
);

impl BuiltinStyle<'_> {
    // Individual padding accessors
    pub fn padding_left(&self) -> PxPct {
        self.style.get(PaddingProp).left.unwrap_or(PxPct::Px(0.0))
    }
    pub fn padding_top(&self) -> PxPct {
        self.style.get(PaddingProp).top.unwrap_or(PxPct::Px(0.0))
    }
    pub fn padding_right(&self) -> PxPct {
        self.style.get(PaddingProp).right.unwrap_or(PxPct::Px(0.0))
    }
    pub fn padding_bottom(&self) -> PxPct {
        self.style.get(PaddingProp).bottom.unwrap_or(PxPct::Px(0.0))
    }

    // Individual margin accessors
    pub fn margin_left(&self) -> PxPctAuto {
        self.style
            .get(MarginProp)
            .left
            .unwrap_or(PxPctAuto::Px(0.0))
    }
    pub fn margin_top(&self) -> PxPctAuto {
        self.style.get(MarginProp).top.unwrap_or(PxPctAuto::Px(0.0))
    }
    pub fn margin_right(&self) -> PxPctAuto {
        self.style
            .get(MarginProp)
            .right
            .unwrap_or(PxPctAuto::Px(0.0))
    }
    pub fn margin_bottom(&self) -> PxPctAuto {
        self.style
            .get(MarginProp)
            .bottom
            .unwrap_or(PxPctAuto::Px(0.0))
    }
}

prop!(
    /// How children overflowing their container in Y axis should affect layout
    pub OverflowX: Overflow {} = Overflow::default()
);

prop!(
    /// How children overflowing their container in X axis should affect layout
    pub OverflowY: Overflow {} = Overflow::default()
);

prop_extractor! {
    pub FontProps {
        pub size: FontSize,
        pub family: FontFamily,
        pub weight: FontWeight,
        pub style: FontStyle,
    }
}

prop_extractor! {
    pub(crate) LayoutProps {
        pub border: BorderProp,
        pub padding: PaddingProp,
        pub margin: MarginProp,

        pub width: Width,
        pub height: Height,

        pub min_width: MinWidth,
        pub min_height: MinHeight,

        pub max_width: MaxWidth,
        pub max_height: MaxHeight,

        pub flex_grow: FlexGrow,
        pub flex_shrink: FlexShrink,
        pub flex_basis: FlexBasis ,

        pub inset_left: InsetLeft,
        pub inset_top: InsetTop,
        pub inset_right: InsetRight,
        pub inset_bottom: InsetBottom,

        pub row_gap: RowGap,
        pub col_gap: ColGap,
    }
}

prop_extractor! {
    pub TransformProps {
        pub scale_x: ScaleX,
        pub scale_y: ScaleY,

        pub translate_x: TranslateX,
        pub translate_y: TranslateY,

        pub rotation: Rotation,
        pub rotate_about: RotateAbout,
        pub scale_about: ScaleAbout,
    }
}
impl TransformProps {
    pub fn affine(&self, size: kurbo::Size) -> Affine {
        let mut transform = Affine::IDENTITY;

        let transform_x = match self.translate_x() {
            crate::unit::PxPct::Px(px) => px,
            crate::unit::PxPct::Pct(pct) => pct / 100.,
        };
        let transform_y = match self.translate_y() {
            crate::unit::PxPct::Px(px) => px,
            crate::unit::PxPct::Pct(pct) => pct / 100.,
        };
        transform *= Affine::translate(Vec2 {
            x: transform_x,
            y: transform_y,
        });

        let scale_x = self.scale_x().0 / 100.;
        let scale_y = self.scale_y().0 / 100.;
        let rotation = self.rotation().to_radians();

        // Get rotation and scale anchor points
        let rotate_about = self.rotate_about();
        let scale_about = self.scale_about();

        // Convert anchor points to fractional positions
        let (rotate_x_frac, rotate_y_frac) = rotate_about.as_fractions();
        let (scale_x_frac, scale_y_frac) = scale_about.as_fractions();

        let rotate_point = Vec2 {
            x: rotate_x_frac * size.width,
            y: rotate_y_frac * size.height,
        };

        let scale_point = Vec2 {
            x: scale_x_frac * size.width,
            y: scale_y_frac * size.height,
        };

        // Apply transformations using the specified anchor points
        if scale_x != 1.0 || scale_y != 1.0 {
            // Manual non-uniform scaling about a point: translate -> scale -> translate back
            let scale_center = scale_point;
            transform = transform
                .then_translate(-scale_center)
                .then_scale_non_uniform(scale_x, scale_y)
                .then_translate(scale_center);
        }
        if rotation != 0.0 {
            // Manual rotation about a point: translate -> rotate -> translate back
            let rotate_center = rotate_point;
            transform = transform
                .then_translate(-rotate_center)
                .then_rotate(rotation)
                .then_translate(rotate_center);
        }

        transform
    }
}

prop_extractor! {
    pub BoxTreeProps {
        pub scale_about: ScaleAbout,
        pub z_index: ZIndex,
        pub pointer_events: PointerEventsProp,
        pub focusable: Focusable,
        pub disabled: Disabled,
        pub display: DisplayProp,
        pub overflow_x: OverflowX,
        pub overflow_y: OverflowY,
        pub border_radius: BorderRadiusProp,
    }
}
impl BoxTreeProps {
    pub fn pickable(&self) -> bool {
        self.pointer_events() != Some(PointerEvents::None)
    }

    // pub fn set_box_tree(
    //     &self,
    //     node_id: understory_box_tree::NodeId,
    //     box_tree: &mut understory_box_tree::Tree,
    // ) {
    //     box_tree.set_z_index(node_id, self.z_index().unwrap_or(0));
    //     let mut flags = NodeFlags::empty();
    //     if self.pickable() {
    //         flags |= NodeFlags::PICKABLE;
    //     }
    //     if self.focusable() && !self.hidden() && self.display() != Display::None && !self.disabled()
    //     {
    //         flags |= NodeFlags::FOCUSABLE;
    //     }
    //     if !self.hidden() {
    //         flags |= NodeFlags::VISIBLE;
    //     }
    //     box_tree.set_flags(node_id, flags);
    // }

    pub fn clip_rect(&self, mut rect: kurbo::Rect) -> Option<RoundedRect> {
        use Overflow::*;

        let (overflow_x, overflow_y) = (self.overflow_x(), self.overflow_y());

        // No clipping if both are visible
        if overflow_x == Visible && overflow_y == Visible {
            return None;
        }

        let border_radius = self
            .border_radius()
            .resolve_border_radii(rect.size().min_side());

        // Extend to infinity on visible axes
        if overflow_x == Visible {
            rect.x0 = f64::NEG_INFINITY;
            rect.x1 = f64::INFINITY;
        }
        if overflow_y == Visible {
            rect.y0 = f64::NEG_INFINITY;
            rect.y1 = f64::INFINITY;
        }

        Some(RoundedRect::from_rect(rect, border_radius))
    }
}

impl LayoutProps {
    pub fn to_style(&self) -> Style {
        let border = self.border();
        let padding = self.padding();
        let margin = self.margin();
        Style::new()
            .width(self.width())
            .height(self.height())
            .apply_border(border)
            .apply_padding(padding)
            .min_width(self.min_width())
            .min_height(self.min_height())
            .max_width(self.max_width())
            .max_height(self.max_height())
            .flex_grow(self.flex_grow())
            .flex_shrink(self.flex_shrink())
            .flex_basis(self.flex_basis())
            .inset_left(self.inset_left())
            .inset_top(self.inset_top())
            .inset_right(self.inset_right())
            .inset_bottom(self.inset_bottom())
            .apply_margin(margin)
            .col_gap(self.col_gap())
            .row_gap(self.row_gap())
    }
}

prop_extractor! {
    pub SelectionStyle {
        pub corner_radius: SelectionCornerRadius,
        pub selection_color: CursorColor,
    }
}

impl Style {
    /// Gets the value of a style property, returning the default if not set.
    pub fn get<P: StyleProp>(&self, _prop: P) -> P::Type {
        self.get_prop_or_default::<P>()
    }

    /// Gets the raw style value of a property, including unset and base states.
    pub fn get_style_value<P: StyleProp>(&self, _prop: P) -> StyleValue<P::Type> {
        self.get_prop_style_value::<P>()
    }

    /// Sets a style property to a specific value.
    pub fn set<P: StyleProp>(self, prop: P, value: impl Into<P::Type>) -> Self {
        self.set_style_value(prop, StyleValue::Val(value.into()))
    }

    pub fn set_style_value<P: StyleProp>(mut self, _prop: P, value: StyleValue<P::Type>) -> Self {
        let insert = match value {
            StyleValue::Val(value) => StyleMapValue::Val(value),
            StyleValue::Animated(value) => StyleMapValue::Animated(value),
            StyleValue::Unset => StyleMapValue::Unset,
            StyleValue::Base => {
                self.map.remove(&P::key());
                return self;
            }
        };
        // Track inherited props for O(1) early-exit in apply_only_inherited
        if P::prop_ref().info().inherited {
            self.has_inherited = true;
        }
        self.map.insert(P::key(), Rc::new(insert));
        self
    }

    /// Sets a transition animation for a specific style property.
    pub fn transition<P: StyleProp>(mut self, _prop: P, transition: Transition) -> Self {
        self.map
            .insert(P::prop_ref().info().transition_key, Rc::new(transition));
        self
    }

    fn selector(mut self, selector: StyleSelector, style: impl FnOnce(Style) -> Style) -> Self {
        let over = style(Style::default());
        self.set_selector(selector, over);
        self
    }

    /// The visual style to apply when the mouse hovers over the view
    pub fn hover(self, style: impl FnOnce(Style) -> Style) -> Self {
        self.selector(StyleSelector::Hover, style)
    }

    /// The visual style to apply when the view has keyboard focus.
    pub fn focus(self, style: impl FnOnce(Style) -> Style) -> Self {
        self.selector(StyleSelector::Focus, style)
    }

    /// Similar to the `:focus-visible` css selector, this style only activates when tab navigation is used.
    pub fn focus_visible(self, style: impl FnOnce(Style) -> Style) -> Self {
        self.selector(StyleSelector::FocusVisible, style)
    }

    /// The visual style to apply when the view is in a selected state.
    pub fn selected(self, style: impl FnOnce(Style) -> Style) -> Self {
        self.selector(StyleSelector::Selected, style)
    }

    /// The visual style to apply when the view is being dragged
    pub fn drag(self, style: impl FnOnce(Style) -> Style) -> Self {
        self.selector(StyleSelector::Dragging, style)
    }

    /// The visual style to apply when the view is disabled.
    pub fn disabled(self, style: impl FnOnce(Style) -> Style) -> Self {
        self.selector(StyleSelector::Disabled, style)
    }

    /// The visual style to apply when the application is in dark mode.
    pub fn dark_mode(self, style: impl FnOnce(Style) -> Style) -> Self {
        self.selector(StyleSelector::DarkMode, style)
    }

    /// The visual style to apply when a file is being dragged over the view.
    pub fn file_hover(self, style: impl FnOnce(Style) -> Style) -> Self {
        self.selector(StyleSelector::FileHover, style)
    }

    /// The visual style to apply when the view is being actively pressed.
    pub fn active(self, style: impl FnOnce(Style) -> Style) -> Self {
        self.selector(StyleSelector::Active, style)
    }

    /// Applies styles that activate at specific screen sizes (responsive design).
    pub fn responsive(mut self, size: ScreenSize, style: impl FnOnce(Style) -> Style) -> Self {
        let over = style(Style::default());
        for breakpoint in size.breakpoints() {
            self.set_breakpoint(breakpoint, over.clone());
        }
        self
    }

    /// Applies styles to views with a specific CSS class.
    pub fn class<C: StyleClass>(mut self, _class: C, style: impl FnOnce(Style) -> Style) -> Self {
        let over = style(Style::default());
        self.set_class(C::class_ref(), over);
        self
    }

    /// Applies a `CustomStyle` type to the `CustomStyle`'s associated style class.
    ///
    /// For example: if the `CustomStyle` you use is `DropdownCustomStyle` then it
    /// will apply the custom style to that custom style type's associated style class
    /// which, in this example, is `DropdownClass`.
    ///
    /// This is especially useful when building a stylesheet or targeting a child view.
    ///
    /// # Examples
    /// ```
    /// // In a style sheet or on a parent view
    /// use floem::prelude::*;
    /// use floem::style::Style;
    /// Style::new().custom_style_class(|s: dropdown::DropdownCustomStyle| s.close_on_accept(false));
    /// // This property is now set on the `DropdownClass` class and will be applied to any dropdowns that are children of this view.
    /// ```
    ///
    /// See also: [`Style::custom`](Self::custom) and [`Style::apply_custom`](Self::apply_custom).
    pub fn custom_style_class<CS: CustomStyle>(mut self, style: impl FnOnce(CS) -> CS) -> Self {
        let over = style(CS::default());
        self.set_class(CS::StyleClass::class_ref(), over.into());
        self
    }

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

    /// Sets the gap between columns in grid or flex layouts.
    pub fn col_gap(self, width: impl Into<PxPct>) -> Self {
        self.set(ColGap, width.into())
    }

    /// Sets the gap between rows in grid or flex layouts.
    pub fn row_gap(self, height: impl Into<PxPct>) -> Self {
        self.set(RowGap, height.into())
    }

    /// Sets different gaps for rows and columns in grid or flex layouts.
    pub fn row_col_gap(self, width: impl Into<PxPct>, height: impl Into<PxPct>) -> Self {
        self.col_gap(width).row_gap(height)
    }

    /// Sets the same gap for both rows and columns in grid or flex layouts.
    pub fn gap(self, gap: impl Into<PxPct>) -> Self {
        let gap = gap.into();
        self.col_gap(gap).row_gap(gap)
    }

    /// Sets both width and height of the view.
    pub fn size(self, width: impl Into<PxPctAuto>, height: impl Into<PxPctAuto>) -> Self {
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
        min_width: impl Into<PxPctAuto>,
        min_height: impl Into<PxPctAuto>,
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
        max_width: impl Into<PxPctAuto>,
        max_height: impl Into<PxPctAuto>,
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
        self.set(BorderColorProp, BorderColor::all(color))
    }

    /// Sets the border properties for all sides of the view.
    pub fn border(self, border: impl Into<StrokeWrap>) -> Self {
        self.set(BorderProp, Border::all(border))
    }

    /// Sets the outline properties of the view.
    pub fn outline(self, outline: impl Into<StrokeWrap>) -> Self {
        self.set_style_value(Outline, StyleValue::Val(outline.into()))
    }

    /// Sets `border_left` and `border_right` to `border`
    pub fn border_horiz(self, border: impl Into<StrokeWrap>) -> Self {
        let mut current = self.get(BorderProp);
        let border = border.into();
        current.left = Some(border.clone());
        current.right = Some(border);
        self.set(BorderProp, current)
    }

    /// Sets `border_top` and `border_bottom` to `border`
    pub fn border_vert(self, border: impl Into<StrokeWrap>) -> Self {
        let mut current = self.get(BorderProp);
        let border = border.into();
        current.top = Some(border.clone());
        current.bottom = Some(border);
        self.set(BorderProp, current)
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
    pub fn padding(self, padding: impl Into<PxPct>) -> Self {
        self.set(PaddingProp, Padding::all(padding))
    }

    /// Sets padding on all sides as a percentage of the parent container width.
    pub fn padding_pct(self, padding: f64) -> Self {
        self.set(PaddingProp, Padding::all(padding.pct()))
    }

    /// Sets `padding_left` and `padding_right` to `padding`
    pub fn padding_horiz(self, padding: impl Into<PxPct>) -> Self {
        let mut current = self.get(PaddingProp);
        let padding = padding.into();
        current.left = Some(padding);
        current.right = Some(padding);
        self.set(PaddingProp, current)
    }

    /// Sets horizontal padding as a percentage of the parent container width.
    pub fn padding_horiz_pct(self, padding: f64) -> Self {
        self.padding_horiz(padding.pct())
    }

    /// Sets `padding_top` and `padding_bottom` to `padding`
    pub fn padding_vert(self, padding: impl Into<PxPct>) -> Self {
        let mut current = self.get(PaddingProp);
        let padding = padding.into();
        current.top = Some(padding);
        current.bottom = Some(padding);
        self.set(PaddingProp, current)
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
    pub fn margin(self, margin: impl Into<PxPctAuto>) -> Self {
        self.set(MarginProp, Margin::all(margin))
    }

    /// Sets margin on all sides as a percentage of the parent container width.
    pub fn margin_pct(self, margin: f64) -> Self {
        self.set(MarginProp, Margin::all(margin.pct()))
    }

    /// Sets `margin_left` and `margin_right` to `margin`
    pub fn margin_horiz(self, margin: impl Into<PxPctAuto>) -> Self {
        let mut current = self.get(MarginProp);
        let margin = margin.into();
        current.left = Some(margin);
        current.right = Some(margin);
        self.set(MarginProp, current)
    }

    /// Sets horizontal margin as a percentage of the parent container width.
    pub fn margin_horiz_pct(self, margin: f64) -> Self {
        self.margin_horiz(margin.pct())
    }

    /// Sets `margin_top` and `margin_bottom` to `margin`
    pub fn margin_vert(self, margin: impl Into<PxPctAuto>) -> Self {
        let mut current = self.get(MarginProp);
        let margin = margin.into();
        current.top = Some(margin);
        current.bottom = Some(margin);
        self.set(MarginProp, current)
    }

    /// Sets vertical margin as a percentage of the parent container width.
    pub fn margin_vert_pct(self, margin: f64) -> Self {
        self.margin_vert(margin.pct())
    }

    /// Sets the left padding of the view.
    pub fn padding_left(self, padding: impl Into<PxPct>) -> Self {
        let mut current = self.get(PaddingProp);
        current.left = Some(padding.into());
        self.set(PaddingProp, current)
    }
    /// Sets the right padding of the view.
    pub fn padding_right(self, padding: impl Into<PxPct>) -> Self {
        let mut current = self.get(PaddingProp);
        current.right = Some(padding.into());
        self.set(PaddingProp, current)
    }
    /// Sets the top padding of the view.
    pub fn padding_top(self, padding: impl Into<PxPct>) -> Self {
        let mut current = self.get(PaddingProp);
        current.top = Some(padding.into());
        self.set(PaddingProp, current)
    }
    /// Sets the bottom padding of the view.
    pub fn padding_bottom(self, padding: impl Into<PxPct>) -> Self {
        let mut current = self.get(PaddingProp);
        current.bottom = Some(padding.into());
        self.set(PaddingProp, current)
    }

    /// Sets the left margin of the view.
    pub fn margin_left(self, margin: impl Into<PxPctAuto>) -> Self {
        let mut current = self.get(MarginProp);
        current.left = Some(margin.into());
        self.set(MarginProp, current)
    }
    /// Sets the right margin of the view.
    pub fn margin_right(self, margin: impl Into<PxPctAuto>) -> Self {
        let mut current = self.get(MarginProp);
        current.right = Some(margin.into());
        self.set(MarginProp, current)
    }
    /// Sets the top margin of the view.
    pub fn margin_top(self, margin: impl Into<PxPctAuto>) -> Self {
        let mut current = self.get(MarginProp);
        current.top = Some(margin.into());
        self.set(MarginProp, current)
    }
    /// Sets the bottom margin of the view.
    pub fn margin_bottom(self, margin: impl Into<PxPctAuto>) -> Self {
        let mut current = self.get(MarginProp);
        current.bottom = Some(margin.into());
        self.set(MarginProp, current)
    }

    /// Applies a complete padding configuration to the view.
    pub fn apply_padding(self, padding: Padding) -> Self {
        self.set(PaddingProp, padding)
    }
    /// Applies a complete margin configuration to the view.
    pub fn apply_margin(self, margin: Margin) -> Self {
        self.set(MarginProp, margin)
    }

    /// Sets the border radius for all corners of the view.
    pub fn border_radius(self, radius: impl Into<PxPct>) -> Self {
        self.set(BorderRadiusProp, BorderRadius::all(radius))
    }

    /// Sets the left border of the view.
    pub fn border_left(self, border: impl Into<StrokeWrap>) -> Self {
        let mut current = self.get(BorderProp);
        current.left = Some(border.into());
        self.set(BorderProp, current)
    }
    /// Sets the right border of the view.
    pub fn border_right(self, border: impl Into<StrokeWrap>) -> Self {
        let mut current = self.get(BorderProp);
        current.right = Some(border.into());
        self.set(BorderProp, current)
    }
    /// Sets the top border of the view.
    pub fn border_top(self, border: impl Into<StrokeWrap>) -> Self {
        let mut current = self.get(BorderProp);
        current.top = Some(border.into());
        self.set(BorderProp, current)
    }
    /// Sets the bottom border of the view.
    pub fn border_bottom(self, border: impl Into<StrokeWrap>) -> Self {
        let mut current = self.get(BorderProp);
        current.bottom = Some(border.into());
        self.set(BorderProp, current)
    }

    /// Sets the left border color of the view.
    pub fn border_left_color(self, color: impl Into<Brush>) -> Self {
        let mut current = self.get(BorderColorProp);
        current.left = Some(color.into());
        self.set(BorderColorProp, current)
    }
    /// Sets the right border color of the view.
    pub fn border_right_color(self, color: impl Into<Brush>) -> Self {
        let mut current = self.get(BorderColorProp);
        current.right = Some(color.into());
        self.set(BorderColorProp, current)
    }
    /// Sets the top border color of the view.
    pub fn border_top_color(self, color: impl Into<Brush>) -> Self {
        let mut current = self.get(BorderColorProp);
        current.top = Some(color.into());
        self.set(BorderColorProp, current)
    }
    /// Sets the bottom border color of the view.
    pub fn border_bottom_color(self, color: impl Into<Brush>) -> Self {
        let mut current = self.get(BorderColorProp);
        current.bottom = Some(color.into());
        self.set(BorderColorProp, current)
    }

    /// Sets the top-left border radius of the view.
    pub fn border_top_left_radius(self, radius: impl Into<PxPct>) -> Self {
        let mut current = self.get(BorderRadiusProp);
        current.top_left = Some(radius.into());
        self.set(BorderRadiusProp, current)
    }
    /// Sets the top-right border radius of the view.
    pub fn border_top_right_radius(self, radius: impl Into<PxPct>) -> Self {
        let mut current = self.get(BorderRadiusProp);
        current.top_right = Some(radius.into());
        self.set(BorderRadiusProp, current)
    }
    /// Sets the bottom-left border radius of the view.
    pub fn border_bottom_left_radius(self, radius: impl Into<PxPct>) -> Self {
        let mut current = self.get(BorderRadiusProp);
        current.bottom_left = Some(radius.into());
        self.set(BorderRadiusProp, current)
    }
    /// Sets the bottom-right border radius of the view.
    pub fn border_bottom_right_radius(self, radius: impl Into<PxPct>) -> Self {
        let mut current = self.get(BorderRadiusProp);
        current.bottom_right = Some(radius.into());
        self.set(BorderRadiusProp, current)
    }

    /// Applies a complete border configuration to the view.
    pub fn apply_border(self, border: Border) -> Self {
        self.set(BorderProp, border)
    }
    /// Applies a complete border color configuration to the view.
    pub fn apply_border_color(self, border_color: BorderColor) -> Self {
        self.set(BorderColorProp, border_color)
    }
    /// Applies a complete border radius configuration to the view.
    pub fn apply_border_radius(self, border_radius: BorderRadius) -> Self {
        self.set(BorderRadiusProp, border_radius)
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
    pub fn inset(self, inset: impl Into<PxPctAuto>) -> Self {
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

    /// Sets the cursor style when hovering over the view.
    pub fn cursor(self, cursor: impl Into<StyleValue<CursorStyle>>) -> Self {
        self.set_style_value(Cursor, cursor.into().map(Some))
    }

    /// Specifies text color for the view.
    pub fn color(self, color: impl Into<StyleValue<Color>>) -> Self {
        self.set_style_value(TextColor, color.into().map(Some))
    }

    /// Sets the background color or pattern of the view.
    pub fn background(self, color: impl Into<Brush>) -> Self {
        let brush = StyleValue::Val(Some(color.into()));
        self.set_style_value(Background, brush)
    }

    /// Specifies shadow blur. The larger this value, the bigger the blur,
    /// so the shadow becomes bigger and lighter.
    pub fn box_shadow_blur(self, blur_radius: impl Into<PxPct>) -> Self {
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
    pub fn box_shadow_spread(self, spread: impl Into<PxPct>) -> Self {
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
    /// use floem::prelude::*;
    /// use floem::prelude::palette::css;
    /// use floem::style::BoxShadow;
    ///
    /// empty().style(|s| s.apply_box_shadows(vec![
    ///    BoxShadow::new()
    ///        .color(css::BLACK)
    ///        .top_offset(5.)
    ///        .bottom_offset(-30.)
    ///        .right_offset(-20.)
    ///        .left_offset(10.)
    ///        .blur_radius(5.)
    ///        .spread(10.)
    /// ]));
    /// ```
    /// ### Info
    /// If you only specify one shadow on the view, use standard style methods directly
    /// on [Style] struct:
    /// ```rust
    /// use floem::prelude::*;
    /// empty().style(|s| s
    ///     .box_shadow_top_offset(-5.)
    ///     .box_shadow_bottom_offset(30.)
    ///     .box_shadow_right_offset(20.)
    ///     .box_shadow_left_offset(-10.)
    ///     .box_shadow_spread(1.)
    ///     .box_shadow_blur(3.)
    /// );
    /// ```
    pub fn apply_box_shadows(self, shadow: impl Into<SmallVec<[BoxShadow; 3]>>) -> Self {
        self.set(BoxShadowProp, shadow.into())
    }

    /// Specifies the offset on horizontal axis.
    /// Negative offset value places the shadow to the left of the view.
    pub fn box_shadow_h_offset(self, h_offset: impl Into<PxPct>) -> Self {
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
    pub fn box_shadow_v_offset(self, v_offset: impl Into<PxPct>) -> Self {
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
    pub fn box_shadow_left_offset(self, left_offset: impl Into<PxPct>) -> Self {
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
    pub fn box_shadow_right_offset(self, right_offset: impl Into<PxPct>) -> Self {
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
    pub fn box_shadow_top_offset(self, top_offset: impl Into<PxPct>) -> Self {
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
    pub fn box_shadow_bottom_offset(self, bottom_offset: impl Into<PxPct>) -> Self {
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

    /// Sets the font size for text content.
    pub fn font_size(self, size: impl Into<Px>) -> Self {
        let px = size.into();
        self.set_style_value(FontSize, StyleValue::Val(Some(px.0 as f32)))
    }

    /// Sets the font family for text content.
    pub fn font_family(self, family: impl Into<StyleValue<String>>) -> Self {
        self.set_style_value(FontFamily, family.into().map(Some))
    }

    /// Sets the font weight (boldness) for text content.
    pub fn font_weight(self, weight: impl Into<StyleValue<Weight>>) -> Self {
        self.set_style_value(FontWeight, weight.into().map(Some))
    }

    /// Sets the font weight to bold.
    pub fn font_bold(self) -> Self {
        self.font_weight(Weight::BOLD)
    }

    /// Sets the font style (italic, normal) for text content.
    pub fn font_style(self, style: impl Into<StyleValue<crate::text::Style>>) -> Self {
        self.set_style_value(FontStyle, style.into().map(Some))
    }

    /// Sets the color of the text cursor.
    pub fn cursor_color(self, color: impl Into<Brush>) -> Self {
        let brush = StyleValue::Val(color.into());
        self.set_style_value(CursorColor, brush)
    }

    /// Sets the line height for text content.
    pub fn line_height(self, normal: f32) -> Self {
        self.set(LineHeight, Some(LineHeightValue::Normal(normal)))
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
        self.text_overflow(TextOverflow::Ellipsis)
    }

    /// Sets text overflow to clip text without showing ellipsis.
    pub fn text_clip(self) -> Self {
        self.text_overflow(TextOverflow::Clip)
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
    /// use floem::style::Style;
    ///
    /// // Create a full-screen overlay
    /// Style::new().fixed().inset(0.0);
    /// ```
    pub fn fixed(self) -> Self {
        self.position(taffy::style::Position::Absolute)
            .is_fixed(true)
    }

    /// Aligns flex items to stretch and fill the cross axis.
    pub fn items_stretch(self) -> Self {
        self.align_items(Some(taffy::style::AlignItems::Stretch))
    }

    /// Aligns flex items to the start of the cross axis.
    pub fn items_start(self) -> Self {
        self.align_items(Some(taffy::style::AlignItems::FlexStart))
    }

    /// Defines the alignment along the cross axis as Centered
    pub fn items_center(self) -> Self {
        self.align_items(Some(taffy::style::AlignItems::Center))
    }

    /// Aligns flex items to the end of the cross axis.
    pub fn items_end(self) -> Self {
        self.align_items(Some(taffy::style::AlignItems::FlexEnd))
    }

    /// Aligns flex items along their baselines.
    pub fn items_baseline(self) -> Self {
        self.align_items(Some(taffy::style::AlignItems::Baseline))
    }

    /// Aligns flex items to the start of the main axis.
    pub fn justify_start(self) -> Self {
        self.justify_content(Some(taffy::style::JustifyContent::FlexStart))
    }

    /// Aligns flex items to the end of the main axis.
    pub fn justify_end(self) -> Self {
        self.justify_content(Some(taffy::style::JustifyContent::FlexEnd))
    }

    /// Defines the alignment along the main axis as Centered
    pub fn justify_center(self) -> Self {
        self.justify_content(Some(taffy::style::JustifyContent::Center))
    }

    /// Distributes flex items with space between them.
    pub fn justify_between(self) -> Self {
        self.justify_content(Some(taffy::style::JustifyContent::SpaceBetween))
    }

    /// Distributes flex items with space around them.
    pub fn justify_around(self) -> Self {
        self.justify_content(Some(taffy::style::JustifyContent::SpaceAround))
    }

    /// Distributes flex items with equal space around them.
    pub fn justify_evenly(self) -> Self {
        self.justify_content(Some(taffy::style::JustifyContent::SpaceEvenly))
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

    /// Sets the stack order of the view.
    pub fn z_index(self, z_index: i32) -> Self {
        self.set(ZIndex, Some(z_index))
    }

    /// Sets uniform scaling for both X and Y axes.
    pub fn scale(self, scale: impl Into<Pct>) -> Self {
        let val = scale.into();
        self.scale_x(val).scale_y(val)
    }

    /// Allow the application of a function if the option exists.
    /// This is useful for chaining together a bunch of optional style changes.
    /// ```rust
    /// use floem::style::Style;
    /// let maybe_none: Option<i32> = None;
    /// let style = Style::default()
    ///     .apply_opt(Some(5.0), Style::padding) // ran
    ///     .apply_opt(maybe_none, Style::margin) // not ran
    ///     .apply_opt(Some(5.0), |s, v| s.border_right(v * 2.0))
    ///     .border_left(5.0); // ran, obviously
    /// ```
    pub fn apply_opt<T>(self, opt: Option<T>, f: impl FnOnce(Self, T) -> Self) -> Self {
        if let Some(t) = opt { f(self, t) } else { self }
    }

    /// Allow the application of a function if the condition holds.
    /// This is useful for chaining together optional style changes.
    /// ```rust
    /// use floem::style::Style;
    /// let style = Style::default()
    ///     .apply_if(true, |s| s.padding(5.0)) // ran
    ///     .apply_if(false, |s| s.margin(5.0)); // not ran
    /// ```
    pub fn apply_if(self, cond: bool, f: impl FnOnce(Self) -> Self) -> Self {
        if cond { f(self) } else { self }
    }

    /// Applies a `CustomStyle` type into this style.
    ///
    /// # Examples
    /// ```
    /// use floem::prelude::*;
    /// text("test").style(|s| s.custom(|s: LabelCustomStyle| s.selectable(false)));
    /// ```
    ///
    /// See also: [`apply_custom`](Self::apply_custom), [`custom_style_class`](Self::custom_style_class)
    pub fn custom<CS: CustomStyle>(self, custom: impl FnOnce(CS) -> CS) -> Self {
        self.apply(custom(CS::default()).into())
    }

    /// Applies a `CustomStyle` type into this style.
    ///
    /// # Examples
    /// ```
    /// use floem::prelude::*;
    /// text("test").style(|s| s.apply_custom(LabelCustomStyle::new().selectable(false)));
    /// ```
    ///
    /// See also: [`custom`](Self::custom), [`custom_style_class`](Self::custom_style_class)
    pub fn apply_custom<CS: Into<Style>>(self, custom_style: CS) -> Self {
        self.apply(custom_style.into())
    }
}

impl Style {
    pub fn to_taffy_style(&self) -> TaffyStyle {
        let style = self.builtin();
        TaffyStyle {
            display: style.display(),
            overflow: taffy::Point {
                x: self.get(OverflowX),
                y: self.get(OverflowY),
            },
            position: style.position(),
            size: taffy::prelude::Size {
                width: style.width().into(),
                height: style.height().into(),
            },
            min_size: taffy::prelude::Size {
                width: style.min_width().into(),
                height: style.min_height().into(),
            },
            max_size: taffy::prelude::Size {
                width: style.max_width().into(),
                height: style.max_height().into(),
            },
            flex_direction: style.flex_direction(),
            flex_grow: style.flex_grow(),
            flex_shrink: style.flex_shrink(),
            flex_basis: style.flex_basis().into(),
            flex_wrap: style.flex_wrap(),
            justify_content: style.justify_content(),
            justify_self: style.justify_self(),
            justify_items: style.justify_items(),
            align_items: style.align_items(),
            align_content: style.align_content(),
            align_self: style.align_self(),
            aspect_ratio: style.aspect_ratio(),
            border: {
                let border = style.style.get(BorderProp);
                Rect {
                    left: LengthPercentage::length(border.left.map_or(0.0, |b| b.0.width) as f32),
                    top: LengthPercentage::length(border.top.map_or(0.0, |b| b.0.width) as f32),
                    right: LengthPercentage::length(border.right.map_or(0.0, |b| b.0.width) as f32),
                    bottom: LengthPercentage::length(
                        border.bottom.map_or(0.0, |b| b.0.width) as f32
                    ),
                }
            },
            padding: {
                let padding = style.style.get(PaddingProp);
                Rect {
                    left: padding.left.unwrap_or(PxPct::Px(0.0)).into(),
                    top: padding.top.unwrap_or(PxPct::Px(0.0)).into(),
                    right: padding.right.unwrap_or(PxPct::Px(0.0)).into(),
                    bottom: padding.bottom.unwrap_or(PxPct::Px(0.0)).into(),
                }
            },
            margin: {
                let margin = style.style.get(MarginProp);
                Rect {
                    left: margin.left.unwrap_or(PxPctAuto::Px(0.0)).into(),
                    top: margin.top.unwrap_or(PxPctAuto::Px(0.0)).into(),
                    right: margin.right.unwrap_or(PxPctAuto::Px(0.0)).into(),
                    bottom: margin.bottom.unwrap_or(PxPctAuto::Px(0.0)).into(),
                }
            },
            inset: Rect {
                left: style.inset_left().into(),
                top: style.inset_top().into(),
                right: style.inset_right().into(),
                bottom: style.inset_bottom().into(),
            },
            gap: Size {
                width: style.col_gap().into(),
                height: style.row_gap().into(),
            },
            grid_template_rows: style.grid_template_rows(),
            grid_template_columns: style.grid_template_columns(),
            grid_row: style.grid_row(),
            grid_column: style.grid_column(),
            grid_auto_rows: style.grid_auto_rows(),
            grid_auto_columns: style.grid_auto_columns(),
            grid_auto_flow: style.grid_auto_flow(),
            ..Default::default()
        }
    }
}
