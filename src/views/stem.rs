//! A generic container view, similar to HTML's `<div>`.

use crate::view::{ParentView, View, ViewId};

/// A generic container view that can hold multiple children.
///
/// `Stem` is the simplest container in Floem - it's just a wrapper that can
/// have children and be styled. It's analogous to HTML's `<div>` element.
/// The name comes from Floem's botanical theme (phloem is plant tissue),
/// where a stem is the main structural support that holds branches.
///
/// Use `Stem` when you need a container without any specific layout behavior.
/// For directional layouts, consider [`Stack`](super::Stack) instead.
///
/// ## Example
/// ```rust,ignore
/// use floem::views::Stem;
///
/// // Empty stem that can be styled
/// Stem::new().style(|s| s.padding(10.0).background(Color::GRAY));
///
/// // Stem with children using builder pattern
/// Stem::new()
///     .child(text("Header"))
///     .children((0..5).map(|i| text(format!("Item {i}"))))
///     .child(text("Footer"));
/// ```
pub struct Stem {
    id: ViewId,
}

impl Stem {
    /// Creates a new empty stem.
    ///
    /// ## Example
    /// ```rust
    /// use floem::views::Stem;
    ///
    /// let container = Stem::new();
    /// ```
    pub fn new() -> Self {
        Stem { id: ViewId::new() }
    }

    /// Creates a new stem with a pre-existing [`ViewId`].
    ///
    /// This is useful for lazy view construction where the `ViewId` is created
    /// before the view itself.
    pub fn with_id(id: ViewId) -> Self {
        Stem { id }
    }
}

impl Default for Stem {
    fn default() -> Self {
        Self::new()
    }
}

impl View for Stem {
    fn id(&self) -> ViewId {
        self.id
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "Stem".into()
    }
}

impl ParentView for Stem {}
