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

use crate::ElementId;
use crate::animate::{AnimStateKind, RepeatMode};
use crate::inspector::CaptureState;
use crate::style::recalc::{StyleReasonFlags, StyleReasonSet};
use crate::style::{StyleClassRef, resolve_nested_maps};
use crate::view::ViewId;
use crate::view::stacking::invalidate_stacking_cache;
use crate::window::state::WindowState;

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

    parent_disabled: bool,
    parent_hidden: bool,
    parent_selected: bool,

    /// The reason this view was marked dirty. Available to `style_pass`
    /// implementations so views can make informed decisions about how much
    /// work to do.
    pub(crate) reason: Option<StyleReasonSet>,

    /// Targeted sub-element invalidations. Populated when one or more specific
    /// `ElementId`s owned by this view need updating without a full view restyle.
    /// Empty means the full view is being restyled.
    ///
    /// `style_pass` implementations should walk this list and update only the
    /// affected elements when non-empty, skipping the full cascade for untouched
    /// sub-elements.
    pub(crate) targeted_elements: SmallVec<[(ElementId, StyleReasonSet); 2]>,
}

impl<'a> StyleCx<'a> {
    /// Returns the targeted sub-element changes for this style pass, if any.
    /// When non-empty, only these specific elements need updating — the view's
    /// own cascade is clean and `style_pass` should avoid full restyle work.
    pub fn targeted_elements(&self) -> &[(ElementId, StyleReasonSet)] {
        &self.targeted_elements
    }

    /// Returns true if this style pass is only updating specific sub-elements
    /// and the view's own cascade is clean.
    pub fn is_targeted_only(&self) -> bool {
        !self.targeted_elements.is_empty()
            && self
                .reason
                .as_ref()
                .map(|r| r.flags == StyleReasonFlags::TARGET)
                .unwrap_or(false)
    }

    pub fn reason(&self) -> Option<&StyleReasonSet> {
        self.reason.as_ref()
    }
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
                let inherited_style = parent_state.style_cx.clone().unwrap_or_default();

