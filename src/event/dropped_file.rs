use std::path::PathBuf;

use dpi::PhysicalPosition;
use peniko::kurbo::Point;

/// A standard `DragEvent` for file drag events.
#[derive(Clone, Debug)]
pub enum FileDragEvent {
    /// A file drag operation has entered the window.
    DragEntered(FileDragEntered),
    /// A file drag operation has moved over the window.
    DragMoved(FileDragMoved),
    /// The file drag operation has dropped file(s) on the window.
    DragDropped(FileDragDropped),
    /// The file drag operation has been cancelled or left the window.
    DragLeft(FileDragLeft),
}

/// A file drag operation has entered the window.
#[derive(Clone, Debug)]
pub struct FileDragEntered {
    /// List of paths that are being dragged onto the window.
    pub paths: Vec<PathBuf>,
    /// (x,y) coordinates in pixels relative to the top-left corner of the window. May be
    /// negative on some platforms if something is dragged over a window's decorations (title
    /// bar, frame, etc).
    pub position: PhysicalPosition<f64>,
    pub scale_factor: f64,
}

/// A file drag operation has moved over the window.
#[derive(Clone, Debug)]
pub struct FileDragMoved {
    /// (x,y) coordinates in pixels relative to the top-left corner of the window. May be
    /// negative on some platforms if something is dragged over a window's decorations (title
    /// bar, frame, etc).
    pub position: PhysicalPosition<f64>,
    pub scale_factor: f64,
}

/// The file drag operation has dropped file(s) on the window.
#[derive(Clone, Debug)]
pub struct FileDragDropped {
    /// List of paths that are being dragged onto the window.
    pub paths: Vec<PathBuf>,
    /// (x,y) coordinates in pixels relative to the top-left corner of the window. May be
    /// negative on some platforms if something is dragged over a window's decorations (title
    /// bar, frame, etc).
    pub position: PhysicalPosition<f64>,
    pub scale_factor: f64,
}

/// The file drag operation has been cancelled or left the window.
#[derive(Clone, Debug)]
pub struct FileDragLeft {
    /// (x,y) coordinates in pixels relative to the top-left corner of the window. May be
    /// negative on some platforms if something is dragged over a window's decorations (title
    /// bar, frame, etc).
    ///
    /// ## Platform-specific
    ///
    /// - **Windows:** Always emits `None`.
    pub position: Option<PhysicalPosition<f64>>,
    pub scale_factor: f64,
}

impl FileDragEvent {
    pub fn logical_point(&self) -> Option<Point> {
        match self {
            FileDragEvent::DragEntered(e) => Some(e.logical_point()),
            FileDragEvent::DragMoved(e) => Some(e.logical_point()),
            FileDragEvent::DragDropped(e) => Some(e.logical_point()),
            FileDragEvent::DragLeft(e) => e.logical_point(),
        }
    }
}

impl FileDragEntered {
    pub fn logical_point(&self) -> Point {
        let log_pos = self.position.to_logical(self.scale_factor);
        Point::new(log_pos.x, log_pos.y)
    }
}

impl FileDragMoved {
    pub fn logical_point(&self) -> Point {
        let log_pos = self.position.to_logical(self.scale_factor);
        Point::new(log_pos.x, log_pos.y)
    }
}

impl FileDragDropped {
    pub fn logical_point(&self) -> Point {
        let log_pos = self.position.to_logical(self.scale_factor);
        Point::new(log_pos.x, log_pos.y)
    }
}

impl FileDragLeft {
    pub fn logical_point(&self) -> Option<Point> {
        self.position.map(|position| {
            let log_pos = position.to_logical(self.scale_factor);
            Point::new(log_pos.x, log_pos.y)
        })
    }
}
