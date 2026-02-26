//! Style recalculation change tracking.
//!
//! This module provides [`StyleRecalcChange`], a graduated system for tracking
//! what kind of style recalculation is needed. Inspired by Chromium's approach,
//! this enables optimizations like:
//! - Skipping style recalc entirely for unchanged views
//! - Using fast "inherited only" paths when only inherited props changed
//! - Limiting recalc to immediate children vs entire subtrees
//!
//! # Comparison with ChangeFlags
//!
//! The existing [`ChangeFlags`] are binary: a view either needs style recalc or doesn't.
//! [`StyleRecalcChange`] adds *why* the recalc is needed, enabling optimization decisions.

use crate::{
    ElementId,
    style::{StyleClassRef, StyleSelector, StyleSelectors},
};

use bitflags::bitflags;
use smallvec::SmallVec;

bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct StyleReasonFlags: u32 {
        /// The view has style selectors (`:hover`, `:focus`, `:disabled`, etc.) whose
        /// matching result may have changed. Requires full `resolve_nested_maps` to re-run
        /// since selector matching depends on interaction state.
        const SELECTOR = 1 << 0;

        /// One or more animations are actively running on this view. The animation stack
        /// needs to be advanced and `animate_into` applied to the combined style. Implies
        /// the view needs to be scheduled for the next frame via `schedule_style`.
        const ANIMATION = 1 << 1;

        /// A CSS-style property transition is in progress (e.g. background color, opacity).
        /// The transition interpolator needs to be stepped and its output applied to the
        /// view style props (`view_style_props.read_explicit`). Implies `schedule_style`.
        const TRANSITION = 1 << 2;

        /// One or more specific sub-element rectangles owned by this view need restyling,
        /// identified by their `ElementId`. Used when a view owns multiple `ElementId`s
        /// (e.g. a scroll view's content area, vertical scrollbar, horizontal scrollbar)
        /// and only a subset need to be updated. The `targets` vec carries each affected
        /// `ElementId` and the `StyleReasonSet` describing why that specific element is dirty,
        /// allowing `style_pass` to dispatch targeted updates rather than fully restyling
        /// the entire view.
        const TARGET = 1 << 3;

        /// Request that the `style_pass` be run again
        const STYLE_PASS = 1 << 4;

        /// Request that the `style_pass` be run again
        const VISIBILITY = 1 << 5;

        /// Parent's inherited scalar properties changed (font size, color, etc.).
        /// Child must recompute computed_style but may skip resolve_nested_maps
        /// if it has no selectors and its own style stack is clean.
        const INHERITED_CHANGE = 1 << 6;

        /// Parent's class context map changed (new class definitions visible to children).
        /// Child must re-run resolve_nested_maps to pick up new class rules,
        /// but inherited scalar props are unchanged so with_context values are stable.
        const CLASS_CONTEXT_CHANGE = 1 << 7;

    }
}

/// Describes why a view has been marked style-dirty. Combines a cheap bitmap of
/// reason categories with optional payloads for the reasons that carry associated data.
///
/// The flags are the primary signal used to decide *how much work* `style_view` needs
/// to do. The payload fields provide the details needed to actually do that work.
///
/// Invariants:
/// - `selectors.is_some()` iff `flags.contains(SELECTOR)`
/// - `targets.is_empty()` iff `!flags.contains(TARGET)`
#[derive(Clone, Debug)]
pub struct StyleReasonSet {
    pub flags: StyleReasonFlags,

    /// The selector set that caused this view to be dirty. Present when `SELECTOR` is set.
    /// Stored so that the style pass can skip `resolve_nested_maps` if the selectors that
    /// changed don't actually affect any properties (e.g. a `:hover` selector exists but
    /// none of the now-hovered selectors match any style rules on this view).
    pub selectors: Option<StyleSelectors>,

    pub classes_changed: Option<SmallVec<[StyleClassRef; 4]>>,

    /// Sub-element rectangles owned by this view that need targeted restyling.
    /// Each entry is `(element_id, reason)` where `element_id` identifies a specific
    /// box-tree rectangle (e.g. the vertical scrollbar of a scroll view) and `reason`
    /// describes why that rectangle is dirty (typically `SELECTOR` due to hover/press
    /// state change on that specific hit region).
    ///
    /// Present when `TARGET` is set. `SmallVec<2>` because most views with sub-elements
    /// have only a small fixed number of them (e.g. scroll view has 3 total, rarely more
    /// than 2 dirty simultaneously).
    pub targets: Vec<(crate::ElementId, StyleReasonSet)>,
}

