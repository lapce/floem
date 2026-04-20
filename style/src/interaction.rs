//! Interaction state inputs to the style engine.
//!
//! These describe the per-node pseudo-class state (hover, focus, active, etc.)
//! the host reports into the engine at style-recomputation time. Hosts build
//! an [`InteractionState`] for each node they're restyling; inheritance from
//! ancestors arrives separately via [`InheritedInteractionCx`].

/// The interaction state of a view, used to determine which style selectors apply.
///
/// This struct captures the current state of user interaction with a view,
/// such as whether it's hovered, focused, being clicked, etc. This state is
/// used during style computation to apply conditional styles like `:hover`,
/// `:active`, `:focus`, etc.
#[derive(Default, Debug, Clone, Copy)]
pub struct InteractionState {
    /// Whether the pointer is currently over this element.
    pub is_hovered: bool,
    /// Whether this element is in a selected state.
    pub is_selected: bool,
    /// Whether this element is disabled.
    pub is_disabled: bool,
    /// Whether this element has keyboard focus.
    pub is_focused: bool,
    /// Whether this element or a descendant currently has keyboard focus.
    pub is_focus_within: bool,
    /// Whether the element has been hidden
    pub is_hidden: bool,
    /// Whether an element is currently in the "active" state.
    /// active: pointer down and not up with the pointer in the element either by
    ///   1. remaining in, or
    ///   2. returning into the element
    ///      or keyboard trigger is down.
    pub is_active: bool,
    /// Whether dark mode is enabled.
    pub is_dark_mode: bool,
    /// Whether a file is being dragged over this element.
    pub is_file_hover: bool,
    /// Whether keyboard navigation is active.
    pub using_keyboard_navigation: bool,
    /// 1-based child index within parent, if this view has a parent.
    pub child_index: Option<usize>,
    /// Number of siblings under this view's parent.
    pub sibling_count: usize,
    /// Current window width in px for responsive selector matching.
    pub window_width: f64,
}

impl InteractionState {
    /// Pack interaction state into bits for efficient hashing.
    pub fn to_bits(self) -> u16 {
        let mut bits = 0u16;
        if self.is_hovered {
            bits |= 1 << 0;
        }
        if self.is_selected {
            bits |= 1 << 1;
        }
        if self.is_disabled {
            bits |= 1 << 2;
        }
        if self.is_focused {
            bits |= 1 << 3;
        }
        if self.is_active {
            bits |= 1 << 4;
        }
        if self.is_dark_mode {
            bits |= 1 << 5;
        }
        if self.is_file_hover {
            bits |= 1 << 6;
        }
        if self.using_keyboard_navigation {
            bits |= 1 << 7;
        }
        if self.is_focus_within {
            bits |= 1 << 8;
        }
        bits
    }
}

/// Inherited interaction context that is propagated from parent to children.
///
/// These states can be set by parent views and are inherited by children,
/// allowing parents to control the disabled or selected state of entire subtrees.
#[derive(Default, Debug, Clone, Copy)]
pub struct InheritedInteractionCx {
    /// Whether this view (or an ancestor) is disabled.
    pub disabled: bool,
    /// Whether this view (or an ancestor) is selected.
    pub selected: bool,
    /// Whether this view (or an ancestor) is hidden.
    pub hidden: bool,
}
