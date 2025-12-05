use std::collections::{HashMap, HashSet};

use muda::MenuId;
use peniko::kurbo::{Affine, Point, Rect, Size, Vec2};
use rustc_hash::{FxHashMap, FxHashSet};
use taffy::{AvailableSpace, NodeId};
use understory_event_state::{click::ClickState, focus::FocusState, hover::HoverState};
use winit::{cursor::CursorIcon, window::Theme};

use crate::{
    VisualId,
    action::add_update_message,
    context::{DragState, FrameUpdate, GlobalEventCx, LayoutCx, WidgetLookupExt},
    event::Event,
    id::ViewId,
    inspector::CaptureState,
    responsive::{GridBreakpoints, ScreenSizeBp},
    style::{CursorStyle, StyleSelector},
    view_storage::{MeasureContext, NodeContext, VIEW_STORAGE},
};

/// Encapsulates and owns the global state of the application,
pub struct WindowState {
    /// when a view is active, it gets mouse event even when the mouse is
    /// not on it
    pub(crate) active: Option<understory_box_tree::NodeId>,
    pub(crate) root_view_id: ViewId,
    pub(crate) root: Option<NodeId>,
    pub(crate) root_size: Size,
    /// this is a scale factor that affects the window that is separate from the monitor scale factor
    pub(crate) scale: f64,
    pub(crate) scheduled_updates: Vec<FrameUpdate>,
    pub(crate) style_dirty: FxHashSet<VisualId>,
    pub(crate) view_style_dirty: FxHashSet<ViewId>,
    // dirty tracking for layout is done in the taffy tree
    pub(crate) request_layout: bool,
    // no need for dirty tracking for paints since we need to repaint everything
    pub(crate) request_paint: bool,
    pub(crate) request_box_tree_commit: bool,
    pub(crate) dragging: Option<DragState>,
    pub(crate) drag_start: Option<(ViewId, Point)>,
    pub(crate) dragging_over: HashSet<ViewId>,
    pub(crate) screen_size_bp: ScreenSizeBp,
    pub grid_bps: GridBreakpoints,
    pub(crate) hover_state: HoverState<VisualId>,
    pub(crate) click_state: ClickState<Vec<VisualId>>,
    pub(crate) focus_state: FocusState<VisualId>,
    pub(crate) file_hovered: FxHashSet<VisualId>,
    pub(crate) cursors: FxHashMap<VisualId, CursorStyle>,
    pub(crate) needs_post_layout: FxHashSet<ViewId>,
    // whether the window is in light or dark mode
    pub(crate) light_dark_theme: winit::window::Theme,
    // if `true`, then the window will not follow the os theme changes
    pub(crate) theme_overriden: bool,
    /// This keeps track of all views that have an animation,
    /// regardless of the status of the animation
    pub(crate) cursor: Option<CursorStyle>,
    pub(crate) needs_cursor_resolution: bool,
    pub(crate) last_cursor: CursorIcon,
    pub(crate) last_pointer: Point,
    pub(crate) keyboard_navigation: bool,
    pub(crate) context_menu: HashMap<MenuId, Box<dyn Fn()>>,

    /// This is set if we're currently capturing the window for the inspector.
    pub(crate) capture: Option<CaptureState>,
}

impl WindowState {
    pub fn new(root_view_id: ViewId, os_theme: Option<Theme>) -> Self {
        Self {
            root: None,
            root_view_id,
            active: None,
            scale: 1.0,
            root_size: Size::ZERO,
            screen_size_bp: ScreenSizeBp::Xs,
            scheduled_updates: Vec::new(),
            view_style_dirty: Default::default(),
            style_dirty: Default::default(),
            request_layout: true,
            request_paint: true,
            request_box_tree_commit: true,
            dragging: None,
            drag_start: None,
            dragging_over: HashSet::new(),
            hover_state: HoverState::new(),
            click_state: ClickState::default(),
            focus_state: FocusState::new(),
            file_hovered: Default::default(),
            cursors: Default::default(),
            needs_post_layout: Default::default(),
            theme_overriden: false,
            light_dark_theme: os_theme.unwrap_or(Theme::Light),
            cursor: None,
            needs_cursor_resolution: false,
            last_cursor: CursorIcon::Default,
            last_pointer: Default::default(),
            keyboard_navigation: false,
            grid_bps: GridBreakpoints::default(),
            context_menu: Default::default(),
            capture: None,
        }
    }

