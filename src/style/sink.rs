//! [`floem_style::StyleSink`] implementation for floem's [`WindowState`].
//!
//! The trait definition itself lives in `floem_style`. This module holds
//! floem's implementation, which derives the per-view `ViewId` from
//! `ElementId::owning_id()` before delegating to `WindowState`'s inherent
//! methods.

use floem_style::StyleSink;

use crate::{ElementId, ElementIdExt};
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

    fn is_hovered(&self, id: ElementId) -> bool {
        WindowState::is_hovered(self, id)
    }
    fn is_focused(&self, id: ElementId) -> bool {
        WindowState::is_focused(self, id)
    }
    fn is_focus_within(&self, id: ElementId) -> bool {
        WindowState::is_focus_within(self, id)
    }
    fn is_active(&self, id: ElementId) -> bool {
        WindowState::is_active(self, id)
    }
    fn is_file_hover(&self, id: ElementId) -> bool {
        WindowState::is_file_hover(self, id)
    }

    fn apply_animations(
        &mut self,
        id: ElementId,
        combined: &mut Style,
        interact: &mut floem_style::InteractionState,
    ) -> bool {
        let view_state = id.owning_id().state();
        view_state.borrow_mut().apply_animations(combined, interact)
    }
}
