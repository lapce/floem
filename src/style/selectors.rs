//! Style selector types for pseudo-class states
//!
//! This module provides [`StyleSelector`] enum and [`StyleSelectors`] bitmask
//! for tracking pseudo-class states like hover, focus, active, etc.

use bitflags::bitflags;

bitflags! {
    #[derive(Copy, Clone, Eq, PartialEq, Hash, Default)]
    pub struct StyleSelectors: u16 {
        const HOVER         = 1 << 0;
        const FOCUS         = 1 << 1;
        const FOCUS_VISIBLE = 1 << 2;
        const FOCUS_WITHIN  = 1 << 3;
        const DISABLED      = 1 << 4;
        const DARK_MODE     = 1 << 5;
        const ACTIVE        = 1 << 6;
        const DRAGGING      = 1 << 7;
        const SELECTED      = 1 << 8;
        const FILE_HOVER    = 1 << 9;
        const RESPONSIVE    = 1 << 10;
    }
}

/// Pseudo-class selectors for conditional styling
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum StyleSelector {
    Hover,
    Focus,
    FocusVisible,
    FocusWithin,
    Disabled,
    DarkMode,
    Active,
    Dragging,
    Selected,
    FileHover,
}

/// `an + b` expression used by `:nth-child(...)`.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct NthChild {
    pub a: isize,
    pub b: isize,
}

impl NthChild {
    /// Match odd indices: `2n + 1`
    pub const fn odd() -> Self {
        Self { a: 2, b: 1 }
    }

    /// Match even indices: `2n`
    pub const fn even() -> Self {
        Self { a: 2, b: 0 }
    }

    /// Match exactly one index.
    pub const fn exact(index: usize) -> Self {
        Self {
            a: 0,
            b: index as isize,
        }
    }

    /// Match CSS-style `an + b`.
    pub const fn an_plus_b(a: isize, b: isize) -> Self {
        Self { a, b }
    }

    pub fn matches(self, index: usize) -> bool {
        if index == 0 {
            return false;
        }
        let index = index as isize;
        if self.a == 0 {
            return index == self.b;
        }
        let diff = index - self.b;
        if diff % self.a != 0 {
            return false;
        }
        diff / self.a >= 0
    }
}

/// Parameterized structural selectors that depend on sibling position.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum StructuralSelector {
    FirstChild,
    LastChild,
    NthChild(NthChild),
}

impl StructuralSelector {
    pub fn matches(&self, child_index: Option<usize>, sibling_count: usize) -> bool {
        let Some(index) = child_index else {
            return false;
        };
        match self {
            StructuralSelector::FirstChild => index == 1,
            StructuralSelector::LastChild => sibling_count > 0 && index == sibling_count,
            StructuralSelector::NthChild(expr) => expr.matches(index),
        }
    }
}

impl StyleSelector {
    pub const fn all() -> &'static [StyleSelector] {
        &[
            StyleSelector::Hover,
            StyleSelector::Focus,
            StyleSelector::FocusVisible,
            StyleSelector::FocusWithin,
            StyleSelector::Disabled,
            StyleSelector::Active,
            StyleSelector::Dragging,
            StyleSelector::Selected,
            StyleSelector::DarkMode,
            StyleSelector::FileHover,
        ]
    }

    pub const fn name(self) -> &'static str {
        match self {
            StyleSelector::Hover => "Hover",
            StyleSelector::Focus => "Focus",
            StyleSelector::FocusVisible => "FocusVisible",
            StyleSelector::FocusWithin => "FocusWithin",
            StyleSelector::Disabled => "Disabled",
            StyleSelector::Active => "Active",
            StyleSelector::Dragging => "Dragging",
            StyleSelector::Selected => "Selected",
            StyleSelector::DarkMode => "DarkMode",
            StyleSelector::FileHover => "FileHover",
        }
    }

    pub const fn flag(self) -> StyleSelectors {
        match self {
            StyleSelector::Hover => StyleSelectors::HOVER,
            StyleSelector::Focus => StyleSelectors::FOCUS,
            StyleSelector::FocusVisible => StyleSelectors::FOCUS_VISIBLE,
            StyleSelector::FocusWithin => StyleSelectors::FOCUS_WITHIN,
            StyleSelector::Disabled => StyleSelectors::DISABLED,
            StyleSelector::DarkMode => StyleSelectors::DARK_MODE,
            StyleSelector::Active => StyleSelectors::ACTIVE,
            StyleSelector::Dragging => StyleSelectors::DRAGGING,
            StyleSelector::Selected => StyleSelectors::SELECTED,
            StyleSelector::FileHover => StyleSelectors::FILE_HOVER,
        }
    }
}

const PROPAGATING_FLAGS: StyleSelectors = StyleSelectors::DISABLED
    .union(StyleSelectors::DARK_MODE)
    .union(StyleSelectors::DRAGGING)
    .union(StyleSelectors::SELECTED)
    .union(StyleSelectors::DISABLED)
    .union(StyleSelectors::RESPONSIVE);

impl StyleSelectors {
    pub const fn set_selector(self, selector: StyleSelector, value: bool) -> Self {
        if value {
            self.union(selector.flag())
        } else {
            self.difference(selector.flag())
        }
    }

    pub fn has(self, selector: StyleSelector) -> bool {
        self.contains(selector.flag())
    }

    pub(crate) const fn responsive(self) -> Self {
        self.union(StyleSelectors::RESPONSIVE)
    }

    pub fn has_responsive(self) -> bool {
        self.contains(StyleSelectors::RESPONSIVE)
    }

    pub fn propagating(self) -> StyleSelectors {
        self & PROPAGATING_FLAGS
    }

    pub fn has_propagating(self) -> bool {
        !self.propagating().is_empty()
    }

    pub fn active_selectors(self) -> Vec<&'static str> {
        StyleSelector::all()
            .iter()
            .filter(|&&s| self.has(s))
            .map(|&s| s.name())
            .collect()
    }

    pub fn debug_string(self) -> String {
        let parts = self.active_selectors();
        let responsive = self.has_responsive();
        match (parts.is_empty(), responsive) {
            (true, false) => "None".to_string(),
            (true, true) => "Responsive".to_string(),
            (false, false) => parts.join(" + "),
            (false, true) => format!("{} (Responsive)", parts.join(" + ")),
        }
    }
}

impl std::fmt::Debug for StyleSelectors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "StyleSelectors({})", self.debug_string())
    }
}
