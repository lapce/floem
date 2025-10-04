use crate::{id::ViewId, view::View};

/// An empty View. See [`empty`].
pub struct Empty {
    pub(crate) id: ViewId,
}

/// An empty View. This view can still have a size, background, border radius, and outline.
///
/// This view can be used as a simple placeholder view when another view requires a child element but
/// there is no meaningful child element to be provided.
pub fn empty() -> Empty {
    Empty { id: ViewId::new() }
}

impl View for Empty {
    fn id(&self) -> ViewId {
        self.id
    }
}
