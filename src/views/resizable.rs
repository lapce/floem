use std::{any::Any, cell::RefCell, rc::Rc};

use crate::{
    ViewId,
    context::{EventCx, LayoutCx, PaintCx, UpdateCx},
    event::{Event, EventPropagation},
    prelude::*,
    prop, prop_extractor,
    style::{CursorStyle, CustomStylable, CustomStyle, FlexDirectionProp, Style, StyleClass},
    style_class,
    unit::{Pct, Px},
    view_storage::UnderTree,
};
use floem_reactive::Effect;
use peniko::{
    Brush,
    color::palette::css,
    kurbo::{Axis, Point, Rect},
};
use rustc_hash::FxHashMap;
use taffy::{FlexDirection, Overflow};
use ui_events::pointer::PointerEvent;
use understory_box_tree::{NodeFlags, NodeId as UnderId};
use understory_responder::types::Phase;

style_class!(
    /// The style class that is applied to all [`ResizableStack`] views.
    pub ResizableClass
);
style_class!(
    /// The style class that is applied to all ResizableHandles.
    pub ResizableHandleClass
);

pub(crate) fn create_resizable(children: Vec<Box<dyn View>>) -> ResizableStack {
    let id = ViewId::new();
    id.needs_post_layout();

    let mut view_children = Vec::new();
    let mut child_ids = Vec::new();

    for c in children.into_iter() {
        let child_id = ViewId::new();
        child_id.add_child(c);
        let resize_child = ResizeChild {
            id: child_id,
            set_basis_percent: None,
        }
        .into_any();
        child_ids.push(child_id);
        view_children.push(resize_child);
    }

    id.set_children(view_children);

    let mut handles = FxHashMap::default();
    for i in 0..child_ids.len() - 1 {
        let child_id = child_ids[i];
        let next_child_id = child_ids[i + 1];
        let handle = Handle::new(id, child_id, next_child_id);
        handles.insert(handle.visual_id, handle);
    }

    ResizableStack {
        id,
        re_style: ReStyle::default(),
        handles,
    }
}

/// Creates a [ResizableStack] from a group of `Views`.
pub fn resizable<VT: ViewTuple + 'static>(children: VT) -> ResizableStack {
    create_resizable(children.into_views())
}

prop!(
    /// The color of the handle
    pub HandleColor: Brush {} = Brush::Solid(css::TRANSPARENT)
);
prop!(
    /// The width of the handle
    pub HandleThickness: Px {} = Px(6.)
);
prop!(
    /// The width of the handle that is used for hit testing.
    pub HandleHitTestThickness: Px {} = Px(10.)
);
prop!(
    /// The cursor style over the handle.
    /// Defaults to automatically handling the style for you.
    pub HandleCursorStyle: Option<CursorStyle> {} = None
);

prop_extractor! {
    ReStyle {
        direction: FlexDirectionProp,
    }
}
prop_extractor! {
    HandleStyle {
        color: HandleColor,
        thickness: HandleThickness,
        hit_test_thickness: HandleHitTestThickness,
        cursor: HandleCursorStyle,
    }
}

pub enum ResizeChildMessage {
    SetBasisPercent(Pct),
    ClearBasis,
}

