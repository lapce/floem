use std::path::PathBuf;

use dpi::PhysicalPosition;
use peniko::kurbo::Point;

/// A standard `DragEvent` for file drag events.
#[derive(Clone, Debug)]
pub enum FileDragEvent {
    /// A file drag operation has entered the window.
    DragEntered {
        /// List of paths that are being dragged onto the window.
        paths: Vec<PathBuf>,
        /// (x,y) coordinates in pixels relative to the top-left corner of the window. May be
        /// negative on some platforms if something is dragged over a window's decorations (title
        /// bar, frame, etc).
        position: PhysicalPosition<f64>,

        scale_factor: f64,
    },
    /// A file drag operation has moved over the window.
    DragMoved {
        /// (x,y) coordinates in pixels relative to the top-left corner of the window. May be
        /// negative on some platforms if something is dragged over a window's decorations (title
        /// bar, frame, etc).
        position: PhysicalPosition<f64>,

        scale_factor: f64,
    },
    /// The file drag operation has dropped file(s) on the window.
    DragDropped {
        /// List of paths that are being dragged onto the window.
        paths: Vec<PathBuf>,
        /// (x,y) coordinates in pixels relative to the top-left corner of the window. May be
        /// negative on some platforms if something is dragged over a window's decorations (title
        /// bar, frame, etc).
        position: PhysicalPosition<f64>,

        scale_factor: f64,
    },
    /// The file drag operation has been cancelled or left the window.
    DragLeft {
        /// (x,y) coordinates in pixels relative to the top-left corner of the window. May be
        /// negative on some platforms if something is dragged over a window's decorations (title
        /// bar, frame, etc).
        ///
        /// ## Platform-specific
        ///
        /// - **Windows:** Always emits [`None`].
        position: Option<PhysicalPosition<f64>>,

        scale_factor: f64,
    },
}

impl FileDragEvent {
    pub fn logical_point(&self) -> Option<Point> {
        match self {
            FileDragEvent::DragEntered {
                position,
                scale_factor,
                ..
            }
            | FileDragEvent::DragMoved {
                position,
                scale_factor,
            }
            | FileDragEvent::DragDropped {
                position,
                scale_factor,
                ..
            }
            | FileDragEvent::DragLeft {
                position: Some(position),
                scale_factor,
            } => {
                let log_pos = position.to_logical(*scale_factor);
                let point = Point::new(log_pos.x, log_pos.y);
                Some(point)
            }
            _ => None,
        }
    }
}