    /// This removes a view from the app state.
    pub fn remove_view(&mut self, id: ViewId) {
        let exists = VIEW_STORAGE.with_borrow(|s| s.view_ids.contains_key(id));
        if !exists {
            return;
        }

        let children = id.children();
        for child in children {
            self.remove_view(child);
        }
        let view_state = id.state();

        let cleanup_listeners = view_state.borrow().cleanup_listeners.borrow().clone();
        for action in cleanup_listeners {
            action();
        }

        let node = view_state.borrow().node;
        let taffy = id.taffy();
        let mut taffy = taffy.borrow_mut();

        let children = taffy.children(node);
        if let Ok(children) = children {
            for child in children {
                let _ = taffy.remove(child);
            }
        }
        let _ = taffy.remove(node);

        let box_tree = VIEW_STORAGE.with_borrow(|s| s.box_tree.clone());
        let mut box_tree = box_tree.borrow_mut();

        let node = view_state.borrow().visual_id;
        let children: Vec<_> = box_tree.children_of(node).to_vec();
        for child in children {
            box_tree.remove(child);
        }
        box_tree.remove(node);

        id.remove();
        self.dragging_over.remove(&id);
        self.file_hovered.remove(&id.visual_id());

        if self.active == Some(id.visual_id()) {
            self.active = None;
        }
    }

    /// Builds an explicit traversal order from style_dirty and view_style_dirty sets.
    /// Returns a Vec<VisualId> in parent-to-child order, containing dirty views and any
    /// non-dirty ancestors that need their context cached.
    pub(crate) fn build_style_traversal(&mut self, root: ViewId) -> Vec<VisualId> {
        // Collect all dirty visual ids from both sets
        let mut dirty_visuals = std::mem::take(&mut self.style_dirty);
        // Add visual ids from view_style_dirty
        for view_id in &self.view_style_dirty {
            let visual_id = view_id.state().borrow().visual_id;
            dirty_visuals.insert(visual_id);
        }
        if dirty_visuals.is_empty() {
            return Vec::new();
        }
        // Build traversal in parent-to-child order using recursive DFS
        let mut traversal = Vec::new();
        let mut visited = FxHashSet::default();
        Self::build_traversal_recursive(
            root.visual_id(),
            &dirty_visuals,
            &mut traversal,
            &mut visited,
        );
        traversal
    }

    fn build_traversal_recursive(
        visual_id: VisualId,
        dirty_visuals: &FxHashSet<VisualId>,
        traversal: &mut Vec<VisualId>,
        visited: &mut FxHashSet<VisualId>,
    ) {
        if visited.contains(&visual_id) {
            return;
        }
        visited.insert(visual_id);

        // Check if this node is dirty
        let is_dirty = dirty_visuals.contains(&visual_id);

        let view_id = visual_id.view_of();

        // Check if this node needs its context cached (no cached parent_style_cx)
        let needs_cache = if let Some(view_id) = view_id {
            let state = view_id.state();
            state.borrow().style_cx.is_none()
        } else {
            false
        };

        // Add this node if it's dirty OR if it needs its context cached for dirty descendants
        if is_dirty || needs_cache {
            traversal.push(visual_id);
        }

        let children =
            VIEW_STORAGE.with_borrow(|s| s.box_tree.borrow().children_of(visual_id).to_vec());

        // Recurse into children in order
        for child in children {
            Self::build_traversal_recursive(child, dirty_visuals, traversal, visited);
        }
    }

    pub fn is_hovered(&self, id: impl Into<understory_box_tree::NodeId>) -> bool {
        self.hover_state.current_path().contains(&id.into())
    }

    pub fn is_focused(&self, id: impl Into<understory_box_tree::NodeId>) -> bool {
        self.focus_state
            .current_path()
            .last()
            .map(|f| *f == id.into())
            .unwrap_or(false)
    }

    pub fn is_active(&self, id: impl Into<understory_box_tree::NodeId>) -> bool {
        self.active.map(|a| a == id.into()).unwrap_or(false)
    }

    pub fn is_clicking(&self, id: impl Into<understory_box_tree::NodeId>) -> bool {
        let id = id.into();
        self.click_state.presses().any(|p| p.target.contains(&id))
    }

