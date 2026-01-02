//! Declarative overlay view that renders content above all other views.
//!
//! This module provides a declarative overlay that automatically manages
//! the overlay lifecycle, without requiring manual `add_overlay`/`remove_overlay` calls.

use floem_reactive::Scope;

use crate::view::{ParentView, View, ViewId};

/// A declarative overlay that renders content above all other views.
///
/// The overlay content remains in the view tree as a child of this view,
/// maintaining proper parent-child lifetime semantics. During paint, overlays
/// escape z-index constraints and are rendered at the root level.
///
/// Visibility can be controlled through styles on the content itself.
///
/// ## Example
/// ```rust
/// use floem::prelude::*;
/// use floem::views::{Overlay, Label, Decorators};
///
/// let show_dialog = RwSignal::new(false);
///
/// Stack::vertical((
///     Button::new("Show Dialog").action(move || show_dialog.set(true)),
///     Overlay::new()
///         .derived_child(move || {
///             let visible = show_dialog.get();
///             Stack::vertical((
///                 Label::derived(|| "This is a dialog!".to_string()),
///                 Button::new("Close").action(move || show_dialog.set(false)),
///             ))
///             .style(move |s| {
///                 s.apply_if(!visible, |s| s.hide())
///                     .background(Color::WHITE)
///                     .padding(20)
///                     .border_radius(8)
///             })
///         }),
/// ));
/// ```
///
/// ## Notes
/// - The overlay is positioned absolutely at the window level
/// - You can style the overlay content using `.child()` or `.derived_child()`
/// - The overlay is automatically removed when this view is cleaned up
pub struct Overlay {
    id: ViewId,
    scope: Scope,
}

impl Overlay {
    /// Creates a new empty overlay.
    ///
    /// Use `.child()` or `.derived_child()` to add content.
    ///
    /// # Example
    /// ```rust
    /// use floem::prelude::*;
    /// use floem::views::{Overlay, Label};
    ///
    /// Overlay::new().child(Label::new("Static overlay content"));
    /// ```
    pub fn new() -> Self {
        Self::with_id(ViewId::new())
    }

    /// Creates a new empty overlay with a specific ViewId.
    ///
    /// This is useful when you need to control the ViewId for the overlay.
    ///
    /// # Arguments
    /// * `id` - The ViewId to use for this overlay
    pub fn with_id(id: ViewId) -> Self {
        let scope = Scope::current().create_child();
        id.register_overlay();
        Overlay { id, scope }
    }
}

impl Default for Overlay {
    fn default() -> Self {
        Self::new()
    }
}

impl View for Overlay {
    fn id(&self) -> ViewId {
        self.id
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "Overlay".into()
    }
}

impl ParentView for Overlay {
    fn scope(&self) -> Option<Scope> {
        Some(self.scope)
    }
}
