#![deny(missing_docs)]
//! Scroll View

use std::any::Any;

use floem_reactive::Effect;
use peniko::color::palette::css;
use peniko::kurbo::{Axis, Point, Rect, RoundedRect, RoundedRectRadii, Stroke, Vec2};
use peniko::{Brush, Color};
use ui_events::pointer::{PointerButton, PointerButtonEvent, PointerEvent, PointerScrollEvent};
use understory_responder::types::Phase;

use crate::context::LayoutCx;
use crate::event::EventPropagation;
use crate::style::{
    BorderColorProp, BorderRadiusProp, CustomStylable, CustomStyle, OverflowX, OverflowY,
    ScrollbarWidth,
};
use crate::unit::PxPct;
use crate::{
    Renderer,
    context::{EventCx, PaintCx},
    event::Event,
    id::ViewId,
    prop, prop_extractor,
    style::{Background, Style, StyleSelector},
    style_class,
    unit::Px,
    view::{IntoView, View},
    window_state::WindowState,
};

use super::Decorators;

enum ScrollState {
    EnsureVisible(Rect),
    ScrollDelta(Vec2),
    ScrollTo(Point),
    ScrollToPercent(f32),
    ScrollToView(ViewId),
}

trait Vec2Ext {
    /// Returns a new Vec2 with the maximum x and y components from self and other
    fn max_by_component(self, other: Self) -> Self;

    /// Returns a new Vec2 with the minimum x and y components from self and other
    fn min_by_component(self, other: Self) -> Self;
}

impl Vec2Ext for Vec2 {
    fn max_by_component(self, other: Self) -> Self {
        Vec2::new(self.x.max(other.x), self.y.max(other.y))
    }

    fn min_by_component(self, other: Self) -> Self {
        Vec2::new(self.x.min(other.x), self.y.min(other.y))
    }
}

/// Denotes which scrollbar, if any, is currently being dragged.
#[derive(Debug, Copy, Clone)]
enum BarHeldState {
    /// Neither scrollbar is being dragged.
    None,
    /// Vertical scrollbar is being dragged. Contains an `f64` with
    /// the initial y-offset of the dragging input.
    Vertical(f64, Vec2),
    /// Horizontal scrollbar is being dragged. Contains an `f64` with
    /// the initial x-offset of the dragging input.
    Horizontal(f64, Vec2),
}

style_class!(
    /// Style class that will be applied to the handles of the scroll view
    pub Handle
);
style_class!(
    /// Style class that will be applied to the scroll tracks of the scroll view
    pub Track
);

prop!(
    /// Determines if scroll handles should be rounded (defaults to true on macOS).
    pub Rounded: bool {} = cfg!(target_os = "macos")
);
prop!(
    /// Defines the border width of a scroll track in pixels.
    pub Border: Px {} = Px(0.0)
);

prop_extractor! {
    ScrollTrackStyle {
        color: Background,
        border_radius: BorderRadiusProp,
        border_color: BorderColorProp,
        border: Border,
        rounded: Rounded,
    }
}

prop!(
    /// Specifies the vertical inset of the scrollable area in pixels.
    pub VerticalInset: Px {} = Px(0.0)
);

prop!(
    /// Defines the horizontal inset of the scrollable area in pixels.
    pub HorizontalInset: Px {} = Px(0.0)
);

prop!(
    /// Controls the visibility of scroll bars. When true, bars are hidden.
    pub HideBars: bool {} = false
);

prop!(
    /// Controls whether scroll bars are shown when not scrolling. When false, bars are only shown during scroll interactions.
    pub ShowBarsWhenIdle: bool {} = true
);

prop!(
    /// Determines if pointer wheel events should propagate to parent elements.
    pub PropagatePointerWheel: bool {} = true
);

prop!(
    /// When true, vertical scroll input is interpreted as horizontal scrolling.
    pub VerticalScrollAsHorizontal: bool {} = false
);

prop_extractor!(ScrollStyle {
    vertical_bar_inset: VerticalInset,
    horizontal_bar_inset: HorizontalInset,
    hide_bar: HideBars,
    show_bars_when_idle: ShowBarsWhenIdle,
    propagate_pointer_wheel: PropagatePointerWheel,
    vertical_scroll_as_horizontal: VerticalScrollAsHorizontal,
    overflow_x: OverflowX,
    overflow_y: OverflowY,
    scrollbar_width: ScrollbarWidth,
});

const HANDLE_COLOR: Brush = Brush::Solid(Color::from_rgba8(0, 0, 0, 120));

style_class!(
    /// Style class that is applied to every scroll view
    pub ScrollClass
);

/// A scroll view
pub struct Scroll {
    id: ViewId,
    child: ViewId,
    // any time this changes, we must update the scroll_offset in the ViewState.
    scroll_offset: Vec2,
    onscroll: Option<Box<dyn Fn(Rect)>>,
    held: BarHeldState,
    v_handle_hover: bool,
    h_handle_hover: bool,
    v_track_hover: bool,
    h_track_hover: bool,
    /// Tracks whether user is currently interacting with scrollbars or recently scrolled
    is_scrolling_or_interacting: bool,
    handle_style: ScrollTrackStyle,
    handle_active_style: ScrollTrackStyle,
    handle_hover_style: ScrollTrackStyle,
    track_style: ScrollTrackStyle,
    track_hover_style: ScrollTrackStyle,
    scroll_style: ScrollStyle,
}

