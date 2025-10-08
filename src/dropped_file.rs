use peniko::kurbo::Point;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct DroppedFilesEvent {
    pub path: Vec<PathBuf>,
    pub pos: Point,
}
