//! Declarative overlay view that renders content above all other views.
//!
//! This module provides a declarative overlay that automatically manages
//! the overlay lifecycle, without requiring manual `add_overlay`/`remove_overlay` calls.

use floem_reactive::{Scope, UpdaterEffect};

use crate::context::UpdateCx;
use crate::view::{IntoView, View, ViewId};

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
/// v_stack((
///     button("Show Dialog").action(move || show_dialog.set(true)),
///     Overlay::derived(move || {
///         v_stack((
///             Label::derived(|| "This is a dialog!".to_string()),
///             button("Close").action(move || show_dialog.set(false)),
///         ))
///         .style(move |s| {
///             s.apply_if(!show_dialog.get(), |s| s.hide())
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
/// - You can style the overlay content using the view factory
/// - The overlay is automatically removed when this view is cleaned up
pub struct Overlay {
    id: ViewId,
    child_scope: Scope,
}

impl Overlay {
    /// Creates a new overlay with static content.
    ///
    /// # Arguments
    /// * `content` - The view to display as overlay content
    ///
    /// # Example
    /// ```rust
    /// use floem::views::{Overlay, Label};
    ///
    /// Overlay::new(Label::new("Static overlay content"));
    /// ```
    pub fn new(child: impl IntoView) -> Self {
        Self::with_id(ViewId::new(), child)
    }

    /// Creates a new overlay with a specific ViewId.
    ///
    /// This is useful when you need to control the ViewId for the overlay.
    ///
    /// # Arguments
    /// * `id` - The ViewId to use for this overlay
    /// * `content` - The view to display as overlay content
    pub fn with_id(id: ViewId, child: impl IntoView) -> Self {
        id.set_children([child.into_view()]);
        id.register_overlay();
        Overlay {
            id,
            child_scope: Scope::current(),
        }
    }

    /// Creates a new overlay with derived (reactive) content.
    ///
    /// The content function is called initially and re-called whenever
    /// its reactive dependencies change, replacing the overlay content.
    ///
    /// # Arguments
    /// * `content` - A function that creates the overlay content view
    ///
    /// # Example
    /// ```rust
    /// use floem::prelude::*;
    /// use floem::views::{Overlay, Label};
    ///
    /// let count = RwSignal::new(0);
    ///
    /// Overlay::derived(move || {
    ///     Label::derived(move || format!("Count: {}", count.get()))
    /// });
    /// ```
    pub fn derived<CF, IV>(content: CF) -> Self
    where
        CF: Fn() -> IV + 'static,
        IV: IntoView + 'static,
    {
        let id = ViewId::new();
        let content_fn = Box::new(Scope::current().enter_child(move |_| content().into_view()));

        let (child, child_scope) = UpdaterEffect::new(
            move || content_fn(()),
            move |(new_view, new_scope)| {
                let old_child = id.children();
                id.set_children([new_view]);
                id.update_state((old_child, new_scope));
            },
        );

        id.set_children([child]);
        id.register_overlay();
        Overlay { id, child_scope }
    }
}

impl View for Overlay {
    fn id(&self) -> ViewId {
        self.id
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "Overlay".into()
    }

    fn update(&mut self, cx: &mut UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(val) = state.downcast::<(Vec<ViewId>, Scope)>() {
            let old_child_scope = self.child_scope;
            let (old_children, child_scope) = *val;
            self.child_scope = child_scope;
            for child in old_children {
                cx.window_state.remove_view(child);
            }
            old_child_scope.dispose();
            self.id.request_all();
        }
    }
}