/// Create a new scroll view
pub fn scroll<V: IntoView + 'static>(child: V) -> Scroll {
    let id = ViewId::new();
    let child = child.into_any();
    let child_id = child.id();
    id.add_child(child);

    Scroll {
        id,
        child: child_id,
        onscroll: None,
        scroll_offset: Vec2::ZERO,
        held: BarHeldState::None,
        v_handle_hover: false,
        h_handle_hover: false,
        v_track_hover: false,
        h_track_hover: false,
        is_scrolling_or_interacting: false,
        handle_style: Default::default(),
        handle_active_style: Default::default(),
        handle_hover_style: Default::default(),
        track_style: Default::default(),
        track_hover_style: Default::default(),
        scroll_style: Default::default(),
    }
    .class(ScrollClass)
}

impl Scroll {
    /// Sets a callback that will be triggered whenever the scroll position changes.
    ///
    /// This callback receives the viewport rectangle that represents the currently
    /// visible portion of the scrollable content.
    pub fn on_scroll(mut self, onscroll: impl Fn(Rect) + 'static) -> Self {
        self.onscroll = Some(Box::new(onscroll));
        self
    }

    /// Ensures that a specific rectangular area is visible within the scroll view by automatically
    /// scrolling to it if necessary.
    ///
    /// # Reactivity
    /// The viewport will automatically update to include the target rectangle whenever the rectangle's
    /// position or size changes, as determined by the `to` function which will update any time there are
    /// changes in the signals that it depends on.
    pub fn ensure_visible(self, to: impl Fn() -> Rect + 'static) -> Self {
        let id = self.id();
        Effect::new(move |_| {
            let rect = to();
            id.update_state_deferred(ScrollState::EnsureVisible(rect));
        });

        self
    }

    /// Scrolls the view by the specified delta vector.
    ///
    /// # Reactivity
    /// The scroll position will automatically update whenever the delta vector changes,
    /// as determined by the `delta` function which will update any time there are changes in the signals that it depends on.
    pub fn scroll_delta(self, delta: impl Fn() -> Vec2 + 'static) -> Self {
        let id = self.id();
        Effect::new(move |_| {
            let delta = delta();
            id.update_state(ScrollState::ScrollDelta(delta));
        });

        self
    }

    /// Scrolls the view to the specified target point.
    ///
    /// # Reactivity
    /// The scroll position will automatically update whenever the target point changes,
    /// as determined by the `origin` function which will update any time there are changes in the signals that it depends on.
    pub fn scroll_to(self, origin: impl Fn() -> Option<Point> + 'static) -> Self {
        let id = self.id();
        Effect::new(move |_| {
            if let Some(origin) = origin() {
                id.update_state_deferred(ScrollState::ScrollTo(origin));
            }
        });

        self
    }

    /// Scrolls the view to the specified percentage (0-100) of its scrollable content.
    ///
    /// # Reactivity
    /// The scroll position will automatically update whenever the target percentage changes,
    /// as determined by the `percent` function which will update any time there are changes in the signals that it depends on.
    pub fn scroll_to_percent(self, percent: impl Fn() -> f32 + 'static) -> Self {
        let id = self.id();
        Effect::new(move |_| {
            let percent = percent() / 100.;
            id.update_state_deferred(ScrollState::ScrollToPercent(percent));
        });
        self
    }

    /// Scrolls the view to make a specific view visible.
    ///
    /// # Reactivity
    /// The scroll position will automatically update whenever the target view changes,
    /// as determined by the `view` function which will update any time there are changes in the signals that it depends on.
    pub fn scroll_to_view(self, view: impl Fn() -> Option<ViewId> + 'static) -> Self {
        let id = self.id();
        Effect::new(move |_| {
            if let Some(view) = view() {
                id.update_state_deferred(ScrollState::ScrollToView(view));
            }
        });

        self
    }
}

/// internal methods
impl Scroll {
    /// this applies a delta, set the viewport in the window state and returns the delta that was actually applied
    ///
    /// If the delta is positive, the view will scroll down, negative will scroll up.
    fn apply_scroll_delta(&mut self, delta: Vec2, lcx: &mut LayoutCx) -> Option<Vec2> {
        let viewport_size = lcx.content_rect_local().size();
        let content_size = self.child.layout_rect_local().size();

        // Calculate max scroll based on overflow settings
        let mut max_scroll =
            (content_size.to_vec2() - viewport_size.to_vec2()).max_by_component(Vec2::ZERO);

        // Zero out scroll in axes that aren't scrollable
        let can_scroll_x = matches!(self.scroll_style.overflow_x(), taffy::Overflow::Scroll);
        let can_scroll_y = matches!(self.scroll_style.overflow_y(), taffy::Overflow::Scroll);

        let mut new_scroll_offset = self.scroll_offset + delta;
        if !can_scroll_x {
            new_scroll_offset.x = 0.0;
            max_scroll.x = 0.0;
        }
        if !can_scroll_y {
            new_scroll_offset.y = 0.0;
            max_scroll.y = 0.0;
        }

        let old_scroll_offset = self.scroll_offset;
        self.scroll_offset = new_scroll_offset
            .max_by_component(Vec2::ZERO)
            .min_by_component(max_scroll);
        self.id.set_scroll_offset(self.scroll_offset);

        if self.scroll_offset != old_scroll_offset {
            Some(self.scroll_offset - old_scroll_offset)
        } else {
            None
        }
    }