impl StyleReasonSet {
    pub fn empty() -> Self {
        Self {
            flags: StyleReasonFlags::empty(),
            selectors: None,
            classes_changed: None,
            targets: Vec::new(),
        }
    }

    pub fn with_selectors(selectors: StyleSelectors) -> Self {
        let mut s = Self::empty();
        s.set_selectors(selectors);
        s
    }

    pub fn with_selector(selector: StyleSelector) -> Self {
        let mut s = Self::empty();
        s.set_selectors(StyleSelectors::empty().set_selector(selector, true));
        s
    }

    pub fn is_empty(&self) -> bool {
        self.flags.is_empty()
    }

    // --- Setters ---

    pub fn set_inherited(&mut self) {
        self.flags |= StyleReasonFlags::INHERITED_CHANGE;
    }

    pub fn set_class_cx(&mut self) {
        self.flags |= StyleReasonFlags::CLASS_CONTEXT_CHANGE;
    }

    pub fn set_animation(&mut self) {
        self.flags |= StyleReasonFlags::ANIMATION;
    }

    pub fn set_transition(&mut self) {
        self.flags |= StyleReasonFlags::TRANSITION;
    }

    pub fn set_style_pass(&mut self) {
        self.flags |= StyleReasonFlags::STYLE_PASS;
    }

    pub fn set_visibility(&mut self) {
        self.flags |= StyleReasonFlags::VISIBILITY;
    }

    pub fn set_selectors(&mut self, selectors: StyleSelectors) {
        self.flags |= StyleReasonFlags::SELECTOR;
        self.selectors = Some(selectors);
    }

    pub fn add_target(&mut self, id: crate::ElementId, reason: StyleReasonSet) {
        self.flags |= StyleReasonFlags::TARGET;
        self.targets.push((id, reason));
    }

    pub fn animation() -> Self {
        let mut s = Self::empty();
        s.set_animation();
        s
    }

    pub fn style_pass() -> Self {
        let mut s = Self::empty();
        s.set_style_pass();
        s
    }

    pub fn visibility() -> Self {
        let mut s = Self::empty();
        s.set_visibility();
        s
    }

    pub fn transition() -> Self {
        let mut s = Self::empty();
        s.set_transition();
        s
    }

    pub fn inherited() -> Self {
        let mut s = Self::empty();
        s.set_inherited();
        s
    }

    pub fn class_cx(classes: SmallVec<[StyleClassRef; 4]>) -> Self {
        let mut s = Self::empty();
        s.set_class_cx();
        s.classes_changed = Some(classes);
        s
    }

    // --- Queries ---

    /// Returns true if this reason set requires a full cascade recomputation
    /// (i.e. `resolve_nested_maps` must run). False means only animation/transition
    /// stepping is needed and the cached `combined_style` can be reused.
    pub fn needs_resolve_nested_maps(&self) -> bool {
        self.flags.intersects(
            StyleReasonFlags::SELECTOR
                | StyleReasonFlags::CLASS_CONTEXT_CHANGE
                | StyleReasonFlags::INHERITED_CHANGE,
        )
    }

    pub fn needs_animation(&self) -> bool {
        self.flags.intersects(StyleReasonFlags::ANIMATION)
    }

    pub fn needs_property_extraction(&self) -> bool {
        self.needs_resolve_nested_maps()
            || self.needs_animation()
            || self.flags.intersects(StyleReasonFlags::TRANSITION)
            || self.flags.intersects(StyleReasonFlags::INHERITED_CHANGE)
            || self
                .flags
                .intersects(StyleReasonFlags::CLASS_CONTEXT_CHANGE)
            || self.flags.intersects(StyleReasonFlags::TRANSITION)
            || self.flags.intersects(StyleReasonFlags::VISIBILITY)
    }

    pub fn needs_style_pass(&self) -> bool {
        self.needs_resolve_nested_maps()
            || self.needs_animation()
            || self.needs_property_extraction()
            || self.has_target()
            || self.flags.intersects(StyleReasonFlags::VISIBILITY)
            || self.flags.intersects(StyleReasonFlags::STYLE_PASS)
    }

    pub fn has_animation(&self) -> bool {
        self.flags.contains(StyleReasonFlags::ANIMATION)
    }

    pub fn has_visiblity(&self) -> bool {
        self.flags.contains(StyleReasonFlags::VISIBILITY)
    }

