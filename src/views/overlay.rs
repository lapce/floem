//! Declarative overlay view that renders content above all other views.
//!
//! This module provides a declarative overlay that automatically manages
//! the overlay lifecycle, without requiring manual `add_overlay`/`remove_overlay` calls.

use floem_reactive::Scope;

use crate::{
    style::Style,
    view::{ParentView, View, ViewId},
};

/// A declarative overlay that renders content above all other views.
///
/// The overlay content remains in the view tree as a child of this view,
/// maintaining proper parent-child lifetime semantics. Floem reparents overlay
/// nodes in the box tree to the window root so they participate in normal
/// z-index ordering above regular content.
///
/// Visibility can be controlled through styles on the content itself.
///
/// ## Example
/// ```rust
/// use floem::prelude::*;
/// use floem::views::{Label, Overlay, Decorators};
///
/// let show_dialog = RwSignal::new(false);
///
/// Stack::vertical((
///     Button::new("Show Dialog").action(move || show_dialog.set(true)),
///     Overlay::new_dyn(move || {
///         let visible = show_dialog.get();
///         Stack::vertical((
///             Label::derived(|| "This is a dialog!".to_string()),
///             Button::new("Close").action(move || show_dialog.set(false)),
///         ))
///         .style(move |s| {
///             s.apply_if(!visible, |s| s.hide())
///                 .background(Color::WHITE)
///                 .padding(20)
///                 .border_radius(8)
///         })
///     }),
/// ));
/// ```
///
/// ## Notes
/// - The overlay is positioned absolutely at the window level
/// - Overlay uses a default `z-index: 1` so it sorts above non-overlay content
/// - You can style the overlay content using the view returned by `Overlay::new(...)`
/// - The overlay is automatically removed when this view is cleaned up
pub struct Overlay {
    id: ViewId,
    scope: Scope,
}

impl Overlay {
    /// Creates a new overlay.
    ///
    ///
    /// # Example
    /// ```rust
    /// use floem::prelude::*;
    /// use floem::views::{Overlay, Label};
    ///
    /// Overlay::new("Static overlay content");
    /// ```
    pub fn new(child: impl crate::IntoView + 'static) -> Self {
        let id = ViewId::new();
        id.add_child(child.into_any());
        Self::with_id(id)
    }

    /// Creates a new overlay with no child.
    ///
    ///
    /// # Example
    /// ```rust
    /// use floem::prelude::*;
    /// use floem::views::Overlay;
    ///
    /// Overlay::base();
    /// ```
    pub fn base() -> Self {
        let id = ViewId::new();
        Self::with_id(id)
    }

    /// Creates a new overlay whose child will dynamically update in response to signal changes.
    ///
    /// # Example
    /// ```rust
    /// use floem::prelude::*;
    /// use floem::views::Overlay;
    ///
    /// let message = RwSignal::new("Loading...".to_string());
    ///
    /// Overlay::new_dyn(move || Label::new(message.get()))
    /// ```
    pub fn new_dyn<CF, V>(child_fn: CF) -> Self
    where
        CF: Fn() -> V + 'static,
        V: crate::IntoView + 'static,
    {
        Self::with_id(ViewId::new()).derived_child(child_fn)
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
        Self::new(())
    }
}

impl View for Overlay {
    fn id(&self) -> ViewId {
        self.id
    }

    fn view_style(&self) -> Option<Style> {
        Some(Style::new().z_index(1))
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

/// A trait that adds an `overlay` method to any type that implements `IntoView`.
pub trait OverlayExt {
    /// Wrap the view in an overlay.
    fn overlay(self) -> Overlay;
}

impl<T: crate::IntoView + 'static> OverlayExt for T {
    fn overlay(self) -> Overlay {
        Overlay::new(self)
    }
}
