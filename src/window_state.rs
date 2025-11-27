use std::collections::{HashMap, HashSet};

use muda::MenuId;
use peniko::kurbo::{Affine, Point, Rect, Size, Vec2};
use taffy::{AvailableSpace, Display, NodeId};
use understory_box_tree::{ClipBehavior, Hit, LocalNode, NodeFlags};
use understory_responder::{
    adapters::box_tree::PathAwareClickState, click::ClickState, focus::FocusState,
    hover::HoverState,
};
use winit::cursor::CursorIcon;
use winit::window::Theme;

use crate::{
    action::add_update_message,
    context::{DragState, FrameUpdate},
    event::{Event, EventListener},
    id::ViewId,
    inspector::CaptureState,
    responsive::{GridBreakpoints, ScreenSizeBp},
    style::{
        CursorStyle, DisplayProp, Focusable, Hidden, PointerEvents, PointerEventsProp,
        StyleSelector, ZIndex,
    },
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
    pub(crate) scale: f64,
    pub(crate) scheduled_updates: Vec<FrameUpdate>,
    pub(crate) request_paint: bool,
    pub(crate) request_layout: bool,
    pub(crate) dragging: Option<DragState>,
    pub(crate) drag_start: Option<(ViewId, Point)>,
    pub(crate) dragging_over: HashSet<ViewId>,
    pub(crate) screen_size_bp: ScreenSizeBp,
    pub(crate) grid_bps: GridBreakpoints,
    pub(crate) hover_state: HoverState<understory_box_tree::NodeId>,
    pub(crate) click_state: PathAwareClickState,
    pub(crate) focus_state: FocusState<understory_box_tree::NodeId>,
    pub(crate) focusable: HashSet<understory_box_tree::NodeId>,
    pub(crate) file_hovered: HashSet<ViewId>,
    // whether the window is in light or dark mode
    pub(crate) light_dark_theme: winit::window::Theme,
    // if `true`, then the window will not follow the os theme changes
    pub(crate) theme_overriden: bool,
    /// This keeps track of all views that have an animation,
    /// regardless of the status of the animation
    pub(crate) cursor: Option<CursorStyle>,
    pub(crate) last_cursor: CursorIcon,
    pub(crate) last_cursor_location: Point,
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
            request_paint: true,
            request_layout: true,
            dragging: None,
            drag_start: None,
            dragging_over: HashSet::new(),
            hover_state: HoverState::new(),
            click_state: PathAwareClickState::default(),
            focus_state: FocusState::new(),
            focusable: HashSet::new(),
            file_hovered: HashSet::new(),
            theme_overriden: false,
            light_dark_theme: os_theme.unwrap_or(Theme::Light),
            cursor: None,
            last_cursor: CursorIcon::Default,
            last_cursor_location: Default::default(),
            keyboard_navigation: false,
            grid_bps: GridBreakpoints::default(),
            context_menu: HashMap::new(),
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
        id.remove();
        self.dragging_over.remove(&id);
        self.file_hovered.remove(&id);
        self.focusable.remove(&id.box_node());

        if self.active == Some(id.box_node()) {
            self.active = None;
        }
    }

    pub fn is_hovered(&self, id: &ViewId) -> bool {
        self.hover_state.current_path().contains(&id.box_node())
    }

    pub fn is_focused(&self, id: &ViewId) -> bool {
        self.focus_state
            .current_path()
            .last()
            .map(|f| *f == id.box_node())
            .unwrap_or(false)
    }

    pub fn is_active(&self, id: &ViewId) -> bool {
        self.active.map(|a| a == id.box_node()).unwrap_or(false)
    }

    pub fn is_clicking(&self, id: &ViewId) -> bool {
        let node = id.box_node();

        self.click_state.is_clicking(&node)
    }

    pub fn is_dark_mode(&self) -> bool {
        self.light_dark_theme == Theme::Dark
    }

    pub fn is_file_hover(&self, id: &ViewId) -> bool {
        self.file_hovered.contains(id)
    }

    pub fn is_dragging(&self) -> bool {
        self.dragging
            .as_ref()
            .map(|d| d.released_at.is_none())
            .unwrap_or(false)
    }

    pub fn set_root_size(&mut self, size: Size) {
        self.root_size = size;
        self.compute_taffy();
    }

    pub fn compute_taffy(&mut self) {
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
                    f(node_id, layout.size);
                }
            }
            drop(taffy);

            compute_absolute_transforms_and_boxes(root, Affine::IDENTITY, None);
            let _damage = VIEW_STORAGE.with_borrow(|s| s.box_tree.borrow_mut().commit());
        }
    }

    /// Requests that the style pass will run for `id` on the next frame, and ensures new frame is
    /// scheduled to happen.
    pub fn schedule_style(&mut self, id: ViewId) {
        self.scheduled_updates.push(FrameUpdate::Style(id));
    }

    /// Requests that the layout pass will run for `id` on the next frame, and ensures new frame is
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

    pub(crate) fn update_active(&mut self, id: ViewId) {
        if self.active.is_some() {
            // the first update_active wins, so if there's active set,
            // don't do anything.
            return;
        }
        self.active = Some(id.box_node());

        // To apply the styles of the Active selector
        if self.has_style_for_sel(id, StyleSelector::Clicking) {
            id.request_style();
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
}

fn compute_absolute_transforms_and_boxes(
    node: NodeId,
    parent_transform_for_children: Affine, // What my parent wants MY children to see
    parent_box_node: Option<understory_box_tree::NodeId>,
) {
    VIEW_STORAGE.with_borrow(|s| {
        let taffy = s.taffy.borrow();
        let layout = taffy.layout(node).unwrap();

        let local_pos = Point::new(layout.location.x as f64, layout.location.y as f64);
        let size = Size::new(layout.size.width as f64, layout.size.height as f64);

        let (view_id, local_transform, scroll_offset) =
            if let Some(&view_id) = s.node_to_view.get(&node) {
                let state = s.states.get(view_id);
                let transform = state
                    .as_ref()
                    .map(|s| s.borrow().view_transform_props.affine(layout))
                    .unwrap_or_default();
                let scroll = state
                    .as_ref()
                    .map(|s| s.borrow().scroll_offset)
                    .unwrap_or_default();
                (Some(view_id), transform, scroll)
            } else {
                (None, Affine::IDENTITY, Vec2::ZERO)
            };

        let local_transform = local_transform
            * parent_transform_for_children
            * Affine::translate(local_pos.to_vec2());

        // What this views children should use as their parent transform (includes scroll )
        let children_parent_transform = Affine::translate(-scroll_offset);

        let current_box_node = if let Some(view_id) = view_id {
            let local_rect = Rect::from_origin_size(Point::ZERO, size);

            let style = s
                .states
                .get(view_id)
                .map(|s| s.borrow().combined_style.clone())
                .unwrap_or_default();
            let z_index = style.get(ZIndex).unwrap_or(0);

            let hidden = style.get(Hidden);
            let display_none = style.get(DisplayProp) == Display::None;

            let flags = if hidden || display_none {
                NodeFlags::empty()
            } else {
                let pickable = s
                    .states
                    .get(view_id)
                    .map(|s| {
                        s.borrow().computed_style.get(PointerEventsProp)
                            != Some(PointerEvents::None)
                    })
                    .unwrap_or(true);
                let focusable = s
                    .states
                    .get(view_id)
                    .map(|s| s.borrow().computed_style.get(Focusable))
                    .unwrap_or(false);

                let mut flags = NodeFlags::VISIBLE;
                if pickable {
                    flags |= NodeFlags::PICKABLE;
                }
                if focusable {
                    flags |= NodeFlags::FOCUSABLE;
                }
                flags
            };

            // Insert or update in box tree
            let box_node_opt = s.states.get(view_id).map(|s| s.borrow().box_node);
            let box_node_id = if let Some(box_node_id) = box_node_opt {
                s.box_tree
                    .borrow_mut()
                    .set_local_bounds(box_node_id, local_rect);
                s.box_tree
                    .borrow_mut()
                    .set_local_transform(box_node_id, local_transform);
                s.box_tree.borrow_mut().set_z_index(box_node_id, z_index); // TODO: do in style pass
                s.box_tree.borrow_mut().set_flags(box_node_id, flags);
                box_node_id
            } else {
                let local_node = LocalNode {
                    local_bounds: local_rect,
                    local_transform,
                    local_clip: None, // TODO: Add clip support if needed
                    clip_behavior: ClipBehavior::default(),
                    z_index,
                    flags,
                };
                let box_node_id = s.box_tree.borrow_mut().insert(parent_box_node, local_node);
                s.box_node_to_view.borrow_mut().insert(box_node_id, view_id);
                box_node_id
            };

            Some(box_node_id)
        } else {
            parent_box_node
        };

        // Traverse children with the current box node as their parent
        if let Ok(children) = taffy.children(node) {
            for &child in &children {
                compute_absolute_transforms_and_boxes(
                    child,
                    children_parent_transform, // They pass this to THEIR children
                    current_box_node,
                );
            }
        }
    });
}
