//! [`floem_style::StyleSink`] implementation for floem's [`WindowState`].
//!
//! The trait definition itself lives in `floem_style`. This module holds
//! floem's implementation, which derives the per-view `ViewId` from
//! `ElementId::owning_id()` before delegating to `WindowState`'s inherent
//! methods.

use floem_style::StyleSink;

use crate::{ElementId, ElementIdExt};
use crate::layout::responsive::ScreenSizeBp;
use crate::style::recalc::StyleReason;
use crate::style::{Style, StyleSelector, StyleSelectors};
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

    fn mark_style_dirty_with(&mut self, id: ElementId, reason: StyleReason) {
        WindowState::mark_style_dirty_with(self, id, reason)
    }
    fn schedule_style(&mut self, id: ElementId, reason: StyleReason) {
        WindowState::schedule_style(self, id.owning_id(), reason)
    }
    fn schedule_style_with_target(&mut self, target: ElementId, reason: StyleReason) {
        WindowState::schedule_style_with_target(self, target, reason)
    }
    fn mark_descendants_with_selector_dirty(&mut self, ancestor: ElementId, selector: StyleSelector) {
        WindowState::mark_descendants_with_selector_dirty(self, ancestor.owning_id(), selector)
    }
    fn mark_descendants_with_responsive_selector_dirty(&mut self, ancestor: ElementId) {
        WindowState::mark_descendants_with_responsive_selector_dirty(self, ancestor.owning_id())
    }
    fn update_selector_interest(&mut self, id: ElementId, selectors: Option<StyleSelectors>) {
        WindowState::update_selector_interest(self, id.owning_id(), selectors)
    }

    fn register_fixed_element(&mut self, id: ElementId) {
        WindowState::register_fixed_element(self, id.owning_id())
    }
    fn unregister_fixed_element(&mut self, id: ElementId) {
        WindowState::unregister_fixed_element(self, id.owning_id())
    }
    fn invalidate_focus_nav_cache(&mut self) {
        WindowState::invalidate_focus_nav_cache(self)
    }
    fn request_paint(&mut self, id: ElementId) {
        WindowState::request_paint(self, id)
    }

    fn mark_needs_cursor_resolution(&mut self) {
        self.needs_cursor_resolution = true;
    }
    fn mark_needs_layout(&mut self) {
        self.needs_layout = true;
    }

    fn set_cursor(
        &mut self,
        id: ElementId,
        cursor: crate::style::CursorStyle,
    ) -> Option<crate::style::CursorStyle> {
        WindowState::set_cursor(self, id, cursor)
    }

    fn clear_cursor(&mut self, id: ElementId) -> Option<crate::style::CursorStyle> {
        WindowState::clear_cursor(self, id)
    }

    fn inspector_capture_style(&mut self, id: ElementId, computed_style: &Style) {
        if let Some(capture) = self.capture.as_mut() {
            capture.record_computed_style(id.owning_id(), computed_style.clone());
        }
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
