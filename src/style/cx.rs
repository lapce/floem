//! Style context for computing view styles.
//!
//! This module contains the context types used during the style phase:
//! - [`StyleCx`] - Context for computing and propagating styles through the view tree
//! - [`InteractionState`] - Captures current user interaction state for style resolution
//! - [`StyleRecalcChange`] - Graduated change tracking for optimized style propagation

use floem_reactive::Scope;
use smallvec::SmallVec;
use understory_box_tree::NodeFlags;

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use crate::{
    ElementId,
    animate::{AnimStateKind, RepeatMode},
    inspector::CaptureState,
    style::{
        StyleClassRef,
        recalc::{StyleReason, StyleReasonFlags},
        resolve_nested_maps,
    },
    view::ViewId,
    window::state::WindowState,
};

use super::{Style, StyleProp};

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
    /// Whether an element is currently in the "active" state
    /// active: pointer down and not up with the pointer in the element either by
    ///   1: remaining in or
    ///   2: returning into the element
    /// or keyboard trigger is down.
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
    pub(crate) inherited: Style,

    /// Class definitions context from ancestors.
    /// Contains `.class(SomeClass, ...)` nested maps from ancestor views.
    ///
    /// Built from parent's combined_style, this provides class definitions that
    /// descendants can apply when computing their combined_style. Separate from
    /// inherited to allow independent propagation and caching.
    pub(crate) class_context: Style,

    /// The resolved style for this view (will become combined_style).
    /// Computed from base + selectors + classes during style resolution.
    ///
    /// This is an intermediate value set during style_view and represents only
    /// what this view explicitly defines, without inherited properties.
    pub(crate) direct: Style,

    pub(crate) now: Instant,

    view_interact_state: InteractionState,

    /// The reason this view was marked dirty. Available to `style_pass`
    /// implementations so views can make informed decisions about how much
    /// work to do.
    pub(crate) reason: StyleReason,

    /// Targeted sub-element invalidations. Populated when one or more specific
    /// `ElementId`s owned by this view need updating without a full view restyle.
    /// Empty means the full view is being restyled.
    ///
    /// `style_pass` implementations should walk this list and update only the
    /// affected elements when non-empty, skipping the full cascade for untouched
    /// sub-elements.
    pub(crate) targeted_elements: SmallVec<[(ElementId, StyleReason); 2]>,
}

