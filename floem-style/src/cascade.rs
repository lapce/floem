//! Style cascade and selector resolution.
//!
//! This module holds the nested-map resolution pipeline ([`resolve_nested_maps`]
//! and friends), the responsive/structural selector containers, and the
//! `selector_xs..xxl` responsive breakpoint keys referenced by
//! `resolve_selectors`.

use std::any::Any;
use std::rc::Rc;

use rustc_hash::FxHashMap;
use smallvec::SmallVec;

use crate::interaction::InteractionState;
use crate::props::{StyleClassRef, StyleKey, RESPONSIVE_SELECTORS_INFO, STRUCTURAL_SELECTORS_INFO};
use crate::responsive::{GridBreakpoints, ScreenSize, ScreenSizeBp};
use crate::selectors::{StructuralSelector, StyleSelector, StyleSelectorKey, StyleSelectors};
use crate::style::Style;
use crate::style_key_selector;
use crate::unit::Pt;

#[doc(hidden)]
pub type StructuralSelectorRules = SmallVec<[(StructuralSelector, Rc<Style>); 2]>;
#[doc(hidden)]
pub type ResponsiveSelectorRules = SmallVec<[(ResponsiveSelector, Rc<Style>); 2]>;

#[doc(hidden)]
pub fn fx_hash_map_with_capacity<K, V>(capacity: usize) -> FxHashMap<K, V> {
    FxHashMap::with_capacity_and_hasher(capacity, Default::default())
}

#[doc(hidden)]
pub fn take_any<T: Any + Clone>(value: Rc<dyn Any>) -> T {
    Rc::downcast::<T>(value)
        .map(|rc| Rc::try_unwrap(rc).unwrap_or_else(|rc| (*rc).clone()))
        .unwrap_or_else(|_| panic!("unexpected style map payload type"))
}

#[derive(Clone)]
pub struct StructuralSelectors(#[doc(hidden)] pub StructuralSelectorRules);

#[derive(Clone)]
pub struct ResponsiveSelectors(#[doc(hidden)] pub ResponsiveSelectorRules);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ResponsiveSelector {
    ScreenSize(ScreenSize),
    MinWidth(Pt),
    MaxWidth(Pt),
    WidthRange { min: Pt, max: Pt },
}

impl ResponsiveSelector {
    pub(crate) fn matches(&self, width: f64) -> bool {
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

pub(crate) fn structural_selectors_key() -> StyleKey {
    StyleKey {
        info: &STRUCTURAL_SELECTORS_INFO,
    }
}

pub(crate) fn responsive_selectors_key() -> StyleKey {
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

    let class_style = resolve_classes(
        classes,
        interact_state,
        screen_size_bp,
        class_context,
        &mut selectors,
    );
    selectors |= class_style.selectors();
    let view_style = resolve_style(style, interact_state, screen_size_bp, &mut selectors);
    let result = class_style
        .apply(view_style)
        .with_inherited_context(inherited_context);
    result.run_deferred_effects();
    let _ = effect_context;
    (result, selectors)
}

fn resolve_classes(
    classes: &[StyleClassRef],
    interact_state: &InteractionState,
    screen_size_bp: ScreenSizeBp,
    class_context: &Style,
    selectors: &mut StyleSelectors,
) -> Style {
    let mut result = Style::with_capacity(classes.len());

    for class in classes {
        if let Some(map) = class_context.get_nested_map(class.key) {
            let resolved = resolve_style(map.clone(), interact_state, screen_size_bp, selectors);
            result.apply_mut(&resolved);
        }
    }

    result
}

fn resolve_style(
    style: Style,
    interact_state: &InteractionState,
    screen_size_bp: ScreenSizeBp,
    selectors: &mut StyleSelectors,
) -> Style {
    resolve_selectors(style, interact_state, screen_size_bp, selectors)
}

fn resolve_selectors(
    mut style: Style,
    interact_state: &InteractionState,
    screen_size_bp: ScreenSizeBp,
    selectors: &mut StyleSelectors,
) -> Style {
    *selectors |= style.selectors();

    // Validate cached selectors in debug builds
    #[cfg(debug_assertions)]
    debug_assert!(
        style
            .cached_selectors
            .contains(style.compute_selectors_slow()),
        "cached_selectors {:?} missing bits from computed {:?}",
        style.cached_selectors,
        style.compute_selectors_slow()
    );

    const MAX_DEPTH: u32 = 20;
    let mut depth = 0;

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
                    style.apply_mut(map.as_ref());
                    changed = true;
                }
            }
        }

        // Apply responsive selectors (parameterized)
        if let Some(responsive_rules) = extract_responsive_selectors(&mut style) {
            for (selector, map) in responsive_rules {
                if selector.matches(interact_state.window_width) {
                    style.apply_mut(map.as_ref());
                    changed = true;
                }
            }
        }

        // Helper to apply a nested map and collect any context mappings from it
        let apply_nested = |style: &mut Style, key: StyleKey| -> bool {
            if let Some(map) = style.get_nested_map(key) {
                style.apply_mut(&map);
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
        if interact_state.is_disabled || style.get(crate::builtin_props::Disabled) {
            if apply_nested(&mut style, StyleSelector::Disabled.to_key()) {
                changed = true;
            }
        } else {
            // Selected
            if (interact_state.is_selected || style.get(crate::builtin_props::Selected))
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

    style
}

pub(crate) fn extract_structural_selectors(style: &mut Style) -> Option<StructuralSelectorRules> {
    let key = structural_selectors_key();
    style
        .map_mut()
        .remove(&key)
        .map(|rc| take_any::<StructuralSelectors>(rc).0)
}

pub(crate) fn extract_responsive_selectors(style: &mut Style) -> Option<ResponsiveSelectorRules> {
    let key = responsive_selectors_key();
    style
        .map_mut()
        .remove(&key)
        .map(|rc| take_any::<ResponsiveSelectors>(rc).0)
}

