//! Engine-owned style tree — the host-agnostic counterpart to Taffy's layout
//! tree.
//!
//! A [`StyleTree`] stores one [`StyleNode`] per styled element, carrying the
//! node's direct style, applied classes, cached cascade outputs, and the
//! parent/child edges needed for selector matching (`:first-child`,
//! `:nth-child`, cascade propagation).
//!
//! Hosts (floem, future floem-native) keep a mapping from their own id type
//! (e.g. `ViewId`) to [`StyleNodeId`] and push state changes here:
//!
//! ```text
//! host creates view   → tree.new_node(element_id)
//! host adds child     → tree.set_parent(child, parent)
//!                     → tree.set_children(parent, &[...])
//! host sets style     → tree.set_direct_style(node, style)
//! host marks dirty    → tree.mark_dirty(node, reason)
//! host runs pass      → tree.compute_style(root, &mut sink)
//! host reads result   → tree.computed_style(node)
//! ```

use rustc_hash::FxHashSet;
use smallvec::SmallVec;
use slotmap::{SlotMap, new_key_type};

use crate::builtin_props::Display;
use crate::cache::{StyleCache, StyleCacheKey};
use crate::cascade::resolve_nested_maps;
use crate::element_id::ElementId;
use crate::interaction::{InheritedInteractionCx, InteractionState};
use crate::props::StyleClassRef;
use crate::recalc::StyleReason;
use crate::selectors::{StyleSelector, StyleSelectors};
use crate::sink::StyleSink;
use crate::style::Style;

new_key_type! {
    /// Identity of a node inside a [`StyleTree`]. Dense key backed by
    /// [`slotmap`]; stable across removals within the same tree but not
    /// across trees.
    pub struct StyleNodeId;
}

/// Per-element state owned by the style engine.
///
/// Most fields are caches populated by `compute_style`. Host-pushed inputs
/// are [`direct_style`](Self::direct_style), [`classes`](Self::classes), the
/// parent/children edges, and
/// [`parent_set_style_interaction`](Self::parent_set_style_interaction).
#[derive(Debug, Clone)]
pub struct StyleNode {
    /// Host-side identity for this node, used when the cascade needs to
    /// query the sink (`is_hovered(element_id)`, etc.) or emit per-element
    /// callbacks (`request_paint(element_id)`).
    pub element_id: ElementId,

    // ── Tree edges ──────────────────────────────────────────────────────
    pub(crate) parent: Option<StyleNodeId>,
    pub(crate) children: Vec<StyleNodeId>,

    // ── Host-pushed inputs ──────────────────────────────────────────────
    /// Style set directly on this node via the `.style(...)` setter.
    pub direct_style: Style,
    /// Classes applied to this node via `.class(...)` / `.apply_class(...)`.
    pub classes: SmallVec<[StyleClassRef; 4]>,
    /// Interaction overrides pushed by a parent (e.g. "force-disable
    /// descendants"). OR'd with inherited parent state during cascade.
    pub parent_set_style_interaction: InheritedInteractionCx,
    /// Optional override of the `(child_index, sibling_count)` used for
    /// `:nth-child` / `:first-child` / `:last-child`. Hosts that use a
    /// separate DOM tree (e.g. floem's `style_cx_parent` split) push the
    /// structural position computed from their own tree here. `None`
    /// means "use the tree's own parent/children edges".
    pub structural_position_override: Option<(Option<usize>, usize)>,

    // ── Cascade outputs (populated by compute_style — Phase 1b) ────────
    /// Resolved style after class + selector merging, without inherited
    /// properties from ancestors. Used for cache keys and child
    /// propagation.
    pub(crate) combined_style: Style,
    /// Final style including inherited properties. This is what prop
    /// extractors read.
    pub(crate) computed_style: Style,
    /// Inherited-only slice of this node's computed style. Passed to
    /// children as their inherited context.
    pub(crate) inherited_context: Style,
    /// Class-map-only slice of this node's direct style. Passed to
    /// children as their class context.
    pub(crate) class_context: Style,
    /// Interaction cx after resolving — becomes the inherited cx for
    /// children.
    pub(crate) style_interaction_cx: InheritedInteractionCx,
    /// Selectors present in this node's style tree, cached for fast-path
    /// dirty propagation.
    pub(crate) has_style_selectors: Option<StyleSelectors>,

    // ── Dirty bookkeeping ──────────────────────────────────────────────
    /// Why this node (or a subtree rooted here) is pending recomputation.
    /// Empty means the caches are up to date.
    pub(crate) dirty: StyleReason,
}

