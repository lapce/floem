use peniko::kurbo::Point;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct DroppedFileEvent {
    pub path: PathBuf,
    pub pos: Point,
}
