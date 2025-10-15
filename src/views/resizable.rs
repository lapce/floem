use std::collections::HashSet;

use crate::{
    context::{ComputeLayoutCx, EventCx, PaintCx, UpdateCx},
    event::{Event, EventPropagation},
    pointer::{MouseButton, PointerButton, PointerInputEvent, PointerMoveEvent},
    prelude::*,
    prop, prop_extractor,
    style::{
        CursorStyle, CustomStylable, CustomStyle, FlexDirectionProp, Style, StyleClass,
        StyleSelector,
    },
    style_class,
    unit::{Px, PxPct},
    view_state::StackOffset,
    ViewId,
};
use floem_reactive::create_effect;
use peniko::{
    kurbo::{self, Line, Point, Rect, Stroke},
    Brush,
};
use taffy::FlexDirection;

style_class!(
    /// The style class that is applied to all [`ResizableStack`] views.
    pub ResizableClass
);

pub(crate) fn create_resizable(children: Vec<Box<dyn View>>) -> ResizableStack {
    let id = ViewId::new();
    let offsets = children
        .iter()
        .map(|c| {
            let state = c.id().state();
            let offset = state.borrow_mut().style.next_offset();
            state.borrow_mut().style.push(Style::new());
            offset
        })
        .collect();
    id.set_children_vec(children);

    ResizableStack {
        id,
        style: Default::default(),
        re_style: ReStyle::default(),
        handle_style: Default::default(),
        hovered_handle_style: Default::default(),
        cursor_pos: Point::ZERO,
        should_clear_on_up: None,
        layouts: Vec::new(),
        handle_state: HandleState::None,
        style_offsets: offsets,
    }
}
/// Creates a [ResizableStack] from a group of `Views`.
pub fn resizable<VT: ViewTuple + 'static>(children: VT) -> ResizableStack {
    create_resizable(children.into_views())
}

prop!(
    /// The color of the handle
    pub HandleColor: Option<Brush> {} = None
);
prop!(
    /// The width of the handle
    pub HandleThickness: Px {} = Px(10.)
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
        cursor: HandleCursorStyle,
    }
}

enum HandleState {
    None,
    Hovered(usize),
    Active(usize),
}

/// A container View around other Views that allows for resizing with a handle.
pub struct ResizableStack {
    id: ViewId,
    style: Style,
    handle_style: HandleStyle,
    hovered_handle_style: HandleStyle,
    re_style: ReStyle,
    cursor_pos: Point,
    should_clear_on_up: Option<usize>,
    layouts: Vec<Rect>,
    handle_state: HandleState,
    style_offsets: Vec<StackOffset<Style>>,
}

impl View for ResizableStack {
    fn id(&self) -> ViewId {
        self.id
    }

    fn view_class(&self) -> Option<crate::style::StyleClassRef> {
        Some(ResizableClass::class_ref())
    }

    fn view_style(&self) -> Option<Style> {
        Some(self.style.clone())
    }

    fn style_pass(&mut self, cx: &mut crate::context::StyleCx<'_>) {
        self.re_style.read(cx);
        self.handle_style.read(cx);
        let style = cx.style();
        let handle_style = match self.handle_state {
            HandleState::None => Style::new(),
            HandleState::Hovered(_) => style.apply_selectors(&[StyleSelector::Hover]),
            HandleState::Active(_) => style.apply_selectors(&[StyleSelector::Active]),
        };
        self.hovered_handle_style.read_style(cx, &handle_style);
        for child in self.id().children() {
            cx.style_view(child);
        }
    }

    fn update(&mut self, _cx: &mut UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(state) = state.downcast::<Vec<(usize, f64)>>() {
            for (idx, size) in *state {
                let child = self.id.with_children(|c| c[idx]);
                let offset = self.style_offsets[idx];
                match self.re_style.direction() {
                    FlexDirection::Row | FlexDirection::RowReverse => {
                        child.update_style(
                            offset,
                            Style::new().width(size).max_width(size).min_width(20.),
                        );
                    }
                    FlexDirection::Column | FlexDirection::ColumnReverse => {
                        child.update_style(
                            offset,
                            Style::new().height(size).max_height(size).min_height(20.),
                        );
                    }
                }
            }
        }
    }