impl StyleNode {
    fn new(element_id: ElementId) -> Self {
        Self {
            element_id,
            parent: None,
            children: Vec::new(),
            direct_style: Style::new(),
            classes: SmallVec::new(),
            parent_set_style_interaction: InheritedInteractionCx::default(),
            structural_position_override: None,
            combined_style: Style::new(),
            computed_style: Style::new(),
            inherited_context: Style::new(),
            class_context: Style::new(),
            style_interaction_cx: InheritedInteractionCx::default(),
            has_style_selectors: None,
            dirty: StyleReason::style_pass(),
        }
    }

    pub fn parent(&self) -> Option<StyleNodeId> {
        self.parent
    }

    pub fn children(&self) -> &[StyleNodeId] {
        &self.children
    }
}

/// Engine-owned slotmap of [`StyleNode`]s. Cheap to clone if you need a
/// snapshot (nodes are plain data). Typical use is a single long-lived
/// instance per host window.
#[derive(Default, Debug)]
pub struct StyleTree {
    nodes: SlotMap<StyleNodeId, StyleNode>,
    cache: StyleCache,
    // Per-selector interest registries, populated as the cascade resolves
    // each node's style. Let descendant-dirty walks skip directly to the
    // small set of nodes that could possibly match a given selector
    // instead of traversing the full subtree.
    responsive_interest: FxHashSet<StyleNodeId>,
    disabled_interest: FxHashSet<StyleNodeId>,
    selected_interest: FxHashSet<StyleNodeId>,
}

impl StyleTree {
    pub fn new() -> Self {
        Self {
            nodes: SlotMap::with_key(),
            cache: StyleCache::new(),
            responsive_interest: FxHashSet::default(),
            disabled_interest: FxHashSet::default(),
            selected_interest: FxHashSet::default(),
        }
    }

    /// Drop every cached cascade result. The host should call this when a
    /// global input changes such that the cache keys are no longer
    /// meaningful (OS theme flip, responsive breakpoint change, etc.).
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    /// Read-only view into the cache stats. Primarily for tests/debug.
    pub fn cache_stats(&self) -> crate::cache::CacheStats {
        self.cache.stats()
    }

    /// Update `node`'s entry in the per-selector interest registries to
    /// match the selector set its style resolved to.
    fn update_selector_interest(
        &mut self,
        node: StyleNodeId,
        selectors: Option<StyleSelectors>,
    ) {
        let has_responsive = selectors.is_some_and(StyleSelectors::has_responsive);
        let has_disabled = selectors.is_some_and(|s| s.has(StyleSelector::Disabled));
        let has_selected = selectors.is_some_and(|s| s.has(StyleSelector::Selected));
        set_membership(&mut self.responsive_interest, node, has_responsive);
        set_membership(&mut self.disabled_interest, node, has_disabled);
        set_membership(&mut self.selected_interest, node, has_selected);
    }

    /// Mark every descendant of `ancestor` that has interest in `selector`
    /// as dirty and return `(element_id, reason)` pairs for each so the
    /// host can feed them into its own per-frame scheduling. Only
    /// `Selected` and `Disabled` are tracked; other selectors return empty.
    pub fn mark_descendants_with_selector_dirty(
        &mut self,
        ancestor: StyleNodeId,
        selector: StyleSelector,
    ) -> SmallVec<[(ElementId, StyleReason); 4]> {
        let candidates: SmallVec<[StyleNodeId; 4]> = match selector {
            StyleSelector::Disabled => self.disabled_interest.iter().copied().collect(),
            StyleSelector::Selected => self.selected_interest.iter().copied().collect(),
            _ => return SmallVec::new(),
        };
        let reason = StyleReason::with_selector(selector);
        self.dirty_ancestry_matches(&candidates, ancestor, &reason)
    }

    /// Responsive counterpart of [`Self::mark_descendants_with_selector_dirty`].
    pub fn mark_descendants_with_responsive_selector_dirty(
        &mut self,
        ancestor: StyleNodeId,
    ) -> SmallVec<[(ElementId, StyleReason); 4]> {
        let candidates: SmallVec<[StyleNodeId; 4]> =
            self.responsive_interest.iter().copied().collect();
        let reason = StyleReason::with_selectors(StyleSelectors::empty().responsive());
        self.dirty_ancestry_matches(&candidates, ancestor, &reason)
    }

