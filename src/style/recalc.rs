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

use bitflags::bitflags;

/// Describes how style changes should propagate to children.
///
/// This enum is ordered by "intensity" - higher variants require more work.
/// The ordering enables `max()` comparisons when combining changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Propagate {
    /// No children need style updates.
    #[default]
    None,

    /// Only update pseudo-elements (like scrollbars).
    /// Children can keep their existing computed styles.
    UpdatePseudoElements,

    /// Only inherited properties changed (font-size, color, etc.).
    /// Children can use a fast path that skips full selector resolution
    /// and only propagates the changed inherited values.
    ///
    /// This is a significant optimization: if a parent's font-size changes,
    /// children using `em` units need recalc, but we can skip re-matching
    /// selectors like :hover, classes, etc.
    InheritedOnly,

    /// Recalculate style for immediate children.
    /// Typically used when a class was applied that children might match.
    RecalcChildren,

    /// Recalculate style for all descendants.
    /// Used when a deep structural change occurred (e.g., selector that
    /// matches nested elements changed).
    RecalcDescendants,
}

impl Propagate {
    /// Returns true if this propagation requires any child traversal.
    pub fn requires_child_traversal(&self) -> bool {
        *self != Propagate::None
    }

    /// Returns true if this propagation requires full style resolution.
    /// When false, the "inherited only" fast path can be used.
    pub fn requires_full_resolution(&self) -> bool {
        matches!(
            self,
            Propagate::RecalcChildren | Propagate::RecalcDescendants
        )
    }

    /// Returns true if all descendants need recalc, not just immediate children.
    pub fn is_recursive(&self) -> bool {
        *self == Propagate::RecalcDescendants
    }
}

bitflags! {
    /// Additional flags that modify how style recalculation proceeds.
    ///
    /// These flags can be combined with [`Propagate`] to express complex
    /// recalculation requirements.
    #[derive(Default, Copy, Clone, Debug, PartialEq, Eq)]
    pub struct RecalcFlags: u16 {
        /// Force layout tree rebuild for affected elements.
        const REATTACH = 1 << 0;

        /// Inherited disabled state changed.
        /// Views with :disabled selectors need recalc.
        const DISABLED_CHANGED = 1 << 1;

        /// Inherited selected state changed.
        /// Views with :selected selectors need recalc.
        const SELECTED_CHANGED = 1 << 2;

        /// Dark mode changed.
        /// Views with dark_mode() selectors need recalc.
        const DARK_MODE_CHANGED = 1 << 3;

        /// Screen size breakpoint changed.
        /// Views with responsive() selectors need recalc.
        const RESPONSIVE_CHANGED = 1 << 4;

        /// A class was added or removed.
        /// Children that might match that class need recalc.
        const CLASS_CHANGED = 1 << 5;

        /// Suppress recalc for this view (used during container queries).
        const SUPPRESS_RECALC = 1 << 6;

        /// Font-relative units may have changed (rem, em, ch, etc.).
        const FONT_UNITS_CHANGED = 1 << 7;
    }
}

/// Tracks what kind of style recalculation is needed.
///
/// This is passed down through the style tree during recalc, allowing
/// parent changes to inform child recalculation decisions.
///
/// # Examples
///
/// ```ignore
/// // When parent's font-size changes:
/// let change = StyleRecalcChange::new(Propagate::InheritedOnly);
///
/// // When a class is applied:
/// let change = StyleRecalcChange::new(Propagate::RecalcChildren)
///     .with_flags(RecalcFlags::CLASS_CHANGED);
///
/// // When dark mode toggles:
/// let change = StyleRecalcChange::new(Propagate::RecalcDescendants)
///     .with_flags(RecalcFlags::DARK_MODE_CHANGED);
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct StyleRecalcChange {
    /// How to propagate changes to children.
    propagate: Propagate,
    /// Additional flags modifying recalc behavior.
    flags: RecalcFlags,
}

impl StyleRecalcChange {
    /// No changes needed.
    pub const NONE: Self = Self {
        propagate: Propagate::None,
        flags: RecalcFlags::empty(),
    };