    pub fn has_transition(&self) -> bool {
        self.flags.contains(StyleReasonFlags::TRANSITION)
    }

    pub fn has_selector(&self) -> bool {
        self.flags.contains(StyleReasonFlags::SELECTOR)
    }

    pub fn has_target(&self) -> bool {
        self.flags.contains(StyleReasonFlags::TARGET)
    }

    pub fn has_target_id(&self, id: crate::ElementId) -> bool {
        self.targets.iter().any(|(tid, _)| *tid == id)
    }

    pub fn target_reason(&self, id: crate::ElementId) -> Option<&StyleReasonSet> {
        self.targets
            .iter()
            .find(|(tid, _)| *tid == id)
            .map(|(_, r)| r)
    }

    // --- Merging ---

    /// Merge another set into self, unioning all reasons.
    pub fn merge(&mut self, other: StyleReasonSet) {
        self.flags |= other.flags;

        // Merge selectors (union, not replace)
        match (self.selectors, other.selectors) {
            (Some(a), Some(b)) => self.selectors = Some(a | b),
            (None, Some(b)) => self.selectors = Some(b),
            _ => {}
        }

        // Merge classes if present
        match (&mut self.classes_changed, other.classes_changed) {
            (Some(a), Some(b)) => {
                for class in b {
                    if !a.contains(&class) {
                        a.push(class);
                    }
                }
            }
            (None, Some(b)) => {
                self.classes_changed = Some(b);
            }
            _ => {}
        }

        // Merge per-target reasons
        for (id, reason) in other.targets {
            if let Some((_, existing)) = self.targets.iter_mut().find(|(tid, _)| *tid == id) {
                existing.merge(reason);
            } else {
                self.targets.push((id, reason));
            }
        }
    }

    /// Returns a new set containing only the reasons that match the given flags.
    pub fn filter(&self, flags: StyleReasonFlags) -> StyleReasonSet {
        let mut out = StyleReasonSet::empty();
        let masked = self.flags & flags;
        out.flags = masked;

        if masked.contains(StyleReasonFlags::SELECTOR) {
            out.selectors = self.selectors;
        }
        if masked.contains(StyleReasonFlags::TARGET) {
            out.targets = self.targets.clone();
        }
        out
    }

    /// Remove a specific flag and its associated data.
    pub fn clear_flag(&mut self, flag: StyleReasonFlags) {
        self.flags.remove(flag);
        if flag.contains(StyleReasonFlags::SELECTOR) {
            self.selectors = None;
        }
        if flag.contains(StyleReasonFlags::CLASS_CONTEXT_CHANGE) {
            self.classes_changed = None;
        }
        if flag.contains(StyleReasonFlags::TARGET) {
            self.targets.clear();
        }
    }

    pub fn clear_target(&mut self, id: crate::ElementId) {
        self.targets.retain(|(tid, _)| *tid != id);
        if self.targets.is_empty() {
            self.flags.remove(StyleReasonFlags::TARGET);
        }
    }

    pub fn for_children(&self) -> StyleReasonSet {
        let mut out = self.clone();
        out.targets.clear();
        out.flags.remove(StyleReasonFlags::TARGET);
        out.flags.remove(StyleReasonFlags::ANIMATION);
        out.flags.remove(StyleReasonFlags::STYLE_PASS);
        // visibility is handled later
        out.flags.remove(StyleReasonFlags::VISIBILITY);

        if let Some(selectors) = out.selectors {
            let propagating = selectors.propagating();
            if propagating.is_empty() {
                out.selectors = None;
                out.flags.remove(StyleReasonFlags::SELECTOR);
            } else {
                out.selectors = Some(propagating);
            }
        }

        out
    }

    pub fn with_target(element_id: ElementId, reason: StyleReasonSet) -> Self {
        let mut s = Self::empty();
        s.add_target(element_id, reason);
        s
    }

    pub fn with_selectors_and_target(element_id: ElementId, selectors: StyleSelectors) -> Self {
        let inner = StyleReasonSet::with_selectors(selectors);
        Self::with_target(element_id, inner)
    }

    /// All flags set — forces a full recalc with no fast paths possible.
    /// Use as a fallback when the specific reason is unknown.
    pub fn full_recalc() -> Self {
        Self {
            flags: StyleReasonFlags::all(),
            selectors: None,
            targets: Vec::new(),
            classes_changed: None,
        }
    }
}
