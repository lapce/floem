//! Style context for computing view styles.
//!
//! This module contains the context types used during the style phase:
//! - [`StyleCx`] - Context for computing and propagating styles through the view tree
//! - [`InteractionState`] - Captures current user interaction state for style resolution
//! - [`StyleRecalcChange`] - Graduated change tracking for optimized style propagation

use floem_reactive::Scope;
use std::rc::Rc;
use understory_box_tree::NodeFlags;

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use crate::ElementId;
use crate::animate::{AnimStateKind, RepeatMode};
use crate::inspector::CaptureState;
use crate::style::{StyleClassRef, resolve_nested_maps};
use crate::view::ViewId;
use crate::view::stacking::{invalidate_all_overlay_caches, invalidate_stacking_cache};
use crate::window::state::WindowState;

use super::cache::StyleCacheKey;
use super::recalc::StyleRecalcChange;
use super::{Disabled, Style, StyleProp, ZIndex};

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
    is_hidden: bool,
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
    /// Whether this view (or an ancestor) is hidden.
    pub hidden: bool,
}

pub struct StyleCx<'a> {
    pub window_state: &'a mut WindowState,

    pub(crate) current_view: ViewId,

    /// Inherited properties context from ancestors.
    /// Contains only properties marked as `inherited` (font-size, color, etc.).
    ///
    /// Built from parent's computed_style, this provides the inherited baseline that
    /// will be merged with this view's combined_style to produce computed_style.
    pub(crate) inherited: Rc<Style>,

    /// Class definitions context from ancestors.
    /// Contains `.class(SomeClass, ...)` nested maps from ancestor views.
    ///
    /// Built from parent's combined_style, this provides class definitions that
    /// descendants can apply when computing their combined_style. Separate from
    /// inherited to allow independent propagation and caching.
    pub(crate) class_context: Rc<Style>,

    /// The resolved style for this view (will become combined_style).
    /// Computed from base + selectors + classes during style resolution.
    ///
    /// This is an intermediate value set during style_view and represents only
    /// what this view explicitly defines, without inherited properties.
    pub(crate) direct: Style,

    pub(crate) now: Instant,

    parent_disabled: bool,
    parent_hidden: bool,
    parent_selected: bool,
}

impl<'a> StyleCx<'a> {
    pub fn new(window_state: &'a mut WindowState, view_id: ViewId) -> Self {
        // Get the style parent: either custom style_cx_parent or DOM parent
        let style_parent = view_id
            .state()
            .borrow()
            .style_cx_parent
            .or_else(|| view_id.parent());

        // Initialize inherited and class contexts separately
        let (inherited, class_context, parent_disabled, parent_selected, parent_hidden) =
            if let Some(parent_id) = style_parent {
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
                    parent_interaction.hidden,
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
                    false,
                )
            };