pub struct ResizeChild {
    id: ViewId,
    set_basis_percent: Option<Pct>,
}
impl View for ResizeChild {
    fn id(&self) -> ViewId {
        self.id
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn view_style(&self) -> Option<Style> {
        Some(
            Style::new()
                .apply_opt(self.set_basis_percent, |s, percent| s.flex_basis(percent))
                .min_size(0., 0.)
                .overflow_x(Overflow::Clip)
                .overflow_y(Overflow::Clip),
        )
    }

    fn update(&mut self, cx: &mut UpdateCx, state: Box<dyn Any>) {
        if let Ok(msg) = state.downcast::<ResizeChildMessage>() {
            cx.window_state.request_view_style(self.id);
            cx.window_state.request_layout();
            match *msg {
                ResizeChildMessage::SetBasisPercent(percent) => {
                    self.set_basis_percent = Some(percent)
                }
                ResizeChildMessage::ClearBasis => self.set_basis_percent = None,
            }
        }
    }
}

#[derive(Debug, Clone)]
struct Handle {
    /// Access to relevent view ids for message passing
    parent_id: ViewId,
    affected_child_id: ViewId,
    next_child_id: ViewId,
    down_point: Option<Point>,
    visual_id: UnderId,
    box_tree: Rc<RefCell<UnderTree>>,
    handle_style: HandleStyle,
}
impl Handle {
    fn new(parent_id: ViewId, affected_child_id: ViewId, next_child_id: ViewId) -> Self {
        let box_tree = parent_id.box_tree();
        let box_node = parent_id.create_child_visual_id();

        Self {
            parent_id,
            affected_child_id,
            next_child_id,
            down_point: None,
            visual_id: box_node,
            box_tree,
            handle_style: Default::default(),
        }
    }

    fn set_position(&mut self, axis: Axis) {
        let mut parent_lcx = LayoutCx::new(self.parent_id);
        let mut affected_lcx = LayoutCx::new(self.affected_child_id);
        let mut next_child_lcx = LayoutCx::new(self.next_child_id);

        let parent_content = parent_lcx.content_rect_local();
        let affected_rect = affected_lcx.layout_rect();
        let next_rect = next_child_lcx.layout_rect();
        let hit_test_thickness = self.handle_style.hit_test_thickness().0;

        let new_rect = match axis {
            Axis::Horizontal => {
                // Center handle in the gap between children
                let center_x = (affected_rect.x1 + next_rect.x0) / 2.0;
                let half_width = hit_test_thickness / 2.0;
                Rect::new(
                    center_x - half_width,
                    parent_content.y0,
                    center_x + half_width,
                    parent_content.y1,
                )
            }
            Axis::Vertical => {
                // Center handle in the gap between children
                let center_y = (affected_rect.y1 + next_rect.y0) / 2.0;
                let half_height = hit_test_thickness / 2.0;
                Rect::new(
                    parent_content.x0,
                    center_y - half_height,
                    parent_content.x1,
                    center_y + half_height,
                )
            }
        };

        self.box_tree
            .borrow_mut()
            .set_local_bounds(self.visual_id, new_rect);
        self.box_tree
            .borrow_mut()
            .set_flags(self.visual_id, NodeFlags::VISIBLE | NodeFlags::PICKABLE);
        self.box_tree.borrow_mut().set_z_index(self.visual_id, 1);
    }

