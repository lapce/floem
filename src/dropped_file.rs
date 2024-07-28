use peniko::kurbo::Point;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct DroppedFileWithPositionEvent {
    pub path: PathBuf,
    pub pos: Point,
}

#[derive(Debug, Clone)]
pub struct DroppedFileEvent {
    pub path: PathBuf,
}