    fn compute_layout(&mut self, cx: &mut ComputeLayoutCx) -> Option<kurbo::Rect> {
        self.layouts.clear();
        let mut layout_rect: Option<Rect> = None;
        for child in self.id.children().iter() {
            if !child.style_has_hidden() {
                let child_layout = cx.compute_view_layout(*child);
                if let Some(child_layout) = child_layout {
                    let layout = child.get_layout().map(|v| {
                        Rect::from_origin_size(
                            (v.location.x, v.location.y),
                            (v.size.width as f64, v.size.height as f64),
                        )
                    });
                    self.layouts.push(layout.unwrap());
                    if let Some(rect) = layout_rect {
                        layout_rect = Some(rect.union(child_layout));
                    } else {
                        layout_rect = Some(child_layout);
                    }
                }
            }
        }
        layout_rect
    }

    fn event_before_children(&mut self, _cx: &mut EventCx, event: &Event) -> EventPropagation {
        match event {
            Event::PointerDown(PointerInputEvent {
                pos,
                button: PointerButton::Mouse(MouseButton::Primary),
                count,
                ..
            }) => {
                if let Some(handle_idx) = self.find_handle_at_position(*pos) {
                    if *count == 2 {
                        self.should_clear_on_up = Some(handle_idx);
                    }
                    self.id.request_active();
                    self.handle_state = HandleState::Active(handle_idx);
                    self.id.request_all();
                    {
                        let cursor = if let Some(cursor) = self.handle_style.cursor() {
                            cursor
                        } else {
                            match self.re_style.direction() {
                                FlexDirection::Row | FlexDirection::RowReverse => {
                                    CursorStyle::ColResize
                                }
                                FlexDirection::Column | FlexDirection::ColumnReverse => {
                                    CursorStyle::RowResize
                                }
                            }
                        };
                        self.id.request_style();
                        self.style = self.style.clone().cursor(cursor);
                    };

                    return EventPropagation::Stop;
                }
            }
            Event::PointerUp(PointerInputEvent {
                button: PointerButton::Mouse(MouseButton::Primary),
                ..
            }) => {
                if let Some(idx) = self.should_clear_on_up {
                    // Reset the handle positions on double-click
                    self.clear_handle_pos(idx);
                    self.should_clear_on_up = None;
                }
                if let HandleState::Active(_) | HandleState::Hovered(_) = self.handle_state {
                    self.id.clear_active();
                    self.handle_state = HandleState::None;
                    self.id.request_all();
                    let cursor = CursorStyle::Default;
                    self.id.request_style();
                    self.style = self.style.clone().cursor(cursor);
                    return EventPropagation::Stop;
                }
            }
            Event::PointerMove(PointerMoveEvent { pos, .. }) => {
                self.cursor_pos = *pos;
                if let HandleState::Active(handle_idx) = self.handle_state {
                    self.update_handle_position(handle_idx, *pos);
                    let cursor = match self.re_style.direction() {
                        FlexDirection::Row | FlexDirection::RowReverse => CursorStyle::ColResize,
                        FlexDirection::Column | FlexDirection::ColumnReverse => {
                            CursorStyle::RowResize
                        }
                    };
                    self.style = self.style.clone().cursor(cursor);
                    self.id.request_style();
                    self.id.request_layout();
                    return EventPropagation::Stop;
                } else if let Some(handle_idx) = self.find_handle_at_position(*pos) {
                    self.handle_state = HandleState::Hovered(handle_idx);
                    let cursor = match self.re_style.direction() {
                        FlexDirection::Row | FlexDirection::RowReverse => CursorStyle::ColResize,
                        FlexDirection::Column | FlexDirection::ColumnReverse => {
                            CursorStyle::RowResize
                        }
                    };
                    self.style = self.style.clone().cursor(cursor);
                    self.id.request_style();
                    return EventPropagation::Stop;
                } else {
                    self.handle_state = HandleState::None;
                    let cursor = CursorStyle::default();
                    self.style = self.style.clone().cursor(cursor);
                    self.id.request_style();
                }
            }
            _ => {}
        }

        EventPropagation::Continue
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        cx.paint_children(self.id());

        let drawn = if let Some(color) = self.hovered_handle_style.color() {
            match self.handle_state {
                HandleState::Hovered(idx) | HandleState::Active(idx) => {
                    let handle_line = self.get_handle_line(idx);
                    cx.stroke(
                        &handle_line,
                        &color,
                        &Stroke::new(self.hovered_handle_style.thickness().0),
                    );
                    Some(idx)
                }
                _ => None,
            }
        } else {
            None
        };

        let color = self.handle_style.color();
        if let Some(color) = color {
            for (idx, handle_line) in
                (0..(self.layouts.len() - 1)).map(|idx| (idx, self.get_handle_line(idx)))
            {
                if Some(idx) == drawn {
                    continue;
                }
                cx.stroke(
                    &handle_line,
                    &color,
                    &Stroke::new(self.handle_style.thickness().0),
                );
            }
        }
    }
}

