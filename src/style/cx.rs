//! Style context for computing view styles.
//!
//! This module contains the context types used during the style phase:
//! - [`StyleCx`] - Context for computing and propagating styles through the view tree
//! - [`InteractionState`] - Captures current user interaction state for style resolution

use floem_reactive::Scope;
use std::rc::Rc;

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use crate::animate::{AnimStateKind, RepeatMode};
use crate::inspector::CaptureState;
use crate::view::ViewId;
use crate::view::stacking::{invalidate_all_overlay_caches, invalidate_stacking_cache};
use crate::view::{ChangeFlags, StackingInfo};
use crate::window::state::WindowState;

use super::{Disabled, DisplayProp, Focusable, Style, StyleProp, ZIndex};

/// The interaction state of a view, used to determine which style selectors apply.
///
/// This struct captures the current state of user interaction with a view,
/// such as whether it's hovered, focused, being clicked, etc. This state is
/// used during style computation to apply conditional styles like `:hover`,
/// `:active`, `:focus`, etc.
#[derive(Default, Debug, Clone, Copy)]
pub struct InteractionState {
    /// Whether the pointer is currently over this view.
    pub is_hovered: bool,
    /// Whether this view is in a selected state.
    pub is_selected: bool,
    /// Whether this view is disabled.
    pub is_disabled: bool,
    /// Whether this view has keyboard focus.
    pub is_focused: bool,
    /// Whether this view is being clicked (pointer down but not yet up).
    pub is_clicking: bool,
    /// Whether dark mode is enabled.
    pub is_dark_mode: bool,
    /// Whether a file is being dragged over this view.
    pub is_file_hover: bool,
    /// Whether keyboard navigation is active.
    pub using_keyboard_navigation: bool,
}

/// Inherited interaction context that is propagated from parent to children.
///
/// These states can be set by parent views and are inherited by children,
/// allowing parents to control the disabled or selected state of entire subtrees.
///
/// Note: The `hidden` field is only used by `parent_set_style_interaction` for
/// programmatic hiding (e.g., Tab view hiding inactive tabs). For style-based
/// hiding via `display: none`, use `is_hidden_state` instead.
#[derive(Default, Debug, Clone, Copy)]
pub struct InheritedInteractionCx {
    /// Whether this view (or an ancestor) is disabled.
    pub disabled: bool,
    /// Whether this view (or an ancestor) is selected.
    pub selected: bool,
    /// Whether this view was hidden by a parent (via `parent_set_hidden()`).
    /// Only used by `parent_set_style_interaction`, not `style_interaction_cx`.
    pub hidden: bool,
}

pub struct StyleCx<'a> {
    pub window_state: &'a mut WindowState,
    pub(crate) current_view: ViewId,
    /// current is used as context for carrying inherited properties between views
    pub(crate) current: Rc<Style>,
    pub(crate) direct: Style,
    saved: Vec<Rc<Style>>,
    pub(crate) now: Instant,
    saved_disabled: Vec<bool>,
    saved_selected: Vec<bool>,
    saved_hidden: Vec<bool>,
    disabled: bool,
    hidden: bool,
    selected: bool,
}

impl<'a> StyleCx<'a> {
    pub(crate) fn new(window_state: &'a mut WindowState, view_id: ViewId) -> Self {
        // Get the style parent: either custom style_cx_parent or DOM parent
        let style_parent = view_id
            .state()
            .borrow()
            .style_cx_parent
            .or_else(|| view_id.parent());

        // Initialize inherited context from parent's style_cx
        let (current, disabled, selected, hidden) = if let Some(parent_id) = style_parent {
            let parent_state = parent_id.state();
            let parent_state = parent_state.borrow();
            let inherited_style = parent_state
                .style_cx
                .clone()
                .map(Rc::new)
                .unwrap_or_default();
            let parent_interaction = parent_state.style_interaction_cx;
            (
                inherited_style,
                parent_interaction.disabled,
                parent_interaction.selected,
                parent_state.is_hidden_state == crate::view::state::IsHiddenState::Hidden
                    || parent_state.parent_set_style_interaction.hidden,
            )
        } else {
            (Default::default(), false, false, false)
        };

        Self {
            window_state,
            current_view: view_id,
            current,
            direct: Default::default(),
            saved: Default::default(),
            now: Instant::now(),
            saved_disabled: Default::default(),
            saved_selected: Default::default(),
            saved_hidden: Default::default(),
            disabled,
            hidden,
            selected,
        }
    }

    /// Marks the current context as selected.
    pub fn selected(&mut self) {
        self.selected = true;
    }

    pub fn hidden(&mut self) {
        self.hidden = true;
    }