    /// Scroll to a specific offset position.
    ///
    /// Sets the scroll offset to the given point, clamping to valid scroll bounds.
    /// The offset represents how much content has scrolled out of view at the top-left.
    ///
    /// # Arguments
    /// * `offset` - The desired scroll offset. Will be clamped to valid range [0, max_scroll]
    fn do_scroll_to(&mut self, offset: Point, lcx: &mut LayoutCx) {
        self.apply_scroll_delta(offset.to_vec2() - self.scroll_offset, lcx);
    }

    /// Ensure that an entire area is visible in the scroll view.
    ///
    /// Scrolls the minimum distance necessary to make the entire rect visible.
    /// If the rect is larger than the viewport, prioritizes showing the top-left.
    ///
    /// # Arguments
    /// * `rect` - The rectangle in content coordinates (relative to the child's layout)
    pub fn do_ensure_visible(&mut self, rect: Rect, lcx: &mut LayoutCx) {
        let viewport = lcx.content_rect_local();
        let viewport_size = viewport.size();

        // Calculate the rect's position relative to current scroll position
        let visible_rect = Rect::from_origin_size(self.scroll_offset.to_point(), viewport_size);

        // If rect is already fully visible, no need to scroll
        if visible_rect.contains_rect(rect) {
            return;
        }

        let mut new_offset = self.scroll_offset;

        // Scroll horizontally if needed
        if rect.width() > viewport_size.width {
            // Rect is wider than viewport - show left edge
            new_offset.x = rect.x0;
        } else if rect.x0 < visible_rect.x0 {
            // Rect is cut off on left - scroll left
            new_offset.x = rect.x0;
        } else if rect.x1 > visible_rect.x1 {
            // Rect is cut off on right - scroll right
            new_offset.x = rect.x1 - viewport_size.width;
        }

        // Scroll vertically if needed
        if rect.height() > viewport_size.height {
            // Rect is taller than viewport - show top edge
            new_offset.y = rect.y0;
        } else if rect.y0 < visible_rect.y0 {
            // Rect is cut off on top - scroll up
            new_offset.y = rect.y0;
        } else if rect.y1 > visible_rect.y1 {
            // Rect is cut off on bottom - scroll down
            new_offset.y = rect.y1 - viewport_size.height;
        }

        self.do_scroll_to(new_offset.to_point(), lcx);
    }

    fn click_bar_area(&mut self, pos: Point, axis: Axis, lcx: &mut LayoutCx) {
        let viewport = lcx.content_rect_local();
        let full_rect = lcx.layout_rect_local();
        let content_size = self.child.layout_rect_local().size();

        let pos_val = pos.get_coord(axis);
        let viewport_size = viewport.size().get_coord(axis);
        let content_size_val = content_size.get_coord(axis);
        let full_rect_size = full_rect.size().get_coord(axis);

        // Calculate handle properties
        let percent_visible = viewport_size / content_size_val;
        let handle_length = (percent_visible * full_rect_size).ceil().max(15.);
        let max_scroll = content_size_val - viewport_size;

        // Convert click position to percentage along the track
        let track_length = full_rect_size;
        let available_travel = track_length - handle_length;

        // Center the handle at the click position
        let target_handle_offset = (pos_val - handle_length / 2.0)
            .max(0.0)
            .min(available_travel);
        let target_percent = if available_travel > 0.0 {
            target_handle_offset / available_travel
        } else {
            0.0
        };

        // Map percentage to scroll offset
        let new_offset = (target_percent * max_scroll).clamp(0.0, max_scroll);

        self.scroll_offset.set_coord(axis, new_offset);
        self.id.set_scroll_offset(self.scroll_offset);
    }

    fn do_scroll_to_view(&mut self, target: ViewId, target_rect: Option<Rect>, lcx: &mut LayoutCx) {
        if target.layout().is_none() || target.is_hidden() {
            return;
        }

        // Get target's rect relative to its parent
        let mut rect = target.view_rect();

        // If a specific sub-rect within the target is specified, adjust
        if let Some(target_rect) = target_rect {
            rect = rect.with_origin(rect.origin() + target_rect.origin().to_vec2());
            let new_size = target_rect
                .size()
                .to_rect()
                .intersect(rect.size().to_rect())
                .size();
            rect = rect.with_size(new_size);
        }

        // Convert from window coordinates to child content coordinates
        // We need to find the position relative to the scrollable child
        let target_window_rect = rect;
        let child_window_origin = self.child.view_rect().origin();

        // Convert to child-relative coordinates
        let rect_in_child = target_window_rect
            .with_origin(target_window_rect.origin() - child_window_origin.to_vec2());

        self.do_ensure_visible(rect_in_child, lcx);
    }