    /// For each candidate whose parent chain passes through `ancestor`,
    /// set its dirty bit and collect `(element_id, reason)`. Shared core
    /// of both descendant-dirty walks. Skips `ancestor` itself — floem's
    /// original walk excludes the node that triggered the descent.
    fn dirty_ancestry_matches(
        &mut self,
        candidates: &[StyleNodeId],
        ancestor: StyleNodeId,
        reason: &StyleReason,
    ) -> SmallVec<[(ElementId, StyleReason); 4]> {
        let mut out: SmallVec<[(ElementId, StyleReason); 4]> = SmallVec::new();
        for &node in candidates {
            if node != ancestor
                && self.is_descendant_of(node, ancestor)
                && let Some(element_id) = self.nodes.get(node).map(|n| n.element_id)
            {
                self.mark_dirty(node, reason.clone());
                out.push((element_id, reason.clone()));
            }
        }
        out
    }

    /// Allocate a new orphan node tied to `element_id`. The node has no
    /// parent and no children until a setter is called.
    pub fn new_node(&mut self, element_id: ElementId) -> StyleNodeId {
        self.nodes.insert(StyleNode::new(element_id))
    }

    /// Remove a node. Detaches from its parent's child list and clears
    /// each child's `parent` pointer (children are NOT recursively removed
    /// — hosts are responsible for removing subtrees explicitly).
    pub fn remove_node(&mut self, id: StyleNodeId) -> Option<StyleNode> {
        let node = self.nodes.remove(id)?;
        self.responsive_interest.remove(&id);
        self.disabled_interest.remove(&id);
        self.selected_interest.remove(&id);
        if let Some(parent) = node.parent
            && let Some(parent_node) = self.nodes.get_mut(parent)
        {
            parent_node.children.retain(|c| *c != id);
        }
        for child in &node.children {
            if let Some(child_node) = self.nodes.get_mut(*child) {
                child_node.parent = None;
            }
        }
        Some(node)
    }

    pub fn contains(&self, id: StyleNodeId) -> bool {
        self.nodes.contains_key(id)
    }

    pub fn get(&self, id: StyleNodeId) -> Option<&StyleNode> {
        self.nodes.get(id)
    }