    fn get_interact_state(&self, id: &ViewId) -> InteractionState {
        InteractionState {
            is_selected: self.selected || id.is_selected(),
            is_hovered: self.window_state.is_hovered(id),
            is_disabled: id.is_disabled() || self.disabled,
            is_focused: self.window_state.is_focused(id),
            is_clicking: self.window_state.is_clicking(id),
            is_dark_mode: self.window_state.is_dark_mode(),
            is_file_hover: self.window_state.is_file_hover(id),
            using_keyboard_navigation: self.window_state.keyboard_navigation,
        }
    }

    /// Internal method used by Floem to compute the styles for the view.
    pub fn style_view(&mut self, view_id: ViewId) {
        self.save();
        let view = view_id.view();
        let view_state = view_id.state();
        {
            let mut view_state = view_state.borrow_mut();
            if !view_state.requested_changes.contains(ChangeFlags::STYLE)
                && !view_state
                    .requested_changes
                    .contains(ChangeFlags::VIEW_STYLE)
            {
                self.restore();
                return;
            }
            view_state.requested_changes.remove(ChangeFlags::STYLE);
        }
        let view_class = view.borrow().view_class();
        {
            let mut view_state = view_state.borrow_mut();
            if view_state
                .requested_changes
                .contains(ChangeFlags::VIEW_STYLE)
            {
                view_state.requested_changes.remove(ChangeFlags::VIEW_STYLE);
                if let Some(view_style) = view.borrow().view_style() {
                    let offset = view_state.view_style_offset;
                    view_state.style.set(offset, view_style);
                }
            }
            // Propagate style requests to children if needed.
            if view_state.request_style_recursive {
                view_state.request_style_recursive = false;
                let children = view_id.children();
                for child in children {
                    let view_state = child.state();
                    let mut state = view_state.borrow_mut();
                    state.request_style_recursive = true;
                    state.requested_changes.insert(ChangeFlags::STYLE);
                    // Also add to style_dirty so children are picked up in next traversal
                    self.window_state.style_dirty.insert(child);
                }
            }
        }

        // Get the base style's interaction properties BEFORE computing combined style
        // This is needed because selectors depend on the current disabled state,
        // not the previous frame's style_interaction_cx value.
        let base_style = view_state.borrow().style();
        let this_view_disabled = base_style.get(Disabled);

        let view_interact_state = InteractionState {
            is_selected: self.selected || view_state.borrow().style_interaction_cx.selected,
            is_hovered: self.window_state.is_hovered(&view_id),
            is_disabled: this_view_disabled || self.disabled,
            is_focused: self.window_state.is_focused(&view_id),
            is_clicking: self.window_state.is_clicking(&view_id),
            is_dark_mode: self.window_state.is_dark_mode(),
            is_file_hover: self.window_state.is_file_hover(&view_id),
            using_keyboard_navigation: self.window_state.keyboard_navigation,
        };
        self.disabled = view_interact_state.is_disabled;

        // Compute style with full selector/responsive/class resolution
        // Cache is disabled for now - see below for cache code
        let (_combined, classes_applied) = view_id.state().borrow_mut().compute_combined(
            view_interact_state,
            self.window_state.screen_size_bp,
            view_class,
            &self.current,
        );

        // Cache code (disabled - use when beneficial):
        // let input_style = view_state.borrow().style();
        // let classes = view_state.borrow().classes.clone();
        // let cache_key = StyleCacheKey::new(
        //     &input_style,
        //     &view_interact_state,
        //     self.window_state.screen_size_bp,
        //     &classes,
        //     &self.current,
        // );
        // if let Some((cached_style, cached_classes_applied)) =
        //     self.window_state.style_cache.get(&cache_key)
        // {
        //     view_state.borrow_mut().combined_style = (*cached_style).clone();
        //     cached_classes_applied
        // } else {
        //     let (combined, applied) = view_id.state().borrow_mut().compute_combined(...);
        //     self.window_state.style_cache.insert(cache_key, combined, applied);
        //     applied
        // };

        if classes_applied {
            let children = view_id.children();
            for child in children {
                let view_state = child.state();
                let mut state = view_state.borrow_mut();
                state.request_style_recursive = true;
                state.requested_changes.insert(ChangeFlags::STYLE);
            }
        }

        self.direct = view_state.borrow().combined_style.clone();
        Style::apply_only_inherited(&mut self.current, &self.direct);

        // Store the inherited context for children to use
        view_state.borrow_mut().style_cx = Some((*self.current).clone());

        let mut computed_style = (*self.current).clone();
        computed_style.apply_mut(self.direct.clone());
        let mut transitioning = false;
        CaptureState::capture_style(view_id, self, computed_style.clone());
        if computed_style.get(Focusable)
            && !computed_style.get(Disabled)
            && computed_style.get(DisplayProp) != taffy::Display::None
        {
            self.window_state.focusable.insert(view_id);
        } else {
            self.window_state.focusable.remove(&view_id);
        }

        // Track fixed elements for efficient window resize handling.
        // When IsFixed changes, we need to request layout since fixed elements
        // have their size computed differently (relative to viewport).
        let new_is_fixed = computed_style.get(super::IsFixed);
        let old_is_fixed = view_state.borrow().computed_style.get(super::IsFixed);
        if new_is_fixed {
            self.window_state.register_fixed_element(view_id);
        } else {
            self.window_state.unregister_fixed_element(view_id);
        }
        if new_is_fixed != old_is_fixed {
            // Use request_layout directly so it's processed in current frame
            view_id.request_layout();
        }

        // Check if this view itself is disabled in its computed style
        let view_is_disabled = computed_style.get(Disabled);
        // Check if this view has display:none (hidden via display property)
        let view_is_display_none = computed_style.get(DisplayProp) == taffy::Display::None;

        view_state.borrow_mut().computed_style = computed_style;

        // Update the inherited state for children (combine parent's inherited state with this view's own state)
        self.disabled = self.disabled || view_is_disabled;
        self.hidden = self.hidden || view_is_display_none;

        // Store the inherited interaction state so is_disabled() and is_selected() work correctly
        // for hit testing and event dispatch
        {
            let mut vs = view_state.borrow_mut();
            vs.style_interaction_cx = InheritedInteractionCx {
                disabled: self.disabled,
                selected: self.selected,
                hidden: false, // Not used; hidden state is tracked via is_hidden_state
            };
        }

        // This is used by the `request_transition` and `style` methods below.
        self.current_view = view_id;

        {
            let mut view_state = view_state.borrow_mut();
            // Extract the relevant layout properties so the content rect can be calculated
            // when painting.
            view_state.layout_props.read_explicit(
                &self.direct,
                &self.current,
                &self.now,
                &mut transitioning,
            );
            if transitioning {
                // If any transitioning layout props, schedule layout.
                self.window_state.schedule_layout(view_id);
            }

            view_state.view_style_props.read_explicit(
                &self.direct,
                &self.current,
                &self.now,
                &mut transitioning,
            );
            if transitioning && !self.hidden {
                self.window_state.schedule_style(view_id);
            }

            // Read transform properties (translate, scale, rotation) for layout.
            // Without this, CSS transforms like translate_x/translate_y won't be
            // applied during layout, causing hit testing to fail for translated elements.
            view_state.view_transform_props.read_explicit(
                &self.direct,
                &self.current,
                &self.now,
                &mut transitioning,
            );
            if transitioning && !self.hidden {
                self.window_state.schedule_layout(view_id);
            }
        }
        // If there's any changes to the Taffy style, request layout.
        let layout_style = view_state.borrow().layout_props.to_style();
        let taffy_style = self.direct.clone().apply(layout_style).to_taffy_style();
        let old_taffy_style = view_state.borrow().taffy_style.clone();
        if taffy_style != old_taffy_style {
            view_state.borrow_mut().taffy_style = taffy_style.clone();
            self.window_state.schedule_layout(view_id);

            // If display changed from None to visible, request style and layout for all children recursively.
            // This is needed because children of display:None elements may not have been properly
            // laid out, and when the parent becomes visible again, the children need to be re-styled
            // and re-laid out.
            let was_hidden = old_taffy_style.display == taffy::Display::None;
            let is_visible = taffy_style.display != taffy::Display::None;
            if was_hidden && is_visible {
                fn request_style_layout_recursive(id: ViewId) {
                    id.request_style_recursive();
                    id.request_layout();
                    for child in id.children() {
                        request_style_layout_recursive(child);
                    }
                }
                for child in view_id.children() {
                    request_style_layout_recursive(child);
                }
            }
        }

        view.borrow_mut().style_pass(self);

        let old_is_hidden_state = view_state.borrow().is_hidden_state;
        let mut is_hidden_state = old_is_hidden_state;
        let computed_display = view_state.borrow().combined_style.get(DisplayProp);
        is_hidden_state.transition(
            computed_display,
            || {
                let count = animations_on_remove(view_id, Scope::current());
                view_state.borrow_mut().num_waiting_animations = count;
                count > 0
            },
            || {
                animations_on_create(view_id);
            },
            || {
                stop_reset_remove_animations(view_id);
            },
            || view_state.borrow().num_waiting_animations,
        );

        // Invalidate stacking cache if hidden state changed
        if old_is_hidden_state != is_hidden_state {
            invalidate_stacking_cache(view_id);
        }

        view_state.borrow_mut().is_hidden_state = is_hidden_state;
        let modified = view_state
            .borrow()
            .combined_style
            .clone()
            .apply_opt(is_hidden_state.get_display(), Style::display);

        view_state.borrow_mut().combined_style = modified;

        // Note: Transform computation moved to layout/cx.rs::compute_view_layout
        // because CSS translate percentages are relative to the element's own size,
        // which is only available after layout.

        // Simplified stacking model:
        // - Every view is implicitly a stacking context
        // - z-index only competes with siblings
        // - Children are always bounded within their parent (no "escaping")
        // We only need to track z-index for sibling sorting within a parent.
        let z_index = view_state.borrow().combined_style.get(ZIndex);
        let new_z_index = z_index.unwrap_or(0);

        // Invalidate stacking cache if z-index changed
        {
            let mut vs = view_state.borrow_mut();
            let old_z_index = vs.stacking_info.effective_z_index;
            if old_z_index != new_z_index {
                invalidate_stacking_cache(view_id);
                // If this is an overlay, also invalidate overlay cache
                if view_id.is_overlay() {
                    invalidate_all_overlay_caches();
                }
            }
            vs.stacking_info = StackingInfo {
                effective_z_index: new_z_index,
            };
        }

        self.restore();
    }

