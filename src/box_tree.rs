use std::ops::{Deref, DerefMut};

use rustc_hash::FxHashMap;
use understory_box_tree::NodeId;

use crate::ViewId;

/// A visual identifier that represents a rectangle in the box tree.
///
/// # ViewId vs ElementId Relationship
///
/// **ViewId** represents a logical view in the view tree (1:1 with View instances).
/// **ElementId** represents a visual rectangle in the box tree (can be many per View).
///
/// ## Key Relationships:
/// - Each **View** has exactly one primary **ViewId** (1:1)
/// - Each **View** can create multiple **ElementIds** for sub-widget rectangles (1:many)
///   - Example: A scroll view creates VisualIds for content area, vertical scrollbar, horizontal scrollbar
/// - Each **VisualId** maps back to exactly one **ViewId** for event routing (many:1)
///   - Call `element_id.view_id()` to get the owning ViewId
///
/// ## Usage:
/// - **Hit testing** operates on VisualIds (tests against individual rectangles in box tree)
/// - **Event handling** happens on ViewIds (the view receives events with target VisualId)
/// - **Painting** iterates through VisualIds in z-index order from the box tree
/// - **View hierarchy** uses ViewIds for parent/child relationships
///
/// ## Structure:
/// - `.0`: The box tree NodeId (identifies the rectangle in the spatial index)
/// - `.1`: The owning ViewId (identifies which view this rectangle belongs to)
///
/// ## Example:
/// ```ignore
/// // A scroll view might create these VisualIds:
/// let scroll_view_id = ViewId::new();
/// let content_element_id = VisualId(node_id_1, scroll_view_id);     // content area
/// let vscroll_element_id = VisualId(node_id_2, scroll_view_id);     // vertical scrollbar
/// let hscroll_element_id = VisualId(node_id_3, scroll_view_id);     // horizontal scrollbar
///
/// // All three VisualIds route events to the same scroll_view_id:
/// assert_eq!(content_element_id.view_id(), scroll_view_id);
/// assert_eq!(vscroll_element_id.view_id(), scroll_view_id);
/// assert_eq!(hscroll_element_id.view_id(), scroll_view_id);
///
/// // But hit testing can distinguish which specific rectangle was hit
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ElementId(pub NodeId, pub u64, pub bool);

impl ElementId {
    /// Returns `true` when this is the primary element for its owning view.
    #[inline]
    pub const fn is_view(&self) -> bool {
        self.2
    }

    /// The owning view for this element id.
    pub fn owning_id(&self) -> ViewId {
        ViewId::from(slotmap::KeyData::from_ffi(self.1))
    }
}

/// Per-element focus navigation metadata kept in the box tree.
///
/// Focus eligibility lives here rather than on raw box-tree flags so focus
/// policy remains owned by Floem. `keyboard_navigable` always implies
/// `focusable`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FocusNavMeta {
    /// Whether the element can receive focus by any means.
    pub focusable: bool,
    /// Whether the element participates in keyboard traversal.
    ///
    /// This always implies `focusable`.
    pub keyboard_navigable: bool,
    /// Optional explicit linear order key (used for tab traversal).
    pub order: Option<i32>,
    /// Optional logical group for policy-aware navigation.
    pub group: Option<understory_focus::FocusSymbol>,
    /// Optional policy selection hint for host-level policy switching.
    pub policy_hint: Option<understory_focus::FocusSymbol>,
    /// Depth within an app-defined focus scope.
    pub scope_depth: u8,
    /// Preferred initial focus candidate for a scope.
    pub autofocus: bool,
    /// Additional enable/disable gate independent of focusability.
    pub enabled: bool,
}

impl Default for FocusNavMeta {
    fn default() -> Self {
        Self {
            focusable: false,
            keyboard_navigable: false,
            order: None,
            group: None,
            policy_hint: None,
            scope_depth: 0,
            autofocus: false,
            enabled: true,
        }
    }
}

impl FocusNavMeta {
    /// Returns `true` when the element can be focused at all.
    #[inline]
    pub const fn is_focusable(self) -> bool {
        self.focusable || self.keyboard_navigable
    }

    /// Returns `true` when the element participates in keyboard focus traversal.
    #[inline]
    pub const fn is_keyboard_navigable(self) -> bool {
        self.keyboard_navigable
    }

    /// Sets whether the element can receive focus.
    ///
    /// Clearing focusable also clears keyboard navigation because keyboard
    /// navigation is a stricter form of focusability.
    #[inline]
    pub fn with_focusable(mut self, focusable: bool) -> Self {
        self.focusable = focusable;
        if !focusable {
            self.keyboard_navigable = false;
        }
        self
    }

    /// Sets whether the element participates in keyboard traversal.
    ///
    /// Enabling keyboard navigation also enables focusability.
    #[inline]
    pub fn with_keyboard_navigable(mut self, keyboard_navigable: bool) -> Self {
        self.keyboard_navigable = keyboard_navigable;
        if keyboard_navigable {
            self.focusable = true;
        }
        self
    }

    /// Mutates focusability in place.
    #[inline]
    pub fn set_focusable(&mut self, focusable: bool) {
        *self = self.with_focusable(focusable);
    }

