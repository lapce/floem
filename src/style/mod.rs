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

use floem_renderer::text::{FontWeight as FontWeightProp, LineHeightValue};
use imbl::hashmap::Entry;
use peniko::color::palette;
use peniko::kurbo::{self, Affine, RoundedRect, Stroke, Vec2};
use peniko::{Brush, Color};
use smallvec::SmallVec;
use std::any::Any;
use std::collections::HashMap;
use std::fmt::{self, Debug};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
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

use crate::layout::responsive::{GridBreakpoints, ScreenSize, ScreenSizeBp};

use crate::style::components::Focus;
use crate::text::{OverflowWrap, WordBreakStrength};
use crate::views::editor::SelectionColor;
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
    Border, BorderColor, BorderRadius, BoxShadow, CursorStyle, Margin, NoWrapOverflow, Padding,
    PointerEvents, TextOverflow,
};
pub use custom::{CustomStylable, CustomStyle};
pub use cx::{InheritedInteractionCx, InteractionState, StyleCx};
pub use props::{
    ExtractorField, StyleClass, StyleClassInfo, StyleClassRef, StyleDebugGroup,
    StyleDebugGroupInfo, StyleDebugGroupRef, StyleKey, StyleKeyInfo, StyleProp, StylePropInfo,
    StylePropReader, StylePropRef,
};
pub use selectors::{NthChild, StructuralSelector, StyleSelector, StyleSelectors};
pub use theme::{DesignSystem, StyleThemeExt};
pub use transition::{DirectTransition, Transition, TransitionState};
pub use unit::{AnchorAbout, Angle, Auto, DurationUnitExt, Pct, Px, PxPct, PxPctAuto, UnitExt};
pub use values::{ObjectFit, StrokeWrap, StyleMapValue, StylePropValue, StyleValue};

pub use cache::{StyleCache, StyleCacheKey};

pub(crate) use props::{
    CONTEXT_MAPPINGS_INFO, ImHashMap, RESPONSIVE_SELECTORS_INFO, STRUCTURAL_SELECTORS_INFO,
    style_key_selector,
};

static NEXT_STYLE_MERGE_ID: AtomicU64 = AtomicU64::new(1);
const MERGE_MIX_CONST: u64 = 0x9E3779B97F4A7C15;

fn next_style_merge_id() -> u64 {
    NEXT_STYLE_MERGE_ID.fetch_add(1, Ordering::Relaxed)
}

fn combine_merge_ids(a: u64, b: u64) -> u64 {
    a.rotate_left(13) ^ b.wrapping_mul(MERGE_MIX_CONST)
}

/// A closure that maps context values to style properties.
type ContextMapFn = Rc<dyn Fn(Style, Box<dyn Fn(StyleKey) -> Option<Rc<dyn Any>>>) -> Style>;
type StructuralSelectorRules = SmallVec<[(StructuralSelector, Rc<Style>); 2]>;
type ResponsiveSelectorRules = SmallVec<[(ResponsiveSelector, Rc<Style>); 2]>;

#[derive(Clone)]
struct StructuralSelectors(StructuralSelectorRules);

#[derive(Clone)]
struct ResponsiveSelectors(ResponsiveSelectorRules);

#[derive(Clone, Copy, Debug, PartialEq)]
enum ResponsiveSelector {
    ScreenSize(ScreenSize),
    MinWidth(Px),
    MaxWidth(Px),
    WidthRange { min: Px, max: Px },
}

impl ResponsiveSelector {
    fn matches(&self, width: f64) -> bool {
        match self {
            ResponsiveSelector::ScreenSize(size) => {
                let bp = GridBreakpoints::default().get_width_bp(width);
                size.breakpoints().contains(&bp)
            }
            ResponsiveSelector::MinWidth(min) => width >= min.0,
            ResponsiveSelector::MaxWidth(max) => width <= max.0,
            ResponsiveSelector::WidthRange { min, max } => width >= min.0 && width <= max.0,
        }
    }
}

/// Simple storage for context mapping closures.
/// Unlike the old ContextMappings, this only stores the closures themselves -
/// selector and inherited prop discovery happens via immediate evaluation.
///
/// Uses `Rc<Vec>` for O(1) clone - avoids copying the entire Vec when reading.
#[derive(Clone)]
pub(crate) struct ContextMappings(pub Rc<Vec<ContextMapFn>>);

style_key_selector!(selector_xs, StyleSelectors::empty().responsive());
style_key_selector!(selector_sm, StyleSelectors::empty().responsive());
style_key_selector!(selector_md, StyleSelectors::empty().responsive());
style_key_selector!(selector_lg, StyleSelectors::empty().responsive());
style_key_selector!(selector_xl, StyleSelectors::empty().responsive());
style_key_selector!(selector_xxl, StyleSelectors::empty().responsive());

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

fn context_mappings_key() -> StyleKey {
    StyleKey {
        info: &CONTEXT_MAPPINGS_INFO,
    }
}

fn structural_selectors_key() -> StyleKey {
    StyleKey {
        info: &STRUCTURAL_SELECTORS_INFO,
    }
}

fn responsive_selectors_key() -> StyleKey {
    StyleKey {
        info: &RESPONSIVE_SELECTORS_INFO,
    }
}

