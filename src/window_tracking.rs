//! Provides tracking to map window ids to windows in order to force repaints
//! on inactive windows (which would otherwise not receive messages), and so
//! that views can retrieve the `WindowId` of the window that contains them
//! and use the methods that look up the `Window` for that id to retrieve information
//! such as screen position.
use crate::ViewId;
use floem_winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    monitor::MonitorHandle,
    window::{Window, WindowId},
};
use peniko::kurbo::{Point, Rect};
use std::{
    collections::HashMap,
    sync::{Arc, OnceLock, RwLock},
};

static WINDOW_FOR_WINDOW_AND_ROOT_IDS: OnceLock<RwLock<WindowMapping>> = OnceLock::new();

/// Add a mapping from `root_id` -> `window_id` -> `window` for the given triple.
pub fn store_window_id_mapping(
    root_id: ViewId,
    window_id: WindowId,
    window: &Arc<floem_winit::window::Window>,
) {
    with_window_map_mut(move |m| m.add(root_id, window_id, window.clone()));
}

/// Remove the mapping from `root_id` -> `window_id` -> `window` for the given triple.
pub fn remove_window_id_mapping(root_id: &ViewId, window_id: &WindowId) {
    with_window_map_mut(move |m| m.remove(root_id, window_id));
}

/// Maps root-id:window-id:window triples, so a view can get its root and
/// from that locate the window-id (if any) that it belongs to.
#[derive(Default, Debug)]
struct WindowMapping {
    window_for_window_id: HashMap<WindowId, Arc<Window>>,
    window_id_for_root_view_id: HashMap<ViewId, WindowId>,
}

impl WindowMapping {
    fn add(&mut self, root: ViewId, window_id: WindowId, window: Arc<Window>) {
        self.window_for_window_id.insert(window_id, window);
        self.window_id_for_root_view_id.insert(root, window_id);
    }

    fn remove(&mut self, root: &ViewId, window_id: &WindowId) {
        let root_found = self.window_id_for_root_view_id.remove(root).is_some();
        let window_found = self.window_for_window_id.remove(window_id).is_some();
        debug_assert!(root_found == window_found,
            "Window mapping state inconsistent. Remove root {:?} success was {} but remove {:?} success was {}",
            root, root_found, window_id, window_found);
    }

    fn with_window_id_and_window<F: FnOnce(&WindowId, &Window) -> T, T>(
        &self,
        root_view_id: ViewId,
        f: F,
    ) -> Option<T> {
        self.window_id_for_root_view_id
            .get(&root_view_id)
            .and_then(|window_id| {
                self.window_for_window_id
                    .get(window_id)
                    .map(|window| f(window_id, window))
            })
    }

    fn with_window<F: FnOnce(&Arc<Window>) -> T, T>(&self, window: &WindowId, f: F) -> Option<T> {
        self.window_for_window_id.get(window).map(f)
    }

    fn window_id_for_root(&self, id: &ViewId) -> Option<WindowId> {
        self.window_id_for_root_view_id.get(id).copied()
    }

    fn root_view_id_for(&self, window_id: &WindowId) -> Option<ViewId> {
        for (k, v) in self.window_id_for_root_view_id.iter() {
            if v == window_id {
                return Some(*k);
            }
        }
        None
    }
}

pub fn with_window_id_and_window<F: FnOnce(&WindowId, &Window) -> T, T>(
    view: &ViewId,
    f: F,
) -> Option<T> {
    view.root()
        .and_then(|root_view_id| with_window_map(|m| m.with_window_id_and_window(root_view_id, f)))
        .unwrap_or(None)
}

fn with_window_map_mut<F: FnMut(&mut WindowMapping)>(mut f: F) -> bool {
    let map = WINDOW_FOR_WINDOW_AND_ROOT_IDS.get_or_init(|| RwLock::new(Default::default()));
    if let Ok(mut map) = map.write() {
        f(&mut map);
        true
    } else {
        false
    }
}

fn with_window_map<F: FnOnce(&WindowMapping) -> T, T>(f: F) -> Option<T> {
    let map = WINDOW_FOR_WINDOW_AND_ROOT_IDS.get_or_init(|| RwLock::new(Default::default()));
    if let Ok(map) = map.read() {
        Some(f(&map))
    } else {
        None
    }
}