    fn v_handle_style(&self) -> &ScrollTrackStyle {
        if let BarHeldState::Vertical(..) = self.held {
            &self.handle_active_style
        } else if self.v_handle_hover {
            &self.handle_hover_style
        } else {
            &self.handle_style
        }
    }

    fn h_handle_style(&self) -> &ScrollTrackStyle {
        if let BarHeldState::Horizontal(..) = self.held {
            &self.handle_active_style
        } else if self.h_handle_hover {
            &self.handle_hover_style
        } else {
            &self.handle_style
        }
    }

    fn scroll_scale(&self, axis: Axis, lcx: &mut LayoutCx) -> f64 {
        let viewport_size = lcx.content_rect_local().size().get_coord(axis);
        let content_size = self.child.layout_rect_local().size().get_coord(axis);

        content_size / viewport_size
    }

    fn calc_handle_bounds(&self, axis: Axis, lcx: &mut LayoutCx) -> Option<Rect> {
        let viewport = lcx.content_rect_local();
        let full_rect = lcx.layout_rect_local();
        let content_size = self.child.layout_rect_local().size();

        let viewport_size = viewport.size().get_coord(axis);
        let content_size_val = content_size.get_coord(axis);
        let full_rect_size = full_rect.size().get_coord(axis);

        // No scrollbar if content fits in viewport
        if viewport_size >= (content_size_val - f64::EPSILON) {
            return None;
        }

        let bar_width = self.scroll_style.scrollbar_width().0;
        let bar_inset = match axis {
            Axis::Vertical => self.scroll_style.vertical_bar_inset().0,
            Axis::Horizontal => self.scroll_style.horizontal_bar_inset().0,
        };

        // Calculate scrollbar handle size and position
        let percent_visible = viewport_size / content_size_val;
        let max_scroll = content_size_val - viewport_size;
        let scroll_offset = self.scroll_offset.get_coord(axis);

        let percent_scrolled = if max_scroll > 0.0 {
            scroll_offset / max_scroll
        } else {
            0.0
        };

        // Handle length proportional to visible content, with minimum size
        // TODO: make the minimum size configurable
        let handle_length = (percent_visible * full_rect_size).ceil().max(15.);

        // Position handle within the available track space
        let track_length = full_rect_size;
        let available_travel = track_length - handle_length;
        let handle_offset = (available_travel * percent_scrolled).ceil();

        // Position in viewport's local coordinates
        let rect = match axis {
            Axis::Vertical => {
                let x0 = full_rect.width() - bar_width - bar_inset;
                let y0 = handle_offset;
                let x1 = full_rect.width() - bar_inset;
                let y1 = handle_offset + handle_length;
                Rect::new(x0, y0, x1, y1)
            }
            Axis::Horizontal => {
                let x0 = handle_offset;
                let y0 = full_rect.height() - bar_width - bar_inset;
                let x1 = handle_offset + handle_length;
                let y1 = full_rect.height() - bar_inset;
                Rect::new(x0, y0, x1, y1)
            }
        };

        Some(rect)
    }

    fn calc_bar_bounds(&self, axis: Axis, lcx: &mut LayoutCx) -> Option<Rect> {
        let viewport = lcx.content_rect_local();
        let full_rect = lcx.layout_rect_local();
        let content_size = self.child.layout_rect_local().size();
        let viewport_size = viewport.size().get_coord(axis);
        let content_size_val = content_size.get_coord(axis);

        // No scrollbar if content fits in viewport
        if viewport_size >= (content_size_val - f64::EPSILON) {
            return None;
        }

        let bar_width = self.scroll_style.scrollbar_width().0;
        let bar_inset = match axis {
            Axis::Vertical => self.scroll_style.vertical_bar_inset().0,
            Axis::Horizontal => self.scroll_style.horizontal_bar_inset().0,
        };

        let rect = match axis {
            Axis::Vertical => {
                let x0 = full_rect.width() - bar_width - bar_inset;
                let y0 = 0.0;
                let x1 = full_rect.width() - bar_inset;
                let y1 = full_rect.height();
                Rect::new(x0, y0, x1, y1)
            }
            Axis::Horizontal => {
                let x0 = 0.0;
                let y0 = full_rect.height() - bar_width - bar_inset;
                let x1 = full_rect.width();
                let y1 = full_rect.height() - bar_inset;
                Rect::new(x0, y0, x1, y1)
            }
        };

        Some(rect)
    }

    fn point_hits_bar(&self, pos: Point, axis: Axis, lcx: &mut LayoutCx) -> bool {
        let bounds = self.calc_bar_bounds(axis, lcx);

        bounds
            .map(|mut bounds| {
                let viewport = lcx.layout_rect_local();
                match axis {
                    // stretch out the hit area to be to the edge of the view
                    Axis::Horizontal => bounds.y1 = viewport.y1,
                    Axis::Vertical => bounds.x1 = viewport.x1,
                }
                bounds.contains(pos)
            })
            .unwrap_or(false)
    }