    /// Create a new change with the specified propagation level.
    pub fn new(propagate: Propagate) -> Self {
        Self {
            propagate,
            flags: RecalcFlags::empty(),
        }
    }

    /// Add flags to this change.
    pub fn with_flags(mut self, flags: RecalcFlags) -> Self {
        self.flags |= flags;
        self
    }

    /// Returns the propagation level.
    pub fn propagate(&self) -> Propagate {
        self.propagate
    }

    /// Returns the flags.
    pub fn flags(&self) -> RecalcFlags {
        self.flags
    }

    /// Returns true if no recalculation is needed.
    pub fn is_empty(&self) -> bool {
        self.propagate == Propagate::None && self.flags.is_empty()
    }

    /// Compute the change for processing children.
    ///
    /// When moving from parent to child, we adjust the propagation level:
    /// - RecalcDescendants stays the same (all descendants need recalc)
    /// - RecalcChildren becomes None (only immediate children were marked)
    /// - InheritedOnly stays the same (inherited props propagate)
    /// - Other levels become None
    pub fn for_children(&self) -> Self {
        let child_propagate = match self.propagate {
            Propagate::RecalcDescendants => Propagate::RecalcDescendants,
            Propagate::InheritedOnly => Propagate::InheritedOnly,
            _ => Propagate::None,
        };

        // Remove SUPPRESS_RECALC when moving to children
        let child_flags = self.flags - RecalcFlags::SUPPRESS_RECALC;

        Self {
            propagate: child_propagate,
            flags: child_flags,
        }
    }

    /// Combine two changes, taking the more intensive propagation.
    ///
    /// When multiple sources contribute to a style change (e.g., both
    /// a class change and a hover state change), we need the union of
    /// their requirements.
    pub fn combine(&self, other: &Self) -> Self {
        Self {
            propagate: self.propagate.max(other.propagate),
            flags: self.flags | other.flags,
        }
    }

    /// Upgrade to at least the given propagation level.
    pub fn ensure_at_least(&self, propagate: Propagate) -> Self {
        Self {
            propagate: self.propagate.max(propagate),
            flags: self.flags,
        }
    }

    /// Force full descendant recalculation.
    pub fn force_recalc_descendants(&self) -> Self {
        Self {
            propagate: Propagate::RecalcDescendants,
            flags: self.flags,
        }
    }

    /// Force children to recalculate.
    pub fn force_recalc_children(&self) -> Self {
        Self {
            propagate: self.propagate.max(Propagate::RecalcChildren),
            flags: self.flags,
        }
    }

    /// Mark as reattach needed.
    pub fn force_reattach(&self) -> Self {
        self.with_flags(RecalcFlags::REATTACH)
    }

    /// Should this view's style be recalculated?
    ///
    /// Returns true if either:
    /// - The view itself is marked dirty
    /// - The parent change requires recalculating children
    pub fn should_recalc(&self, view_is_dirty: bool) -> bool {
        if self.flags.contains(RecalcFlags::SUPPRESS_RECALC) {
            return false;
        }
        view_is_dirty || self.propagate.requires_child_traversal()
    }

    /// Can we use the "inherited only" fast path?
    ///
    /// Returns true if we only need to propagate inherited properties
    /// and can skip full selector resolution.
    pub fn can_use_inherited_fast_path(&self, view_has_selectors: bool) -> bool {
        // If the view has state-dependent selectors, we can't use the fast path
        // because those selectors might match differently now
        if view_has_selectors {
            return false;
        }

        // Only use fast path for InheritedOnly propagation
        self.propagate == Propagate::InheritedOnly
            && !self.flags.intersects(
                RecalcFlags::DISABLED_CHANGED
                    | RecalcFlags::SELECTED_CHANGED
                    | RecalcFlags::DARK_MODE_CHANGED
                    | RecalcFlags::CLASS_CHANGED,
            )
    }

    /// Check if reattach is needed.
    pub fn needs_reattach(&self) -> bool {
        self.flags.contains(RecalcFlags::REATTACH)
    }

