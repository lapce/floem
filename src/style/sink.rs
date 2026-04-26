//! Floem's `AnimationBackend` for the style engine's cascade.
//!
//! Engine-side, the cascade reads everything it needs from a
//! [`floem_style::CascadeInputs`] struct and delegates the animation
//! tick to an [`AnimationBackend`]. Floem keeps its animation state on
//! per-view `ViewState.animations` stacks, so its backend is a small
//! adapter that finds the owning `ViewId` for a `StyleNodeId` and
//! forwards the tick into that view's stack.
//!
//! The closure `WindowState::run_style_cascade` uses to populate
//! `CascadeInputs::interactions` lives inline at the call site —
//! it's simpler than a trait impl and the short lifetime is exactly
//! right for the duration of one `compute_style` call.

use floem_style::{AnimationBackend, StyleNodeId};
use rustc_hash::FxHashMap;

use crate::style::{InteractionState, Style};
use crate::view::ViewId;

/// `AnimationBackend` wired to floem's per-view `ViewState.animations`
/// stacks. Holds only a borrow of the `StyleNodeId → ViewId` reverse
/// map so the method can find the right view to tick for each node.
/// All mutation happens through `ViewState`'s `RefCell`, so the
/// `&self` signature is honest.
pub(crate) struct FloemAnimationBackend<'a> {
    pub(crate) style_node_to_view: &'a FxHashMap<StyleNodeId, ViewId>,
}

impl AnimationBackend for FloemAnimationBackend<'_> {
    fn apply(
        &self,
        node: StyleNodeId,
        combined: &mut Style,
        interact: &mut InteractionState,
    ) -> bool {
        let Some(view_id) = self.style_node_to_view.get(&node).copied() else {
            return false;
        };
        let view_state = view_id.state();
        view_state.borrow_mut().apply_animations(combined, interact)
    }
}