    fn point_hits_handle(&self, pos: Point, axis: Axis, lcx: &mut LayoutCx) -> bool {
        let bounds = self.calc_handle_bounds(axis, lcx);

        bounds
            .map(|mut bounds| {
                let viewport = lcx.layout_rect_local();
                match axis {
                    // stretch out the hit area to be to the edge of the view
                    Axis::Horizontal => bounds.y1 = viewport.y1,
                    Axis::Vertical => bounds.x1 = viewport.x1,
                }
                bounds.contains(pos)
            })
            .unwrap_or(false)
    }

    /// true if either scrollbar is currently held down/being dragged
    fn are_bars_held(&self) -> bool {
        !matches!(self.held, BarHeldState::None)
    }

    fn update_hover_states(&mut self, pos: Point, lcx: &mut LayoutCx) {
        // pos is already in local coordinates, no need to adjust by scroll offset

        let v_handle_hover = self.point_hits_handle(pos, Axis::Vertical, lcx);
        if self.v_handle_hover != v_handle_hover {
            self.v_handle_hover = v_handle_hover;
            self.id.request_paint();
        }

        let h_handle_hover = self.point_hits_handle(pos, Axis::Horizontal, lcx);
        if self.h_handle_hover != h_handle_hover {
            self.h_handle_hover = h_handle_hover;
            self.id.request_paint();
        }

        let v_track_hover = self.point_hits_bar(pos, Axis::Vertical, lcx);
        if self.v_track_hover != v_track_hover {
            self.v_track_hover = v_track_hover;
            self.id.request_paint();
        }

        let h_track_hover = self.point_hits_bar(pos, Axis::Horizontal, lcx);
        if self.h_track_hover != h_track_hover {
            self.h_track_hover = h_track_hover;
            self.id.request_paint();
        }

        // Set scrolling/interacting state if hovering over scrollbars
        let any_hover =
            self.v_handle_hover || self.h_handle_hover || self.v_track_hover || self.h_track_hover;
        if any_hover != self.is_scrolling_or_interacting {
            self.is_scrolling_or_interacting = any_hover;
            self.id.request_paint();
        }
    }

    fn draw_bars(&self, cx: &mut PaintCx, lcx: &mut LayoutCx) {
        // Check if scrollbars should be shown based on the show_bars_when_idle property
        if !self.scroll_style.show_bars_when_idle() && !self.is_scrolling_or_interacting {
            return;
        }

        let raw_layout_rect = lcx.layout_rect_local();

        let radius = |style: &ScrollTrackStyle, rect: Rect, vertical| {
            if style.rounded() {
                if vertical {
                    RoundedRectRadii::from_single_radius((rect.x1 - rect.x0) / 2.)
                } else {
                    RoundedRectRadii::from_single_radius((rect.y1 - rect.y0) / 2.)
                }
            } else {
                let size = rect.size().min_side();
                let border_radius = style.border_radius();
                RoundedRectRadii {
                    top_left: crate::view::border_radius(
                        border_radius.top_left.unwrap_or(PxPct::Px(0.)),
                        size,
                    ),
                    top_right: crate::view::border_radius(
                        border_radius.top_right.unwrap_or(PxPct::Px(0.)),
                        size,
                    ),
                    bottom_left: crate::view::border_radius(
                        border_radius.bottom_left.unwrap_or(PxPct::Px(0.)),
                        size,
                    ),
                    bottom_right: crate::view::border_radius(
                        border_radius.bottom_right.unwrap_or(PxPct::Px(0.)),
                        size,
                    ),
                }
            }
        };

        if let Some(bounds) = self.calc_handle_bounds(Axis::Vertical, lcx) {
            let style = self.v_handle_style();
            let track_style =
                if self.v_track_hover || matches!(self.held, BarHeldState::Vertical(..)) {
                    &self.track_hover_style
                } else {
                    &self.track_style
                };

            if let Some(color) = track_style.color() {
                let mut bounds = bounds;
                bounds.y0 = raw_layout_rect.y0;
                bounds.y1 = raw_layout_rect.y1;
                cx.fill(&bounds, &color, 0.0);
            }
            let edge_width = style.border().0;
            let rect = bounds.inset(-edge_width / 2.0);
            let rect = rect.to_rounded_rect(radius(style, rect, true));
            cx.fill(&rect, &style.color().unwrap_or(HANDLE_COLOR), 0.0);
            if edge_width > 0.0 {
                if let Some(color) = style.border_color().right {
                    cx.stroke(&rect, &color, &Stroke::new(edge_width));
                }
            }
        }

        // Horizontal bar
        if let Some(bounds) = self.calc_handle_bounds(Axis::Horizontal, lcx) {
            let style = self.h_handle_style();
            let track_style =
                if self.h_track_hover || matches!(self.held, BarHeldState::Horizontal(..)) {
                    &self.track_hover_style
                } else {
                    &self.track_style
                };

            if let Some(color) = track_style.color() {
                let mut bounds = bounds;
                bounds.x0 = raw_layout_rect.x0;
                bounds.x1 = raw_layout_rect.x1;
                cx.fill(&bounds, &color, 0.0);
            }
            let edge_width = style.border().0;
            let rect = bounds.inset(-edge_width / 2.0);
            let rect = rect.to_rounded_rect(radius(style, rect, false));
            cx.fill(&rect, &style.color().unwrap_or(HANDLE_COLOR), 0.0);
            if edge_width > 0.0 {
                if let Some(color) = style.border_color().right {
                    cx.stroke(&rect, &color, &Stroke::new(edge_width));
                }
            }
        }
    }