    pub fn is_dark_mode(&self) -> bool {
        self.light_dark_theme == Theme::Dark
    }

    pub fn is_file_hover(&self, id: impl Into<understory_box_tree::NodeId>) -> bool {
        self.file_hovered.contains(&id.into())
    }

    pub fn is_dragging(&self) -> bool {
        self.dragging
            .as_ref()
            .map(|d| d.released_at.is_none())
            .unwrap_or(false)
    }

    pub fn set_root_size(&mut self, size: Size) {
        self.root_size = size;
        self.compute_layout();
        self.commit_box_tree();
    }

    pub fn compute_layout(&mut self) {
        let mut measure_context = MeasureContext::default();
        if let Some(root) = self.root {
            let _ = self
                .root_view_id
                .taffy()
                .borrow_mut()
                .compute_layout_with_measure(
                    root,
                    taffy::prelude::Size {
                        width: AvailableSpace::Definite((self.root_size.width / self.scale) as f32),
                        height: AvailableSpace::Definite(
                            (self.root_size.height / self.scale) as f32,
                        ),
                    },
                    |known_dimensions, available_space, node_id, node_context, style| {
                        match node_context {
                            Some(NodeContext::Custom {
                                measure,
                                finalize: _,
                            }) => measure(
                                known_dimensions,
                                available_space,
                                node_id,
                                style,
                                &mut measure_context,
                            ),
                            None => taffy::Size::ZERO,
                        }
                    },
                );

            // Finalize nodes that requested it
            let taffy = self.root_view_id.taffy();
            let taffy = taffy.borrow();
            for node_id in measure_context.needs_finalization {
                if let Ok(layout) = taffy.layout(node_id)
                    && let Some(NodeContext::Custom {
                        finalize: Some(f), ..
                    }) = taffy.get_node_context(node_id)
                {
                    f(node_id, layout);
                }
            }
        }
    }

    pub fn commit_box_tree(&mut self) {
        if let Some(root) = self.root {
            compute_absolute_transforms_and_boxes(
                root,
                Affine::IDENTITY,
                ScrollContext::default(),
                None,
            );
            let damage = VIEW_STORAGE.with_borrow(|s| s.box_tree.borrow_mut().commit());
            for id in self.needs_post_layout.iter() {
                let lcx = &mut LayoutCx::new(*id);
                id.view().borrow_mut().post_layout(lcx);
            }
            let cursor = self.last_pointer;
            for damage_rect in &damage.dirty_rects {
                if damage_rect.contains(cursor) {
                    GlobalEventCx::new(self, Event::VisualDamageOverCursor)
                        .update_hover_from_point(cursor);
                }
            }
        }
    }

    /// Requests that the style pass will run for `id` on the next frame, and ensures new frame is
    /// scheduled to happen.
    pub fn schedule_style(&mut self, id: ViewId) {
        self.scheduled_updates.push(FrameUpdate::Style(id));
    }

    /// Requests that the layout pass will run on the next frame, and ensures new frame is
    /// scheduled to happen.
    pub fn schedule_layout(&mut self) {
        add_update_message(crate::update::UpdateMessage::RequestPaint);
        self.scheduled_updates.push(FrameUpdate::Layout);
    }

    /// Requests that the paint pass will run for `id` on the next frame, and ensures new frame is
    /// scheduled to happen.
    pub fn schedule_paint(&mut self, id: ViewId) {
        self.scheduled_updates.push(FrameUpdate::Paint(id));
    }

    // `Id` is unused currently, but could be used to calculate damage regions.
    pub fn request_paint(&mut self, _id: ViewId) {
        self.request_paint = true;
    }

    /// Request that the visual id be styled
    pub fn request_style(&mut self, id: impl Into<VisualId>) {
        self.style_dirty.insert(id.into());
    }

    /// Request that the the `view_style` method from the `View` trait be rerun
    pub fn request_view_style(&mut self, id: ViewId) {
        self.view_style_dirty.insert(id);
    }

    /// Request that layout be recomputed
    pub fn request_layout(&mut self) {
        self.request_layout = true;
    }

