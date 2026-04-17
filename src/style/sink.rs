//! The interface the style engine uses to talk to the host.
//!
//! `StyleCx` interacts with the host (today: [`WindowState`]) exclusively through
//! this trait. Keeping the engine's outbound surface explicit lets us later
//! generalize `StyleCx` over any sink implementor, which is how a second
//! consumer such as `floem-native` will plug into the same style engine
//! without depending on `WindowState` or `ViewId`.

use crate::ElementId;
use crate::layout::responsive::ScreenSizeBp;
use crate::style::recalc::StyleReason;
use crate::style::{Style, StyleCache, StyleSelector, StyleSelectors};
use crate::view::ViewId;
use crate::window::state::WindowState;

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

#[allow(dead_code)] // Phase 2 will generalize StyleCx over this trait; until then a
// subset of methods are dispatched via WindowState's inherent impls.
pub(crate) trait StyleSink {
    // --- Frame / root state ---
    fn frame_start(&self) -> Instant;
    fn screen_size_bp(&self) -> ScreenSizeBp;
    fn keyboard_navigation(&self) -> bool;
    fn root_size_width(&self) -> f64;
    fn is_dark_mode(&self) -> bool;

    // --- Theme defaults ---
    fn default_theme_classes(&self) -> &Style;
    fn default_theme_inherited(&self) -> &Style;

    // --- Style cache ---
    fn style_cache_mut(&mut self) -> &mut StyleCache;

    // --- Per-element interaction reads ---
    fn is_hovered(&self, id: ElementId) -> bool;
    fn is_focused(&self, id: ElementId) -> bool;
    fn is_focus_within(&self, id: ElementId) -> bool;
    fn is_active(&self, id: ElementId) -> bool;
    fn is_file_hover(&self, id: ElementId) -> bool;

    // --- Dirty / schedule / invalidate ---
    fn mark_style_dirty_with(&mut self, id: ElementId, reason: StyleReason);
    fn schedule_style(&mut self, id: ViewId, reason: StyleReason);
    fn schedule_style_with_target(&mut self, target: ElementId, reason: StyleReason);
    fn mark_descendants_with_selector_dirty(&mut self, ancestor: ViewId, selector: StyleSelector);
    fn mark_descendants_with_responsive_selector_dirty(&mut self, ancestor: ViewId);
    fn update_selector_interest(&mut self, id: ViewId, selectors: Option<StyleSelectors>);

    // --- Host side-effects ---
    fn register_fixed_element(&mut self, id: ViewId);
    fn unregister_fixed_element(&mut self, id: ViewId);
    fn invalidate_focus_nav_cache(&mut self);
    fn request_paint(&mut self, id: ElementId);
    fn mark_needs_cursor_resolution(&mut self);
    fn mark_needs_layout(&mut self);
}

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

    fn style_cache_mut(&mut self) -> &mut StyleCache {
        &mut self.style_cache
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
    fn schedule_style(&mut self, id: ViewId, reason: StyleReason) {
        WindowState::schedule_style(self, id, reason)
    }
    fn schedule_style_with_target(&mut self, target: ElementId, reason: StyleReason) {
        WindowState::schedule_style_with_target(self, target, reason)
    }
    fn mark_descendants_with_selector_dirty(&mut self, ancestor: ViewId, selector: StyleSelector) {
        WindowState::mark_descendants_with_selector_dirty(self, ancestor, selector)
    }
    fn mark_descendants_with_responsive_selector_dirty(&mut self, ancestor: ViewId) {
        WindowState::mark_descendants_with_responsive_selector_dirty(self, ancestor)
    }
    fn update_selector_interest(&mut self, id: ViewId, selectors: Option<StyleSelectors>) {
        WindowState::update_selector_interest(self, id, selectors)
    }

    fn register_fixed_element(&mut self, id: ViewId) {
        WindowState::register_fixed_element(self, id)
    }
    fn unregister_fixed_element(&mut self, id: ViewId) {
        WindowState::unregister_fixed_element(self, id)
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
}