    fn event(&mut self, cx: &mut EventCx, axis: Axis) {
        if cx
            .listeners
            .contains(&crate::event::EventListener::DoubleClick)
        {
            // Reset to equal sizes
            self.affected_child_id
                .update_state(ResizeChildMessage::ClearBasis);
            self.next_child_id
                .update_state(ResizeChildMessage::ClearBasis);
            return;
        }

        if cx
            .listeners
            .contains(&crate::event::EventListener::PointerLeave)
        {
            cx.window_state.clear_cursor(self.visual_id);
        }

        match &cx.event.clone() {
            Event::Pointer(PointerEvent::Down(e)) => {
                cx.window_state.update_active(self.visual_id);
                self.down_point = Some(e.state.logical_point());
            }
            Event::Pointer(PointerEvent::Move(u)) => {
                let cursor = match axis {
                    Axis::Horizontal => CursorStyle::ColResize,
                    Axis::Vertical => CursorStyle::RowResize,
                };
                let cursor = self.handle_style.cursor().unwrap_or(cursor);
                cx.window_state.set_cursor(self.visual_id, cursor);

                if cx.window_state.is_active(self.visual_id)
                    && let Some(start_point) = self.down_point
                    && u.current.logical_point().distance(start_point).abs() > 1.
                {
                    let mut affected_lcx = LayoutCx::new(self.affected_child_id);
                    let mut next_child_lcx = LayoutCx::new(self.next_child_id);

                    let point = u.current.logical_point();
                    let affected_rect = affected_lcx.layout_rect();
                    let next_rect = next_child_lcx.layout_rect();

                    // Calculate the gap between children
                    let (_, affected_x1) = affected_rect.get_coords(axis);
                    let (next_x0, _) = next_rect.get_coords(axis);
                    let gap_size = next_x0 - affected_x1;

                    // Use the CURRENT rendered sizes of just these two children
                    let pair_total =
                        affected_rect.size().get_coord(axis) + next_rect.size().get_coord(axis);

                    if pair_total <= 0.0 {
                        return;
                    }

                    // The mouse position relative to where the affected child starts
                    let mouse_offset =
                        point.get_coord(axis) - affected_rect.origin().get_coord(axis);

                    // Subtract half the gap since the handle is centered in it
                    let affected_size = mouse_offset - (gap_size / 2.0);

                    // What fraction of the pair does the affected child want?
                    let affected_fraction = affected_size / pair_total;

                    // Apply min/max as fractions
                    let min_fraction = 0.1; // 10%
                    let max_fraction = 0.9; // 90%
                    let clamped_fraction = affected_fraction.clamp(min_fraction, max_fraction);

                    // Calculate the new sizes
                    let new_affected_size = clamped_fraction * pair_total;
                    let new_next_size = (1.0 - clamped_fraction) * pair_total;

                    // Convert these pixel sizes to percentages of parent
                    let mut parent_lcx = LayoutCx::new(self.parent_id);
                    let parent_content = parent_lcx.content_rect_local();
                    let parent_size = parent_content.size().get_coord(axis);

                    if parent_size > 0.0 {
                        let affected_percent = (new_affected_size / parent_size) * 100.0;
                        let next_percent = (new_next_size / parent_size) * 100.0;

                        self.affected_child_id
                            .update_state(ResizeChildMessage::SetBasisPercent(Pct(
                                affected_percent,
                            )));
                        self.next_child_id
                            .update_state(ResizeChildMessage::SetBasisPercent(Pct(next_percent)));
                    }
                }
            }

            Event::Pointer(PointerEvent::Up(_e)) => {
                self.down_point = None;
            }
            _ => {}
        }
    }

    fn style(&mut self, cx: &mut crate::context::StyleCx<'_>, axis: Axis) {
        let interact_state = cx.get_interact_state(self.visual_id);
        let context = &mut cx.indirect_style().clone();
        let resolved = crate::style::resolve_nested_maps(
            context.clone(),
            &interact_state,
            cx.window_state.screen_size_bp,
            &[ResizableHandleClass::class_ref()],
            context,
        )
        .0;
        if self.handle_style.read_style(cx, &resolved) {
            let cursor = match axis {
                Axis::Horizontal => CursorStyle::ColResize,
                Axis::Vertical => CursorStyle::RowResize,
            };
            let cursor = self.handle_style.cursor().unwrap_or(cursor);
            cx.window_state.set_cursor(self.visual_id, cursor);
        }
    }

    fn paint(&self, cx: &mut PaintCx<'_>, axis: Axis) {
        let box_tree = self.box_tree.borrow();
        let transform = box_tree.world_transform(self.visual_id).unwrap_or_default();
        let rect = box_tree.world_bounds(self.visual_id).unwrap_or_default();
        let rect = transform.transform_rect_bbox(rect);
        let thickness = self.handle_style.thickness().0;

        // Center the actual thickness within the hit-testable rect
        let paint_rect = match axis {
            Axis::Horizontal => {
                let center_x = (rect.x0 + rect.x1) / 2.0;
                let half_thickness = thickness / 2.0;
                Rect::new(
                    center_x - half_thickness,
                    rect.y0,
                    center_x + half_thickness,
                    rect.y1,
                )
            }
            Axis::Vertical => {
                let center_y = (rect.y0 + rect.y1) / 2.0;
                let half_thickness = thickness / 2.0;
                Rect::new(
                    rect.x0,
                    center_y - half_thickness,
                    rect.x1,
                    center_y + half_thickness,
                )
            }
        };

        cx.fill(&paint_rect, &self.handle_style.color(), 0.);
    }
}

/// A container View around other Views that allows for resizing with a handle.
pub struct ResizableStack {
    id: ViewId,
    re_style: ReStyle,
    handles: FxHashMap<UnderId, Handle>,
}

impl View for ResizableStack {
    fn id(&self) -> ViewId {
        self.id
    }

