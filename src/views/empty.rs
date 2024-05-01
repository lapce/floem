use crate::{id::ViewId, view::View};

/// An empty View. See [`empty`].
pub struct Empty {
    id: ViewId,
}

/// An empty View. This view can still have a size, background, border radius, and outline.
///
/// This view can also be useful if you have another view that requires a child element but there is not a meaningful child element that needs to be provided.
pub fn empty() -> Empty {
    Empty { id: ViewId::new() }
}

impl View for Empty {
    fn id(&self) -> ViewId {
        self.id
    }
}