    pub fn get_mut(&mut self, id: StyleNodeId) -> Option<&mut StyleNode> {
        self.nodes.get_mut(id)
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    // ── Tree-structure setters ──────────────────────────────────────────

    /// Set `child`'s parent to `parent`. Detaches `child` from any previous
    /// parent. Panics if either id is unknown or if the operation would
    /// create a cycle (parent must not be a descendant of child).
    pub fn set_parent(&mut self, child: StyleNodeId, parent: Option<StyleNodeId>) {
        assert!(self.contains(child), "unknown child node");
        if let Some(p) = parent {
            assert!(self.contains(p), "unknown parent node");
            debug_assert!(
                !self.is_descendant_of(p, child),
                "set_parent would create a cycle"
            );
        }

        let old_parent = self.nodes[child].parent;
        if old_parent == parent {
            return;
        }

        if let Some(old) = old_parent
            && let Some(old_node) = self.nodes.get_mut(old)
        {
            old_node.children.retain(|c| *c != child);
        }

        self.nodes[child].parent = parent;
        if let Some(p) = parent {
            self.nodes[p].children.push(child);
        }
        let reason = StyleReason::inherited();
        self.mark_dirty(child, reason);
    }

    /// Replace `parent`'s child list. Each child's `parent` pointer is
    /// updated to point at `parent`. Previous children retained in the new
    /// list keep their position; removed children become orphans.
    pub fn set_children(&mut self, parent: StyleNodeId, children: &[StyleNodeId]) {
        assert!(self.contains(parent), "unknown parent node");
        for c in children {
            assert!(self.contains(*c), "unknown child node");
        }

        let old_children = std::mem::take(&mut self.nodes[parent].children);
        for c in &old_children {
            if !children.contains(c)
                && let Some(node) = self.nodes.get_mut(*c)
            {
                node.parent = None;
            }
        }
        for c in children {
            if let Some(node) = self.nodes.get_mut(*c) {
                if let Some(prev_parent) = node.parent
                    && prev_parent != parent
                    && let Some(prev) = self.nodes.get_mut(prev_parent)
                {
                    prev.children.retain(|existing| existing != c);
                }
                self.nodes[*c].parent = Some(parent);
            }
        }
        self.nodes[parent].children = children.to_vec();
        self.mark_dirty(parent, StyleReason::inherited());
    }

    fn is_descendant_of(&self, candidate: StyleNodeId, root: StyleNodeId) -> bool {
        let mut cursor = Some(candidate);
        while let Some(id) = cursor {
            if id == root {
                return true;
            }
            cursor = self.nodes.get(id).and_then(|n| n.parent);
        }
        false
    }

    // ── Style input setters ─────────────────────────────────────────────

    pub fn set_direct_style(&mut self, id: StyleNodeId, style: Style) {
        if let Some(node) = self.nodes.get_mut(id) {
            node.direct_style = style;
            node.dirty.merge(StyleReason::style_pass());
        }
    }

    pub fn set_classes(&mut self, id: StyleNodeId, classes: &[StyleClassRef]) {
        if let Some(node) = self.nodes.get_mut(id) {
            node.classes.clear();
            node.classes.extend_from_slice(classes);
            node.dirty.merge(StyleReason::class_cx(Default::default()));
        }
    }

    pub fn set_parent_interaction(
        &mut self,
        id: StyleNodeId,
        interaction: InheritedInteractionCx,
    ) {
        if let Some(node) = self.nodes.get_mut(id) {
            node.parent_set_style_interaction = interaction;
            node.dirty.merge(StyleReason::style_pass());
        }
    }

    /// Override the structural position used for `:nth-child` et al. Hosts
    /// whose structural-position notion differs from the tree's own
    /// parent/children edges (e.g. floem's list-item re-parenting) push
    /// the host-computed `(child_index, sibling_count)` here. Pass `None`
    /// to defer to the tree's own edges.
    pub fn set_structural_position_override(
        &mut self,
        id: StyleNodeId,
        pos: Option<(Option<usize>, usize)>,
    ) {
        if let Some(node) = self.nodes.get_mut(id) {
            if node.structural_position_override != pos {
                node.dirty.merge(StyleReason::style_pass());
            }
            node.structural_position_override = pos;
        }
    }

    pub fn mark_dirty(&mut self, id: StyleNodeId, reason: StyleReason) {
        if let Some(node) = self.nodes.get_mut(id) {
            node.dirty.merge(reason);
        }
    }

    pub fn is_dirty(&self, id: StyleNodeId) -> bool {
        self.nodes
            .get(id)
            .map(|n| !n.dirty.is_empty())
            .unwrap_or(false)
    }

    // ── Cached-output readers ──────────────────────────────────────────

    /// Final computed style (host-inherited + direct + selectors). Returns
    /// the *last result of `compute_style`* — will be empty until the
    /// cascade has run (Phase 1b).
    pub fn computed_style(&self, id: StyleNodeId) -> Option<&Style> {
        self.nodes.get(id).map(|n| &n.computed_style)
    }

    /// The pre-inheritance merged style (classes + selectors resolved) for
    /// this node. Used for cache keys.
    pub fn combined_style(&self, id: StyleNodeId) -> Option<&Style> {
        self.nodes.get(id).map(|n| &n.combined_style)
    }

    pub fn has_style_selectors(&self, id: StyleNodeId) -> Option<StyleSelectors> {
        self.nodes.get(id).and_then(|n| n.has_style_selectors)
    }

    pub fn style_interaction_cx(&self, id: StyleNodeId) -> Option<InheritedInteractionCx> {
        self.nodes.get(id).map(|n| n.style_interaction_cx)
    }

    /// Inherited-only slice of the computed style. Hosts copy this to
    /// their per-view inherited context so children see it on their
    /// next cascade.
    pub fn inherited_context(&self, id: StyleNodeId) -> Option<&Style> {
        self.nodes.get(id).map(|n| &n.inherited_context)
    }

    /// Class-map-only slice of the direct style, propagated to
    /// descendants for class resolution.
    pub fn class_context(&self, id: StyleNodeId) -> Option<&Style> {
        self.nodes.get(id).map(|n| &n.class_context)
    }

    // ── Cascade ─────────────────────────────────────────────────────────

    /// Run the style cascade over the subtree rooted at `root`.
    ///
    /// For each dirty node in the subtree this resolves classes + selectors
    /// + inherited context into `combined_style` / `computed_style`,
    /// derives the child-facing `inherited_context` and `class_context`,
    /// applies host-owned animations via [`StyleSink::apply_animations`],
    /// and emits per-element side-effects through the sink:
    ///
    /// - `register_fixed_element` / `unregister_fixed_element` when the
    ///   resolved `Position::Fixed` flag flips
    /// - `update_selector_interest` with the node's live selector set
    /// - `mark_style_dirty_with` / `mark_descendants_with_selector_dirty`
    ///   when inherited context, class context, or the cascaded
    ///   `hidden` / `selected` / `disabled` bits flip
    /// - `mark_needs_layout` when a visibility flip needs a relayout
    /// - `inspector_capture_style` so hosts can snapshot computed styles
    /// - `schedule_style` when animation is still in flight
    ///
    /// Clean nodes are traversed (so dirty descendants are visited) but
    /// their own computed state is not recomputed.
    ///
    /// # Integration
    ///
    /// ```ignore
    /// use floem_style::{ElementId, Style, StyleSink, StyleTree};
    /// # struct Host;
    /// # impl StyleSink for Host { /* …required methods… */ }
    ///
    /// let mut tree = StyleTree::new();
    /// let mut host: Host = /* host state implementing StyleSink */ ;
    ///
    /// // Allocate engine nodes and wire edges.
    /// let root = tree.new_node(ElementId::from(0));
    /// let child = tree.new_node(ElementId::from(1));
    /// tree.set_parent(child, Some(root));
    ///
    /// // Push style inputs (direct style, classes, parent interaction).
    /// tree.set_direct_style(root, Style::new());
    /// tree.set_direct_style(child, Style::new());
    ///
    /// // Drive the cascade. Emits side-effects through `host`.
    /// tree.compute_style(root, &mut host);
    ///
    /// // Read outputs.
    /// let _computed = tree.computed_style(child);
    /// let _inherited = tree.inherited_context(child);
    /// ```
    ///
    /// See `tests/mock_sink.rs` and `tests/style_tree_cascade.rs` in this
    /// crate for complete, executable examples with a real [`StyleSink`]
    /// implementation.
    pub fn compute_style(&mut self, root: StyleNodeId, sink: &mut dyn StyleSink) {
        if !self.contains(root) {
            return;
        }
        self.compute_subtree(root, sink);
    }

    fn compute_subtree(&mut self, id: StyleNodeId, sink: &mut dyn StyleSink) {
        if self.is_dirty(id) {
            self.compute_one(id, sink);
        }
        // Always descend — a clean parent may have dirty descendants.
        let children: Vec<StyleNodeId> = self
            .nodes
            .get(id)
            .map(|n| n.children.clone())
            .unwrap_or_default();
        for child in children {
            self.compute_subtree(child, sink);
        }
    }

    fn compute_one(&mut self, id: StyleNodeId, sink: &mut dyn StyleSink) {
        // Gather parent context (or theme defaults if orphan / root).
        let (parent_inherited, parent_class_cx, parent_interaction_cx) = {
            let parent_id = self.nodes[id].parent;
            match parent_id.and_then(|p| self.nodes.get(p)) {
                Some(p) => (
                    p.inherited_context.clone(),
                    p.class_context.clone(),
                    p.style_interaction_cx,
                ),
                None => (
                    sink.default_theme_inherited().clone(),
                    sink.default_theme_classes().clone(),
                    InheritedInteractionCx::default(),
                ),
            }
        };

        // Structural position among siblings (1-based; None if orphan).
        // Honor a host-pushed override if set (floem uses this for list
        // items whose style-cx parent differs from their DOM parent).
        let (child_index, sibling_count) = self
            .nodes
            .get(id)
            .and_then(|n| n.structural_position_override)
            .unwrap_or_else(|| self.structural_position(id));

        // Snapshot node inputs while borrowed immutably.
        let (element_id, direct_style, classes, parent_overrides) = {
            let node = &self.nodes[id];
            (
                node.element_id,
                node.direct_style.clone(),
                node.classes.clone(),
                node.parent_set_style_interaction,
            )
        };

        // Build the interaction state the cascade reads.
        let mut interact_state = InteractionState {
            is_selected: parent_overrides.selected | parent_interaction_cx.selected,
            is_disabled: parent_overrides.disabled | parent_interaction_cx.disabled,
            is_hidden: parent_overrides.hidden | parent_interaction_cx.hidden,
            is_hovered: sink.is_hovered(element_id),
            is_focused: sink.is_focused(element_id),
            is_focus_within: sink.is_focus_within(element_id),
            is_active: sink.is_active(element_id),
            is_dark_mode: sink.is_dark_mode(),
            is_file_hover: sink.is_file_hover(element_id),
            using_keyboard_navigation: sink.keyboard_navigation(),
            child_index,
            sibling_count,
            window_width: sink.root_size_width(),
        };

        // Pull the direct style's own disabled/selected/hidden bits into
        // `interact_state` BEFORE cascading so `:disabled` / `:selected`
        // selectors activate based on the style itself, not just sink
        // state inherited from the parent.
        {
            let builtin = direct_style.builtin();
            interact_state.is_disabled |= builtin.set_disabled();
            interact_state.is_selected |= builtin.set_selected();
            interact_state.is_hidden |= builtin.display() == Display::None;
        }

        // Consult the engine-owned style cache. A hit lets us skip the
        // cascade entirely — it stores `combined_style`,
        // `has_style_selectors`, and the post-cascade interaction flags.
        let cacheable = StyleCache::is_cacheable(&direct_style)
            && !parent_class_cx.has_structural_selectors();
        let cache_key = cacheable.then(|| {
            StyleCacheKey::new_from_hash(
                direct_style.content_hash(),
                &interact_state,
                sink.screen_size_bp(),
                &classes,
                &parent_class_cx,
            )
        });

        let cache_hit = cache_key
            .as_ref()
            .and_then(|k| self.cache.get(k, &parent_inherited));

        let (mut combined_style, selectors, post_interact) = if let Some(hit) = cache_hit {
            let sels = hit.has_style_selectors.unwrap_or_default();
            let post = hit.post_interact;
            // Match the OR'ing a cascade would have done so selectors
            // depending on these bits activate for downstream logic.
            interact_state.is_disabled |= post.disabled;
            interact_state.is_selected |= post.selected;
            interact_state.is_hidden |= post.hidden;
            (hit.combined_style, sels, post)
        } else {
            let (combined_style, selectors) = resolve_nested_maps(
                direct_style,
                &mut interact_state,
                sink.screen_size_bp(),
                &classes,
                &parent_inherited,
                &parent_class_cx,
            );
            // After cascade, the combined style may have set_disabled /
            // set_selected / display:None explicitly (e.g. via a selector
            // branch). OR those into the interaction cx we store & propagate.
            {
                let builtin = combined_style.builtin();
                interact_state.is_disabled |= builtin.set_disabled();
                interact_state.is_selected |= builtin.set_selected();
                interact_state.is_hidden |= builtin.display() == Display::None;
            }
            let post_interact = InheritedInteractionCx {
                hidden: combined_style.builtin().display() == Display::None,
                selected: combined_style.builtin().set_selected(),
                disabled: combined_style.builtin().set_disabled(),
            };
            if let Some(key) = cache_key {
                self.cache.insert(
                    key,
                    &combined_style,
                    Some(selectors),
                    post_interact,
                    &parent_inherited,
                );
            }
            (combined_style, selectors, post_interact)
        };

        // Apply host-owned animations on top of the cached/cascaded
        // combined_style. Animations must run AFTER the cache write so the
        // cache holds pre-animation baselines (per-pass time-varying
        // values can't be cached), and BEFORE inherited context derivation
        // below so animated inherited props propagate to descendants.
        let has_active_animation =
            sink.apply_animations(element_id, &mut combined_style, &mut interact_state);
        if has_active_animation {
            sink.schedule_style(element_id, StyleReason::animation());
        }

        // Merge inherited + combined → computed style for this node.
        let mut computed_style = parent_inherited.clone();
        computed_style.apply_mut(&combined_style);
        let computed_style = computed_style.with_inherited_context(&parent_inherited);

        if computed_style.builtin().is_fixed() {
            sink.register_fixed_element(element_id);
        } else {
            sink.unregister_fixed_element(element_id);
        }

        // Derive the inherited + class contexts children will see.
        let mut new_inherited = parent_inherited.clone();
        Style::apply_only_inherited(&mut new_inherited, &combined_style);
        let mut new_class_cx = parent_class_cx.clone();
        Style::apply_only_class_maps(&mut new_class_cx, &combined_style);

        // Did child-facing context change? If so, dirty the children.
        let old_inherited_id = self.nodes[id].inherited_context.merge_id();
        let inherited_changed = new_inherited.merge_id() != old_inherited_id;
        let changed_classes = self
            .nodes[id]
            .class_context
            .class_maps_eq(&new_class_cx);
        let class_cx_changed = !changed_classes.is_empty();

        // Children see the full post-cascade interaction state — combined
        // style bits OR'd with parent_set_style_interaction and ancestor
        // context — so e.g. descendants of a list-selected item inherit
        // the selected bit. Matches floem's old `style_interaction_cx`
        // write-back. `post_interact` is still stored separately for
        // cache validation.
        let new_interaction_cx = InheritedInteractionCx {
            disabled: interact_state.is_disabled,
            selected: interact_state.is_selected,
            hidden: interact_state.is_hidden,
        };
        let _ = post_interact;

        // Snapshot the previous interaction cx + the dirty reason that
        // drove this compute before we overwrite them below. We use
        // these for descendant-dirtying side-effects so that e.g. a
        // view flipping to hidden re-lays out and its kids restyle.
        let old_interaction_cx = self.nodes[id].style_interaction_cx;
        let old_dirty_selectors = self.nodes[id].dirty.selectors;

        {
            let node = &mut self.nodes[id];
            node.combined_style = combined_style;
            node.computed_style = computed_style;
            node.inherited_context = new_inherited;
            node.class_context = new_class_cx;
            node.style_interaction_cx = new_interaction_cx;
            node.has_style_selectors = Some(selectors);
            node.dirty = StyleReason::empty();
        }

        if inherited_changed || class_cx_changed {
            let children: Vec<StyleNodeId> = self.nodes[id].children.clone();
            let reason = if class_cx_changed {
                StyleReason::class_cx(changed_classes)
            } else {
                StyleReason::inherited()
            };
            for child in children {
                self.mark_dirty(child, reason.clone());
                // Also surface the dirty to the host so floem's own
                // `style_dirty` map includes the child on the next
                // traversal — otherwise downstream per-view work
                // (animations, taffy push) wouldn't run for
                // inherited/class-context-changed descendants.
                if let Some(child_element_id) =
                    self.nodes.get(child).map(|n| n.element_id)
                {
                    sink.mark_style_dirty_with(child_element_id, reason.clone());
                }
            }
        }

        // Propagate hidden/selected/disabled flips to descendants.
        // `mark_descendants_with_selector_dirty` is skipped when the
        // caller's `dirty` already carried the matching selector — the
        // descendant walk has already been scheduled in that case.
        if old_interaction_cx.hidden != new_interaction_cx.hidden {
            let children: Vec<ElementId> = self
                .nodes[id]
                .children
                .iter()
                .filter_map(|c| self.nodes.get(*c).map(|n| n.element_id))
                .collect();
            for child_element_id in children {
                sink.mark_style_dirty_with(child_element_id, StyleReason::visibility());
            }
            sink.mark_needs_layout();
        }
        if old_interaction_cx.selected != new_interaction_cx.selected
            && !old_dirty_selectors.is_some_and(|s| s.has(StyleSelector::Selected))
        {
            let dirtied = self
                .mark_descendants_with_selector_dirty(id, StyleSelector::Selected);
            for (child_element_id, reason) in dirtied {
                sink.mark_style_dirty_with(child_element_id, reason);
            }
        }
        if old_interaction_cx.disabled != new_interaction_cx.disabled
            && !old_dirty_selectors.is_some_and(|s| s.has(StyleSelector::Disabled))
        {
            let dirtied = self
                .mark_descendants_with_selector_dirty(id, StyleSelector::Disabled);
            for (child_element_id, reason) in dirtied {
                sink.mark_style_dirty_with(child_element_id, reason);
            }
        }

        // Update this node's entry in the tree's per-selector interest
        // registries. Subsequent descendant-dirty walks use these to
        // visit only the subset of nodes that could match.
        self.update_selector_interest(id, Some(selectors));

        // Let the host snapshot the computed style (inspector, tests, etc.).
        let computed_ref = self.nodes[id].computed_style.clone();
        sink.inspector_capture_style(element_id, &computed_ref);
    }

    /// Return `(child_index, sibling_count)` for `id` using the tree's own
    /// parent/children edges. `child_index` is 1-based to match the CSS
    /// `:nth-child()` semantics expected by [`crate::cascade`].
    fn structural_position(&self, id: StyleNodeId) -> (Option<usize>, usize) {
        match self.nodes.get(id).and_then(|n| n.parent) {
            Some(parent) => {
                let siblings = self
                    .nodes
                    .get(parent)
                    .map(|p| p.children.as_slice())
                    .unwrap_or(&[]);
                let idx = siblings.iter().position(|c| *c == id).map(|i| i + 1);
                (idx, siblings.len())
            }
            None => (None, 0),
        }
    }
}

fn set_membership(set: &mut FxHashSet<StyleNodeId>, id: StyleNodeId, present: bool) {
    if present {
        set.insert(id);
    } else {
        set.remove(&id);
    }
}

#[cfg(test)]
mod tests {
    //! Phase 1a scaffolding tests: node allocation, parent/child edges,
    //! style setters. Cascade behavior is covered in Phase 1b tests.