    fn view_class(&self) -> Option<crate::style::StyleClassRef> {
        Some(ResizableClass::class_ref())
    }

    fn style_pass(&mut self, cx: &mut crate::context::StyleCx<'_>) {
        self.re_style.read(cx);
        for handle in self.handles.values_mut() {
            handle.style(cx, self.re_style.direction().axis());
        }
    }

    fn post_layout(&mut self, _lcx: &mut LayoutCx) {
        for handle in self.handles.values_mut() {
            handle.set_position(self.re_style.direction().axis());
        }
    }

    fn update(&mut self, cx: &mut UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(state) = state.downcast::<ResizableMessage>() {
            match *state {
                ResizableMessage::SetSizesPercent(sizes) => {
                    cx.window_state.request_style(self.id);
                    cx.window_state.request_layout();

                    for (idx, percent) in sizes {
                        let child = self.id.with_children(|children| children.get(idx).copied());
                        if let Some(child) = child {
                            child.update_state(ResizeChildMessage::SetBasisPercent(percent));
                        }
                    }
                }
                ResizableMessage::SetSizesPixels(sizes) => {
                    cx.window_state.request_style(self.id);
                    cx.window_state.request_layout();

                    let axis = self.re_style.direction().axis();

                    for (idx, pixel_size) in sizes {
                        // Convert pixels to percentages for this child and the next
                        let (affected_percent, next_percent) =
                            self.pixels_to_percent_for_pair(idx, pixel_size, axis);

                        let (affected, next) = self.id.with_children(|children| {
                            (children.get(idx).copied(), children.get(idx + 1).copied())
                        });
                        if let Some(child) = affected {
                            child.update_state(ResizeChildMessage::SetBasisPercent(Pct(
                                affected_percent,
                            )));
                        }
                        if let Some(next_child) = next {
                            next_child.update_state(ResizeChildMessage::SetBasisPercent(Pct(
                                next_percent,
                            )));
                        }
                    }
                }
                ResizableMessage::ClearSize(idx) => {
                    let child = self.id.with_children(|c| c.get(idx).copied());
                    if let Some(child) = child {
                        child.update_state(ResizeChildMessage::ClearBasis);
                    }
                }
                ResizableMessage::ClearAll => {
                    for child in self.id.children() {
                        child.update_state(ResizeChildMessage::ClearBasis);
                    }
                }
            }
        }
    }

