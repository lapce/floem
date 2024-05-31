//! Tools for computing screen locations from locations within a View and
//! vice-versa.
use crate::ViewId;
use floem_winit::window::{Window, WindowId};
use peniko::kurbo::{Point, Rect, Size};

use crate::window_tracking::{
    monitor_bounds_for_monitor, rect_from_physical_bounds_for_window, with_window_id_and_window,
};

/// Create a ScreenLayout for a view.  This can fail if the view or an
/// ancestor of it has no parent and is not realized on-screen, or if the
/// platform does not support reading window inner or outer bounds.  ScreenLayout
/// is useful when needing to convert locations within a view into absolute
/// positions on-screen, such as for creating a window at position relative
/// to that view.
pub fn try_create_screen_layout(view: &ViewId) -> Option<ScreenLayout> {
    with_window_id_and_window(view, |window_id, window| {
        window
            .current_monitor()
            .map(|monitor| {
                window
                    .inner_position()
                    .map(|inner_position| {
                        window
                            .outer_position()
                            .map(|outer_position| {
                                let monitor_bounds = monitor_bounds_for_monitor(window, &monitor);
                                let inner_size = window.inner_size();
                                let outer_size = window.outer_size();

                                let window_bounds = rect_from_physical_bounds_for_window(
                                    window,
                                    outer_position,
                                    outer_size,
                                );

                                let window_content_bounds = rect_from_physical_bounds_for_window(
                                    window,
                                    inner_position,
                                    inner_size,
                                );

                                let view_origin_in_window = find_window_origin(view);
                                let monitor_scale = window.scale_factor();

                                ScreenLayout {
                                    monitor_scale,
                                    monitor_bounds,
                                    window_content_bounds,
                                    window_bounds,
                                    view_origin_in_window: Some(view_origin_in_window),
                                    window_id: *window_id,
                                }
                            })
                            .ok()
                    })
                    .ok()
            })
            .unwrap_or(None)
            .unwrap_or(None)
    })
    .unwrap_or(None)
}

pub fn screen_layout_for_window(window_id: WindowId, window: &Window) -> Option<ScreenLayout> {
    window
        .current_monitor()
        .map(|monitor| {
            window
                .inner_position()
                .map(|inner_position| {
                    window
                        .outer_position()
                        .map(|outer_position| {
                            let monitor_bounds = monitor_bounds_for_monitor(window, &monitor);
                            let inner_size = window.inner_size();
                            let outer_size = window.outer_size();

                            let window_bounds = rect_from_physical_bounds_for_window(
                                window,
                                outer_position,
                                outer_size,
                            );

                            let window_content_bounds = rect_from_physical_bounds_for_window(
                                window,
                                inner_position,
                                inner_size,
                            );

                            let view_origin_in_window = None;
                            let monitor_scale = window.scale_factor();

                            ScreenLayout {
                                monitor_scale,
                                monitor_bounds,
                                window_content_bounds,
                                window_bounds,
                                view_origin_in_window,
                                window_id,
                            }
                        })
                        .ok()
                })
                .ok()
        })
        .unwrap_or(None)
        .unwrap_or(None)
}

/// Relates a realized `View` to the bounds of the window that contains it,
/// and the window to the bounds of the monitor that contains it.  All fields
/// are in logical coordinates (if the OS scales physical coordinates for high
/// DPI displays, the scaling is already applied).
///
/// Instances are a snapshot in time of the location of the view and window
/// at the time of creation, and are not updated if view or window or monitor
/// in use is.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ScreenLayout {
    /// The window id
    pub window_id: WindowId,
    /// The scaling of the monitor, if any
    pub monitor_scale: f64,
    /// The logical bounds of the monitor
    pub monitor_bounds: Rect,
    /// The bounds of the view content within the monitor's bounds
    pub window_content_bounds: Rect,
    /// The bounds of the window within the monitor's bounds
    pub window_bounds: Rect,
    /// The origin of the view within the window, if this ScreenLayout was
    /// created from a `View` rather than a `WindowId` - needed for computing
    /// relative offsets from, e.g., the location of a mouse click within
    /// a `View`.
    pub view_origin_in_window: Option<Point>,
}

impl ScreenLayout {
    /// Unscales this Screen to physical device coordinates, less any DPI
    /// scaling done by hardware.
    pub fn to_physical_scale(&self) -> Self {
        // The bounds we have are pre-scaled by 1.0/window.scale(), so
        // inverting them is multiplication, not division.
        Self {
            monitor_scale: 1.0,
            monitor_bounds: scale_rect(self.monitor_scale, self.monitor_bounds),
            window_bounds: scale_rect(self.monitor_scale, self.window_bounds),
            window_content_bounds: scale_rect(self.monitor_scale, self.window_content_bounds),
            view_origin_in_window: self
                .view_origin_in_window
                .map(|origin| scale_point(self.monitor_scale, origin)),
            window_id: self.window_id,
        }
    }

