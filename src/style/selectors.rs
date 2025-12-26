//! Style selector types for pseudo-class states
//!
//! This module provides [`StyleSelector`] enum and [`StyleSelectors`] bitmask
//! for tracking pseudo-class states like hover, focus, active, etc.

/// Pseudo-class selectors for conditional styling
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum StyleSelector {
    Hover,
    Focus,
    FocusVisible,
    Disabled,
    DarkMode,
    Active,
    Dragging,
    Selected,
    FileHover,
}

impl StyleSelector {
    pub const fn all() -> &'static [StyleSelector] {
        &[
            StyleSelector::Hover,
            StyleSelector::Focus,
            StyleSelector::FocusVisible,
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
            StyleSelector::Disabled => "Disabled",
            StyleSelector::Active => "Active",
            StyleSelector::Dragging => "Dragging",
            StyleSelector::Selected => "Selected",
            StyleSelector::DarkMode => "DarkMode",
            StyleSelector::FileHover => "FileHover",
        }
    }
}

/// Bitmask of active style selectors
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Default)]
pub struct StyleSelectors {
    selectors: u8,
    responsive: bool,
}

impl StyleSelectors {
    pub(crate) const fn new() -> Self {
        StyleSelectors {
            selectors: 0,
            responsive: false,
        }
    }

    pub(crate) const fn set(mut self, selector: StyleSelector, value: bool) -> Self {
        let v = selector as u8;
        if value {
            self.selectors |= v;
        } else {
            self.selectors &= !v;
        }
        self
    }

    pub(crate) fn has(self, selector: StyleSelector) -> bool {
        let v = selector as u8;
        self.selectors & v == v
    }

    pub(crate) fn union(self, other: StyleSelectors) -> StyleSelectors {
        StyleSelectors {
            selectors: self.selectors | other.selectors,
            responsive: self.responsive | other.responsive,
        }
    }

    pub(crate) const fn responsive(mut self) -> Self {
        self.responsive = true;
        self
    }

    pub(crate) fn has_responsive(self) -> bool {
        self.responsive
    }

    /// Returns a formatted string representation of the active selectors
    pub fn debug_string(&self) -> String {
        let parts = self.active_selectors();

        if parts.is_empty() {
            if self.responsive {
                "Responsive".to_string()
            } else {
                "None".to_string()
            }
        } else {
            let selector_str = parts.join(" + ");
            if self.responsive {
                format!("{} (Responsive)", selector_str)
            } else {
                selector_str
            }
        }
    }

    /// Returns a vector of individual selector names
    pub fn active_selectors(&self) -> Vec<&'static str> {
        StyleSelector::all()
            .iter()
            .filter(|&&selector| self.has(selector))
            .map(|&selector| selector.name())
            .collect()
    }

    /// Returns true if any selectors are active
    pub fn is_empty(&self) -> bool {
        self.selectors == 0 && !self.responsive
    }
}