    fn event(&mut self, cx: &mut EventCx) -> EventPropagation {
        if cx.phase == Phase::Capture {
            return EventPropagation::Continue;
        }

        if let Some(handle) = self.handles.get_mut(&cx.target) {
            handle.event(cx, self.re_style.direction().axis());
            return EventPropagation::Stop;
        }

        EventPropagation::Continue
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        cx.paint_children(self.id());

        for handle in self.handles.values_mut() {
            handle.paint(cx, self.re_style.direction().axis())
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

pub enum ResizableMessage {
    SetSizesPercent(Vec<(usize, Pct)>), // (index, percentage)
    SetSizesPixels(Vec<(usize, f64)>),
    ClearSize(usize),
    ClearAll,
}

impl ResizableStack {
    /// Convert pixel sizes to percentages for adjacent children
    fn pixels_to_percent_for_pair(&self, idx: usize, pixel_size: f64, axis: Axis) -> (f64, f64) {
        // Get parent size instead of pair size
        let mut parent_lcx = LayoutCx::new(self.id);
        let parent_content = parent_lcx.content_rect_local();
        let parent_size = parent_content.size().get_coord(axis);

        if parent_size > 0.0 {
            let affected_percent = (pixel_size / parent_size) * 100.0;
            let next_percent = 100.0 - affected_percent;
            return (affected_percent, next_percent);
        }

        (50.0, 50.0) // Default to equal split
    }

    pub fn custom_sizes(self, sizes: impl Fn() -> Vec<(usize, f64)> + 'static) -> Self {
        let id = self.id;
        Effect::new(move |_| {
            let sizes = sizes();
            id.update_state(ResizableMessage::SetSizesPixels(sizes));
        });
        self
    }

    pub fn custom_sizes_pct(self, sizes: impl Fn() -> Vec<(usize, Pct)> + 'static) -> Self {
        let id = self.id;
        Effect::new(move |_| {
            let sizes = sizes();
            id.update_state(ResizableMessage::SetSizesPercent(sizes));
        });
        self
    }

    /// Sets the custom style properties of the `ResizableStack`.
    pub fn resizable_style(
        self,
        style: impl Fn(ResizableCustomStyle) -> ResizableCustomStyle + 'static,
    ) -> Self {
        self.custom_style(style)
    }
}

#[derive(Debug, Default, Clone)]
pub struct ResizableCustomStyle(Style);
impl From<ResizableCustomStyle> for Style {
    fn from(val: ResizableCustomStyle) -> Self {
        val.0
    }
}
impl From<Style> for ResizableCustomStyle {
    fn from(val: Style) -> Self {
        Self(val)
    }
}
impl CustomStyle for ResizableCustomStyle {
    type StyleClass = ResizableHandleClass;
}

impl CustomStylable<ResizableCustomStyle> for ResizableStack {
    type DV = Self;
}

impl ResizableCustomStyle {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the color of the handle handle.
    ///
    /// # Arguments
    /// * `color` - An optional `Brush` that sets the handle's color. If `None` is provided, the handle color is not set.
    pub fn handle_color(mut self, color: impl Into<Brush>) -> Self {
        let color = color.into();
        self = ResizableCustomStyle(self.0.set(HandleColor, color));
        self
    }

    /// Sets the thickness of the handle.
    ///
    /// # Arguments
    /// * `Thickness` - A `Px` value that sets the handle's thickness.
    pub fn handle_thickness(mut self, width: impl Into<Px>) -> Self {
        self = ResizableCustomStyle(self.0.set(HandleThickness, width));
        self
    }

    /// Sets the cursor style over the handle.
    ///
    /// # Arguments
    /// * `cursor_style` - An optional `CursorStyle` that sets the handle's cursor style.
    ///   If `None` is provided, default automatic cursor style is used.
    pub fn handle_cursor_style(mut self, cursor_style: impl Into<Option<CursorStyle>>) -> Self {
        self = ResizableCustomStyle(self.0.set(HandleCursorStyle, cursor_style));
        self
    }
}

// trait HitExt {
//     fn hit(&self, point: Point, threshhold: f64) -> bool;
// }

// impl<T> HitExt for T
// where
//     T: kurbo::ParamCurveNearest,
// {
//     fn hit(&self, point: Point, threshhold: f64) -> bool {
//         const ACCURACY: f64 = 0.1;
//         let nearest = self.nearest(point, ACCURACY);
//         nearest.distance_sq < threshhold * threshhold
//     }
// }

trait AxisExt {
    fn axis(&self) -> Axis;
}
impl AxisExt for FlexDirection {
    fn axis(&self) -> Axis {
        match self {
            Self::Row | Self::RowReverse => Axis::Horizontal,
            Self::Column | Self::ColumnReverse => Axis::Vertical,
        }
    }
}