    /// Get the insets required to transform the outer rectangle into
    /// the inner one in the form `(left, top, right, bottom)`
    pub fn window_frame_insets(&self) -> (f64, f64, f64, f64) {
        // Kurbo contains an Insets type obtainable from
        //   self.window_bounds - self.window_content_bounds
        // but uses a definition of "insets" that is has nothing
        // to do with what any UI toolkit has ever meant by the word.
        (
            self.window_content_bounds.x0 - self.window_bounds.x0,
            self.window_content_bounds.y0 - self.window_bounds.y0,
            self.window_bounds.x1 - self.window_content_bounds.x1,
            self.window_bounds.y1 - self.window_content_bounds.y1,
        )
    }

    /// If true, this instance has scaling applied.
    pub fn is_scaled(&self) -> bool {
        self.monitor_scale != 0_f64
    }

    /// Convert a screen position to a position within the view that created
    /// this one.
    pub fn view_location_from_screen(&self, screen_point: Point) -> Point {
        let mut result = screen_point;
        if let Some(origin) = self.view_origin_in_window {
            result.x -= origin.x + self.window_content_bounds.x0;
            result.y -= origin.y + self.window_content_bounds.y0;
        }
        result
    }

    /// Determine if this `ScreenBounds` has a different bounding rectangle for
    /// the content and frame bounds.  Some X11 window managers (Openbox, for one)
    /// appear to support getting frame position separately from content position,
    /// but in fact report the same bounds for both.
    pub fn contains_frame_decoration_insets(&self) -> bool {
        self.window_content_bounds != self.window_bounds
    }

    /// Compute a position, in screen coordinates, relative to the view this layout
    /// was created from.  If a target size is passed, the implementation will attempt
    /// to adjust the resulting point so that a rectangle of the required size fits
    /// entirely on-screen.
    pub fn screen_location_from_view(
        &self,
        relative_position: Option<Point>,
        target_size: Option<Size>,
    ) -> Point {
        let mut result = Point::new(self.window_content_bounds.x0, self.window_content_bounds.y0);
        if let Some(offset) = relative_position {
            result.x += offset.x;
            result.y += offset.y;
        }

        if let Some(origin) = self.view_origin_in_window {
            result.x += origin.x;
            result.y += origin.y;
        }

        // If we have a size, adjust the resulting point to ensure the resulting
        // bounds will fit on screen (if it is possible)
        if let Some(size) = target_size {
            let mut target_bounds = Rect::new(
                result.x,
                result.y,
                result.x + size.width,
                result.y + size.height,
            );
            if target_bounds.x1 > self.monitor_bounds.x1 {
                let offset = target_bounds.x1 - self.monitor_bounds.x1;
                target_bounds.x0 -= offset;
                target_bounds.x1 -= offset;
            }
            if target_bounds.y1 > self.monitor_bounds.y1 {
                let offset = target_bounds.y1 - self.monitor_bounds.y1;
                target_bounds.y0 -= offset;
                target_bounds.y1 -= offset;
            }
            if target_bounds.x0 < self.monitor_bounds.x0 {
                let offset = self.monitor_bounds.x0 - target_bounds.x0;
                target_bounds.x0 += offset;
                target_bounds.x1 += offset;
            }
            if target_bounds.y0 < self.monitor_bounds.y0 {
                let offset = self.monitor_bounds.y0 - target_bounds.y0;
                target_bounds.y0 += offset;
                target_bounds.y1 += offset
            }
            result.x = target_bounds.x0;
            result.y = target_bounds.y0;
        }
        result
    }
}

fn find_window_origin(view: &ViewId) -> Point {
    let mut pt = Point::ZERO;
    recursively_find_window_origin(*view, &mut pt);
    pt
}

fn recursively_find_window_origin(view: ViewId, point: &mut Point) {
    if let Some(layout) = view.get_layout() {
        point.x += layout.location.x as f64;
        point.y += layout.location.y as f64;
        if let Some(parent) = view.parent() {
            recursively_find_window_origin(parent, point);
        }
    }
}

fn scale_point(by: f64, mut pt: Point) -> Point {
    pt.x *= by;
    pt.y *= by;
    pt
}

fn scale_rect(by: f64, mut bds: Rect) -> Rect {
    bds.x0 *= by;
    bds.y0 *= by;
    bds.x1 *= by;
    bds.y1 *= by;
    bds
}