/// the bool in the return is a classes_applied flag. if a new class has been applied, we need to do a request_style_recursive
pub fn resolve_nested_maps(
    style: Style,
    interact_state: &mut InteractionState,
    screen_size_bp: ScreenSizeBp,
    classes: &[StyleClassRef],
    inherited_context: &Style,
    class_context: &Style,
) -> (Style, StyleSelectors) {
    // TODO: update interact state as each map is resolved

    let mut selectors = StyleSelectors::empty();

    let effect_context = style.effect_context.clone();

    // Phase 1: Resolve class styles (with selectors, collecting context mappings)
    let (class_style, mut class_context_mappings) = resolve_classes_collecting_mappings(
        classes,
        interact_state,
        screen_size_bp,
        class_context,
        &mut selectors,
    );
    selectors |= class_style.selectors();

    // Phase 2: Resolve view's inline style (with selectors, collecting context mappings)
    let (view_style, mut view_context_mappings) =
        resolve_style_collecting_mappings(style, interact_state, screen_size_bp, &mut selectors);

    // Phase 3: Apply class context mappings (with recursive resolution)
    // Use class_style as the base, context includes class_style + view_style
    let mut context_result = class_style.clone();
    let mut i = 0;
    while i < class_context_mappings.len() {
        let mapping = class_context_mappings[i].clone();

        let saved_effect = floem_reactive::Runtime::get_current_effect();
        if let Some(effect) = &effect_context {
            floem_reactive::Runtime::set_current_effect(Some(effect.clone()));
        }

        let mapped = mapping(
            context_result.clone(),
            Box::new({
                let view_style = view_style.clone();
                let context_result = context_result.clone();
                let inherited_context = inherited_context.clone();
                move |k| {
                    view_style
                        .map
                        .get(&k)
                        .or_else(|| {
                            context_result
                                .map
                                .get(&k)
                                .or_else(|| inherited_context.map.get(&k))
                        })
                        .cloned()
                }
            }),
        );

        let (resolved, new_mappings) = resolve_style_collecting_mappings(
            mapped,
            interact_state,
            screen_size_bp,
            &mut selectors,
        );
        floem_reactive::Runtime::set_current_effect(saved_effect);
        context_result.apply_mut_no_mappings(resolved);
        class_context_mappings.splice(i + 1..i + 1, new_mappings);
        i += 1;
    }

    // Apply view style over the context result (view style wins)
    let mut result = context_result.apply(view_style);

    // Phase 4: Apply view context mappings over result (with recursive resolution)
    let mut i = 0;
    let mut combined_context = inherited_context.clone();
    while i < view_context_mappings.len() {
        let mapping = view_context_mappings[i].clone();
        combined_context.apply_mut_no_mappings(result.clone());

        let saved_effect = floem_reactive::Runtime::get_current_effect();
        if let Some(effect) = &effect_context {
            floem_reactive::Runtime::set_current_effect(Some(effect.clone()));
        }

        let mapped = mapping(
            result.clone(),
            Box::new({
                let result = result.clone();
                let inherited_context = inherited_context.clone();
                move |k| {
                    result
                        .clone()
                        .map
                        .get(&k)
                        .or_else(|| inherited_context.map.get(&k))
                        .cloned()
                }
            }),
        );

        let (resolved, new_mappings) = resolve_style_collecting_mappings(
            mapped,
            interact_state,
            screen_size_bp,
            &mut selectors,
        );
        floem_reactive::Runtime::set_current_effect(saved_effect);
        result.apply_mut_no_mappings(resolved);
        view_context_mappings.splice(i + 1..i + 1, new_mappings);
        i += 1;
    }

    (result, selectors)
}

fn resolve_classes_collecting_mappings(
    classes: &[StyleClassRef],
    interact_state: &InteractionState,
    screen_size_bp: ScreenSizeBp,
    class_context: &Style,
    selectors: &mut StyleSelectors,
) -> (Style, Vec<ContextMapFn>) {
    let mut result = Style::new();
    let mut mappings = Vec::new();

    for class in classes {
        if let Some(map) = class_context.get_nested_map(class.key) {
            let (resolved, class_mappings) = resolve_style_collecting_mappings(
                map.clone(),
                interact_state,
                screen_size_bp,
                selectors,
            );
            result.apply_mut_no_mappings(resolved);
            mappings.extend(class_mappings);
        }
    }

    (result, mappings)
}

fn resolve_style_collecting_mappings(
    mut style: Style,
    interact_state: &InteractionState,
    screen_size_bp: ScreenSizeBp,
    selectors: &mut StyleSelectors,
) -> (Style, Vec<ContextMapFn>) {
    let mut mappings = Vec::new();

    // Extract context mappings from style before resolving
    if let Some(style_mappings) = extract_context_mappings(&mut style) {
        mappings.extend(style_mappings);
    }

    // Resolve all selectors (and collect any new mappings found)
    let (resolved, selector_mappings) =
        resolve_selectors_collecting_mappings(style, interact_state, screen_size_bp, selectors);
    mappings.extend(selector_mappings);

    (resolved, mappings)
}

