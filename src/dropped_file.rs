use std::path::PathBuf;

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
        position: Point,
    },
    /// A file drag operation has moved over the window.
    DragMoved {
        /// (x,y) coordinates in pixels relative to the top-left corner of the window. May be
        /// negative on some platforms if something is dragged over a window's decorations (title
        /// bar, frame, etc).
        position: Point,
    },
    /// The file drag operation has dropped file(s) on the window.
    DragDropped {
        /// List of paths that are being dragged onto the window.
        paths: Vec<PathBuf>,
        /// (x,y) coordinates in pixels relative to the top-left corner of the window. May be
        /// negative on some platforms if something is dragged over a window's decorations (title
        /// bar, frame, etc).
        position: Point,
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
        position: Option<Point>,
    },
}