    fn get_clip_rect(&self, lcx: &mut LayoutCx, radii: RoundedRectRadii) -> Option<RoundedRect> {
        let should_clip_x = matches!(
            self.scroll_style.overflow_x(),
            taffy::Overflow::Clip | taffy::Overflow::Hidden | taffy::Overflow::Scroll
        );
        let should_clip_y = matches!(
            self.scroll_style.overflow_y(),
            taffy::Overflow::Clip | taffy::Overflow::Hidden | taffy::Overflow::Scroll
        );

        if should_clip_x && should_clip_y {
            // Clip both axes
            Some(lcx.content_rect_local().to_rounded_rect(radii))
        } else if should_clip_x || should_clip_y {
            // Clip only one axis - extend the other to child content bounds
            let child_content_rect = self.child.content_rect_local();
            let mut clip_rect = lcx.content_rect_local();
            if !should_clip_x {
                clip_rect.x0 = child_content_rect.x0 - self.scroll_offset.x;
                clip_rect.x1 = child_content_rect.x1 - self.scroll_offset.x;
            }
            if !should_clip_y {
                clip_rect.y0 = child_content_rect.y0 - self.scroll_offset.y;
                clip_rect.y1 = child_content_rect.y1 - self.scroll_offset.y;
            }
            Some(clip_rect.to_rounded_rect(0.))
        } else {
            // Both are Visible, no clipping
            None
        }
    }
}

impl View for Scroll {
    fn id(&self) -> ViewId {
        self.id
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "Scroll".into()
    }

    fn view_style(&self) -> Option<Style> {
        Some(
            Style::new()
                .items_start()
                .set(OverflowX, taffy::Overflow::Scroll)
                .set(OverflowY, taffy::Overflow::Scroll),
        )
    }

    fn update(&mut self, _cx: &mut crate::context::UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(state) = state.downcast::<ScrollState>() {
            let lcx = &mut LayoutCx::new(self.id);
            match *state {
                ScrollState::EnsureVisible(rect) => {
                    self.do_ensure_visible(rect, lcx);
                }
                ScrollState::ScrollDelta(delta) => {
                    self.apply_scroll_delta(delta, lcx);
                }
                ScrollState::ScrollTo(origin) => {
                    self.do_scroll_to(origin, lcx);
                }
                ScrollState::ScrollToPercent(percent) => {
                    let content_size = self.child.layout_rect_local().size();
                    let viewport_size = lcx.content_rect_local().size();

                    // Calculate max scroll (content size - viewport size)
                    let max_scroll = (content_size.to_vec2() - viewport_size.to_vec2())
                        .max_by_component(Vec2::ZERO);

                    // Apply percentage to max scroll
                    let target_offset = max_scroll * (percent as f64);

                    self.do_scroll_to(target_offset.to_point(), lcx);
                }
                ScrollState::ScrollToView(id) => {
                    self.do_scroll_to_view(id, None, lcx);
                }
            }
            self.id.request_layout();
        }
    }

    fn scroll_to(&mut self, cx: &mut WindowState, target: ViewId, rect: Option<Rect>) -> bool {
        let found = self.child.view().borrow_mut().scroll_to(cx, target, rect);
        if found {
            let lcx = &mut LayoutCx::new(self.id);
            self.do_scroll_to_view(target, rect, lcx);
        }
        found
    }