    /// Check if font-relative units may have changed.
    pub fn font_units_may_have_changed(&self) -> bool {
        self.flags.contains(RecalcFlags::FONT_UNITS_CHANGED)
            || self.propagate == Propagate::RecalcDescendants
    }
}

/// Tracks which inherited properties actually changed.
///
/// This enables the "independent inheritance" optimization:
/// when only specific inherited props change, we can propagate
/// just those values without full style resolution.
#[derive(Debug, Clone, Default)]
pub struct InheritedChanges {
    /// Bit flags for which inherited property groups changed.
    changed_groups: InheritedGroups,
}

bitflags! {
    /// Groups of inherited properties that can change together.
    #[derive(Default, Copy, Clone, Debug, PartialEq, Eq)]
    pub struct InheritedGroups: u8 {
        /// Font properties: font-size, font-family, font-weight, etc.
        const FONT = 1 << 0;
        /// Text properties: color, text-align, etc.
        const TEXT = 1 << 1;
        /// Other inherited properties.
        const OTHER = 1 << 2;
    }
}

impl InheritedChanges {
    /// Create with specific changed groups.
    pub fn with_groups(groups: InheritedGroups) -> Self {
        Self {
            changed_groups: groups,
        }
    }

    /// Check if any inherited properties changed.
    pub fn has_changes(&self) -> bool {
        !self.changed_groups.is_empty()
    }

    /// Check if font properties changed.
    pub fn font_changed(&self) -> bool {
        self.changed_groups.contains(InheritedGroups::FONT)
    }

    /// Check if text properties changed.
    pub fn text_changed(&self) -> bool {
        self.changed_groups.contains(InheritedGroups::TEXT)
    }

    /// Combine with another set of changes.
    pub fn combine(&self, other: &Self) -> Self {
        Self {
            changed_groups: self.changed_groups | other.changed_groups,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_propagate_ordering() {
        assert!(Propagate::None < Propagate::UpdatePseudoElements);
        assert!(Propagate::UpdatePseudoElements < Propagate::InheritedOnly);
        assert!(Propagate::InheritedOnly < Propagate::RecalcChildren);
        assert!(Propagate::RecalcChildren < Propagate::RecalcDescendants);
    }

    #[test]
    fn test_combine_takes_max() {
        let a = StyleRecalcChange::new(Propagate::InheritedOnly);
        let b = StyleRecalcChange::new(Propagate::RecalcChildren);
        let combined = a.combine(&b);
        assert_eq!(combined.propagate(), Propagate::RecalcChildren);
    }

    #[test]
    fn test_for_children() {
        // RecalcDescendants stays
        let change = StyleRecalcChange::new(Propagate::RecalcDescendants);
        assert_eq!(
            change.for_children().propagate(),
            Propagate::RecalcDescendants
        );

        // RecalcChildren becomes None (only immediate children)
        let change = StyleRecalcChange::new(Propagate::RecalcChildren);
        assert_eq!(change.for_children().propagate(), Propagate::None);

        // InheritedOnly propagates
        let change = StyleRecalcChange::new(Propagate::InheritedOnly);
        assert_eq!(change.for_children().propagate(), Propagate::InheritedOnly);
    }

    #[test]
    fn test_inherited_fast_path() {
        let change = StyleRecalcChange::new(Propagate::InheritedOnly);
        assert!(change.can_use_inherited_fast_path(false));
        assert!(!change.can_use_inherited_fast_path(true)); // has selectors

        let change = StyleRecalcChange::new(Propagate::RecalcChildren);
        assert!(!change.can_use_inherited_fast_path(false));

        let change =
            StyleRecalcChange::new(Propagate::InheritedOnly).with_flags(RecalcFlags::CLASS_CHANGED);
        assert!(!change.can_use_inherited_fast_path(false)); // class changed
    }

    #[test]
    fn test_suppress_recalc() {
        let change = StyleRecalcChange::new(Propagate::RecalcChildren)
            .with_flags(RecalcFlags::SUPPRESS_RECALC);
        assert!(!change.should_recalc(true)); // suppressed even if dirty
        assert!(!change.should_recalc(false));
    }
}