pub fn with_window<F: FnOnce(&Window) -> T, T>(window: &WindowId, f: F) -> Option<T> {
    with_window_map(|m| m.with_window(window, |w| f(w.as_ref()))).unwrap_or(None)
}

pub fn root_view_id(window: &WindowId) -> Option<ViewId> {
    with_window_map(|m| m.root_view_id_for(window)).unwrap_or(None)
}

/// Force a single window to repaint - this is necessary in cases where the
/// window is not the active window and otherwise would not process update
/// messages sent to it.
pub fn force_window_repaint(id: &WindowId) -> bool {
    with_window_map(|m| {
        m.with_window(id, |window| window.request_redraw())
            .is_some()
    })
    .unwrap_or(false)
}

pub fn window_id_for_root(root_id: ViewId) -> Option<WindowId> {
    with_window_map(|map| map.window_id_for_root(&root_id)).unwrap_or(None)
}

pub fn monitor_bounds(id: &WindowId) -> Option<Rect> {
    with_window_map(|m| {
        m.with_window(id, |window| {
            window
                .current_monitor()
                .map(|monitor| monitor_bounds_for_monitor(window, &monitor))
        })
        .unwrap_or(None)
    })
    .unwrap_or(None)
}

pub fn monitor_bounds_for_monitor(window: &Window, monitor: &MonitorHandle) -> Rect {
    let scale = 1.0 / window.scale_factor();
    let pos = monitor.position();
    let sz = monitor.size();
    let x = pos.x as f64 * scale;
    let y = pos.y as f64 * scale;
    Rect::new(
        x,
        y,
        x + sz.width as f64 * scale,
        y + sz.height as f64 * scale,
    )
}

fn scale_rect(window: &Window, mut rect: Rect) -> Rect {
    let scale = 1.0 / window.scale_factor();
    rect.x0 *= scale;
    rect.y0 *= scale;
    rect.x1 *= scale;
    rect.y1 *= scale;
    rect
}

fn scale_point(window: &Window, mut rect: Point) -> Point {
    let scale = 1.0 / window.scale_factor();
    rect.x *= scale;
    rect.y *= scale;
    rect
}

pub fn window_inner_screen_position(id: &WindowId) -> Option<Point> {
    with_window_map(|m| {
        m.with_window(id, |window| {
            window
                .inner_position()
                .map(|pos| Some(scale_point(window, Point::new(pos.x as f64, pos.y as f64))))
                .unwrap_or(None)
        })
        .unwrap_or(None)
    })
    .unwrap_or(None)
}

pub fn window_inner_screen_bounds(id: &WindowId) -> Option<Rect> {
    with_window_map(|m| {
        m.with_window(id, |window| {
            window
                .inner_position()
                .map(|pos| {
                    Some(rect_from_physical_bounds_for_window(
                        window,
                        pos,
                        window.inner_size(),
                    ))
                })
                .unwrap_or(None)
        })
        .unwrap_or(None)
    })
    .unwrap_or(None)
}

pub fn rect_from_physical_bounds_for_window(
    window: &Window,
    pos: PhysicalPosition<i32>,
    sz: PhysicalSize<u32>,
) -> Rect {
    scale_rect(
        window,
        Rect::new(
            pos.x as f64,
            pos.y as f64,
            pos.x as f64 + sz.width.max(0) as f64,
            pos.y as f64 + sz.height.max(0) as f64,
        ),
    )
}

pub fn window_outer_screen_position(id: &WindowId) -> Option<Point> {
    with_window_map(|m| {
        m.with_window(id, |window| {
            window
                .outer_position()
                .map(|pos| Some(scale_point(window, Point::new(pos.x as f64, pos.y as f64))))
                .unwrap_or(None)
        })
        .unwrap_or(None)
    })
    .unwrap_or(None)
}

pub fn window_outer_screen_bounds(id: &WindowId) -> Option<Rect> {
    with_window_map(|m| {
        m.with_window(id, |window| {
            window
                .outer_position()
                .map(|pos| {
                    Some(rect_from_physical_bounds_for_window(
                        window,
                        pos,
                        window.outer_size(),
                    ))
                })
                .unwrap_or(None)
        })
        .unwrap_or(None)
    })
    .unwrap_or(None)
}
