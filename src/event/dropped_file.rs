use std::{path::PathBuf, rc::Rc};

use peniko::kurbo::Point;

/// A standard `DragEvent` for file drag events.
#[derive(Clone, Debug)]
pub enum FileDragEvent {
    /// A file drag operation has entered an element.
    Enter(FileDragEnter),
    /// A file drag operation has moved over an element.
    Move(FileDragMove),
    /// A file drag operation has left an element.
    Leave(FileDragLeave),
    /// The file drag operation has dropped file(s).
    Drop(FileDragDropped),
}

/// A file drag operation has entered an element.
#[derive(Clone, Debug)]
pub struct FileDragEnter {
    /// List of paths that are being dragged.
    pub paths: Rc<[PathBuf]>,
    /// Logical position (x,y) relative to the window's top-left corner.
    pub position: Point,
}

/// A file drag operation has moved over an element.
#[derive(Clone, Debug)]
pub struct FileDragMove {
    /// List of paths that are being dragged.
    pub paths: Rc<[PathBuf]>,
    /// Logical position (x,y) relative to the window's top-left corner.
    pub position: Point,
}

/// A file drag operation has left an element.
#[derive(Clone, Debug)]
pub struct FileDragLeave {
    /// Logical position (x,y) relative to the window's top-left corner.
    pub position: Point,
}

/// The file drag operation has dropped file(s).
#[derive(Clone, Debug)]
pub struct FileDragDropped {
    /// List of paths that were dropped.
    pub paths: Rc<[PathBuf]>,
    /// Logical position (x,y) relative to the window's top-left corner.
    pub position: Point,
}

impl FileDragEvent {
    pub fn logical_point(&self) -> Point {
        match self {
            FileDragEvent::Enter(e) => e.position,
            FileDragEvent::Move(e) => e.position,
            FileDragEvent::Leave(e) => e.position,
            FileDragEvent::Drop(e) => e.position,
        }
    }

    pub fn paths(&self) -> Option<&Rc<[PathBuf]>> {
        match self {
            FileDragEvent::Enter(e) => Some(&e.paths),
            FileDragEvent::Move(e) => Some(&e.paths),
            FileDragEvent::Leave(_) => None,
            FileDragEvent::Drop(e) => Some(&e.paths),
        }
    }
}