    pub fn update_active(&mut self, visual_id: impl Into<VisualId>) {
        if self.active.is_some() {
            // the first update_active wins, so if there's active set,
            // don't do anything.
            return;
        }
        let visual_id = visual_id.into();
        self.active = Some(visual_id);

        // To apply the styles of the Active selector
        if let Some(view_id) = visual_id.view_of()
            && self.has_style_for_sel(view_id, StyleSelector::Clicking)
        {
            self.style_dirty.insert(visual_id);
        }
    }

    pub fn clear_focus(&mut self) {
        self.focus_state.clear();
    }

    pub(crate) fn update_screen_size_bp(&mut self, size: Size) {
        let bp = self.grid_bps.get_width_bp(size.width);
        self.screen_size_bp = bp;
    }

    pub(crate) fn has_style_for_sel(&mut self, id: ViewId, selector_kind: StyleSelector) -> bool {
        let view_state = id.state();
        let view_state = view_state.borrow();

        view_state.has_style_selectors.has(selector_kind)
    }

    pub(crate) fn update_context_menu(
        &mut self,
        actions: HashMap<MenuId, Box<dyn Fn() + 'static>>,
    ) {
        self.context_menu = actions;
    }

    /// returns the previously set cursor if there was one
    pub fn set_cursor(&mut self, visual_id: VisualId, cursor: CursorStyle) -> Option<CursorStyle> {
        self.needs_cursor_resolution = true;
        self.cursors.insert(visual_id, cursor)
    }

    /// returns the previously set cursor if there was one
    pub fn clear_cursor(&mut self, visual_id: VisualId) -> Option<CursorStyle> {
        self.needs_cursor_resolution = true;
        self.cursors.remove(&visual_id)
    }
}

#[derive(Clone, Default)]
pub struct ScrollContext {
    pub offset: Vec2, // total accumulated offset from all scroll ancestors
}

fn compute_absolute_transforms_and_boxes(
    node: NodeId,
    parent_transform_for_children: Affine,
    parent_scroll_context: ScrollContext,
    parent_box_node: Option<understory_box_tree::NodeId>,
) {
    VIEW_STORAGE.with_borrow(|s| {
        let mut scroll_ctx = parent_scroll_context.clone();
        let taffy = s.taffy.borrow();
        let layout = taffy.layout(node).unwrap();

        let local_pos = Point::new(layout.location.x as f64, layout.location.y as f64);
        let size = Size::new(layout.size.width as f64, layout.size.height as f64);

        let (view_id, local_transform, scroll_offset) =
            if let Some(&view_id) = s.node_to_view.get(&node) {
                let state = s.states.get(view_id);
                let style_transform = state
                    .as_ref()
                    .map(|s| s.borrow().view_transform_props.affine(layout))
                    .unwrap_or_default();
                let transform = state
                    .as_ref()
                    .map(|s| s.borrow().transform)
                    .unwrap_or_default();
                let scroll = state
                    .as_ref()
                    .map(|s| s.borrow().scroll_offset)
                    .unwrap_or_default();
                (Some(view_id), style_transform * transform, scroll)
            } else {
                (None, Affine::IDENTITY, Vec2::ZERO)
            };

        let local_transform = local_transform
            * parent_transform_for_children
            * Affine::translate(local_pos.to_vec2());

        // What this views children should use as their parent transform (includes scroll )
        let children_parent_transform = Affine::translate(-scroll_offset);

        let current_box_node = if let Some(view_id) = view_id {
            if scroll_offset != Vec2::ZERO {
                scroll_ctx.offset += scroll_offset;
            }
            if let Some(s) = s.states.get(view_id) {
                s.borrow_mut().scroll_ctx = scroll_ctx.clone();
            }

            let local_rect = Rect::from_origin_size(Point::ZERO, size);
            // Insert or update in box tree
            let box_node_id = s.states.get(view_id).map(|s| s.borrow().visual_id).unwrap();
            s.box_tree
                .borrow_mut()
                .set_local_bounds(box_node_id, local_rect);
            s.box_tree
                .borrow_mut()
                .set_local_transform(box_node_id, local_transform);

            Some(box_node_id)
        } else {
            parent_box_node
        };

        // Traverse children with the current box node as their parent
        if let Ok(children) = taffy.children(node) {
            for &child in &children {
                compute_absolute_transforms_and_boxes(
                    child,
                    children_parent_transform,
                    scroll_ctx.clone(),
                    current_box_node,
                );
            }
        }
    });
}
