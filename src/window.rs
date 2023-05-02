use glazier::kurbo::{Point, Size};

#[derive(Default)]
pub struct WindowConfig {
    pub(crate) size: Option<Size>,
    pub(crate) position: Option<Point>,
    pub(crate) show_titlebar: Option<bool>,
}

impl WindowConfig {
    pub fn size(mut self, size: impl Into<Size>) -> Self {
        self.size = Some(size.into());
        self
    }

    pub fn position(mut self, position: Point) -> Self {
        self.position = Some(position);
        self
    }

    pub fn show_titlebar(mut self, show_titlebar: bool) -> Self {
        self.show_titlebar = Some(show_titlebar);
        self
    }
}
