//! # Floem
//! Floem is cross-platform GUI framework for Rust 🦀. It aims to be extremely performant while providing world-class developer ergonomics.
//!
//! ## Views
//! Floem models the UI using a tree of [Views](view::View)s that are built once. `Views` react to state changes and events.
//! Views are type-erased, self-contained components that can be composed together to create complex UIs.
//!
//! ## Ids and Id paths
//! [Id](id::Id)s are unique identifiers for views. They're used to identify views in the view tree. These ids are assigned via the [AppContext](context::AppContext) and are unique across the entire application.
//!
//! ## Events
//! Events are passed down from the window to the view tree. Each view can handle events and propagate them to its children.
//! Events can be passed to a specific view using its [Id](id::Id) path. [Id](id::Id) paths are unique ways to quickly identify a view, its children and its parent in the tree.
//!
//! Floem is responsible for making a view active. for handling drag and drop mechanics
//! It's a View's responsibility to handle events and decide whether they're handled.
//!
//! ## State management  
//!
//! You will probably want your view components to have some state. You should place any state that affects
//! you view inside a signal so that you can react to updates and update the `View`. Signals are reactive values that can be read from and written to.
//!
//! To affect the layout and rendering of your component, you will need to send a state update to your component with [Id::update_state](id::Id::update_state)
//! and then call [UpdateCx::request_layout](context::UpdateCx::request_layout) to request a layout which will cause a repaint.
//!
//! To share state between components, you can simply pass down a signal to your children. Here's a contrived example:
//!
//!```rust,no_run
//! // super contrived example showing how to pass down state to children.
//!
//! struct Parent<V> {
//!     id: Id,
//!     text: ReadSignal<String>,
//!     child: V,
//! }
//!
//! // Creates a new parent view with the given child.
//! fn parent<V>(new_child: impl FnOnce(ReadSignal<String>) -> V) -> Parent<impl View>
//! where
//!     V: View + 'static,
//! {
//!     let text = create_rw_signal(cx.scope, "World!".to_string());
//!     // share the signal between the two children
//!     let (id, child) = AppContext::new_id_with_child(stack(|| (text_input(text)), new_child(text.read_only()));
//!     Parent { id, text, child }
//! }
//!
//! impl<V> View for Parent<V>
//! where
//!     V: View,
//! {
//! // implementation omitted for brevity
//! }
//!
//! struct Child {
//!     id: Id,
//!     label: Label,
//! }
//!
//! /// Creates a new child view with the given state (a read only signal)
//! fn child(text: ReadSignal<String>) -> Child {
//!     let (id, label) = AppContext::new_id_with_child(|| label(move || format!("Hello, {}", text.get()));
//!     Child { id, label }
//! }
//!
//! impl View for Child {
//!   // implementation omitted for brevity
//! }
//!
//! // Usage
//! fn main() {
//!     floem::launch(parent(child));
//! }
//!
//! ```
//!
//! Global state TBD
//!
//! # Styling
//! Floem sizing and positioning layout system is based on the flexbox model using Taffy as the layout engine.
//! TBD on how visual styles work
//!
//! # Style  
//! Styles are divided into two parts:
//! [`ComputedStyle`]: A style with definite values for most fields.  
//!
//! [`Style`]: A style with [`StyleValue`]s for the fields, where `Unset` falls back to the relevant
//! field in the [`ComputedStyle`] and `Base` falls back to the underlying [`Style`] or the
//! [`ComputedStyle`].
//!
//!
//!
//!
//! ## Render loop and update lifecycle
//! TBD: event -> update -> layout -> paint.
//!
//! potentially discuss update messages, deferred update messages, animations
//!
//! ## Notable invariants and tolerances
//! - There can be only one root `View`
//! - Only one view can be active at a time.
//! - Only one view can be focused at a time.
//! - Any other important invariants?
//!
//!
//!
//!
//!
//! # Terminology
//! - **Active view**: The view that can receive mouse events even if the mouse is outside its bounds.
//!
//! - **Focused view**: The view that is currently focused. It's the view that has focus and is receiving keyboard events.
//!
//!
//!
pub mod animate;
mod app;
mod app_handle;
pub mod context;
pub mod event;
pub mod ext_event;
pub mod id;
pub mod menu;
pub mod renderer;
pub mod responsive;
pub mod style;
pub mod view;
pub mod view_tuple;
pub mod views;
pub mod window;

pub use app::{launch, AppEvent, Application};
pub use app_handle::AppContext;
pub use floem_renderer::cosmic_text;
pub use floem_renderer::Renderer;
pub use glazier;
pub use leptos_reactive as reactive;
pub use taffy;
pub use vello::peniko;
