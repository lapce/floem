//! Style context for computing view styles.
//!
//! This module contains the context types used during the style phase:
//! - [`StyleCx`] - Context for computing and propagating styles through the view tree
//! - [`InteractionState`] - Captures current user interaction state for style resolution
//! - [`StyleRecalcChange`] - Graduated change tracking for optimized style propagation

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

use super::cache::StyleCacheKey;
use super::recalc::StyleRecalcChange;
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
#[derive(Default, Debug, Clone, Copy)]
pub struct InheritedInteractionCx {
    /// Whether this view (or an ancestor) is disabled.
    pub disabled: bool,
    /// Whether this view (or an ancestor) is selected.
    pub selected: bool,
}

pub struct StyleCx<'a> {
    pub window_state: &'a mut WindowState,
    pub(crate) current_view: ViewId,
    /// Inherited properties context - carries inherited props (font-size, color, etc.)
    /// from ancestors to descendants. Only contains properties marked as `inherited`.
    pub(crate) inherited: Rc<Style>,
    /// Class context - carries class nested maps (`.class(ListItemClass, ...)`) from
    /// ancestors so descendants with matching classes can apply the styling.
    /// Separate from inherited to allow independent propagation and caching.
    pub(crate) class_context: Rc<Style>,
    pub(crate) direct: Style,
    saved_inherited: Vec<Rc<Style>>,
    saved_class_context: Vec<Rc<Style>>,
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

        // Initialize inherited and class contexts separately
        let (inherited, class_context, disabled, selected) = if let Some(parent_id) = style_parent {
            let parent_state = parent_id.state();
            let parent_state = parent_state.borrow();

            // Inherited props come from parent's style_cx (contains only inherited props)
            let inherited_style = parent_state
                .style_cx
                .clone()
                .map(Rc::new)
                .unwrap_or_default();

            // Class context comes from parent's class_cx (contains class nested maps)
            let class_ctx = parent_state
                .class_cx
                .clone()
                .map(Rc::new)
                .unwrap_or_default();

            let parent_interaction = parent_state.style_interaction_cx;
            (
                inherited_style,
                class_ctx,
                parent_interaction.disabled,
                parent_interaction.selected,
            )
        } else {
            // Root view: use cached inherited props from default theme.
            // The default theme sets props like font_size(14.0) at root level which should
            // be accessible via with_context in all descendant views.
            // We use the pre-computed cache to avoid iterating through the theme map
            // on every StyleCx::new() call.
            (
                window_state.default_theme_inherited.clone(),
                window_state.default_theme.clone(),
                false,
                false,
            )
        };

        Self {
            window_state,
            current_view: view_id,
            inherited,
            class_context,
            direct: Default::default(),
            saved_inherited: Default::default(),
            saved_class_context: Default::default(),
            now: Instant::now(),
            saved_disabled: Default::default(),
            saved_selected: Default::default(),
            saved_hidden: Default::default(),
            disabled,
            hidden: false,
            selected,
        }
    }

    /// Marks the current context as selected.
    pub fn selected(&mut self) {
        self.selected = true;
    }

    /// Internal method used by Floem to compute the styles for the view.
    ///
    /// This is a convenience wrapper that uses default change propagation.
    /// For optimized recalculation with graduated propagation, use [`style_view_with_change`].
    pub fn style_view(&mut self, view_id: ViewId) {
        self.style_view_with_change(view_id, StyleRecalcChange::NONE);
    }

    /// Compute styles for a view with graduated change propagation.
    ///
    /// The `change` parameter describes what kind of recalculation is needed,
    /// enabling optimizations like:
    /// - Skipping views that don't need recalc
    /// - Using the inherited-only fast path when only inherited props changed
    /// - Limiting recalc to immediate children vs entire subtrees
    ///
    /// See [`StyleRecalcChange`] for details on the propagation model.
    pub fn style_view_with_change(&mut self, view_id: ViewId, change: StyleRecalcChange) {
        self.save();
        let view = view_id.view();
        let view_state = view_id.state();

        // ─────────────────────────────────────────────────────────────────────
        // Phase 1: Check if recalculation is needed and gather initial state
        // ─────────────────────────────────────────────────────────────────────
        let (view_is_dirty, has_selectors) = {
            let vs = view_state.borrow();
            let dirty = vs.requested_changes.contains(ChangeFlags::STYLE)
                || vs.requested_changes.contains(ChangeFlags::VIEW_STYLE);
            let selectors = !vs.has_style_selectors.is_empty();
            (dirty, selectors)
        };

        if !change.should_recalc(view_is_dirty) {
            self.restore();
            return;
        }

        // Fast path: only propagate inherited properties, skip full resolution
        if change.can_use_inherited_fast_path(has_selectors) && !view_is_dirty {
            self.apply_inherited_only(view_id, change);
            self.restore();
            return;
        }

        // ─────────────────────────────────────────────────────────────────────
        // Phase 2: Clear dirty flags, update view style, propagate to children
        // ─────────────────────────────────────────────────────────────────────
        let view_class = view.borrow().view_class();
        let (base_style, all_classes, selected_from_state) = {
            let mut vs = view_state.borrow_mut();

            // Clear STYLE flag
            vs.requested_changes.remove(ChangeFlags::STYLE);

            // Update view style if needed
            if vs.requested_changes.contains(ChangeFlags::VIEW_STYLE) {
                vs.requested_changes.remove(ChangeFlags::VIEW_STYLE);
                if let Some(view_style) = view.borrow().view_style() {
                    let offset = vs.view_style_offset;
                    vs.style.set(offset, view_style);
                }
            }

            // Propagate style requests to children if needed
            if vs.request_style_recursive {
                vs.request_style_recursive = false;
                for child in view_id.children() {
                    let child_state = child.state();
                    let mut state = child_state.borrow_mut();
                    state.request_style_recursive = true;
                    state.requested_changes.insert(ChangeFlags::STYLE);
                    self.window_state.style_dirty.insert(child);
                }
            }

            // Gather data needed for later phases
            let base = vs.style();
            let mut classes = vs.classes.clone();
            if let Some(vc) = view_class {
                classes.push(vc);
            }
            let selected = vs.parent_set_style_interaction.selected;

            (base, classes, selected)
        };

        // ─────────────────────────────────────────────────────────────────────
        // Phase 3: Build interaction state for selector matching
        // ─────────────────────────────────────────────────────────────────────
        let this_view_disabled = base_style.get(Disabled);
        let view_interact_state = InteractionState {
            is_selected: self.selected || selected_from_state,
            is_hovered: self.window_state.is_hovered(&view_id),
            is_disabled: this_view_disabled || self.disabled,
            is_focused: self.window_state.is_focused(&view_id),
            is_clicking: self.window_state.is_clicking(&view_id),
            is_dark_mode: self.window_state.is_dark_mode(),
            is_file_hover: self.window_state.is_file_hover(&view_id),
            using_keyboard_navigation: self.window_state.keyboard_navigation,
        };
        self.disabled = view_interact_state.is_disabled;

        // ─────────────────────────────────────────────────────────────────────
        // Phase 4: Resolve combined style (with cache optimization)
        // ─────────────────────────────────────────────────────────────────────
        let cache_key = StyleCacheKey::new(
            &base_style,
            &view_interact_state,
            self.window_state.screen_size_bp,
            &all_classes,
            &self.class_context,
        );

        // Try cache first if view is not dirty
        let cached = if !view_is_dirty {
            self.window_state
                .style_cache
                .get(&cache_key, &self.inherited)
        } else {
            None
        };

        let classes_applied = if let Some((cached_style, cached_classes_applied)) = cached {
            // Cache hit
            let mut vs = view_state.borrow_mut();
            vs.combined_style = (*cached_style).clone();
            vs.has_style_selectors = base_style.selectors();
            cached_classes_applied
        } else {
            // Cache miss or dirty - compute style
            let (combined, classes_applied) = view_state.borrow_mut().compute_combined(
                view_interact_state,
                self.window_state.screen_size_bp,
                view_class,
                &self.inherited,
                &self.class_context,
            );
            if super::cache::StyleCache::is_cacheable(&base_style) {
                self.window_state.style_cache.insert(
                    cache_key,
                    combined,
                    &self.inherited,
                    classes_applied,
                );
            }
            classes_applied
        };

        // ─────────────────────────────────────────────────────────────────────
        // Phase 5: Compute final style and propagate contexts to children
        // ─────────────────────────────────────────────────────────────────────
        let combined_style = view_state.borrow().combined_style.clone();
        self.direct = combined_style;

        // Propagate inherited properties to children (separate from class context)
        Style::apply_only_inherited(&mut self.inherited, &self.direct);

        // Capture old class_context pointer before updating.
        // Because save() was called earlier, class_context Rc is shared (ref count > 1).
        // If apply_only_class_maps modifies it, Rc::make_mut will clone and create a new Rc.
        let old_class_context_ptr = Rc::as_ptr(&self.class_context);

        // Propagate class nested maps to class_context for children.
        // Class maps like `.class(ListItemClass, ...)` need to flow down so
        // descendant views with matching classes can apply the styling.
        Style::apply_only_class_maps(&mut self.class_context, &self.direct);

        // Check if class_context changed using O(1) pointer comparison.
        // This works because the Rc is shared (from save()), so any modification
        // via Rc::make_mut will clone the inner data and change the pointer.
        let class_context_changed = Rc::as_ptr(&self.class_context) != old_class_context_ptr;

        // ─────────────────────────────────────────────────────────────────────
        // Phase 6: Propagate changes to children if needed
        // ─────────────────────────────────────────────────────────────────────
        // Mark children for restyling if:
        // 1. This view applied classes from class_context (affects inherited props)
        // 2. This view's class_context changed (new class definitions for children)
        let child_change = if classes_applied || class_context_changed {
            // Mark all children for recursive style recalc and add to dirty set
            for child in view_id.children() {
                let child_state = child.state();
                let mut state = child_state.borrow_mut();
                state.request_style_recursive = true;
                state.requested_changes.insert(ChangeFlags::STYLE);
                self.window_state.style_dirty.insert(child);
            }
            change.force_recalc_descendants()
        } else {
            change.for_children()
        };

        // Compute the final style by merging inherited context with direct style
        let mut computed_style = (*self.inherited).clone();
        computed_style.apply_mut(self.direct.clone());

        CaptureState::capture_style(view_id, self, computed_style.clone());

        // Update focusable set
        let is_focusable = computed_style.get(Focusable)
            && !computed_style.get(Disabled)
            && computed_style.get(DisplayProp) != taffy::Display::None;
        if is_focusable {
            self.window_state.focusable.insert(view_id);
        } else {
            self.window_state.focusable.remove(&view_id);
        }

        // Track fixed elements for viewport-relative sizing
        let new_is_fixed = computed_style.get(super::IsFixed);
        let view_is_disabled = computed_style.get(Disabled);
        let view_is_display_none = computed_style.get(DisplayProp) == taffy::Display::None;

        // Update view state in a single borrow
        let (old_is_fixed, old_taffy_style, force_hidden) = {
            let mut vs = view_state.borrow_mut();
            let old_fixed = vs.computed_style.get(super::IsFixed);
            let old_taffy = vs.taffy_style.clone();
            let force_hid = vs.visibility.force_hidden;

            vs.style_cx = Some((*self.inherited).clone());
            vs.class_cx = Some((*self.class_context).clone());
            vs.computed_style = computed_style;
            vs.style_interaction_cx = InheritedInteractionCx {
                disabled: self.disabled || view_is_disabled,
                selected: self.selected,
            };

            (old_fixed, old_taffy, force_hid)
        };

        // Handle fixed element registration
        if new_is_fixed {
            self.window_state.register_fixed_element(view_id);
        } else {
            self.window_state.unregister_fixed_element(view_id);
        }
        if new_is_fixed != old_is_fixed {
            view_id.request_layout();
        }

        self.disabled = self.disabled || view_is_disabled;
        self.hidden = view_is_display_none || force_hidden;
        self.current_view = view_id;

        // ─────────────────────────────────────────────────────────────────────
        // Phase 7: Extract layout and transform properties
        // ─────────────────────────────────────────────────────────────────────
        let mut transitioning = false;
        {
            let mut vs = view_state.borrow_mut();

            // Layout properties (padding, margin, size, etc.)
            vs.layout_props.read_explicit(
                &self.direct,
                &self.inherited,
                &self.now,
                &mut transitioning,
            );

            // View style properties (background, border, etc.)
            vs.view_style_props.read_explicit(
                &self.direct,
                &self.inherited,
                &self.now,
                &mut transitioning,
            );

            // Transform properties (translate, scale, rotation)
            vs.view_transform_props.read_explicit(
                &self.direct,
                &self.inherited,
                &self.now,
                &mut transitioning,
            );
        }

        if transitioning {
            self.window_state.schedule_layout(view_id);
            if !self.hidden {
                self.window_state.schedule_style(view_id);
            }
        }

        // ─────────────────────────────────────────────────────────────────────
        // Phase 8: Update taffy style if layout properties changed
        // ─────────────────────────────────────────────────────────────────────
        let layout_style = view_state.borrow().layout_props.to_style();
        let taffy_style = self.direct.clone().apply(layout_style).to_taffy_style();

        if taffy_style != old_taffy_style {
            view_state.borrow_mut().taffy_style = taffy_style.clone();
            self.window_state.schedule_layout(view_id);

            // Handle visibility transition: None -> visible
            let was_hidden = old_taffy_style.display == taffy::Display::None;
            let is_visible = taffy_style.display != taffy::Display::None;
            if was_hidden && is_visible {
                request_style_layout_recursive_for_children(view_id);
            }
        }

        // ─────────────────────────────────────────────────────────────────────
        // Phase 9: Call view's style_pass and handle animations
        // ─────────────────────────────────────────────────────────────────────
        self.window_state
            .pending_child_change
            .insert(view_id, child_change);

        view.borrow_mut().style_pass(self);

        // Handle visibility transitions and animations
        // Skip transition if view has been explicitly hidden via set_hidden()
        if !force_hidden {
            let (old_phase, computed_display) = {
                let vs = view_state.borrow();
                (vs.visibility.phase, vs.combined_style.get(DisplayProp))
            };
            let mut phase = old_phase;

            phase.transition(
                computed_display,
                || {
                    let count = animations_on_remove(view_id, Scope::current());
                    view_state.borrow_mut().num_waiting_animations = count;
                    count > 0
                },
                || animations_on_create(view_id),
                || stop_reset_remove_animations(view_id),
                || view_state.borrow().num_waiting_animations,
            );

            if old_phase != phase {
                invalidate_stacking_cache(view_id);
                view_state.borrow_mut().visibility.phase = phase;
            }
        }

        // Apply visibility phase to combined style
        {
            let mut vs = view_state.borrow_mut();
            let phase = vs.visibility.phase;
            if let Some(display) = phase.get_display() {
                vs.combined_style = vs.combined_style.clone().display(display);
            }
        }

        // ─────────────────────────────────────────────────────────────────────
        // Phase 10: Update stacking context (z-index)
        // ─────────────────────────────────────────────────────────────────────
        {
            let mut vs = view_state.borrow_mut();
            let new_z_index = vs.combined_style.get(ZIndex).unwrap_or(0);
            let old_z_index = vs.stacking_info.effective_z_index;

            if old_z_index != new_z_index {
                invalidate_stacking_cache(view_id);
                if view_id.is_overlay() {
                    invalidate_all_overlay_caches();
                }
                vs.stacking_info = StackingInfo {
                    effective_z_index: new_z_index,
                };
            }
        }

        self.restore();
    }

    /// Fast path for inherited-only changes.
    ///
    /// When only inherited properties changed (e.g., font-size, color), we can skip
    /// the full selector resolution and just propagate inherited values to children.
    /// This is a significant optimization for deeply nested UIs.
    fn apply_inherited_only(&mut self, view_id: ViewId, change: StyleRecalcChange) {
        let view_state = view_id.state();
        let view = view_id.view();

        // Update inherited context from parent (class context unchanged in fast path)
        Style::apply_only_inherited(&mut self.inherited, &self.direct);

        // Clone combined_style before mutable borrow to avoid borrow conflicts
        let combined_style = view_state.borrow().combined_style.clone();

        // Recompute computed_style with new inherited values
        {
            let mut vs = view_state.borrow_mut();
            let mut computed_style = (*self.inherited).clone();
            computed_style.apply_mut(combined_style.clone());
            vs.computed_style = computed_style;
            vs.style_cx = Some((*self.inherited).clone());
        }

        // Update prop extractors with potentially changed inherited values
        let mut transitioning = false;
        {
            let mut vs = view_state.borrow_mut();
            vs.layout_props.read_explicit(
                &combined_style,
                &self.inherited,
                &self.now,
                &mut transitioning,
            );
            if transitioning {
                self.window_state.schedule_layout(view_id);
            }

            vs.view_style_props.read_explicit(
                &combined_style,
                &self.inherited,
                &self.now,
                &mut transitioning,
            );
            if transitioning && !self.hidden {
                self.window_state.schedule_style(view_id);
            }
        }

        self.current_view = view_id;

        // Store child change for views that process children in style_pass
        let child_change = change.for_children();
        self.window_state
            .pending_child_change
            .insert(view_id, child_change);

        // Let the view do any custom style pass work
        view.borrow_mut().style_pass(self);
    }

    /// Get the pending child change for a view.
    ///
    /// This is used by views that manually process their children in `style_pass`
    /// to get the appropriate change propagation level.
    pub fn get_child_change(&self, view_id: ViewId) -> StyleRecalcChange {
        self.window_state
            .pending_child_change
            .get(&view_id)
            .copied()
            .unwrap_or(StyleRecalcChange::NONE)
    }

    pub fn now(&self) -> Instant {
        self.now
    }

    pub fn save(&mut self) {
        self.saved_inherited.push(self.inherited.clone());
        self.saved_class_context.push(self.class_context.clone());
        self.saved_disabled.push(self.disabled);
        self.saved_selected.push(self.selected);
        self.saved_hidden.push(self.hidden);
    }

    pub fn restore(&mut self) {
        self.inherited = self.saved_inherited.pop().unwrap_or_default();
        self.class_context = self.saved_class_context.pop().unwrap_or_default();
        self.disabled = self.saved_disabled.pop().unwrap_or_default();
        self.selected = self.saved_selected.pop().unwrap_or_default();
        self.hidden = self.saved_hidden.pop().unwrap_or_default();
    }

    pub fn get_prop<P: StyleProp>(&self, _prop: P) -> Option<P::Type> {
        self.direct
            .get_prop::<P>()
            .or_else(|| self.inherited.get_prop::<P>())
    }

    pub fn style(&self) -> Style {
        (*self.inherited).clone().apply(self.direct.clone())
    }

    pub fn direct_style(&self) -> &Style {
        &self.direct
    }

    pub fn indirect_style(&self) -> &Style {
        &self.inherited
    }

    pub fn request_transition(&mut self) {
        let id = self.current_view;
        self.window_state.schedule_style(id);
    }
}

// Helper function for visibility transitions
fn request_style_layout_recursive_for_children(view_id: ViewId) {
    fn recurse(id: ViewId) {
        id.request_style_recursive();
        id.request_layout();
        for child in id.children() {
            recurse(child);
        }
    }
    for child in view_id.children() {
        recurse(child);
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
