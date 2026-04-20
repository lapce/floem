//! [`floem_style::StyleSink`] implementation for floem's [`WindowState`].
//!
//! The trait definition lives in `floem_style`. Every per-node method
//! receives a `StyleNodeId` — the engine's opaque handle — and this impl
//! translates back to floem's `ViewId` via `WindowState.style_node_to_view`
//! before dispatching to `WindowState`'s view-keyed helpers. Same shape
//! as taffy hosts keep — engine identity ↔ host identity is a host
//! sidecar map.

use floem_style::{StyleNodeId, StyleSink};

use crate::ElementIdExt;
use crate::layout::responsive::ScreenSizeBp;
use crate::style::Style;
use crate::window::state::WindowState;

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

impl StyleSink for WindowState {
    fn frame_start(&self) -> Instant {
        self.frame_start
    }
    fn screen_size_bp(&self) -> ScreenSizeBp {
        self.screen_size_bp
    }
    fn keyboard_navigation(&self) -> bool {
        self.keyboard_navigation
    }
    fn root_size_width(&self) -> f64 {
        self.root_size.width
    }
    fn is_dark_mode(&self) -> bool {
        WindowState::is_dark_mode(self)
    }

    fn default_theme_classes(&self) -> &Style {
        &self.default_theme
    }
    fn default_theme_inherited(&self) -> &Style {
        &self.default_theme_inherited
    }

    fn is_hovered(&self, node: StyleNodeId) -> bool {
        self.style_node_to_view
            .get(&node)
            .is_some_and(|v| WindowState::is_hovered(self, v.get_element_id()))
    }
    fn is_focused(&self, node: StyleNodeId) -> bool {
        self.style_node_to_view
            .get(&node)
            .is_some_and(|v| WindowState::is_focused(self, v.get_element_id()))
    }
    fn is_focus_within(&self, node: StyleNodeId) -> bool {
        self.style_node_to_view
            .get(&node)
            .is_some_and(|v| WindowState::is_focus_within(self, v.get_element_id()))
    }
    fn is_active(&self, node: StyleNodeId) -> bool {
        self.style_node_to_view
            .get(&node)
            .is_some_and(|v| WindowState::is_active(self, v.get_element_id()))
    }
    fn is_file_hover(&self, node: StyleNodeId) -> bool {
        self.style_node_to_view
            .get(&node)
            .is_some_and(|v| WindowState::is_file_hover(self, v.get_element_id()))
    }

    fn apply_animations(
        &mut self,
        node: StyleNodeId,
        combined: &mut Style,
        interact: &mut floem_style::InteractionState,
    ) -> bool {
        let Some(view_id) = self.style_node_to_view.get(&node).copied() else {
            return false;
        };
        let view_state = view_id.state();
        view_state.borrow_mut().apply_animations(combined, interact)
    }
}