impl ResizableStack {
    pub fn custom_sizes(self, sizes: impl Fn() -> Vec<(usize, f64)> + 'static) -> Self {
        let id = self.id;
        create_effect(move |_| {
            let sizes = sizes();
            id.update_state(sizes);
        });
        self
    }

    fn get_handle_line(&self, handle_idx: usize) -> Line {
        assert!(
            handle_idx < self.layouts.len() - 1,
            "handle_idx must be less than layouts.len() - 1"
        );

        let current_layout = self.layouts[handle_idx];
        let next_layout = self.layouts[handle_idx + 1];

        match self.re_style.direction() {
            FlexDirection::Row | FlexDirection::RowReverse => {
                let x = current_layout.x1;
                let min_y = current_layout.y0.min(next_layout.y0);
                let max_y = current_layout.y1.max(next_layout.y1);
                Line::new(Point::new(x, min_y), Point::new(x, max_y))
            }
            FlexDirection::Column | FlexDirection::ColumnReverse => {
                let y = current_layout.y1;
                let min_x = current_layout.x0.min(next_layout.x0);
                let max_x = current_layout.x1.max(next_layout.x1);
                Line::new(Point::new(min_x, y), Point::new(max_x, y))
            }
        }
    }

    fn find_handle_at_position(&self, pos: Point) -> Option<usize> {
        for i in 0..self.layouts.len() - 1 {
            let handle_rect = self.get_handle_line(i);
            // TODO make hit target configurable
            if handle_rect.hit(pos, 5.) {
                return Some(i);
            }
        }
        None
    }

