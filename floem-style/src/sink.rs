//! The interface the style engine uses to talk to the host.
//!
//! `StyleCx` interacts with the host (today: floem's `WindowState`) exclusively
//! through this trait. Keeping the engine's outbound surface explicit lets us
//! later generalize `StyleCx` over any sink implementor, which is how a second
//! consumer such as `floem-native` will plug into the same style engine.
//!
//! All per-element methods take [`ElementId`] so the trait carries no host-specific
//! node-id type. Implementors derive whatever internal identity they need
//! (e.g. floem's `ViewId` via `ElementId::owning_id()`).
//!
//! The trait is `#[allow(dead_code)]` because `StyleCx` currently calls
//! host-specific inherent methods directly for most operations; a follow-up
//! step will retarget those calls through this trait to complete the
//! abstraction.

use crate::cache::StyleCache;
use crate::element_id::ElementId;
use crate::recalc::StyleReason;
use crate::responsive::ScreenSizeBp;
use crate::selectors::{StyleSelector, StyleSelectors};
use crate::style::Style;

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

#[allow(dead_code)]
pub trait StyleSink {
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
    fn schedule_style(&mut self, id: ElementId, reason: StyleReason);
    fn schedule_style_with_target(&mut self, target: ElementId, reason: StyleReason);
    fn mark_descendants_with_selector_dirty(
        &mut self,
        ancestor: ElementId,
        selector: StyleSelector,
    );
    fn mark_descendants_with_responsive_selector_dirty(&mut self, ancestor: ElementId);
    fn update_selector_interest(&mut self, id: ElementId, selectors: Option<StyleSelectors>);

    // --- Host side-effects ---
    fn register_fixed_element(&mut self, id: ElementId);
    fn unregister_fixed_element(&mut self, id: ElementId);
    fn invalidate_focus_nav_cache(&mut self);
    fn request_paint(&mut self, id: ElementId);
    fn mark_needs_cursor_resolution(&mut self);
    fn mark_needs_layout(&mut self);
}
