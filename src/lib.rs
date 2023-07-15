//! # Floem
//! Floem is cross-platform GUI framework for Rust 🦀. It aims to be extremely performant while providing world-class developer ergonomics.
//!
//! ## Views
//! Floem models the UI using a tree of [Views](view::View) that are built once. `Views` react to state changes and events.
//! Views ar self-contained components that can be composed together to create complex UIs.
//!
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
//! You may want some of your view components to share state.
//! You should place any state that changes over time and affects
//! your view inside a signal so that you can react to updates and update the `View`. Signals are reactive values that can be read from and written to.
//! See [leptos_reactive](https://docs.rs/leptos_reactive/latest/leptos_reactive/) for more info.
//!
//! ### Local state
//!
//! You can can create state within a constructor and then bind state to a view by passing it into the view's constructor.
//!
//! ```ignore
//! pub fn label_and_input() -> impl View {
//!     let cx = ViewContext::get_current();
//!     let text = create_rw_signal(cx.scope, "Hello world".to_string());
//!     stack(|| (text_input(text), label(|| text.get())))
//!         .style(|| Style::BASE.padding_px(10.0))
//! }
//! ```
//!
//! ### Global state
//!
//! Global state can be implemented using Leptos' [provide_context](leptos_reactive::provide_context) and [use_context](leptos_reactive::use_context).
//!
//! ## Styling
//! You can style your views using the [Style](style::Style) struct. Styles are inherited from parent views and can be overridden.
//! Floem sizing and positioning layout system is based on the flexbox model using Taffy as the layout engine.
//!
//! Styles are applied with the [Style](style::Style) struct. Styles can react to changes in referenced state.
//!
//! ```ignore
//!     some_view()
//!     // reactive styles provided through the `Decorators` trait
//!     .style(move || {
//!         Style::BASE
//!             .flex_row()
//!             .width_pct(100.0)
//!             .height_px(32.0)
//!             .border_bottom(1.0)
//!             .border_color(Color::LIGHT_GRAY)
//!             .apply_if(index == active_tab.get(), |s| {
//!                 s.background(Color::GRAY)
//!             })
//!     })
//! ```
//!
//!
//! ## Render loop and update lifecycle
//!
//! #### event -> update -> layout -> paint.
//!
//! ##### Event
//! After an event comes in (e.g. the user clicked the mouse, pressed a key etc), the event will be propagated from the root view to the children.
//! The parent will decide whether sending the event to the child(ren) based on the logic in the event method in the parent View.
//! There's also event listeners that users can use to respond to events the users choose.
//! The event propagation is stopped whenever a child or an event listener returns `true` on the event handling.
//!
//!
//! #### Event handling -> reactive system updates
//! During the event handling, there could be state changes through the reactive system. E.g., on the counter example, when you click increment,
//! it updates the counter and because the label listens to the change (see [leptos_reactive::create_effect]), the label will update the text it presents.
//!
//! #### Update
//! The update of states on the Views could cause some of them to need a new layout recalculation, because the size might have changed etc.
//! The reactive system can't directly manipulate the view state of the label because the AppState owns all the views. And instead, it will send the update to a message queue via [Id::update_state](id::Id::update_state)
//! After the event propagation is done, Floem will process all the update messages in the queue, and it can manipulate the state of a particular view through the update method.
//!
//!
//! #### Layout
//! The layout method is called from the root view to re-layout the views that have requested a layout call.
//! The layout call is to change the layout properties at Taffy, and after the layout call is done, compute_layout is called to calculate the sizes and positions of each view.
//!
//! #### Paint
//! And in the end, paint is called to render all the views to the screen.
//!
//!
//! # Terminology
//! #### Active view
//!
//! Affects pointer events. Pointer events will only be sent to the active View. The View will continue to receive pointer events even if the mouse is outside its bounds.
//! It is useful when you drag things, e.g. the scroll bar, you set the scroll bar active after pointer down, then when you drag, the `PointerMove` will always be sent to the View, even if your mouse is outside of the view.
//!
//! #### Focused view
//! Affects keyboard events. Keyboard events will only be sent to the focused View. The View will continue to receive keyboard events even if it's not the active View.
//!
//! ## Notable invariants and tolerances
//! - There can be only one root `View`
//! - Only one view can be active at a time.
//! - Only one view can be focused at a time.
//!
//!
//! # Advanced
//!
//! #### Authoring your own [Views](view::View)
//! See the [View module](view) for more info.
//!
//! #### Understanding Ids
//! See the [Id module](id) for more info.
//!
//! #### Understanding Styles
//! See the [Style module](style) for more info.
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
pub use app_handle::ViewContext;
pub use floem_renderer::cosmic_text;
pub use floem_renderer::Renderer;
pub use glazier;
pub use leptos_reactive as reactive;
pub use peniko;
pub use taffy;