    pub fn now(&self) -> Instant {
        self.now
    }

    pub fn save(&mut self) {
        self.saved.push(self.current.clone());
        self.saved_disabled.push(self.disabled);
        self.saved_selected.push(self.selected);
        self.saved_hidden.push(self.hidden);
    }

    pub fn restore(&mut self) {
        self.current = self.saved.pop().unwrap_or_default();
        self.disabled = self.saved_disabled.pop().unwrap_or_default();
        self.selected = self.saved_selected.pop().unwrap_or_default();
        self.hidden = self.saved_hidden.pop().unwrap_or_default();
    }

    pub fn get_prop<P: StyleProp>(&self, _prop: P) -> Option<P::Type> {
        self.direct
            .get_prop::<P>()
            .or_else(|| self.current.get_prop::<P>())
    }

    pub fn style(&self) -> Style {
        (*self.current).clone().apply(self.direct.clone())
    }

    pub fn direct_style(&self) -> &Style {
        &self.direct
    }

    pub fn indirect_style(&self) -> &Style {
        &self.current
    }

    pub fn request_transition(&mut self) {
        let id = self.current_view;
        self.window_state.schedule_style(id);
    }
}

// Animation helper functions used by StyleCx::style_view

fn animations_on_remove(id: ViewId, scope: Scope) -> u16 {
    let mut wait_for = 0;
    let state = id.state();
    let mut state = state.borrow_mut();
    state.num_waiting_animations = 0;
    let animations = &mut state.animations.stack;
    let mut request_style = false;
    for anim in animations {
        if anim.run_on_remove && !matches!(anim.repeat_mode, RepeatMode::LoopForever) {
            anim.reverse_mut();
            request_style = true;
            wait_for += 1;
            let trigger = anim.on_visual_complete;
            scope.create_updater(
                move || trigger.track(),
                move |_| {
                    id.transition_anim_complete();
                },
            );
        }
    }
    drop(state);
    if request_style {
        id.request_style();
    }

    id.children()
        .into_iter()
        .fold(wait_for, |acc, id| acc + animations_on_remove(id, scope))
}

fn stop_reset_remove_animations(id: ViewId) {
    let state = id.state();
    let mut state = state.borrow_mut();
    let animations = &mut state.animations.stack;
    let mut request_style = false;
    for anim in animations {
        if anim.run_on_remove
            && anim.state_kind() == AnimStateKind::PassInProgress
            && !matches!(anim.repeat_mode, RepeatMode::LoopForever)
        {
            anim.start_mut();
            request_style = true;
        }
    }
    drop(state);
    if request_style {
        id.request_style();
    }

    id.children()
        .into_iter()
        .for_each(stop_reset_remove_animations)
}

fn animations_on_create(id: ViewId) {
    let state = id.state();
    let mut state = state.borrow_mut();
    state.num_waiting_animations = 0;
    let animations = &mut state.animations.stack;
    let mut request_style = false;
    for anim in animations {
        if anim.run_on_create && !matches!(anim.repeat_mode, RepeatMode::LoopForever) {
            anim.start_mut();
            request_style = true;
        }
    }
    drop(state);
    if request_style {
        id.request_style();
    }

    id.children().into_iter().for_each(animations_on_create);
}