    fn style_pass(&mut self, cx: &mut crate::context::StyleCx<'_>) {
        let style = cx.style();

        self.scroll_style.read(cx);

        let handle_style = style.clone().apply_class(Handle);
        self.handle_style.read_style(cx, &handle_style);
        self.handle_hover_style.read_style(
            cx,
            &handle_style
                .clone()
                .apply_selectors(&[StyleSelector::Hover]),
        );
        self.handle_active_style.read_style(
            cx,
            &handle_style.apply_selectors(&[StyleSelector::Clicking]),
        );

        let track_style = style.apply_class(Track);
        self.track_style.read_style(cx, &track_style);
        self.track_hover_style
            .read_style(cx, &track_style.apply_selectors(&[StyleSelector::Hover]));

        cx.style_view(self.child);
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event, phase: Phase) -> EventPropagation {
        match phase {
            Phase::Bubble | Phase::Target => {
                if self.scroll_style.hide_bar() {
                    return EventPropagation::Continue;
                }

                let lcx = &mut LayoutCx::new(self.id);

                match event {
                    Event::Pointer(PointerEvent::Down(PointerButtonEvent {
                        button: Some(PointerButton::Primary),
                        state,
                        ..
                    })) => {
                        let pos = state.logical_point();

                        // Check vertical scrollbar
                        if self.point_hits_bar(pos, Axis::Vertical, lcx) {
                            if !self.point_hits_handle(pos, Axis::Vertical, lcx) {
                                self.click_bar_area(pos, Axis::Vertical, lcx);
                            }
                            self.held = BarHeldState::Vertical(pos.y, self.scroll_offset);
                            cx.window_state.update_active(self.id());

                            self.id.request_paint();
                            return EventPropagation::Stop;
                        }

                        // Check horizontal scrollbar
                        if self.point_hits_bar(pos, Axis::Horizontal, lcx) {
                            if !self.point_hits_handle(pos, Axis::Horizontal, lcx) {
                                self.click_bar_area(pos, Axis::Horizontal, lcx);
                            }
                            self.held = BarHeldState::Horizontal(pos.x, self.scroll_offset);
                            cx.window_state.update_active(self.id());

                            self.id.request_paint();
                            return EventPropagation::Stop;
                        }
                    }

                    Event::Pointer(PointerEvent::Up { .. }) => {
                        if self.are_bars_held() {
                            self.held = BarHeldState::None;
                            self.id.request_paint();
                        }
                    }

                    Event::Pointer(PointerEvent::Move(pu)) => {
                        let pos = pu.current.logical_point();
                        self.update_hover_states(pos, lcx);
                        match self.held {
                            BarHeldState::Vertical(start_y, initial_offset) => {
                                let scale = self.scroll_scale(Axis::Vertical, lcx);
                                let scroll_delta = (pos.y - start_y) * scale;
                                self.do_scroll_to(
                                    Point::new(initial_offset.x, initial_offset.y + scroll_delta),
                                    lcx,
                                );
                            }
                            BarHeldState::Horizontal(start_x, initial_offset) => {
                                let scale = self.scroll_scale(Axis::Horizontal, lcx);
                                let scroll_delta = (pos.x - start_x) * scale;
                                self.do_scroll_to(
                                    Point::new(initial_offset.x + scroll_delta, initial_offset.y),
                                    lcx,
                                );
                            }
                            BarHeldState::None
                                if self.point_hits_bar(pos, Axis::Vertical, lcx)
                                    || self.point_hits_bar(pos, Axis::Horizontal, lcx) =>
                            {
                                return EventPropagation::Stop;
                            }
                            _ => {}
                        }
                    }

                    Event::Pointer(PointerEvent::Leave(_)) => {
                        self.v_handle_hover = false;
                        self.h_handle_hover = false;
                        self.v_track_hover = false;
                        self.h_track_hover = false;
                        self.is_scrolling_or_interacting = false;
                        self.id.request_paint();
                    }

                    Event::Pointer(PointerEvent::Scroll(PointerScrollEvent { state, .. })) => {
                        if let Some(delta) = event.pixel_scroll_delta_vec2() {
                            let delta = -if self.scroll_style.vertical_scroll_as_horizontal()
                                && delta.x == 0.0
                                && delta.y != 0.0
                            {
                                Vec2::new(delta.y, delta.x)
                            } else {
                                delta
                            };

                            let change = self.apply_scroll_delta(delta, lcx);

                            // Check if the scroll bars now hover
                            self.update_hover_states(state.logical_point(), lcx);

                            return if self.scroll_style.propagate_pointer_wheel()
                                && change.is_none()
                            {
                                EventPropagation::Continue
                            } else {
                                EventPropagation::Stop
                            };
                        }
                    }

                    _ => {}
                }

                EventPropagation::Continue
            }
            Phase::Capture => EventPropagation::Continue,
        }
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        let lcx = &mut LayoutCx::new(self.id);

        // this apply scroll delta of zero is cheap.
        // it is here in the case that the available delta changed, this will catch it and update it to a better size
        self.apply_scroll_delta(Vec2::ZERO, lcx);

        let raw_rect_local = lcx.layout_rect_local();
        let pre = cx.transform;
        cx.save();

        let radii = crate::view::border_to_radii(
            &self.id.state().borrow().combined_style,
            raw_rect_local.size(),
        );

        let clip_rect = self.get_clip_rect(lcx, radii);

        if let Some(clip_rect) = clip_rect {
            cx.clip(&clip_rect);
        }
        self.id.set_box_tree_clip(clip_rect);
        self.child
            .set_box_tree_clip_behavior(understory_box_tree::ClipBehavior::Inherit);

        cx.paint_view(self.child);
        cx.restore();
        let post = cx.transform;
        assert_eq!(pre, post);
        if !self.scroll_style.hide_bar() {
            self.draw_bars(cx, lcx);
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
/// Represents a custom style for a `Scroll`.
#[derive(Default, Debug, Clone)]
pub struct ScrollCustomStyle(Style);
impl From<ScrollCustomStyle> for Style {
    fn from(value: ScrollCustomStyle) -> Self {
        value.0
    }
}
impl From<Style> for ScrollCustomStyle {
    fn from(value: Style) -> Self {
        Self(value)
    }
}
impl CustomStyle for ScrollCustomStyle {
    type StyleClass = ScrollClass;
}

impl CustomStylable<ScrollCustomStyle> for Scroll {
    type DV = Self;
}

impl ScrollCustomStyle {
    /// Creates a new `ScrollCustomStyle`.
    pub fn new() -> Self {
        Self(Style::new())
    }

    /// Configures the scroll view to allow the viewport to be smaller than the inner content,
    /// while still taking up the full available space in its container.
    ///
    /// Use this when you need a scroll view that can shrink its viewport size to fit within
    /// the container, ensuring the content remains scrollable even if the inner content is
    /// greater than the parent size.
    ///
    /// Internally this does a `s.min_size(0., 0.).size_full()`.
    pub fn shrink_to_fit(mut self) -> Self {
        self = Self(self.0.min_size(0., 0.).size_full());
        self
    }

    /// Sets the background color for the handle.
    pub fn handle_background(mut self, color: impl Into<Brush>) -> Self {
        self = Self(self.0.class(Handle, |s| s.background(color.into())));
        self
    }

    /// Sets the border radius for the handle.
    pub fn handle_border_radius(mut self, border_radius: impl Into<PxPct>) -> Self {
        self = Self(self.0.class(Handle, |s| s.border_radius(border_radius)));
        self
    }

    /// Sets the border color for the handle.
    pub fn handle_border_color(mut self, border_color: impl Into<Brush>) -> Self {
        self = Self(self.0.class(Handle, |s| s.border_color(border_color)));
        self
    }

    /// Sets the border thickness for the handle.
    pub fn handle_border(mut self, border: impl Into<Px>) -> Self {
        self = Self(self.0.class(Handle, |s| s.set(Border, border)));
        self
    }

    /// Sets whether the handle should have rounded corners.
    pub fn handle_rounded(mut self, rounded: impl Into<bool>) -> Self {
        self = Self(self.0.class(Handle, |s| s.set(Rounded, rounded)));
        self
    }

    /// Sets the background color for the track.
    pub fn track_background(mut self, color: impl Into<Brush>) -> Self {
        self = Self(self.0.class(Track, |s| s.background(color.into())));
        self
    }

    /// Sets the border radius for the track.
    pub fn track_border_radius(mut self, border_radius: impl Into<PxPct>) -> Self {
        self = Self(self.0.class(Track, |s| s.border_radius(border_radius)));
        self
    }

    /// Sets the border color for the track.
    pub fn track_border_color(mut self, border_color: impl Into<Brush>) -> Self {
        self = Self(self.0.class(Track, |s| s.border_color(border_color)));
        self
    }

    /// Sets the border thickness for the track.
    pub fn track_border(mut self, border: impl Into<Px>) -> Self {
        self = Self(self.0.class(Track, |s| s.set(Border, border)));
        self
    }

    /// Sets whether the track should have rounded corners.
    pub fn track_rounded(mut self, rounded: impl Into<bool>) -> Self {
        self = Self(self.0.class(Track, |s| s.set(Rounded, rounded)));
        self
    }

    /// Sets the vertical track inset.
    pub fn vertical_track_inset(mut self, inset: impl Into<Px>) -> Self {
        self = Self(self.0.set(VerticalInset, inset));
        self
    }

    /// Sets the horizontal track inset.
    pub fn horizontal_track_inset(mut self, inset: impl Into<Px>) -> Self {
        self = Self(self.0.set(HorizontalInset, inset));
        self
    }

    /// Controls the visibility of the scroll bars.
    pub fn hide_bars(mut self, hide: impl Into<bool>) -> Self {
        self = Self(self.0.set(HideBars, hide));
        self
    }

    /// Sets whether the pointer wheel events should be propagated.
    pub fn propagate_pointer_wheel(mut self, propagate: impl Into<bool>) -> Self {
        self = Self(self.0.set(PropagatePointerWheel, propagate));
        self
    }

    /// Sets whether vertical scrolling should be interpreted as horizontal scrolling.
    pub fn vertical_scroll_as_horizontal(mut self, vert_as_horiz: impl Into<bool>) -> Self {
        self = Self(self.0.set(VerticalScrollAsHorizontal, vert_as_horiz));
        self
    }

    /// Controls whether scroll bars are shown when not scrolling. When false, bars are only shown during scroll interactions.
    pub fn show_bars_when_idle(mut self, show: impl Into<bool>) -> Self {
        self = Self(self.0.set(ShowBarsWhenIdle, show));
        self
    }
}

/// A trait that adds a `scroll` method to any type that implements `IntoView`.
pub trait ScrollExt {
    /// Wrap the view in a scroll view.
    fn scroll(self) -> Scroll;
}

impl<T: IntoView + 'static> ScrollExt for T {
    fn scroll(self) -> Scroll {
        scroll(self)
    }
}