fn resolve_selectors_collecting_mappings(
    mut style: Style,
    interact_state: &InteractionState,
    screen_size_bp: ScreenSizeBp,
    selectors: &mut StyleSelectors,
) -> (Style, Vec<ContextMapFn>) {
    *selectors |= style.selectors();

    const MAX_DEPTH: u32 = 20;
    let mut depth = 0;
    let mut all_mappings = Vec::new();

    loop {
        if depth >= MAX_DEPTH {
            break;
        }
        depth += 1;

        let mut changed = false;

        // Apply structural selectors (:first-child, :last-child, :nth-child(...))
        // before state selectors so nested :hover/:focus inside structural maps can
        // be discovered and applied in the same resolution loop.
        if let Some(structural_rules) = extract_structural_selectors(&mut style) {
            for (selector, map) in structural_rules {
                if selector.matches(interact_state.child_index, interact_state.sibling_count) {
                    let mut map = map.as_ref().clone();
                    if let Some(mappings) = extract_context_mappings(&mut map) {
                        all_mappings.extend(mappings);
                    }
                    style.apply_mut_no_mappings(map);
                    changed = true;
                }
            }
        }

        // Apply responsive selectors (parameterized)
        if let Some(responsive_rules) = extract_responsive_selectors(&mut style) {
            for (selector, map) in responsive_rules {
                if selector.matches(interact_state.window_width) {
                    let mut map = map.as_ref().clone();
                    if let Some(mappings) = extract_context_mappings(&mut map) {
                        all_mappings.extend(mappings);
                    }
                    style.apply_mut_no_mappings(map);
                    changed = true;
                }
            }
        }

        // Helper to apply a nested map and collect any context mappings from it
        let mut apply_nested = |style: &mut Style, key: StyleKey| -> bool {
            if let Some(mut map) = style.get_nested_map(key) {
                // Extract mappings before applying

                if let Some(mappings) = extract_context_mappings(&mut map) {
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
            if interact_state.is_focused && apply_nested(&mut style, StyleSelector::Focus.to_key())
            {
                changed = true;
            }

            if interact_state.is_focus_within
                && apply_nested(&mut style, StyleSelector::FocusWithin.to_key())
            {
                changed = true;
            }

            if interact_state.is_focused && interact_state.using_keyboard_navigation {
                if apply_nested(&mut style, StyleSelector::FocusVisible.to_key()) {
                    changed = true;
                }

                if interact_state.is_active
                    && apply_nested(&mut style, StyleSelector::Active.to_key())
                {
                    changed = true;
                }
            }

            // Active (mouse)
            if interact_state.is_active
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

fn extract_context_mappings(style: &mut Style) -> Option<Vec<ContextMapFn>> {
    let key = context_mappings_key();
    style.map.remove(&key).map(|rc| {
        let mappings = rc.downcast_ref::<ContextMappings>().unwrap();
        mappings.0.iter().cloned().collect()
    })
}

fn extract_structural_selectors(style: &mut Style) -> Option<StructuralSelectorRules> {
    let key = structural_selectors_key();
    style
        .map
        .remove(&key)
        .map(|rc| rc.downcast_ref::<StructuralSelectors>().unwrap().0.clone())
}

fn extract_responsive_selectors(style: &mut Style) -> Option<ResponsiveSelectorRules> {
    let key = responsive_selectors_key();
    style
        .map
        .remove(&key)
        .map(|rc| rc.downcast_ref::<ResponsiveSelectors>().unwrap().0.clone())
}

#[derive(Clone)]
pub struct Style {
    pub(crate) map: ImHashMap<StyleKey, Rc<dyn Any>>,
    /// Deterministic identity for style merges.
    merge_id: u64,
    /// Cached flag indicating whether this style contains any class maps.
    /// This enables O(1) early-exit in `apply_only_class_maps` for the common case
    /// where a view's style has no class definitions.
    has_class_maps: bool,
    /// Cached flag indicating whether this style contains any inherited properties.
    /// This enables O(1) early-exit in `apply_only_inherited` for the common case
    /// where a view's style has no inherited properties.
    has_inherited: bool,
    /// The effect context that was active when this style was created.
    /// This is restored when evaluating context mappings and selectors to ensure
    /// reactive dependencies are tracked correctly.
    effect_context: Option<Rc<dyn floem_reactive::EffectTrait>>,

    context_selectors: StyleSelectors,
}
impl Default for Style {
    fn default() -> Self {
        let effect_context = floem_reactive::Runtime::get_current_effect();
        let map = ImHashMap::default();
        Self {
            merge_id: next_style_merge_id(),
            map,
            has_class_maps: false,
            has_inherited: false,
            effect_context,
            context_selectors: StyleSelectors::empty(),
        }
    }
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
    pub fn apply_only_inherited(to: &mut Style, from: &Style) {
        if from.any_inherited() {
            let inherited = from.map.iter().filter(|(p, _)| p.inherited());
            to.apply_iter(inherited, None);
            to.merge_id = combine_merge_ids(to.merge_id, from.merge_id);
            to.context_selectors |= from.context_selectors;
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
                new_style.apply_iter(inherited, None);
                new_style.merge_id = combine_merge_ids(new_style.merge_id, from.merge_id);
                new_style.context_selectors |= from.context_selectors;
            }

            // Apply class nested maps so they flow to descendants
            if has_class_maps {
                let class_maps = from
                    .map
                    .iter()
                    .filter(|(k, _)| matches!(k.info, StyleKeyInfo::Class(..)));
                new_style.apply_iter(class_maps, None);
                new_style.merge_id = combine_merge_ids(new_style.merge_id, from.merge_id);
                new_style.context_selectors |= from.context_selectors;
            }

            *to = Rc::new(new_style);
        }
    }

    /// Apply only class nested maps from `from` style to `to` style.
    /// This is used during style propagation to pass class definitions to children.
    ///
    /// Only class nested maps (`.class(SomeClass, ...)`) are applied, not inherited props.
    pub fn apply_only_class_maps(to: &mut Style, from: &Style) {
        if !from.has_class_maps {
            return;
        }
        let class_maps = from
            .map
            .iter()
            .filter(|(k, _)| matches!(k.info, StyleKeyInfo::Class(..)));
        to.apply_iter(class_maps, None);
        to.merge_id = combine_merge_ids(to.merge_id, from.merge_id);
        to.context_selectors |= from.context_selectors;
    }

    pub(crate) fn merge_id(&self) -> u64 {
        self.merge_id
    }

    pub fn class_maps_eq(&self, other: &Style) -> SmallVec<[StyleClassRef; 4]> {
        // Pass 1: every Class entry in self must exist in other
        let mut changed = SmallVec::new();
        for (k, v) in &self.map {
            let StyleKeyInfo::Class(_) = k.info else {
                continue;
            };

            match other.map.get(k) {
                Some(other_v) => {
                    let v_style = v.downcast_ref::<Style>().unwrap();
                    let other_v_style = other_v.downcast_ref::<Style>().unwrap();

                    if v_style.merge_id != other_v_style.merge_id
                        && !v_style.map.ptr_eq(&other_v_style.map)
                    {
                        changed.push(StyleClassRef { key: *k });
                    }
                }
                None => {
                    changed.push(StyleClassRef { key: *k });
                }
            }
        }

        // Pass 2: ensure other does not contain extra Class entries
        for k in other.map.keys() {
            if !matches!(k.info, StyleKeyInfo::Class(..)) {
                continue;
            }

            if !self.map.contains_key(k) {
                changed.push(StyleClassRef { key: *k });
            }
        }

        changed
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
        let mut result = self.context_selectors;

        for (k, v) in &self.map {
            match k.info {
                StyleKeyInfo::Selector(selector) => {
                    result = result
                        .union(*selector)
                        .union(v.downcast_ref::<Style>().unwrap().selectors());
                }
                StyleKeyInfo::StructuralSelectors => {
                    let rules = &v.downcast_ref::<StructuralSelectors>().unwrap().0;
                    for (_, nested_style) in rules {
                        result = result.union(nested_style.as_ref().selectors());
                    }
                }
                StyleKeyInfo::ResponsiveSelectors => {
                    let rules = &v.downcast_ref::<ResponsiveSelectors>().unwrap().0;
                    for (_, nested_style) in rules {
                        result = result.union(nested_style.as_ref().selectors());
                    }
                }
                StyleKeyInfo::DebugGroup(..) => {}
                _ => {}
            }
        }

        result
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
        let default_value = P::default_value();
        let result = f(self.clone(), &default_value);
        let result_selectors = result.selectors();

        // Create a closure for context-aware re-evaluation during style resolution.
        // Cache default_value inside closure to avoid repeated allocations.
        let mapper: ContextMapFn = Rc::new(
            move |style: Style, context: Box<dyn Fn(StyleKey) -> Option<Rc<dyn Any>>>| {
                // Try getting the property from style first, then from context if not found
                let value = context(P::key()).and_then(|v| {
                    v.downcast_ref::<StyleMapValue<P::Type>>()
                        .unwrap()
                        .as_ref()
                        .cloned()
                });

                if let Some(value) = value {
                    f(style, &value)
                } else {
                    f(style, &P::default_value())
                }
            },
        );

        // Store the closure for later context resolution
        let key = context_mappings_key();

        // Build new mappings vec - use Rc::make_mut for efficient copy-on-write
        let mut mappings_vec = self
            .map
            .get(&key)
            .and_then(|v| v.downcast_ref::<ContextMappings>())
            .map(|cm| (*cm.0).clone())
            .unwrap_or_default();
        mappings_vec.push(mapper);

        // Start with the immediate result (has selectors/inherited props)
        // but add our closure storage
        let mut final_result = self;
        final_result
            .map
            .insert(key, Rc::new(ContextMappings(Rc::new(mappings_vec))));
        final_result.merge_id = next_style_merge_id();
        final_result.context_selectors |= result_selectors;
        final_result
    }

    /// Apply a context-based style transformation for optional props.
    ///
    /// Like `with_context`, this evaluates immediately with defaults for discovery,
    /// and stores the closure for context-aware re-evaluation.
    pub fn with_context_opt<P: StyleProp<Type = Option<T>>, T: 'static + Default + Clone>(
        self,
        f: impl Fn(Self, T) -> Self + 'static,
    ) -> Self {
        let result = f(self.clone(), T::default());
        let result_selectors = result.selectors();
        // Create a closure for context-aware re-evaluation
        let mapper: ContextMapFn = Rc::new(
            move |style: Style, context: Box<dyn Fn(StyleKey) -> Option<Rc<dyn Any>>>| {
                // Try getting the property from style first, then from context if not found
                let value = context(P::key()).and_then(|v| {
                    v.downcast_ref::<StyleMapValue<P::Type>>()
                        .unwrap()
                        .as_ref()
                        .cloned()
                });

                match value {
                    Some(Some(value)) => f(style, value),
                    _ => style,
                }
            },
        );

        // Store the closure
        let key = context_mappings_key();

        // Build new mappings vec efficiently
        let mut mappings_vec = self
            .map
            .get(&key)
            .and_then(|v| v.downcast_ref::<ContextMappings>())
            .map(|cm| (*cm.0).clone())
            .unwrap_or_default();
        mappings_vec.push(mapper);

        let mut final_result = self;
        final_result
            .map
            .insert(key, Rc::new(ContextMappings(Rc::new(mappings_vec))));
        final_result.merge_id = next_style_merge_id();
        final_result.context_selectors |= result_selectors;
        final_result
    }

    pub(crate) fn get_nested_map(&self, key: StyleKey) -> Option<Style> {
        self.map
            .get(&key)
            .map(|map| map.downcast_ref::<Style>().unwrap().clone())
    }

    pub(crate) fn debug_group_enabled(&self, key: StyleKey) -> bool {
        self.map
            .get(&key)
            .and_then(|value| value.downcast_ref::<bool>().copied())
            .unwrap_or(false)
    }

    pub(crate) fn remove_nested_map(&mut self, key: StyleKey) -> Option<Style> {
        let removed = self
            .map
            .remove(&key)
            .map(|map| map.downcast_ref::<Style>().unwrap().clone());
        if removed.is_some() {
            self.merge_id = next_style_merge_id();
        }
        removed
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

            new.apply_iter(inherited, None);
            new.merge_id = combine_merge_ids(new.merge_id, self.merge_id);
            new.context_selectors |= self.context_selectors;
        }
        new
    }

    fn set_selector(&mut self, selector: StyleSelector, map: Style) {
        self.set_map_selector(selector.to_key(), map)
    }

    fn set_structural_selector(&mut self, selector: StructuralSelector, map: Style) {
        let key = structural_selectors_key();
        match self.map.entry(key) {
            Entry::Occupied(mut e) => {
                let mut rules = e
                    .get()
                    .downcast_ref::<StructuralSelectors>()
                    .unwrap()
                    .0
                    .clone();
                rules.push((selector, Rc::new(map)));
                *e.get_mut() = Rc::new(StructuralSelectors(rules));
            }
            Entry::Vacant(e) => {
                let mut rules = SmallVec::new();
                rules.push((selector, Rc::new(map)));
                e.insert(Rc::new(StructuralSelectors(rules)));
            }
        }
        self.merge_id = next_style_merge_id();
    }

    fn set_responsive_selector(&mut self, selector: ResponsiveSelector, map: Style) {
        let key = responsive_selectors_key();
        match self.map.entry(key) {
            Entry::Occupied(mut e) => {
                let mut rules = e
                    .get()
                    .downcast_ref::<ResponsiveSelectors>()
                    .unwrap()
                    .0
                    .clone();
                rules.push((selector, Rc::new(map)));
                *e.get_mut() = Rc::new(ResponsiveSelectors(rules));
            }
            Entry::Vacant(e) => {
                let mut rules = SmallVec::new();
                rules.push((selector, Rc::new(map)));
                e.insert(Rc::new(ResponsiveSelectors(rules)));
            }
        }
        self.context_selectors |= StyleSelectors::empty().responsive();
        self.merge_id = next_style_merge_id();
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
        self.merge_id = next_style_merge_id();
    }

    fn set_class(&mut self, class: StyleClassRef, map: Style) {
        self.has_class_maps = true;
        self.set_map_selector(class.key, map)
    }

    pub fn debug_group<G: StyleDebugGroup>(mut self, _group: G) -> Self {
        self.map.insert(G::key(), Rc::new(true));
        self.merge_id = next_style_merge_id();
        self
    }

    pub fn unset_debug_group<G: StyleDebugGroup>(mut self, _group: G) -> Self {
        self.map.insert(G::key(), Rc::new(false));
        self.merge_id = next_style_merge_id();
        self
    }

    pub fn builtin(&self) -> BuiltinStyle<'_> {
        BuiltinStyle { style: self }
    }

    pub(crate) fn apply_iter<'a>(
        &mut self,
        iter: impl Iterator<Item = (&'a StyleKey, &'a Rc<dyn Any>)>,
        source_effect_context: Option<Rc<dyn floem_reactive::EffectTrait>>,
    ) {
        if self.effect_context.is_none() && source_effect_context.is_some() {
            self.effect_context = source_effect_context;
        }
        for (k, v) in iter {
            match k.info {
                StyleKeyInfo::Class(..) | StyleKeyInfo::Selector(..) => {
                    // Track class maps for O(1) early-exit in apply_only_class_maps
                    if matches!(k.info, StyleKeyInfo::Class(..)) {
                        self.has_class_maps = true;
                    }
                    match self.map.entry(*k) {
                        Entry::Occupied(mut e) => {
                            let existing_rc = Rc::clone(e.get());

                            // Skip only this key
                            if Rc::ptr_eq(&existing_rc, v) {
                                continue;
                            }

                            let new_style = v.downcast_ref::<Style>().unwrap();

                            match Rc::get_mut(e.get_mut()) {
                                Some(current_any) => {
                                    current_any
                                        .downcast_mut::<Style>()
                                        .unwrap()
                                        .apply_mut(new_style.clone());
                                }
                                None => {
                                    let mut current =
                                        existing_rc.downcast_ref::<Style>().unwrap().clone();
                                    current.apply_mut(new_style.clone());
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
                StyleKeyInfo::StructuralSelectors => match self.map.entry(*k) {
                    Entry::Occupied(mut e) => {
                        let new_rules = &v.downcast_ref::<StructuralSelectors>().unwrap().0;
                        let current = &e.get().downcast_ref::<StructuralSelectors>().unwrap().0;
                        let mut merged: StructuralSelectorRules = current.clone();
                        merged.extend(new_rules.iter().cloned());
                        *e.get_mut() = Rc::new(StructuralSelectors(merged));
                    }
                    Entry::Vacant(e) => {
                        e.insert(v.clone());
                    }
                },
                StyleKeyInfo::ResponsiveSelectors => match self.map.entry(*k) {
                    Entry::Occupied(mut e) => {
                        let new_rules = &v.downcast_ref::<ResponsiveSelectors>().unwrap().0;
                        let current = &e.get().downcast_ref::<ResponsiveSelectors>().unwrap().0;
                        let mut merged: ResponsiveSelectorRules = current.clone();
                        merged.extend(new_rules.iter().cloned());
                        *e.get_mut() = Rc::new(ResponsiveSelectors(merged));
                    }
                    Entry::Vacant(e) => {
                        e.insert(v.clone());
                    }
                },
                StyleKeyInfo::Transition | StyleKeyInfo::DebugGroup(..) => {
                    self.map.insert(*k, v.clone());
                }
                StyleKeyInfo::Prop(info) => {
                    // Track inherited props for O(1) early-exit in apply_only_inherited
                    if info.inherited {
                        self.has_inherited = true;
                    }
                    match self.map.entry(*k) {
                        Entry::Occupied(mut e) => {
                            e.insert(v.clone());
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
        source_effect_context: Option<Rc<dyn floem_reactive::EffectTrait>>,
    ) {
        if self.effect_context.is_none() && source_effect_context.is_some() {
            self.effect_context = source_effect_context;
        }
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
                                        .apply_mut_no_mappings(v.clone());
                                }
                                None => {
                                    let mut current =
                                        e.get_mut().downcast_ref::<Style>().unwrap().clone();
                                    current.apply_mut_no_mappings(v.clone());
                                    *e.get_mut() = Rc::new(current);
                                }
                            }
                        }
                        Entry::Vacant(e) => {
                            e.insert(v.clone());
                        }
                    }
                }
                StyleKeyInfo::Transition | StyleKeyInfo::DebugGroup(..) => {
                    self.map.insert(*k, v.clone());
                }
                StyleKeyInfo::StructuralSelectors => match self.map.entry(*k) {
                    Entry::Occupied(mut e) => {
                        let new_rules = &v.downcast_ref::<StructuralSelectors>().unwrap().0;
                        let current = &e.get().downcast_ref::<StructuralSelectors>().unwrap().0;
                        let mut merged: StructuralSelectorRules = current.clone();
                        merged.extend(new_rules.iter().cloned());
                        *e.get_mut() = Rc::new(StructuralSelectors(merged));
                    }
                    Entry::Vacant(e) => {
                        e.insert(v.clone());
                    }
                },
                StyleKeyInfo::ResponsiveSelectors => match self.map.entry(*k) {
                    Entry::Occupied(mut e) => {
                        let new_rules = &v.downcast_ref::<ResponsiveSelectors>().unwrap().0;
                        let current = &e.get().downcast_ref::<ResponsiveSelectors>().unwrap().0;
                        let mut merged: ResponsiveSelectorRules = current.clone();
                        merged.extend(new_rules.iter().cloned());
                        *e.get_mut() = Rc::new(ResponsiveSelectors(merged));
                    }
                    Entry::Vacant(e) => {
                        e.insert(v.clone());
                    }
                },
                StyleKeyInfo::Prop(info) => {
                    // Track inherited props for O(1) early-exit in apply_only_inherited
                    if info.inherited {
                        self.has_inherited = true;
                    }
                    match self.map.entry(*k) {
                        Entry::Occupied(mut e) => {
                            e.insert(v.clone());
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
        // FAST PATH: identical semantic payload identity
        if self.merge_id == over.merge_id {
            return;
        }
        let over_merge_id = over.merge_id;
        let effect_context = over.effect_context.clone();
        self.apply_iter(over.map.iter(), effect_context);
        self.merge_id = combine_merge_ids(self.merge_id, over_merge_id);
        self.context_selectors |= over.context_selectors;
    }

    pub(crate) fn apply_mut_no_mappings(&mut self, over: Style) {
        // FAST PATH: identical semantic payload identity
        if self.merge_id == over.merge_id {
            return;
        }
        let over_merge_id = over.merge_id;
        let effect_context = over.effect_context.clone();
        self.apply_iter_no_mappings(over.map.iter(), effect_context);
        self.merge_id = combine_merge_ids(self.merge_id, over_merge_id);
        self.context_selectors |= over.context_selectors;
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

style_key_selector!(
    hover,
    StyleSelectors::empty().set_selector(StyleSelector::Hover, true)
);
style_key_selector!(
    file_hover,
    StyleSelectors::empty().set_selector(StyleSelector::FileHover, true)
);
style_key_selector!(
    focus,
    StyleSelectors::empty().set_selector(StyleSelector::Focus, true)
);
style_key_selector!(
    focus_visible,
    StyleSelectors::empty().set_selector(StyleSelector::FocusVisible, true)
);
style_key_selector!(
    focus_within,
    StyleSelectors::empty().set_selector(StyleSelector::FocusWithin, true)
);
style_key_selector!(
    disabled,
    StyleSelectors::empty().set_selector(StyleSelector::Disabled, true)
);
style_key_selector!(
    active,
    StyleSelectors::empty().set_selector(StyleSelector::Active, true)
);
style_key_selector!(
    dragging,
    StyleSelectors::empty().set_selector(StyleSelector::Dragging, true)
);
style_key_selector!(
    selected,
    StyleSelectors::empty().set_selector(StyleSelector::Selected, true)
);
style_key_selector!(
    darkmode,
    StyleSelectors::empty().set_selector(StyleSelector::DarkMode, true)
);

impl StyleSelector {
    fn to_key(self) -> StyleKey {
        match self {
            StyleSelector::Hover => hover(),
            StyleSelector::Focus => focus(),
            StyleSelector::FocusVisible => focus_visible(),
            StyleSelector::FocusWithin => focus_within(),
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
    BorderLeftColor border_left_color { nocb, tr }: Option<Brush> {} = None,
    /// Sets the top border color.
    BorderTopColor border_top_color { nocb, tr }: Option<Brush> {} = None,
    /// Sets the right border color.
    BorderRightColor border_right_color { nocb, tr }: Option<Brush> {} = None,
    /// Sets the bottom border color.
    BorderBottomColor border_bottom_color { nocb, tr }: Option<Brush> {} = None,

    /// Sets the top-left border radius.
    BorderTopLeftRadius border_top_left_radius { tr }: PxPct {} = PxPct::Px(0.),
    /// Sets the top-right border radius.
    BorderTopRightRadius border_top_right_radius { tr }: PxPct {} = PxPct::Px(0.),
    /// Sets the bottom-left border radius.
    BorderBottomLeftRadius border_bottom_left_radius { tr }: PxPct {} = PxPct::Px(0.),
    /// Sets the bottom-right border radius.
    BorderBottomRightRadius border_bottom_right_radius { tr }: PxPct {} = PxPct::Px(0.),

    /// Sets the left padding.
    PaddingLeft padding_left { tr }: PxPct {} = PxPct::Px(0.),
    /// Sets the top padding.
    PaddingTop padding_top { tr }: PxPct {} = PxPct::Px(0.),
    /// Sets the right padding.
    PaddingRight padding_right { tr }: PxPct {} = PxPct::Px(0.),
    /// Sets the bottom padding.
    PaddingBottom padding_bottom { tr }: PxPct {} = PxPct::Px(0.),

    /// Sets the left margin.
    MarginLeft margin_left { tr }: PxPctAuto {} = PxPctAuto::Px(0.),
    /// Sets the top margin.
    MarginTop margin_top { tr }: PxPctAuto {} = PxPctAuto::Px(0.),
    /// Sets the right margin.
    MarginRight margin_right { tr }: PxPctAuto {} = PxPctAuto::Px(0.),
    /// Sets the bottom margin.
    MarginBottom margin_bottom { tr }: PxPctAuto {} = PxPctAuto::Px(0.),

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
    /// This is not a global z-index and will only be used as an override to the sorted order of sibling elements.
    /// If you want a view positioned above others, use an overlay.
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
    FontWeight font_weight { nocb }: Option<FontWeightProp> { inherited } = None,

    /// Sets the font style (italic, normal) for text content.
    ///
    /// This property is inherited by child views.
    FontStyle font_style { nocb }: Option<crate::text::FontStyle> { inherited } = None,

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
    // TODO: rename this TextSelectable
    Selectable selectable {}: bool { inherited } = true,

    /// Controls how overflowed text content is handled.
    ///
    /// Determines whether text wraps or gets clipped.
    TextOverflowProp text_overflow {}: TextOverflow { inherited } = TextOverflow::NoWrap(NoWrapOverflow::Clip),

    /// Sets text alignment within the view.
    ///
    /// Controls horizontal alignment of text content.
    TextAlignProp text_align {}: Option<crate::text::Alignment> {} = None,

    /// Sets the line height for text content.
    ///
    /// This property is inherited by child views.
    LineHeight line_height { nocb, tr }: Option<LineHeightValue> { inherited } = None,

    /// Sets the preferred aspect ratio for the view.
    ///
    /// Maintains width-to-height proportions during layout.
    AspectRatio aspect_ratio {tr}: Option<f32> {} = None,

    /// Controls how replaced content (like images) should be resized to fit its container.
    ///
    /// This property specifies how an image or other replaced element should be resized
    /// to fit within its container while potentially preserving its aspect ratio.
    /// Corresponds to the CSS `object-fit` property.
    ObjectFitProp object_fit {}: ObjectFit {} = ObjectFit::Fill,

    /// Sets the gap between columns in grid or flex layouts.
    ///
    /// Creates space between items in the horizontal direction.
    ColGap col_gap { nocb, tr }: PxPct {} = PxPct::Px(0.),

    /// Sets the gap between rows in grid or flex layouts.
    ///
    /// Creates space between items in the vertical direction.
    RowGap row_gap { nocb, tr }: PxPct {} = PxPct::Px(0.),

    /// Width of the scrollbar track in pixels.
    ///
    /// This property reserves space for scrollbars when `overflow_x` or `overflow_y` is set to `Scroll`.
    /// The reserved space reduces the available content area but ensures content doesn't flow under the scrollbar.
    ///
    /// **Layout behavior:**
    /// - When `overflow_y: Scroll`, reserves `scrollbar_width` from the right side of the container
    /// - When `overflow_x: Scroll`, reserves `scrollbar_width` from the bottom of the container
    /// - Space is reserved inside the container's bounds, reducing the content rect size
    /// - No space is reserved for `overflow: Hidden`, `Visible`, or `Clip`
    ///
    /// **Example:**
    /// ```rust,ignore
    /// // Reserve 10px for scrollbar
    /// .scrollbar_width(10.0)
    ///
    /// // Thinner scrollbar for compact UI
    /// .scrollbar_width(6.0)
    /// ```
    ///
    /// **Default:** `8px`
    ScrollbarWidth scrollbar_width {tr}: Px {} = Px(8.),

    /// How children overflowing their container in X axis should affect layout
    OverflowX overflow_x {}: Overflow {} = Overflow::default(),

    /// How children overflowing their container in Y axis should affect layout
    OverflowY overflow_y {}: Overflow {} = Overflow::default(),

    /// Sets the horizontal scale transform.
    ///
    /// Values less than 100% shrink the view, greater than 100% enlarge it.
    /// Scale is applied last in the transform sequence, after translation and rotation.
    /// The scaling occurs around the anchor point specified by `scale_about`.
    /// Transform order: translate → rotate → scale (matches CSS individual transform properties).
    ScaleX scale_x {tr}: Pct {} = Pct(100.),

    /// Sets the vertical scale transform.
    ///
    /// Values less than 100% shrink the view, greater than 100% enlarge it.
    /// Scale is applied last in the transform sequence, after translation and rotation.
    /// The scaling occurs around the anchor point specified by `scale_about`.
    /// Transform order: translate → rotate → scale (matches CSS individual transform properties).
    ScaleY scale_y {tr}: Pct {} = Pct(100.),

    /// Sets the horizontal translation transform.
    ///
    /// Moves the view left (negative) or right (positive).
    /// Translation is applied first in the transform sequence, in the element's local coordinate space.
    /// This matches CSS individual transform properties behavior.
    /// Transform order: translate → rotate → scale.
    TranslateX translate_x {tr}: PxPct {} = PxPct::Px(0.),

    /// Sets the vertical translation transform.
    ///
    /// Moves the view up (negative) or down (positive).
    /// Translation is applied first in the transform sequence, in the element's local coordinate space.
    /// This matches CSS individual transform properties behavior.
    /// Transform order: translate → rotate → scale.
    TranslateY translate_y {tr}: PxPct {} = PxPct::Px(0.),

    /// Sets the rotation transform angle.
    ///
    /// Positive values rotate clockwise, negative values rotate counter-clockwise.
    /// Use `.deg()` or `.rad()` methods to specify the angle unit.
    /// Rotation is applied after translation but before scaling, around the anchor point
    /// specified by `rotate_about`.
    /// Transform order: translate → rotate → scale (matches CSS individual transform properties).
    Rotation rotate {tr}: Angle {} = Angle::Rad(0.0),

    /// Sets the anchor point for rotation transformations.
    ///
    /// Determines the point around which the view rotates. Use predefined constants
    /// like `AnchorAbout::CENTER` or create custom anchor points with pixel or percentage values.
    /// The anchor point is specified in the element's local coordinate space (before any transforms).
    RotateAbout rotate_about {}: AnchorAbout {} = AnchorAbout::CENTER,

    /// Sets the anchor point for scaling transformations.
    ///
    /// Determines the point around which the view scales. Use predefined constants
    /// like `AnchorAbout::CENTER` or create custom anchor points with pixel or percentage values.
    /// The anchor point is specified in the element's local coordinate space (before any transforms).
    /// Transform order: translate → rotate → scale (matches CSS individual transform properties).
    ScaleAbout scale_about {tr}: AnchorAbout {} = AnchorAbout::CENTER,

    /// Sets a custom affine transformation matrix.
    ///
    /// This property allows you to specify an arbitrary 2D affine transformation that will be
    /// applied in addition to the individual transform properties (translate_x, translate_y,
    /// scale_x, scale_y, rotate).
    ///
    /// **Transform application order:**
    /// 1. Individual `translate_x` and `translate_y` properties
    /// 2. Individual `rotate` property
    /// 3. Individual `scale_x` and `scale_y` properties
    /// 4. **This `transform` property (applied last)**
    ///
    /// This matches CSS behavior where individual transform properties are applied before
    /// the `transform` property. The `transform` matrix is applied in the final coordinate
    /// space after all individual transforms.
    ///
    /// **Example:**
    /// ```rust
    /// # use floem::peniko::kurbo::Affine;
    /// # use floem::style::Style;
    /// let _style = Style::new()
    ///     .translate_x(10.0) // Applied first
    ///     .scale(1.5) // Applied second
    ///     .transform(Affine::rotate(0.5)); // Applied last
    /// ```
    Transform transform {tr}: Affine {} = Affine::IDENTITY,

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

    /// Controls whether the view can receive focus during navigation such as tab or arrow navigation.
    Focusable set_focus {}: Focus { } = Focus::None,
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
        // display is used here to just to properly trigger transitions on layout change. it is not transitioned here
        pub border_left: BorderLeft,
        pub border_top: BorderTop,
        pub border_right: BorderRight,
        pub border_bottom: BorderBottom,

        pub padding_left: PaddingLeft,
        pub padding_top: PaddingTop,
        pub padding_right: PaddingRight,
        pub padding_bottom: PaddingBottom,

        pub margin_left: MarginLeft,
        pub margin_top: MarginTop,
        pub margin_right: MarginRight,
        pub margin_bottom: MarginBottom,

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
    /// These are properties that when changed the box tree needs committed.
    pub TransformProps {
        pub scale_x: ScaleX,
        pub scale_y: ScaleY,

        pub translate_x: TranslateX,
        pub translate_y: TranslateY,

        pub rotation: Rotation,
        pub rotate_about: RotateAbout,
        pub scale_about: ScaleAbout,

        pub transform: Transform,

        pub overflow_x: OverflowX,
        pub overflow_y: OverflowY,
        pub border_top_left_radius: BorderTopLeftRadius,
        pub border_top_right_radius: BorderTopRightRadius,
        pub border_bottom_left_radius: BorderBottomLeftRadius,
        pub border_bottom_right_radius: BorderBottomRightRadius,
    }
}
impl TransformProps {
    pub fn border_radius(&self) -> BorderRadius {
        BorderRadius {
            top_left: Some(self.border_top_left_radius()),
            top_right: Some(self.border_top_right_radius()),
            bottom_left: Some(self.border_bottom_left_radius()),
            bottom_right: Some(self.border_bottom_right_radius()),
        }
    }

    pub fn affine(&self, size: kurbo::Size) -> Affine {
        let mut result = Affine::IDENTITY;
        // CANONICAL ORDER (matches CSS individual properties):
        // 1. translate → 2. rotate → 3. scale → 4. transform property

        // 1. Translate
        let transform_x = match self.translate_x() {
            crate::unit::PxPct::Px(px) => px,
            crate::unit::PxPct::Pct(pct) => (pct / 100.) * size.width,
        };
        let transform_y = match self.translate_y() {
            crate::unit::PxPct::Px(px) => px,
            crate::unit::PxPct::Pct(pct) => (pct / 100.) * size.height,
        };
        result *= Affine::translate(Vec2 {
            x: transform_x,
            y: transform_y,
        });

        // 2. Rotate (around rotate_about anchor)
        let rotation = self.rotation().to_radians();
        if rotation != 0.0 {
            let rotate_about = self.rotate_about();
            let (rotate_x_frac, rotate_y_frac) = rotate_about.as_fractions();
            let rotate_point = Vec2 {
                x: rotate_x_frac * size.width,
                y: rotate_y_frac * size.height,
            };
            result *= Affine::translate(rotate_point)
                * Affine::rotate(rotation)
                * Affine::translate(-rotate_point);
        }

        // 3. Scale (around scale_about anchor)
        let scale_x = self.scale_x().0 / 100.;
        let scale_y = self.scale_y().0 / 100.;
        if scale_x != 1.0 || scale_y != 1.0 {
            let scale_about = self.scale_about();
            let (scale_x_frac, scale_y_frac) = scale_about.as_fractions();
            let scale_point = Vec2 {
                x: scale_x_frac * size.width,
                y: scale_y_frac * size.height,
            };
            result *= Affine::translate(scale_point)
                * Affine::scale_non_uniform(scale_x, scale_y)
                * Affine::translate(-scale_point);
        }

        // 4. Apply custom transform property last
        result *= self.transform();
        result
    }

    pub fn clip_rect(&self, mut local_rect: kurbo::Rect) -> Option<RoundedRect> {
        use Overflow::*;

        let (overflow_x, overflow_y) = (self.overflow_x(), self.overflow_y());

        // No clipping if both are visible
        if overflow_x == Visible && overflow_y == Visible {
            return None;
        }

        let border_radius = self
            .border_radius()
            .resolve_border_radii(local_rect.size().min_side());

        // Extend to infinity on visible axes
        if overflow_x == Visible {
            local_rect.x0 = f64::NEG_INFINITY;
            local_rect.x1 = f64::INFINITY;
        }
        if overflow_y == Visible {
            local_rect.y0 = f64::NEG_INFINITY;
            local_rect.y1 = f64::INFINITY;
        }

        Some(RoundedRect::from_rect(local_rect, border_radius))
    }
}

impl LayoutProps {
    pub fn border(&self) -> Border {
        Border {
            left: Some(self.border_left()),
            top: Some(self.border_top()),
            right: Some(self.border_right()),
            bottom: Some(self.border_bottom()),
        }
    }

    pub fn padding(&self) -> Padding {
        Padding {
            left: Some(self.padding_left()),
            top: Some(self.padding_top()),
            right: Some(self.padding_right()),
            bottom: Some(self.padding_bottom()),
        }
    }

    pub fn margin(&self) -> Margin {
        Margin {
            left: Some(self.margin_left()),
            top: Some(self.margin_top()),
            right: Some(self.margin_right()),
            bottom: Some(self.margin_bottom()),
        }
    }

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
        pub selection_color: SelectionColor,
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
                if self.map.remove(&P::key()).is_some() {
                    self.merge_id = next_style_merge_id();
                }
                return self;
            }
        };
        // Track inherited props for O(1) early-exit in apply_only_inherited
        if P::prop_ref().info().inherited {
            self.has_inherited = true;
        }
        self.map.insert(P::key(), Rc::new(insert));
        self.merge_id = next_style_merge_id();
        self
    }

    /// Sets a transition animation for a specific style property.
    pub fn transition<P: StyleProp>(mut self, _prop: P, transition: Transition) -> Self {
        self.map
            .insert(P::prop_ref().info().transition_key, Rc::new(transition));
        self.merge_id = next_style_merge_id();
        self
    }

    fn selector(mut self, selector: StyleSelector, style: impl FnOnce(Style) -> Style) -> Self {
        let over = style(Style::default());
        self.set_selector(selector, over);
        self
    }

    pub(crate) fn structural_selector(
        mut self,
        selector: StructuralSelector,
        style: impl FnOnce(Style) -> Style,
    ) -> Self {
        let over = style(Style::default());
        self.set_structural_selector(selector, over);
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

    /// Similar to the `:focus-visible` css selector, this style only activates when the view was focused via tab or arrow navigation.
    pub fn focus_visible(self, style: impl FnOnce(Style) -> Style) -> Self {
        self.selector(StyleSelector::FocusVisible, style)
    }

    /// Similar to the `:focus-within` css selector, this style activates when this
    /// view or any descendant is in the focus path.
    pub fn focus_within(self, style: impl FnOnce(Style) -> Style) -> Self {
        self.selector(StyleSelector::FocusWithin, style)
    }

    /// Similar to the `:first-child` css selector.
    pub fn first_child(self, style: impl FnOnce(Style) -> Style) -> Self {
        self.structural_selector(StructuralSelector::FirstChild, style)
    }

    /// Similar to the `:last-child` css selector.
    pub fn last_child(self, style: impl FnOnce(Style) -> Style) -> Self {
        self.structural_selector(StructuralSelector::LastChild, style)
    }

    /// Similar to the `:nth-child(...)` css selector.
    pub fn nth_child(self, nth: NthChild, style: impl FnOnce(Style) -> Style) -> Self {
        self.structural_selector(StructuralSelector::NthChild(nth), style)
    }

    /// Convenience for `:nth-child(odd)`.
    pub fn odd(self, style: impl FnOnce(Style) -> Style) -> Self {
        self.nth_child(NthChild::odd(), style)
    }

    /// Convenience for `:nth-child(even)`.
    pub fn even(self, style: impl FnOnce(Style) -> Style) -> Self {
        self.nth_child(NthChild::even(), style)
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
        self.set_responsive_selector(ResponsiveSelector::ScreenSize(size), over);
        self
    }

    /// Applies styles when window width is at least `min`.
    pub fn min_window_width(
        mut self,
        min: impl Into<Px>,
        style: impl FnOnce(Style) -> Style,
    ) -> Self {
        let over = style(Style::default());
        self.set_responsive_selector(ResponsiveSelector::MinWidth(min.into()), over);
        self
    }

    /// Applies styles when window width is at most `max`.
    pub fn max_window_width(
        mut self,
        max: impl Into<Px>,
        style: impl FnOnce(Style) -> Style,
    ) -> Self {
        let over = style(Style::default());
        self.set_responsive_selector(ResponsiveSelector::MaxWidth(max.into()), over);
        self
    }

    /// Applies styles when window width is within `[min, max]` (inclusive).
    pub fn window_width_range(
        mut self,
        min: impl Into<Px>,
        max: impl Into<Px>,
        style: impl FnOnce(Style) -> Style,
    ) -> Self {
        let over = style(Style::default());
        self.set_responsive_selector(
            ResponsiveSelector::WidthRange {
                min: min.into(),
                max: max.into(),
            },
            over,
        );
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

    /// Makes the view fully keyboard navigable.
    ///
    /// The view can receive focus via Tab/Shift+Tab navigation, arrow keys,
    /// pointer clicks, and programmatic focus calls. This is the recommended
    /// setting for interactive controls like buttons, inputs, and links.
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
    ///
    /// Equivalent to `focus(Focus::PointerAndProgrammatic)`.
    pub fn focusable(self) -> Self {
        self.set(Focusable, Focus::PointerAndProgrammatic)
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
    pub fn padding(self, padding: impl Into<PxPct>) -> Self {
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
    pub fn padding_horiz(self, padding: impl Into<PxPct>) -> Self {
        let padding = padding.into();
        self.set(PaddingLeft, padding).set(PaddingRight, padding)
    }

    /// Sets horizontal padding as a percentage of the parent container width.
    pub fn padding_horiz_pct(self, padding: f64) -> Self {
        self.padding_horiz(padding.pct())
    }

    /// Sets `padding_top` and `padding_bottom` to `padding`
    pub fn padding_vert(self, padding: impl Into<PxPct>) -> Self {
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
    pub fn margin(self, margin: impl Into<PxPctAuto>) -> Self {
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
    pub fn margin_horiz(self, margin: impl Into<PxPctAuto>) -> Self {
        let margin = margin.into();
        self.set(MarginLeft, margin).set(MarginRight, margin)
    }

    /// Sets horizontal margin as a percentage of the parent container width.
    pub fn margin_horiz_pct(self, margin: f64) -> Self {
        self.margin_horiz(margin.pct())
    }

    /// Sets `margin_top` and `margin_bottom` to `margin`
    pub fn margin_vert(self, margin: impl Into<PxPctAuto>) -> Self {
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
    pub fn border_radius(self, radius: impl Into<PxPct>) -> Self {
        let radius = radius.into();
        self.set(BorderTopLeftRadius, radius)
            .set(BorderTopRightRadius, radius)
            .set(BorderBottomLeftRadius, radius)
            .set(BorderBottomRightRadius, radius)
    }

    /// Sets the left border color of the view.
    pub fn border_left_color(self, color: impl Into<Brush>) -> Self {
        self.set(BorderLeftColor, Some(color.into()))
    }
    /// Sets the right border color of the view.
    pub fn border_right_color(self, color: impl Into<Brush>) -> Self {
        self.set(BorderRightColor, Some(color.into()))
    }
    /// Sets the top border color of the view.
    pub fn border_top_color(self, color: impl Into<Brush>) -> Self {
        self.set(BorderTopColor, Some(color.into()))
    }
    /// Sets the bottom border color of the view.
    pub fn border_bottom_color(self, color: impl Into<Brush>) -> Self {
        self.set(BorderBottomColor, Some(color.into()))
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
    pub fn font_family(self, family: impl Into<String>) -> Self {
        let family = family.into();
        self.set_style_value(FontFamily, StyleValue::Val(family).map(Some))
    }

    /// Sets the font weight (boldness) for text content.
    pub fn font_weight(self, weight: impl Into<StyleValue<FontWeightProp>>) -> Self {
        self.set_style_value(FontWeight, weight.into().map(Some))
    }

    /// Sets the font weight to bold.
    pub fn font_bold(self) -> Self {
        self.font_weight(FontWeightProp::BOLD)
    }

    /// Sets the font style (italic, normal) for text content.
    pub fn font_style(self, style: impl Into<StyleValue<crate::text::FontStyle>>) -> Self {
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
                Rect {
                    left: LengthPercentage::length(style.border_left().width as f32),
                    top: LengthPercentage::length(style.border_top().width as f32),
                    right: LengthPercentage::length(style.border_right().width as f32),
                    bottom: LengthPercentage::length(style.border_bottom().width as f32),
                }
            },
            padding: {
                Rect {
                    left: style.padding_left().into(),
                    top: style.padding_top().into(),
                    right: style.padding_right().into(),
                    bottom: style.padding_bottom().into(),
                }
            },
            margin: {
                Rect {
                    left: style.margin_left().into(),
                    top: style.margin_top().into(),
                    right: style.margin_right().into(),
                    bottom: style.margin_bottom().into(),
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
            scrollbar_width: style.scrollbar_width().0 as f32,
            ..Default::default()
        }
    }
}