impl<'a> StyleCx<'a> {
    pub fn new(window_state: &'a mut WindowState, view_id: ViewId, reason: StyleReason) -> Self {
        // Get the style parent: either custom style_cx_parent or DOM parent
        let style_parent = view_id
            .state()
            .borrow()
            .style_cx_parent
            .or_else(|| view_id.parent());

        // Initialize inherited and class contexts separately
        let (inherited, class_context) = if let Some(parent_id) = style_parent {
            let parent_state = parent_id.state();
            let parent_state = parent_state.borrow();

            let inherited_style = parent_state.style_cx.clone();
            let class_ctx = parent_state.class_cx.clone();

            (inherited_style, class_ctx)
        } else {
            (
                window_state.default_theme_inherited.clone(),
                window_state.default_theme.clone(),
            )
        };

        let view = view_id.view();
        let view_state = view_id.state();

        let reason_for_children = reason.for_children();
        if !reason_for_children.is_empty() {
            for child in view_id.children() {
                window_state
                    .mark_style_dirty_with(child.get_element_id(), reason_for_children.clone());
            }
        }

        let targeted_elements = reason
            .targets
            .iter()
            .map(|(id, r)| (*id, r.clone()))
            .collect();

        // ─────────────────────────────────────────────────────────────────────
        // Phase 1: Clear dirty flags, update view style, propagate to children
        // ─────────────────────────────────────────────────────────────────────

        {
            if reason.flags.contains(StyleReasonFlags::VIEW_STYLE)
                && let Some(view_style) = view.borrow().view_style()
            {
                let mut vs = view_state.borrow_mut();
                let offset = vs.view_style_offset;
                vs.style.set(offset, view_style);
            }
        }

        // ─────────────────────────────────────────────────────────────────────
        // Phase 2: Build interaction state for selector matching
        // ─────────────────────────────────────────────────────────────────────
        let view_interact_state = Self::get_interact_state(window_state, view_id);

        Self {
            window_state,
            current_view: view_id,
            inherited,
            class_context,
            direct: Default::default(),
            now: Instant::now(),
            view_interact_state,
            reason,
            targeted_elements,
        }
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
    pub fn style_view(&mut self) {
        let view_id = self.current_view;
        let view = view_id.view();
        let view_state = view_id.state();
        let view_class = view.borrow().view_class();

        let (active_selectors, classes) = {
            let vs = view_state.borrow();
            let selectors = vs.has_style_selectors;

            // Build the full class list: view's classes + view type class
            let mut all_classes = vs.classes.clone();
            if let Some(vc) = view_class {
                all_classes.push(vc);
            }
            (selectors, all_classes)
        };

        if let Some(selectors) = self.reason.selectors {
            let intersection = active_selectors.map(|s| s.intersection(selectors));

            if intersection.is_some_and(|i| i.is_empty()) {
                self.reason.clear_flag(StyleReasonFlags::SELECTOR);
            }
        }

        if let Some(changed) = &self.reason.classes_changed
            && !changed.iter().any(|c| classes.contains(c))
        {
            self.reason
                .clear_flag(StyleReasonFlags::CLASS_CONTEXT_CHANGE);
        }

        // ─────────────────────────────────────────────────────────────────────
        // Phase 4: Resolve combined style
        // ─────────────────────────────────────────────────────────────────────
        if self.reason.needs_resolve_nested_maps() {
            // Cache miss or dirty - compute style
            view_state.borrow_mut().compute_combined(
                &mut self.view_interact_state,
                self.window_state.screen_size_bp,
                view_class,
                &self.inherited,
                &self.class_context,
            );
        } else {
            // Fast path: nested-map resolution was skipped, so reapply the view-local
            // interaction state saved from the last combined-style computation.
            let cached = view_state.borrow().post_compute_combined_interaction;
            self.view_interact_state.is_hidden |= cached.hidden;
            self.view_interact_state.is_selected |= cached.selected;
            self.view_interact_state.is_disabled |= cached.disabled;
        }

        if self.reason.needs_animation() {
            let has_active_animation = view_state
                .borrow_mut()
                .apply_animations(&mut self.view_interact_state);
            if has_active_animation {
                self.window_state
                    .schedule_style(view_id, StyleReason::animation());
            }
        }

        let (old_interact_state, old_taffy_style) = {
            let vs = view_state.borrow();
            (vs.style_interaction_cx, vs.taffy_style.clone())
        };

        let mut need_paint = false;

        // ─────────────────────────────────────────────────────────────────────
        // Phase 5: Compute final style and propagate contexts to children
        // ─────────────────────────────────────────────────────────────────────
        self.direct = view_state.borrow().combined_style.clone();

        // Capture the inner map pointer before updating so we can detect whether
        // inherited properties actually changed.
        let old_inherited_map = view_state.borrow().style_cx.clone();
        // Propagate inherited properties to children (separate from class context)
        Style::apply_only_inherited(&mut self.inherited, &self.direct);
        let inherited_changed = self.inherited.merge_id() != old_inherited_map.merge_id();

        let old_class_context = view_state.borrow().class_cx.clone();
        Style::apply_only_class_maps(&mut self.class_context, &self.direct);
        let changed_classes = self.class_context.class_maps_eq(&old_class_context);
        let class_context_changed = !changed_classes.is_empty();

        // ─────────────────────────────────────────────────────────────────────
        // Phase 5.5: Propagate changes to children if needed
        // ─────────────────────────────────────────────────────────────────────
        // Mark children for restyling if:
        // 1. This view applied classes from class_context (affects inherited props)
        // 2. This view's class_context changed (new class definitions for children)
        if inherited_changed || class_context_changed {
            for child in view_id.children() {
                let element_id = child.get_element_id();
                if inherited_changed {
                    self.window_state
                        .mark_style_dirty_with(element_id, StyleReason::inherited());
                }
                if class_context_changed {
                    self.window_state.mark_style_dirty_with(
                        element_id,
                        StyleReason::class_cx(changed_classes.clone()),
                    );
                }
            }
        }

        // Compute the final style by merging inherited context with direct style
        let mut computed_style = self.inherited.clone();
        computed_style.apply_mut(self.direct.clone());

        // ─────────────────────────────────────────────────────────────────────
        // Phase 6: Update window and view state.
        // ─────────────────────────────────────────────────────────────────────

        // Track fixed elements for viewport-relative sizing
        let new_is_fixed = computed_style.builtin().is_fixed();
        // Handle fixed element registration
        if new_is_fixed {
            self.window_state.register_fixed_element(view_id);
        } else {
            self.window_state.unregister_fixed_element(view_id);
        }

        // Update view state in a single borrow
        {
            let mut vs = view_state.borrow_mut();

            vs.style_cx = self.inherited.clone();
            vs.class_cx = self.class_context.clone();
            vs.computed_style = computed_style;

            vs.style_interaction_cx = InheritedInteractionCx {
                disabled: self.view_interact_state.is_disabled,
                selected: self.view_interact_state.is_selected,
                hidden: self.view_interact_state.is_hidden,
            };
        }

        if self.reason.needs_property_extraction() {
            // ─────────────────────────────────────────────────────────────────────
            // Phase 7: Extract layout and transform properties
            // ─────────────────────────────────────────────────────────────────────
            // We read from the computed_style (which includes animated values) rather than
            // from the raw styles, so that animations affect layout and visual properties.
            let mut transitioning = false;
            {
                let mut vs = view_state.borrow_mut();

                // Clone the computed style to avoid borrow conflicts with the mutable
                // borrow needed for the extractors. This includes animated values.
                let computed = vs.computed_style.clone();

                // Layout properties (padding, margin, size, etc.)
                vs.layout_props
                    .read_explicit(&computed, &computed, &self.now, &mut transitioning);

                // View style properties (background, border, etc.)
                need_paint |= vs.view_style_props.read_explicit(
                    &computed,
                    &computed,
                    &self.now,
                    &mut transitioning,
                );

                // Transform properties (translate, scale, rotation)
                let mut box_tree_changed = false;
                box_tree_changed |= vs.view_transform_props.read_explicit(
                    &computed,
                    &computed,
                    &self.now,
                    &mut transitioning,
                );
                if box_tree_changed {
                    view_id.request_box_tree_update_for_view();
                }

                let old_cursor = vs.style_cursor;
                if old_cursor != computed.builtin().cursor() {
                    vs.style_cursor = computed.builtin().cursor();
                    self.window_state.needs_cursor_resolution = true;
                }
            }

            if transitioning {
                self.window_state
                    .schedule_style(view_id, StyleReason::transition());
            }
        }

        if old_interact_state.hidden != self.view_interact_state.is_hidden {
            for child in view_id.children() {
                self.window_state
                    .mark_style_dirty_with(child.get_element_id(), StyleReason::visibility());
            }
            view_id.request_layout();
        }
        if old_interact_state.selected != self.view_interact_state.is_selected {
            for child in view_id.children() {
                self.window_state.mark_style_dirty_with(
                    child.get_element_id(),
                    StyleReason::with_selector(super::StyleSelector::Selected),
                );
            }
        }
        if old_interact_state.disabled != self.view_interact_state.is_disabled {
            for child in view_id.children() {
                self.window_state.mark_style_dirty_with(
                    child.get_element_id(),
                    StyleReason::with_selector(super::StyleSelector::Disabled),
                );
            }
        }

        CaptureState::capture_style(view_id, self, view_state.borrow().computed_style.clone());

        // ─────────────────────────────────────────────────────────────────────
        // Phase 8: visibility transitions and set taffy style
        // ─────────────────────────────────────────────────────────────────────

        let parent_set_hidden = {
            let view_state = view_id.state();
            let view_state = view_state.borrow();
            view_state.parent_set_style_interaction.hidden
        };
        let display_override = if !parent_set_hidden {
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
                view_state.borrow_mut().visibility.phase = phase;
            }
            phase.get_display_override()
        } else {
            // parent set hidden - no transition
            Some(taffy::Display::None)
        };

        {
            // ─────────────────────────────────────────────────────────────────────
            // Phase 8.1: Get the final hidden state for view queries, layout, etc.
            // Update taffy style if layout properties changed (must happen after visibility phase override)
            // ─────────────────────────────────────────────────────────────────────
            let mut vs = view_state.borrow_mut();
            let is_hidden_final = self.view_interact_state.is_hidden
                || display_override.is_some_and(|d| d == taffy::Display::None);
            let taffy_style = vs.combined_style.clone();
            let transitioned_layout_props = vs.layout_props.to_style();
            let mut taffy_style = taffy_style
                .apply(transitioned_layout_props)
                .to_taffy_style();
            if let Some(display_override) = display_override {
                taffy_style.display = display_override;
            }

            if taffy_style != old_taffy_style {
                let taffy_node = vs.layout_id;
                vs.taffy_style = taffy_style.clone();
                view_id
                    .taffy()
                    .borrow_mut()
                    .set_style(taffy_node, taffy_style.clone())
                    .unwrap();
                if !is_hidden_final {
                    self.window_state.needs_layout = true;
                }
            }
            // ─────────────────────────────────────────────────────────────────────
            // Phase 8.2: Update box tree visiblity dependent props  (must happen after visibility phase override)
            // ─────────────────────────────────────────────────────────────────────
            {
                let box_tree = view_id.box_tree();
                let element_id = vs.element_id;
                let box_tree = &mut box_tree.borrow_mut();
                let mut flags = NodeFlags::empty();
                // need to update this after visibility.
                if (vs.computed_style.builtin().pointer_events()
                    != Some(crate::style::PointerEvents::None))
                    && !is_hidden_final
                {
                    flags |= NodeFlags::PICKABLE;
                }
                if vs
                    .computed_style
                    .builtin()
                    .set_focus()
                    .allows_keyboard_navigation()
                    && !is_hidden_final
                    && !self.view_interact_state.is_disabled
                {
                    flags |= NodeFlags::KEYBOARD_NAVIGABLE;
                }
                if vs.computed_style.builtin().set_focus().is_focusable()
                    && !is_hidden_final
                    && !self.view_interact_state.is_disabled
                {
                    flags |= NodeFlags::FOCUSABLE;
                }
                if !is_hidden_final {
                    flags |= NodeFlags::VISIBLE;
                }
                box_tree.set_flags(element_id.0, flags);

                let new_z_index = vs.combined_style.builtin().z_index().unwrap_or(0);

                // Get old z-index from box tree
                let old_z_index = box_tree.z_index(element_id.0).unwrap_or(0);
                if old_z_index != new_z_index {
                    box_tree.set_z_index(element_id.0, new_z_index);
                }
            }
            // ─────────────────────────────────────────────────────────────────────
            // Phase 8.3: request paint for view style changes if not hidden
            // ─────────────────────────────────────────────────────────────────────
            if !is_hidden_final && need_paint {
                self.window_state.request_paint(view_id);
            }
        }

        // ─────────────────────────────────────────────────────────────────────
        // Phase 9: Call view's style_pass
        // ─────────────────────────────────────────────────────────────────────

        view.borrow_mut().style_pass(self);
    }

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
        let mut view_interact_state = Self::get_interact_state(self.window_state, element_id);
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