        Self {
            window_state,
            current_view: view_id,
            inherited,
            class_context,
            direct: Default::default(),
            now: Instant::now(),
            parent_disabled,
            parent_hidden,
            parent_selected,
        }
    }

    /// Marks the current context as selected.
    pub fn selected(&mut self) {
        self.parent_selected = true;
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
    pub fn style_view(&mut self, view_id: ViewId, change: StyleRecalcChange) {
        let view = view_id.view();
        let view_state = view_id.state();

        // ─────────────────────────────────────────────────────────────────────
        // Phase 1: Gather initial state
        // ─────────────────────────────────────────────────────────────────────
        // let has_selectors = {
        //     let vs = view_state.borrow();
        //     let selectors = !vs.has_style_selectors.is_empty();
        //     selectors
        // };

        // if !change.should_recalc(true) {
        //     return;
        // }

        // // Fast path: only propagate inherited properties, skip full resolution
        // if change.can_use_inherited_fast_path(has_selectors) && !view_is_dirty {
        //     self.apply_inherited_only(view_id, change);
        //     return;
        // }

        // ─────────────────────────────────────────────────────────────────────
        // Phase 2: Clear dirty flags, update view style, propagate to children
        // ─────────────────────────────────────────────────────────────────────
        self.window_state.style_dirty.remove(&view_id);

        let view_class = view.borrow().view_class();
        let base_style = {
            let mut vs = view_state.borrow_mut();

            // Update view style if needed
            if self.window_state.view_style_dirty.contains(&view_id) {
                self.window_state.view_style_dirty.remove(&view_id);
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
                    self.window_state.style_dirty.insert(child);
                }
            }

            // Gather data needed for later phases
            vs.style()
        };

        // ─────────────────────────────────────────────────────────────────────
        // Phase 3: Build interaction state for selector matching
        // ─────────────────────────────────────────────────────────────────────
        let mut view_interact_state = self.get_interact_state(view_id);

        // start updating the view interaction state with properties from the base style
        let base_style_disabled = base_style.builtin().set_disabled();
        let base_style_selected = base_style.builtin().set_selected();
        let base_style_hidden = base_style.builtin().display() == taffy::Display::None;
        view_interact_state.is_disabled |= base_style_disabled;
        view_interact_state.is_selected |= base_style_selected;
        view_interact_state.is_hidden |= base_style_hidden;

        // ─────────────────────────────────────────────────────────────────────
        // Phase 4: Resolve combined style
        // ─────────────────────────────────────────────────────────────────────

        // Cache miss or dirty - compute style
        let (combined_style, classes_applied, has_active_animation) =
            view_state.borrow_mut().compute_combined(
                &mut view_interact_state,
                self.window_state.screen_size_bp,
                view_class,
                &self.inherited,
                &self.class_context,
            );
        // Schedule next frame if any animation is in progress
        if has_active_animation {
            self.window_state.schedule_style(view_id);
        }

        // ─────────────────────────────────────────────────────────────────────
        // Phase 5: Compute final style and propagate contexts to children
        // ─────────────────────────────────────────────────────────────────────
        self.direct = combined_style;

        // Propagate inherited properties to children (separate from class context)
        Style::apply_only_inherited(&mut self.inherited, &self.direct);

        // Capture old class_context pointer before updating.
        // apply_only_class_maps either returns early (no class maps), or creates
        // a new Rc via Rc::new(), so we can detect changes with O(1) pointer comparison.
        let old_class_context_ptr = Rc::as_ptr(&self.class_context);

        // Propagate class nested maps to class_context for children.
        // Class maps like `.class(ListItemClass, ...)` need to flow down so
        // descendant views with matching classes can apply the styling.
        Style::apply_only_class_maps(&mut self.class_context, &self.direct);

        // Pointer changed means apply_only_class_maps created a new Rc (had class maps)
        let class_context_changed = Rc::as_ptr(&self.class_context) != old_class_context_ptr;

        // ─────────────────────────────────────────────────────────────────────
        // Phase 6: Propagate changes to children if needed
        // ─────────────────────────────────────────────────────────────────────
        // Mark children for restyling if:
        // 1. This view applied classes from class_context (affects inherited props)
        // 2. This view's class_context changed (new class definitions for children)
        let child_change = if classes_applied || class_context_changed {
            // Mark all children for style recalc and add to dirty set
            for child in view_id.children() {
                self.window_state.style_dirty.insert(child);
            }
            change.force_recalc_descendants()
        } else {
            change.for_children()
        };

        // Compute the final style by merging inherited context with direct style
        let mut computed_style = (*self.inherited).clone();
        computed_style.apply_mut(self.direct.clone());

        // ─────────────────────────────────────────────────────────────────────
        // Phase 7: Update window and view state.
        // ─────────────────────────────────────────────────────────────────────

        CaptureState::capture_style(view_id, self, computed_style.clone());

        // Track fixed elements for viewport-relative sizing
        let new_is_fixed = computed_style.builtin().is_fixed();
        let computed_style_has_disabled = computed_style.builtin().set_disabled();
        let computed_style_has_selected = computed_style.builtin().set_selected();
        let compute_style_has_hidden = computed_style.builtin().display() == taffy::Display::None;
        view_interact_state.is_hidden |= compute_style_has_hidden;
        view_interact_state.is_selected |= computed_style_has_selected;
        view_interact_state.is_disabled |= computed_style_has_disabled;

        let parent_set = {
            let view_state = view_id.state();
            let view_state = view_state.borrow();
            view_state.parent_set_style_interaction
        };

        // Update view state in a single borrow
        let (old_is_fixed, old_taffy_style) = {
            let mut vs = view_state.borrow_mut();
            let old_fixed = vs.computed_style.builtin().is_fixed();
            let old_taffy = vs.taffy_style.clone();

            vs.style_cx = Some((*self.inherited).clone());
            vs.class_cx = Some((*self.class_context).clone());
            vs.computed_style = computed_style;
            vs.style_interaction_cx = InheritedInteractionCx {
                disabled: self.parent_disabled
                    || computed_style_has_disabled
                    || parent_set.disabled,
                selected: self.parent_selected
                    || computed_style_has_selected
                    || parent_set.selected,
                hidden: self.parent_hidden || compute_style_has_hidden || parent_set.hidden,
            };

            (old_fixed, old_taffy)
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

        // ─────────────────────────────────────────────────────────────────────
        // Phase 7.1: Extract layout and transform properties
        // ─────────────────────────────────────────────────────────────────────
        // We read from the computed_style (which includes animated values) rather than
        // from the raw styles, so that animations affect layout and visual properties.
        let mut transitioning = false;
        let mut layout_transitioning = false;
        let mut view_style_transitioning = false;
        let mut view_style_changed = false;
        {
            let mut vs = view_state.borrow_mut();

            // Clone the computed style to avoid borrow conflicts with the mutable
            // borrow needed for the extractors. This includes animated values.
            let computed = vs.computed_style.clone();

            // Layout properties (padding, margin, size, etc.)
            if vs.layout_props.read_explicit(
                &computed,
                &computed,
                &self.now,
                &mut layout_transitioning,
            ) {
                // layout_transitioning = true;
            }
            transitioning |= layout_transitioning;

            // View style properties (background, border, etc.)
            if vs.view_style_props.read_explicit(
                &computed,
                &computed,
                &self.now,
                &mut view_style_transitioning,
            ) {
                view_style_changed = true;
            }
            transitioning |= view_style_transitioning;

            // Transform properties (translate, scale, rotation)
            let mut box_tree_transitioning = false;
            if vs.view_transform_props.read_explicit(
                &computed,
                &computed,
                &self.now,
                &mut box_tree_transitioning,
            ) || box_tree_transitioning
            {
                self.window_state.needs_box_tree_commit = true;
            }
            transitioning |= box_tree_transitioning;

            let old_cursor = vs.style_cursor;
            if old_cursor != computed.builtin().cursor() {
                vs.style_cursor = computed.builtin().cursor();
                self.window_state.needs_cursor_resolution = true;
            }
        }

        if transitioning {
            self.window_state.schedule_style(view_id);
        }

        // Handle visibility transition: None -> visible
        // store was_hidden before updating interaction state with base style
        let was_hidden = view_id.state().borrow().is_hidden;

        let is_visible = !compute_style_has_hidden && !self.parent_hidden && !parent_set.hidden;
        if was_hidden && is_visible {
            request_style_layout_recursive_for_children(view_id);
            view_id.request_layout();
        }
        view_id.state().borrow_mut().is_hidden = !is_visible;

        // ─────────────────────────────────────────────────────────────────────
        // Phase 8: Call view's style_pass
        // ─────────────────────────────────────────────────────────────────────
        self.window_state
            .pending_child_change
            .insert(view_id, child_change);

        view.borrow_mut().style_pass(self);

        // ─────────────────────────────────────────────────────────────────────
        // Phase 9: visibility transitions and set taffy style
        // ─────────────────────────────────────────────────────────────────────

        // Handle visibility transitions and animations
        // Skip transition if view has been explicitly hidden via set_hidden()
        let parent_set_hidden = {
            let view_state = view_id.state();
            let view_state = view_state.borrow();
            view_state.parent_set_style_interaction.hidden
        };
        if !parent_set_hidden {
            let (old_phase, computed_display) = {
                let vs = view_state.borrow();
                (vs.visibility.phase, vs.combined_style.builtin().display())
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
                invalidate_stacking_cache(view_id.get_element_id());
                view_state.borrow_mut().visibility.phase = phase;
            }
            // Apply visibility phase to combined style
            if let Some(display) = phase.get_display_override() {
                let mut vs = view_state.borrow_mut();
                vs.combined_style = vs.combined_style.clone().display(display);
            }
        } else {
            // parent set hidden - no transition
            let mut vs = view_state.borrow_mut();
            vs.combined_style = vs.combined_style.clone().display(taffy::Display::None);
        }

        {
            // ─────────────────────────────────────────────────────────────────────
            // Phase 9.1: Get the final hidden state for view queries, layout, etc.
            // Update taffy style if layout properties changed (must happen after visibility phase override)
            // ─────────────────────────────────────────────────────────────────────
            let mut vs = view_state.borrow_mut();
            // TODO: fix this is hidden logic
            let is_hidden = view_interact_state.is_hidden
                || (vs.is_hidden
                    && vs
                        .visibility
                        .phase
                        .get_display_override()
                        .is_none_or(|d| d == taffy::Display::None));
            let taffy_style = vs.combined_style.clone();
            let transitioned_layout_props = vs.layout_props.to_style();
            let taffy_style = taffy_style
                .apply(transitioned_layout_props)
                .to_taffy_style();

            if taffy_style != old_taffy_style {
                let taffy_node = vs.layout_id;
                vs.taffy_style = taffy_style.clone();
                view_id
                    .taffy()
                    .borrow_mut()
                    .set_style(taffy_node, taffy_style.clone())
                    .unwrap();
                if !is_hidden {
                    self.window_state.schedule_layout();
                }
            }
            // ─────────────────────────────────────────────────────────────────────
            // Phase 9.2: Update box tree visiblity dependent props  (must happen after visibility phase override)
            // ─────────────────────────────────────────────────────────────────────
            {
                let box_tree = view_id.box_tree();
                let elment_id = vs.element_id;
                let box_tree = &mut box_tree.borrow_mut();
                let mut flags = NodeFlags::empty();
                if (vs.computed_style.builtin().pointer_events()
                    != Some(crate::style::PointerEvents::None))
                    && !is_hidden
                {
                    flags |= NodeFlags::PICKABLE;
                }
                // need to update this after visibility.
                if vs.computed_style.builtin().keyboard_navigable()
                    && !is_hidden
                    && !view_interact_state.is_disabled
                {
                    flags |= NodeFlags::FOCUSABLE;
                }
                if !is_hidden {
                    flags |= NodeFlags::VISIBLE;
                }
                box_tree.set_flags(elment_id.0, flags);
            }
            // ─────────────────────────────────────────────────────────────────────
            // Phase 9.3: request paint for view style changes if not hidden
            // ─────────────────────────────────────────────────────────────────────
            if !is_hidden && (view_style_transitioning || view_style_changed) {
                self.window_state.request_paint(view_id);
            }
        }

        // ─────────────────────────────────────────────────────────────────────
        // Phase 10: Update stacking context (z-index)
        // ─────────────────────────────────────────────────────────────────────
        {
            let vs = view_state.borrow();
            let new_z_index = vs.combined_style.builtin().z_index().unwrap_or(0);
            let element_id = view_id.get_element_id();

            // Get old z-index from box tree
            let box_tree = self.window_state.box_tree.borrow();
            let old_z_index = box_tree
                .local_z_index(element_id.0)
                .and_then(|opt| opt)
                .unwrap_or(0);
            drop(box_tree);
            drop(vs);

            if old_z_index != new_z_index {
                invalidate_stacking_cache(element_id);
                if view_id.is_overlay() {
                    invalidate_all_overlay_caches();
                }

                // Update box tree immediately (don't wait for layout)
                self.window_state
                    .box_tree
                    .borrow_mut()
                    .set_local_z_index(element_id.0, Some(new_z_index));
            }
        }
    }

    // /// Fast path for inherited-only changes.
    // ///
    // /// When only inherited properties changed (e.g., font-size, color), we can skip
    // /// the full selector resolution and just propagate inherited values to children.
    // /// This is a significant optimization for deeply nested UIs.
    // fn apply_inherited_only(&mut self, view_id: ViewId, change: StyleRecalcChange) {
    //     let view_state = view_id.state();
    //     let view = view_id.view();

    //     // Update inherited context from parent (class context unchanged in fast path)
    //     Style::apply_only_inherited(&mut self.inherited, &self.direct);

    //     // Clone combined_style before mutable borrow to avoid borrow conflicts
    //     let combined_style = view_state.borrow().combined_style.clone();

    //     // Recompute computed_style with new inherited values
    //     {
    //         let mut vs = view_state.borrow_mut();
    //         let mut computed_style = (*self.inherited).clone();
    //         computed_style.apply_mut(combined_style.clone());
    //         vs.computed_style = computed_style;
    //         vs.style_cx = Some((*self.inherited).clone());
    //     }

    //     // Update prop extractors with potentially changed inherited values
    //     let mut transitioning = false;
    //     {
    //         let mut vs = view_state.borrow_mut();
    //         vs.layout_props.read_explicit(
    //             &combined_style,
    //             &self.inherited,
    //             &self.now,
    //             &mut transitioning,
    //         );
    //         if transitioning && !self.parent_hidden {
    //             self.window_state.schedule_layout();
    //         }

    //         vs.view_style_props.read_explicit(
    //             &combined_style,
    //             &self.inherited,
    //             &self.now,
    //             &mut transitioning,
    //         );
    //         if transitioning && !self.parent_hidden {
    //             self.window_state.schedule_style(view_id);
    //         }
    //     }

    //     self.current_view = view_id;

    //     // Store child change for views that process children in style_pass
    //     let child_change = change.for_children();
    //     self.window_state
    //         .pending_child_change
    //         .insert(view_id, child_change);

    //     // Let the view do any custom style pass work
    //     view.borrow_mut().style_pass(self);
    // }

    /// Resolve all nested maps in the base style for the given classes.
    /// This will use the current style cx as context and get the interaction state for the given element
    pub fn resolve_nested_maps(
        &self,
        base_style: Style,
        classes: &[StyleClassRef],
        element_id: impl Into<ElementId>,
    ) -> Style {
        let base_style_disabled = base_style.builtin().set_disabled();
        let base_style_selected = base_style.builtin().set_selected();
        let base_style_hidden = base_style.builtin().display() == taffy::Display::None;
        let mut view_interact_state = self.get_interact_state(element_id);
        view_interact_state.is_disabled |= base_style_disabled;
        view_interact_state.is_selected |= base_style_selected;
        view_interact_state.is_hidden |= base_style_hidden;
        resolve_nested_maps(
            base_style,
            &mut view_interact_state,
            self.window_state.screen_size_bp,
            classes,
            &self.inherited,
            &self.class_context,
        )
        .0
    }

    /// The base style the base style for this view that will override any inherited properties
    pub fn resolve_nested_maps_with_state(
        &self,
        base_style: Style,
        classes: &[StyleClassRef],
        mut interact_state: InteractionState,
    ) -> Style {
        resolve_nested_maps(
            base_style,
            &mut interact_state,
            self.window_state.screen_size_bp,
            classes,
            &self.inherited,
            &self.class_context,
        )
        .0
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
        if !self.parent_hidden {
            self.window_state.schedule_style(id);
        }
    }

    pub fn get_interact_state(&self, id: impl Into<crate::ElementId>) -> InteractionState {
        let id: crate::ElementId = id.into();
        let view_id = id.owning_id();
        let parent_override = {
            let view_state = view_id.state();
            let view_state = view_state.borrow();
            view_state.parent_set_style_interaction
        };
        InteractionState {
            is_selected: self.parent_selected | parent_override.selected,
            is_disabled: self.parent_disabled | parent_override.disabled,
            is_hidden: self.parent_hidden | parent_override.hidden,
            is_hovered: self.window_state.is_hovered(id),
            is_focused: self.window_state.is_focused(id),
            is_clicking: self.window_state.is_clicking(id),
            is_dark_mode: self.window_state.is_dark_mode(),
            is_file_hover: self.window_state.is_file_hover(id),
            using_keyboard_navigation: self.window_state.keyboard_navigation,
        }
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