    /// Mutates keyboard navigation in place.
    #[inline]
    pub fn set_keyboard_navigable(&mut self, keyboard_navigable: bool) {
        *self = self.with_keyboard_navigable(keyboard_navigable);
    }
}

/// Metadata stored per box tree node.
///
/// Keeps the `ElementId` used by hit-testing/event routing plus optional
/// navigation hints for high-quality keyboard focus behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ElementMeta {
    pub element_id: ElementId,
    pub focus: FocusNavMeta,
}

impl ElementMeta {
    pub const fn new(element_id: ElementId) -> Self {
        Self {
            element_id,
            focus: FocusNavMeta {
                focusable: false,
                keyboard_navigable: false,
                order: None,
                group: None,
                policy_hint: None,
                scope_depth: 0,
                autofocus: false,
                enabled: true,
            },
        }
    }
}

type RawBoxTree = understory_box_tree::Tree<understory_index::backends::GridF64>;

#[derive(Debug)]
pub struct BoxTree {
    tree: RawBoxTree,
    metadata: FxHashMap<understory_box_tree::NodeId, ElementMeta>,
}

impl BoxTree {
    pub fn with_backend(backend: understory_index::backends::GridF64) -> Self {
        Self {
            tree: RawBoxTree::with_backend(backend),
            metadata: FxHashMap::default(),
        }
    }

    pub fn element_meta(&self, id: understory_box_tree::NodeId) -> Option<ElementMeta> {
        self.tree
            .is_alive(id)
            .then(|| self.metadata.get(&id).copied())
            .flatten()
    }

    pub fn set_element_meta(
        &mut self,
        id: understory_box_tree::NodeId,
        meta: Option<ElementMeta>,
    ) -> bool {
        if !self.tree.is_alive(id) {
            return false;
        }
        if let Some(meta) = meta {
            self.metadata.insert(id, meta);
        } else {
            self.metadata.remove(&id);
        }
        true
    }

    pub fn element_id_of(&self, id: understory_box_tree::NodeId) -> Option<ElementId> {
        self.element_meta(id).map(|meta| meta.element_id)
    }

    pub fn focus_nav_meta(&self, id: understory_box_tree::NodeId) -> Option<FocusNavMeta> {
        self.element_meta(id).map(|meta| meta.focus)
    }

    pub fn set_focus_nav_meta(
        &mut self,
        id: understory_box_tree::NodeId,
        focus: FocusNavMeta,
    ) -> bool {
        let Some(mut meta) = self.element_meta(id) else {
            return false;
        };
        meta.focus = focus.with_keyboard_navigable(focus.keyboard_navigable);
        self.metadata.insert(id, meta);
        true
    }

    /// Returns `true` when this node can receive focus by any means.
    #[inline]
    pub fn is_focusable(&self, id: understory_box_tree::NodeId) -> bool {
        self.focus_nav_meta(id)
            .is_some_and(FocusNavMeta::is_focusable)
    }

    /// Returns `true` when this node participates in keyboard traversal.
    #[inline]
    pub fn is_keyboard_navigable(&self, id: understory_box_tree::NodeId) -> bool {
        self.focus_nav_meta(id)
            .is_some_and(FocusNavMeta::is_keyboard_navigable)
    }

    /// Sets whether this node can receive focus at all.
    ///
    /// Clearing focusability also clears keyboard navigation.
    pub fn set_focusable(&mut self, id: understory_box_tree::NodeId, focusable: bool) -> bool {
        let Some(mut meta) = self.element_meta(id) else {
            return false;
        };
        meta.focus.set_focusable(focusable);
        self.metadata.insert(id, meta);
        true
    }

    /// Sets whether this node participates in keyboard traversal.
    ///
    /// Enabling keyboard navigation also enables focusability.
    pub fn set_keyboard_navigable(
        &mut self,
        id: understory_box_tree::NodeId,
        keyboard_navigable: bool,
    ) -> bool {
        let Some(mut meta) = self.element_meta(id) else {
            return false;
        };
        meta.focus.set_keyboard_navigable(keyboard_navigable);
        self.metadata.insert(id, meta);
        true
    }

    pub fn reparent(
        &mut self,
        id: understory_box_tree::NodeId,
        new_parent: Option<understory_box_tree::NodeId>,
    ) {
        self.tree.reparent(id, new_parent);
    }

    pub fn remove(&mut self, id: understory_box_tree::NodeId) {
        if !self.tree.is_alive(id) {
            return;
        }
        let mut pending = vec![id];
        while let Some(node) = pending.pop() {
            pending.extend(self.tree.children_of(node).iter().copied());
            self.metadata.remove(&node);
        }
        self.tree.remove(id);
    }
}

impl Default for BoxTree {
    fn default() -> Self {
        Self::with_backend(understory_index::backends::GridF64::new(100.))
    }
}

impl Deref for BoxTree {
    type Target = RawBoxTree;

    fn deref(&self) -> &Self::Target {
        &self.tree
    }
}

impl DerefMut for BoxTree {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.tree
    }
}

static FOCUS_NAV_META_REVISION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

pub(crate) fn bump_focus_nav_meta_revision() {
    FOCUS_NAV_META_REVISION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

pub(crate) fn focus_nav_meta_revision() -> u64 {
    FOCUS_NAV_META_REVISION.load(std::sync::atomic::Ordering::Relaxed)
}