                // Class context comes from parent's class_cx (contains class nested maps)
                let class_ctx = parent_state.class_cx.clone().unwrap_or_default();

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
            reason: None,
            targeted_elements: SmallVec::new(),
        }
    }

    pub fn style_view(&mut self, view_id: ViewId) {
        let reason = self.window_state.take_style_reason(view_id);

        // Always populate targeted_elements from the reason so style_pass can
        // inspect them regardless of which path we take. A full cascade pass
        // may still want to know which specific elements were targeted in addition
        // to the view-level restyle.
        self.targeted_elements = reason
            .as_ref()
            .map(|r| r.targets.iter().map(|(id, r)| (*id, *r.clone())).collect())
            .unwrap_or_default();
        self.reason = reason.clone();

        // TARGET-only fast path: only specific sub-elements need updating.
        if let Some(ref r) = reason {
            if r.flags == StyleReasonFlags::TARGET {
                dbg!("fast path");
                let vs = view_id.state();
                let vs = vs.borrow();
                if let (Some(style_cx), Some(class_cx)) = (vs.style_cx.clone(), vs.class_cx.clone())
                {
                    self.inherited = style_cx;
                    self.class_context = class_cx;
                    drop(vs);
                    view_id.view().borrow_mut().style_pass(self);
                    self.targeted_elements.clear();
                    self.reason = None;
                    return;
                }
                // View has never had a full style pass yet — fall through to full restyle.
                drop(vs);
            }
        }

        let needs_full = reason
            .as_ref()
            .map(|r| r.needs_full_recalc())
            .unwrap_or(true);

        if needs_full {
            self.style_view_cascade(view_id, reason);
        } else {
            self.style_view_anim_transition_only(view_id);
        }

        self.targeted_elements.clear();
        self.reason = None;
    }

    /// Full cascade recomputation: phases 2–6 then finalize.
    /// Runs resolve_nested_maps, animations, computes combined style,
    /// propagates inherited/class context to children.
    fn style_view_cascade(&mut self, view_id: ViewId, reason: Option<StyleReasonSet>) {
        let view = view_id.view();
        let view_state = view_id.state();

        // ── Phase 2: Update view style, propagate recursive requests ──────────
        let view_class = view.borrow().view_class();
        let base_style = {
            let mut vs = view_state.borrow_mut();

            if self.window_state.view_style_dirty.contains(&view_id) {
                self.window_state.view_style_dirty.remove(&view_id);
                if let Some(view_style) = view.borrow().view_style() {
                    let offset = vs.view_style_offset;
                    vs.style.set(offset, view_style);
                }
            }

            if vs.request_style_recursive {
                vs.request_style_recursive = false;
                for child in view_id.children() {
                    let child_state = child.state();
                    child_state.borrow_mut().request_style_recursive = true;
                    let element_id = child_state.borrow().element_id;
                    self.window_state.mark_style_dirty(element_id);
                }
            }

            vs.style()
        };

        // ── Phase 3: Build interaction state ──────────────────────────────────
        let mut view_interact_state = self.get_interact_state(view_id);
        view_interact_state.is_disabled |= base_style.builtin().set_disabled();
        view_interact_state.is_selected |= base_style.builtin().set_selected();
        view_interact_state.is_hidden |= base_style.builtin().display() == taffy::Display::None;

        // ── Phase 3.5a: Selector fast path ────────────────────────────────────
        // If the only reason this view is dirty is a selector change, check whether
        // this view actually has any styles gated on that selector. If not, the
        // selector firing cannot affect this view's computed style.
        if let Some(reason) = &reason {
            if reason.flags == StyleReasonFlags::SELECTOR {
                if let Some(selectors) = &reason.selectors {
                    let has_relevant_selector = {
                        let vs = view_state.borrow();
                        vs.has_style_selectors.intersects(*selectors)
                    };
                    if !has_relevant_selector {
                        // Still need to apply this view's inherited/class contributions
                        // so that StyleCx contexts are correct for subsequent views.
                        let combined = view_state.borrow().combined_style.clone();
                        self.direct = combined.clone();
                        Style::apply_only_inherited(&mut self.inherited, &self.direct);
                        Style::apply_only_class_maps(&mut self.class_context, &self.direct);

                        // Forward only propagating selectors to children.
                        let child_reason = reason.for_children();
                        if !child_reason.flags.is_empty() {
                            for child in view_id.children() {
                                self.window_state.mark_style_dirty_with(
                                    child.get_element_id(),
                                    child_reason.clone(),
                                );
                            }
                        }

                        let mut computed_style = self.inherited.clone();
                        computed_style.apply_mut(combined);
                        let view_interact_state = self.get_interact_state(view_id);
                        self.style_view_finalize(view_id, computed_style, view_interact_state);
                        return;
                    }
                }
            }
        }

        // ── Phase 3.5: Inherited/class context fast paths ─────────────────────
        if let Some(reason) = &reason {
            let only_context_changed = !reason.flags.intersects(
                StyleReasonFlags::SELECTOR
                    | StyleReasonFlags::TARGET
                    | StyleReasonFlags::ANIMATION
                    | StyleReasonFlags::TRANSITION,
            ) && reason.flags.intersects(
                StyleReasonFlags::INHERITED_CHANGE | StyleReasonFlags::CLASS_CONTEXT_CHANGE,
            );

            if only_context_changed && !self.window_state.view_style_dirty.contains(&view_id) {
                let inherited_only = reason.flags == StyleReasonFlags::INHERITED_CHANGE;
                let class_only = reason.flags == StyleReasonFlags::CLASS_CONTEXT_CHANGE;
                let both = reason.flags.contains(
                    StyleReasonFlags::INHERITED_CHANGE | StyleReasonFlags::CLASS_CONTEXT_CHANGE,
                );

                let has_selectors = !view_state.borrow().has_style_selectors.is_empty();
                let uses_classes = !view_state.borrow().classes.is_empty();

                let can_skip = match (inherited_only, class_only, both) {
                    (true, _, _) => !has_selectors, // inherited only: skip if no selectors
                    (_, true, _) => !uses_classes,  // class only: skip if no classes
                    (_, _, true) => !has_selectors && !uses_classes, // both: must satisfy both
                    _ => false,
                };

                if can_skip {
                    let mut computed_style = self.inherited.clone();
                    let combined = view_state.borrow().combined_style.clone();
                    computed_style.apply_mut(combined.clone());

                    // Even though the cascade output didn't change, we still need to update
                    // self.inherited and self.class_context to reflect this view's contribution,
                    // so that children and subsequent style passes see the correct contexts.
                    self.direct = combined;
                    let old_inherited_map = self.inherited.map.clone();
                    Style::apply_only_inherited(&mut self.inherited, &self.direct);
                    let inherited_changed = !self.inherited.map.ptr_eq(&old_inherited_map);

                    let old_class_context = self.class_context.clone();
                    Style::apply_only_class_maps(&mut self.class_context, &self.direct);
                    let class_context_changed =
                        !self.class_context.class_maps_ptr_eq(&old_class_context);

                    // Re-check whether context actually changed after applying this view's
                    // contribution — it may not have, in which case children don't need dirtying.
                    let child_reason = reason.for_children();
                    if !child_reason.flags.is_empty()
                        && (inherited_changed || class_context_changed)
                    {
                        for child in view_id.children() {
                            self.window_state.mark_style_dirty_with(
                                child.get_element_id(),
                                child_reason.clone(),
                            );
                        }
                    }

                    let view_interact_state = self.get_interact_state(view_id);
                    self.style_view_finalize(view_id, computed_style, view_interact_state);
                    return;
                }
            }
        }

        // ── Phase 4: Resolve combined style (cascade + animations) ────────────
        let (combined_style, _classes_applied, has_active_animation) =
            view_state.borrow_mut().compute_combined(
                &mut view_interact_state,
                self.window_state.screen_size_bp,
                view_class,
                &self.inherited,
                &self.class_context,
            );

        if has_active_animation {
            self.window_state
                .schedule_style(view_id, StyleReasonSet::animation());
        }

        // ── Phase 5: Update inherited/class contexts ───────────────────────────
        self.direct = combined_style;

        let old_inherited_map = self.inherited.map.clone(); // O(1) Rc clone
        Style::apply_only_inherited(&mut self.inherited, &self.direct);
        let inherited_changed = !self.inherited.map.ptr_eq(&old_inherited_map);

        let old_class_context = self.class_context.clone(); // cheap ImHashMap clone
        Style::apply_only_class_maps(&mut self.class_context, &self.direct);
        let class_context_changed = !self.class_context.class_maps_ptr_eq(&old_class_context);

        // ── Phase 6: Dirty children based on what actually changed ────────────
        if inherited_changed || class_context_changed {
            for child in view_id.children() {
                let mut reason = StyleReasonSet::empty();
                if inherited_changed {
                    reason.flags |= StyleReasonFlags::INHERITED_CHANGE;
                }
                if class_context_changed {
                    reason.flags |= StyleReasonFlags::CLASS_CONTEXT_CHANGE;
                }
                self.window_state
                    .mark_style_dirty_with(child.get_element_id(), reason);
            }
        }

        let mut computed_style = self.inherited.clone();
        computed_style.apply_mut(self.direct.clone());

        self.style_view_finalize(view_id, computed_style, view_interact_state);
    }

    /// Animation/transition fast path: skip cascade entirely, step animations and
    /// transitions over the already-resolved `combined_style` from the last full pass.
    /// Valid because animations/transitions are post-processing and don't affect the
    /// inherited context, class resolution, or selector matching.
    fn style_view_anim_transition_only(&mut self, view_id: ViewId) {
        let view_state = view_id.state();

        // Restore the owning view's resolved contexts so that computed_style is
        // built correctly. Animations are post-processing on top of the view's
        // own cascade output — they need the view's style_cx, not just what was
        // inherited from the parent.
        {
            let vs = view_state.borrow();
            if let (Some(style_cx), Some(class_cx)) = (vs.style_cx.clone(), vs.class_cx.clone()) {
                self.inherited = style_cx;
                self.class_context = class_cx;
            } else {
                // View has never completed a full style pass — can't safely animate.
                // Fall through to full cascade.
                drop(vs);
                self.style_view_cascade(view_id, None);
                return;
            }
        }

        let mut vs = view_state.borrow_mut();
        let mut combined = vs.combined_style.clone();
        let mut has_active_animation = false;
        for animation in vs
            .animations
            .stack
            .iter_mut()
            .filter(|a| a.can_advance() || a.should_apply_folded())
        {
            if animation.can_advance() {
                has_active_animation = true;
                animation.animate_into(&mut combined);
                animation.advance();
            } else {
                animation.apply_folded(&mut combined);
            }
        }
        vs.combined_style = combined.clone();
        drop(vs);

        if has_active_animation {
            self.window_state
                .schedule_style(view_id, StyleReasonSet::animation());
        }

        self.direct = combined;
        let mut computed_style = self.inherited.clone();
        computed_style.apply_mut(self.direct.clone());

        let view_interact_state = self.get_interact_state(view_id);
        self.style_view_finalize(view_id, computed_style, view_interact_state);
    }

    /// Phases 7–10: always runs regardless of which compute path was taken.
    /// Updates view state, taffy layout, box tree flags, paint requests, z-index.
    fn style_view_finalize(
        &mut self,
        view_id: ViewId,
        computed_style: Style,
        mut view_interact_state: InteractionState,
    ) {
        let view = view_id.view();
        let view_state = view_id.state();

        // ── Phase 7: Update view state ─────────────────────────────────────────
        CaptureState::capture_style(view_id, self, computed_style.clone());

        let new_is_fixed = computed_style.builtin().is_fixed();
        let computed_style_has_disabled = computed_style.builtin().set_disabled();
        let computed_style_has_selected = computed_style.builtin().set_selected();
        let compute_style_has_hidden = computed_style.builtin().display() == taffy::Display::None;
        view_interact_state.is_hidden |= compute_style_has_hidden;
        view_interact_state.is_selected |= computed_style_has_selected;
        view_interact_state.is_disabled |= computed_style_has_disabled;

        let parent_set = view_state.borrow().parent_set_style_interaction;

        let old_taffy_style = {
            let mut vs = view_state.borrow_mut();
            let old_taffy = vs.taffy_style.clone();
            vs.style_cx = Some(self.inherited.clone());
            vs.class_cx = Some(self.class_context.clone());
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
            old_taffy
        };

        if new_is_fixed {
            self.window_state.register_fixed_element(view_id);
        } else {
            self.window_state.unregister_fixed_element(view_id);
        }

        // ── Phase 7.1: Extract layout/transform/view-style props ──────────────
        let mut transitioning = false;
        let mut layout_transitioning = false;
        let mut view_style_transitioning = false;
        let mut view_style_changed = false;
        {
            let mut vs = view_state.borrow_mut();
            let computed = vs.computed_style.clone();

            vs.layout_props.read_explicit(
                &computed,
                &computed,
                &self.now,
                &mut layout_transitioning,
            );
            transitioning |= layout_transitioning;

            if vs.view_style_props.read_explicit(
                &computed,
                &computed,
                &self.now,
                &mut view_style_transitioning,
            ) {
                view_style_changed = true;
            }
            transitioning |= view_style_transitioning;

            let mut box_tree_transitioning = false;
            if vs.view_transform_props.read_explicit(
                &computed,
                &computed,
                &self.now,
                &mut box_tree_transitioning,
            ) || box_tree_transitioning
            {
                view_id.request_box_tree_update_for_view();
            }
            transitioning |= box_tree_transitioning;

            let old_cursor = vs.style_cursor;
            if old_cursor != computed.builtin().cursor() {
                vs.style_cursor = computed.builtin().cursor();
                self.window_state.needs_cursor_resolution = true;
            }
        }

        if transitioning {
            self.window_state
                .schedule_style(view_id, StyleReasonSet::transition());
        }

        // ── Visibility change detection ────────────────────────────────────────
        let was_hidden = view_state.borrow().is_hidden;
        let is_visible = !compute_style_has_hidden && !self.parent_hidden && !parent_set.hidden;
        if was_hidden == is_visible {
            for child in view_id.children() {
                self.window_state.mark_style_dirty(child.get_element_id());
            }
            view_id.request_layout();
        }
        view_state.borrow_mut().is_hidden = !is_visible;

        // ── Phase 8: style_pass ────────────────────────────────────────────────
        view.borrow_mut().style_pass(self);

        // ── Phase 9: Visibility transitions ───────────────────────────────────
        let parent_set_hidden = view_state.borrow().parent_set_style_interaction.hidden;
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
            if let Some(display) = phase.get_display_override() {
                let mut vs = view_state.borrow_mut();
                vs.combined_style = vs.combined_style.clone().display(display);
            }
        } else {
            let mut vs = view_state.borrow_mut();
            vs.combined_style = vs.combined_style.clone().display(taffy::Display::None);
        }

        // ── Phase 9.1–9.3: Taffy style, box tree flags, paint ─────────────────
        {
            let mut vs = view_state.borrow_mut();
            // TODO: simplify this is_hidden logic
            let is_hidden = view_interact_state.is_hidden
                || (vs.is_hidden
                    && vs
                        .visibility
                        .phase
                        .get_display_override()
                        .is_none_or(|d| d == taffy::Display::None));

            let taffy_style = vs
                .combined_style
                .clone()
                .apply(vs.layout_props.to_style())
                .to_taffy_style();

            if taffy_style != old_taffy_style {
                let taffy_node = vs.layout_id;
                vs.taffy_style = taffy_style.clone();
                view_id
                    .taffy()
                    .borrow_mut()
                    .set_style(taffy_node, taffy_style)
                    .unwrap();
                if !is_hidden {
                    self.window_state.schedule_layout();
                }
            }

            {
                let box_tree = view_id.box_tree();
                let element_id = vs.element_id;
                let box_tree = &mut box_tree.borrow_mut();
                let mut flags = NodeFlags::empty();
                if vs.computed_style.builtin().pointer_events()
                    != Some(crate::style::PointerEvents::None)
                    && !is_hidden
                {
                    flags |= NodeFlags::PICKABLE;
                }
                if vs
                    .computed_style
                    .builtin()
                    .set_focus()
                    .allows_keyboard_navigation()
                    && !is_hidden
                    && !view_interact_state.is_disabled
                {
                    flags |= NodeFlags::KEYBOARD_NAVIGABLE;
                }
                if vs.computed_style.builtin().set_focus().is_focusable()
                    && !is_hidden
                    && !view_interact_state.is_disabled
                {
                    flags |= NodeFlags::FOCUSABLE;
                }
                if !is_hidden {
                    flags |= NodeFlags::VISIBLE;
                }
                box_tree.set_flags(element_id.0, flags);
            }

            if !is_hidden && (view_style_transitioning || view_style_changed) {
                self.window_state.request_paint(view_id);
            }
        }

        // ── Phase 10: Z-index / stacking context ──────────────────────────────
        {
            let vs = view_state.borrow();
            let new_z_index = vs.combined_style.builtin().z_index().unwrap_or(0);
            let element_id = view_id.get_element_id();
            let old_z_index = self
                .window_state
                .box_tree
                .borrow()
                .z_index(element_id.0)
                .unwrap_or(0);
            drop(vs);

            if old_z_index != new_z_index {
                invalidate_stacking_cache(element_id);
                self.window_state
                    .box_tree
                    .borrow_mut()
                    .set_z_index(element_id.0, new_z_index);
            }
        }
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

    pub fn now(&self) -> Instant {
        self.now
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
        let id = self.current_view;
        if !self.parent_hidden {
            self.window_state
                .schedule_style(id, StyleReasonSet::transition());
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
            is_active: self.window_state.is_active(id),
            is_dark_mode: self.window_state.is_dark_mode(),
            is_file_hover: self.window_state.is_file_hover(id),
            using_keyboard_navigation: self.window_state.keyboard_navigation,
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
        id.request_style(StyleReasonSet::animation());
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
        id.request_style(StyleReasonSet::animation());
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
        id.request_style(StyleReasonSet::animation());
    }

    id.children().into_iter().for_each(animations_on_create);
}