    pub fn now(&self) -> Instant {
        self.now
    }

    pub fn current_view(&self) -> ViewId {
        self.current_view
    }

    pub fn get_prop<P: StyleProp>(&self, _prop: P) -> Option<P::Type> {
        self.direct
            .get_prop::<P>()
            .or_else(|| self.inherited.get_prop::<P>())
    }

    pub fn style(&self) -> Style {
        self.inherited.clone().apply(self.direct.clone())
    }

    pub fn direct_style(&self) -> &Style {
        &self.direct
    }

    pub fn indirect_style(&self) -> &Style {
        &self.inherited
    }

    pub fn request_transition(&mut self) {
        self.request_transition_for(self.current_view.get_element_id());
    }

    /// Request a transition-driven style update for a specific style target.
    ///
    /// This allows transition requests to target sub-elements instead of always
    /// assuming the owning view element.
    pub fn request_transition_for(&mut self, target: impl Into<ElementId>) {
        if !self.view_interact_state.is_hidden {
            self.window_state
                .schedule_style_with_target(target.into(), StyleReason::transition());
        }
    }

    pub fn get_interact_state(
        window_state: &WindowState,
        id: impl Into<crate::ElementId>,
    ) -> InteractionState {
        let id: crate::ElementId = id.into();
        let view_id = id.owning_id();
        let style_parent = view_id
            .state()
            .borrow()
            .style_cx_parent
            .or_else(|| view_id.parent());
        let parent_override = {
            let view_state = view_id.state();
            let view_state = view_state.borrow();
            view_state.parent_set_style_interaction
        };
        let parent_cx = style_parent
            .map(|p| p.state().borrow().style_interaction_cx)
            .unwrap_or_default();
        // TODO: use box tree child order instead
        let (child_index, sibling_count) = if let Some(parent) = style_parent {
            // When using a custom style parent, the styled view may not be a direct
            // child of that parent (e.g. list item wrappers). Use the nearest
            // ancestor that is directly under the style parent for structural index.
            let indexed_child = parent.with_children(|siblings| {
                if siblings.contains(&view_id) {
                    Some(view_id)
                } else {
                    let mut cursor = view_id.parent();
                    while let Some(ancestor) = cursor {
                        if ancestor.parent() == Some(parent) {
                            return Some(ancestor);
                        }
                        cursor = ancestor.parent();
                    }
                    None
                }
            });

            parent.with_children(|siblings| {
                (
                    indexed_child
                        .and_then(|id| siblings.iter().position(|child| *child == id))
                        .map(|i| i + 1),
                    siblings.len(),
                )
            })
        } else {
            (None, 0)
        };

        InteractionState {
            is_selected: parent_override.selected | parent_cx.selected,
            is_disabled: parent_override.disabled | parent_cx.disabled,
            is_hidden: parent_override.hidden | parent_cx.hidden,
            is_hovered: window_state.is_hovered(id),
            is_focused: window_state.is_focused(id),
            is_focus_within: window_state.is_focus_within(id),
            is_active: window_state.is_active(id),
            is_dark_mode: window_state.is_dark_mode(),
            is_file_hover: window_state.is_file_hover(id),
            using_keyboard_navigation: window_state.keyboard_navigation,
            child_index,
            sibling_count,
            window_width: window_state.root_size.width,
        }
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
        id.request_style(StyleReason::animation());
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
        id.request_style(StyleReason::animation());
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
        id.request_style(StyleReason::animation());
    }

    id.children().into_iter().for_each(animations_on_create);
}