    fn update_handle_position(&mut self, handle_idx: usize, pos: Point) {
        if handle_idx >= self.layouts.len() - 1 {
            return;
        }
        let current_layout = self.layouts[handle_idx];
        let next_layout = self.layouts[handle_idx + 1];

        match self.re_style.direction() {
            FlexDirection::Row | FlexDirection::RowReverse => {
                // Calculate potential new width
                let mut new_width = pos.x - current_layout.x0;

                // Apply minimum constraint to current element
                new_width = new_width.max(20.);

                // Calculate available space in next element
                let available_space = next_layout.width() - 20.0; // Reserve minimum 20px
                let diff = new_width - current_layout.width();

                // If requested change exceeds available space, limit the change
                if diff > available_space {
                    new_width = current_layout.width() + available_space;
                }

                // Only proceed if there's an actual change
                if (new_width - current_layout.width()).abs() < 0.1 {
                    return;
                }

                let child = self.id.with_children(|c| c[handle_idx]);
                let offset = self.style_offsets[handle_idx];
                let is_last = handle_idx == self.style_offsets.len() - 1;
                child.update_style(
                    offset,
                    Style::new()
                        .width(new_width)
                        .min_width(20.)
                        .apply_if(!is_last, |s| s.max_width(new_width))
                        .apply_if(is_last, |s| s.flex_grow(1.)),
                );

                // Calculate next width based on actual applied change
                let actual_diff = new_width - current_layout.width();
                let next_width = next_layout.width() - actual_diff;

                let child = self.id.with_children(|c| c[handle_idx + 1]);
                let offset = self.style_offsets[handle_idx + 1];
                let is_last = handle_idx + 1 == self.style_offsets.len() - 1;
                child.update_style(
                    offset,
                    Style::new()
                        .width(next_width)
                        .min_width(20.)
                        .apply_if(!is_last, |s| s.max_width(next_width))
                        .apply_if(is_last, |s| s.flex_grow(1.)),
                );
            }
            FlexDirection::Column | FlexDirection::ColumnReverse => {
                // Calculate potential new height
                let mut new_height = pos.y - current_layout.y0;

                // Apply minimum constraint to current element
                new_height = new_height.max(20.);

                // Calculate available space in next element
                let available_space = next_layout.height() - 20.0; // Reserve minimum 20px
                let diff = new_height - current_layout.height();

                // If requested change exceeds available space, limit the change
                if diff > available_space {
                    new_height = current_layout.height() + available_space;
                }

                // Only proceed if there's an actual change
                if (new_height - current_layout.height()).abs() < 0.1 {
                    return;
                }

                let child = self.id.with_children(|c| c[handle_idx]);
                let offset = self.style_offsets[handle_idx];
                let is_last = handle_idx == self.style_offsets.len() - 1;
                child.update_style(
                    offset,
                    Style::new()
                        .height(new_height)
                        .min_height(20.)
                        .apply_if(!is_last, |s| s.max_height(new_height))
                        .apply_if(is_last, |s| s.flex_grow(1.)),
                );

                // Calculate next height based on actual applied change
                let actual_diff = new_height - current_layout.height();
                let next_height = next_layout.height() - actual_diff;

                let child = self.id.with_children(|c| c[handle_idx + 1]);
                let offset = self.style_offsets[handle_idx + 1];
                let is_last = handle_idx + 1 == self.style_offsets.len() - 1;
                child.update_style(
                    offset,
                    Style::new()
                        .height(next_height)
                        .min_height(20.)
                        .apply_if(!is_last, |s| s.max_height(next_height))
                        .apply_if(is_last, |s| s.flex_grow(1.)),
                );
            }
        }
    }

    fn clear_handle_pos(&mut self, handle_idx: usize) {
        if handle_idx >= self.layouts.len() - 1 {
            return;
        }

        let child = self.id.with_children(|c| c[handle_idx]);
        let offset = self.style_offsets[handle_idx];
        child.update_style(offset, Style::new());

        let child = self.id.with_children(|c| c[handle_idx + 1]);
        let offset = self.style_offsets[handle_idx + 1];
        child.update_style(offset, Style::new());

        self.id.request_layout();
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
    type StyleClass = ResizableClass;
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
        self = ResizableCustomStyle(self.0.set(HandleColor, Some(color)));
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

pub trait HitExt {
    fn hit(&self, point: Point, threshhold: f64) -> bool;
}

impl<T> HitExt for T
where
    T: kurbo::ParamCurveNearest,
{
    fn hit(&self, point: Point, threshhold: f64) -> bool {
        const ACCURACY: f64 = 0.1;
        let nearest = self.nearest(point, ACCURACY);
        nearest.distance_sq < threshhold * threshhold
    }
}