    use super::*;
    use crate::builtin_props::Background;
    use understory_box_tree::{LocalNode, Tree};

    fn fresh_element(tree: &mut Tree, owning: u64) -> ElementId {
        let node = tree.push_child(None, LocalNode::default());
        ElementId(node, owning, true)
    }

    #[test]
    fn new_node_allocates_a_fresh_id() {
        let mut tree = Tree::new();
        let mut st = StyleTree::new();
        let a = st.new_node(fresh_element(&mut tree, 1));
        let b = st.new_node(fresh_element(&mut tree, 2));
        assert_ne!(a, b);
        assert_eq!(st.len(), 2);
        assert!(st.contains(a));
        assert!(st.contains(b));
    }

    #[test]
    fn set_parent_updates_both_sides() {
        let mut tree = Tree::new();
        let mut st = StyleTree::new();
        let parent = st.new_node(fresh_element(&mut tree, 1));
        let child = st.new_node(fresh_element(&mut tree, 2));

        st.set_parent(child, Some(parent));
        assert_eq!(st.get(child).unwrap().parent(), Some(parent));
        assert_eq!(st.get(parent).unwrap().children(), &[child]);
    }

    #[test]
    fn set_parent_detaches_from_old_parent() {
        let mut tree = Tree::new();
        let mut st = StyleTree::new();
        let p1 = st.new_node(fresh_element(&mut tree, 1));
        let p2 = st.new_node(fresh_element(&mut tree, 2));
        let child = st.new_node(fresh_element(&mut tree, 3));

        st.set_parent(child, Some(p1));
        st.set_parent(child, Some(p2));
        assert_eq!(st.get(p1).unwrap().children(), &[]);
        assert_eq!(st.get(p2).unwrap().children(), &[child]);
    }

