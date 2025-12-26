use crate::{view::View, view::ViewId};

/// An empty View.
///
/// This view can still have a size, background, border radius, and outline.
/// It can be used as a simple placeholder view when another view requires a child element but
/// there is no meaningful child element to be provided.
pub struct Empty {
    pub(crate) id: ViewId,
}

impl Empty {
    /// Creates a new empty view.
    ///
    /// ## Example
    /// ```rust
    /// use floem::views::Empty;
    ///
    /// let placeholder = Empty::new();
    /// ```
    pub fn new() -> Self {
        Empty { id: ViewId::new() }
    }

    /// Creates a new empty view with a pre-existing [`ViewId`].
    ///
    /// This is useful for lazy view construction where the `ViewId` is created
    /// before the view itself.
    pub fn with_id(id: ViewId) -> Self {
        Empty { id }
    }
}

impl Default for Empty {
    fn default() -> Self {
        Self::new()
    }
}

/// An empty View. This view can still have a size, background, border radius, and outline.
///
/// This view can be used as a simple placeholder view when another view requires a child element but
/// there is no meaningful child element to be provided.
#[deprecated(since = "0.2.0", note = "Use Empty::new() instead")]
pub fn empty() -> Empty {
    Empty::new()
}

impl View for Empty {
    fn id(&self) -> ViewId {
        self.id
    }
}