    #[test]
    fn set_children_replaces_and_updates_parents() {
        let mut tree = Tree::new();
        let mut st = StyleTree::new();
        let parent = st.new_node(fresh_element(&mut tree, 1));
        let c1 = st.new_node(fresh_element(&mut tree, 2));
        let c2 = st.new_node(fresh_element(&mut tree, 3));
        let c3 = st.new_node(fresh_element(&mut tree, 4));

        st.set_children(parent, &[c1, c2]);
        assert_eq!(st.get(parent).unwrap().children(), &[c1, c2]);
        assert_eq!(st.get(c1).unwrap().parent(), Some(parent));
        assert_eq!(st.get(c2).unwrap().parent(), Some(parent));

        // Replacing with a different set detaches c2, attaches c3.
        st.set_children(parent, &[c1, c3]);
        assert_eq!(st.get(parent).unwrap().children(), &[c1, c3]);
        assert_eq!(st.get(c2).unwrap().parent(), None);
        assert_eq!(st.get(c3).unwrap().parent(), Some(parent));
    }

    #[test]
    fn remove_node_detaches_from_tree() {
        let mut tree = Tree::new();
        let mut st = StyleTree::new();
        let parent = st.new_node(fresh_element(&mut tree, 1));
        let child = st.new_node(fresh_element(&mut tree, 2));
        let grandchild = st.new_node(fresh_element(&mut tree, 3));
        st.set_parent(child, Some(parent));
        st.set_parent(grandchild, Some(child));

        let removed = st.remove_node(child);
        assert!(removed.is_some());
        assert!(!st.contains(child));
        // Parent's child list no longer references it.
        assert_eq!(st.get(parent).unwrap().children(), &[]);
        // Grandchild is orphaned (not recursively removed).
        assert_eq!(st.get(grandchild).unwrap().parent(), None);
    }

    #[test]
    fn set_direct_style_stores_style_and_marks_dirty() {
        let mut tree = Tree::new();
        let mut st = StyleTree::new();
        let n = st.new_node(fresh_element(&mut tree, 1));

        let style = Style::new().background(peniko::color::palette::css::RED);
        st.set_direct_style(n, style);

        assert_eq!(
            st.get(n).unwrap().direct_style.get(Background),
            Some(peniko::color::palette::css::RED.into())
        );
        assert!(st.is_dirty(n));
    }

    #[test]
    #[should_panic(expected = "would create a cycle")]
    fn set_parent_rejects_cycle() {
        let mut tree = Tree::new();
        let mut st = StyleTree::new();
        let a = st.new_node(fresh_element(&mut tree, 1));
        let b = st.new_node(fresh_element(&mut tree, 2));
        st.set_parent(b, Some(a));
        // This would put `a` under its own descendant.
        st.set_parent(a, Some(b));
    }
}
