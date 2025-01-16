//! # Floem
//! Floem is a cross-platform GUI library for Rust. It aims to be extremely performant while providing world-class developer ergonomics.
//!
//! The following is a simple example to demonstrate Floem's API and capabilities.
//!
//! ## Example: Counter
//! ```rust
//! use floem::prelude::*;
//!
//! let mut counter = RwSignal::new(0);
//!
//! v_stack((
//!     label(move || format!("Value: {counter}")),
//!     h_stack((
//!         button("Increment").action(move || counter += 1),
//!         button("Decrement").action(move || counter -= 1),
//!     )),
//! ));
//! ```
//! This example demonstrates the core concepts of building a reactive GUI with Floem:
//!
//! - State Management: The `RwSignal` provides a reactive way to manage the counter's state.
//! - Widgets: Common UI elements like `stack`, `label`, and `button` are easily implemented.
//! - Reactivity: The label automatically updates when the counter changes.
//! - Event Handling: Button clicks are handled with simple closures that modify the state.
//!
//! Floem's objectives prioritize simplicity and performance, enabling the development of complex graphical user interfaces with minimal boilerplate.
//!
//! ## Views
//! Floem models the UI using a tree of [Views](view::View). Views, such as the `h_stack`, `label`, and
//! `button` elements are the building blocks of UI in Floem.
//!
//! Floem's main view tree is constructed only once.
//! This guards against unnecessary and expensive rebuilds of your views;
//! however, even though the tree is built only once, views can still receive reactive updates.
//!
//! ### Composition and Flexibility
//! Views in Floem are composable, allowing for the construction of complex user interfaces by integrating simpler components.
//! In the counter example, label and button views were combined within vertical (`v_stack`) and horizontal (`h_stack`) layouts to create a more intricate interface.
//!
//! This compositional approach provides the following benefits:
//! - Reusable UI components
//! - Easy and consistent refactoring
//!
//! ### Learn More
//! Floem provides a set of built-in views to help you create UIs quickly.
//! To learn more about the built-in views, check out the [views module](crate::views) documentation.
//!
//! ## State management
//!
//! Floem uses a reactive system built on signals and effects for its state management.
//!
//! Floem uses its own reactive system with an API that is similar to the one in the [leptos_reactive](https://docs.rs/leptos_reactive/latest/leptos_reactive/index.html) crate.
//!
//! ### Signals as State
//!
//! You can create reactive state by creating a signal anywhere in the program using [`RwSignal::new()`](floem_reactive::RwSignal::new), [`RwSignal::new_split()`](floem_reactive::RwSignal::new_split), or use a [different signal type](floem_reactive).
//!
//! When you use a signal by calling the [`get`](floem_reactive::SignalGet::get) or [`with`](floem_reactive::SignalWith::with) methods, (which are also called when you use an operator such as `==`)
//! the runtime will automatically subscribe the correct side effects
//! to changes in that signal, creating reactivity. To the programmer this is transparent.
//! By simply accessing the value where you want to use it, the reactivity will "just work" and
//! your views will stay in sync with changes to that signal.
//!
//!
//! #### Example: Changing Text
//! ```rust
//! # use floem::reactive::*;
//! # use floem::views::*;
//! # use floem::IntoView;
//! fn app_view() -> impl IntoView {
//!
//!     // All signal types implement `Copy`, so they can be easily used without needing to manually clone them.
//!     let text = RwSignal::new("Hello, World!".to_string());
//!
//!     let label_view = label(move || text.get());
//!
//!     let button = button("Change Text").action(move || text.set("Hello, Floem!".to_string()));
//!
//!     v_stack((button, label_view))
//! }
//! ```
//!
//! In this example, `text` is a signal containing a `String` that can be both read from and written to.
//! The button, when clicked, changes the text in the signal.
//! The label view is subscribed to changes in the `text` signal and will automatically trigger a re-render with the updated text value whenever the signal changes.
//!
//! ### Functions as a Primitive of Reactivity
//!
//! The most fundamental primitive of reactivity is a function that can be re-run in response to changes in a signal.
//! For this reason, many of Floem's APIs accept functions as arguments (such as in the label view in the `Changing Text` example above).
//! Most of the functions is Floem's API will
//! update reactively, but not all of them do. For this reason, all arguments in Floem's API that are functions will
//! be marked with a `# Reactivity` section that will inform you if the function will be re-run in response to reactive updates.
//!
//! ### Learn More
//! To learn more about signals and
//! effects, you may want to explore the Leptos [documentation](https://docs.rs/leptos_reactive/latest/leptos_reactive/index.html)
//! and the [leptos book](https://leptos-rs.github.io/leptos/).
//!
//! ## Style: Customizing Appearance
//!
//! Floem has a powerful, built-in styling system that allows you to customize the appearance of your UI.
//!
//! Example:
//! ```
//! #  use floem::peniko::color::palette;
//! #  use floem::reactive::*;
//! #  use floem::style::Style;
//! #  use floem::unit::UnitExt;
//! #  use floem::View;
//! #  use floem::views::{text, Decorators};
//! #
//! text("Some text").style(|s| s.font_size(21.).color(palette::css::DARK_GRAY));
//! ```
//!
//! The text view is styled by calling the [`style`](crate::views::Decorators::style) method (you'll need to import the
//! [`Decorators`](crate::views::Decorators) trait to use the it). The `style` method takes a closure that takes and returns a
//! [`Style`](crate::style::Style) value using the builder pattern. Through this value, you can access methods that modify a variety
//! of familiar properties such as width, padding, and background. Some `Style` properties
//! such as font size are `inherited` and will apply to all of a view's children until overridden.
// TODO: Add links on these
//!
//! In this same style value, floem supports:
//! - themeing with [classes](style::Style::class)
//! - [property transitions](style::Style::transition)
//! - defining styles on different [interaction states](style::Style::hover)
//! - reactive updates
//! - applying styles [conditionally](style::Style::apply_if)
//! - setting custom properties
//! - and more
//!
//! For additional information about styling, [see here](crate::style).
//!
//! ## Animation
//!
//! In addition to [property transitions](style::Style::transition) that can be added to `Style`s,
//! Floem has a full keyframe animation system that allows you to animate any property that can be [interpolated](style::StylePropValue::interpolate) and builds on the capabilities and ergonomics of the style system.
//!
//! Animations in Floem, by default, have keyframes ranging from 0-100.
//!
//! #### Example: Rectangle to Square
//!
//! ```
//! #  use floem::peniko::color::palette;
//! #  use floem::reactive::*;
//! #  use floem::style::Style;
//! #  use floem::unit::{UnitExt, DurationUnitExt};
//! #  use floem::View;
//! #  use floem::views::*;
//! #
//! empty()
//!     .style(|s| s.background(palette::css::RED).size(500, 100))
//!     .animation(move |a| {
//!         a.duration(5.seconds())
//!             .keyframe(0, |f| f.computed_style())
//!             .keyframe(50, |f| {
//!                 f.style(|s| s.background(palette::css::BLACK).size(30, 30))
//!                     .ease_in()
//!             })
//!             .keyframe(100, |f| {
//!                 f.style(|s| s.background(palette::css::AQUAMARINE).size(10, 300))
//!                     .ease_out()
//!             })
//!             .auto_reverse(true)
//!             .repeat(true)
//!     });
//! ```
//! - The first keyframe will use the computed style, which will include the red background with size of 500x100.
//! - At 50%, the animation will animate to a black square of 30x30 with a bezier easing of `ease_in`.
//! - At 100% the animation will animate to an aquamarine rectangle of 10x300 with an bezier easing of `ease_out`.
//! - The animation is also set to automatically reverse (the animation will run and reverse in 5 seconds) and repeat forever.
//!
//! You can add animations to a `View` instance by calling the [`animation`](crate::views::Decorators::animation) method from the `Decorators` trait.
//! The `animation` method takes a closure that takes and returns an [`Animation`](crate::animate::Animation) value using the builder pattern.
//!
//! For additional information about animation, [see here](crate::animate::Animation).

pub mod action;
pub mod animate;
mod app;
#[cfg(target_os = "macos")]
mod app_delegate;
mod app_handle;
pub(crate) mod app_state;
#[cfg(feature = "vello")]
mod border_path_iter;
mod clipboard;
pub mod context;
pub mod dropped_file;
pub mod easing;
pub mod event;
pub mod ext_event;
pub mod file;
#[cfg(any(feature = "rfd-async-std", feature = "rfd-tokio"))]
pub mod file_action;
pub(crate) mod id;
mod inspector;
pub mod keyboard;
pub mod menu;
mod nav;
pub mod pointer;
mod profiler;
mod renderer;
pub mod responsive;
mod screen_layout;
pub mod style;
pub(crate) mod theme;
pub mod touchpad;
pub mod unit;
mod update;
pub(crate) mod view;
pub(crate) mod view_state;
pub(crate) mod view_storage;
pub mod view_tuple;
pub mod views;
pub mod window;
mod window_handle;
mod window_id;
mod window_tracking;

pub use app::{launch, quit_app, reopen, AppConfig, AppEvent, Application};
pub use app_state::AppState;
pub use clipboard::{Clipboard, ClipboardError};
pub use ext_event::resource::Resource;
pub use floem_reactive as reactive;
pub use floem_renderer::gpu_resources::GpuResources;
pub use floem_renderer::text;
pub use floem_renderer::Renderer;
pub use floem_renderer::Svg as RendererSvg;
pub use id::ViewId;
pub use muda;
pub use peniko;
pub use peniko::kurbo;
pub use screen_layout::ScreenLayout;
pub use taffy;
pub use view::{recursively_layout_view, AnyView, IntoView, View};
pub use window::{close_window, new_window};
pub use window_id::{Urgency, WindowIdExt};

pub mod prelude {
    pub use crate::unit::{DurationUnitExt, UnitExt};
    pub use crate::view_tuple::ViewTuple;
    pub use crate::views::*;
    pub use crate::Renderer;
    pub use crate::{IntoView, View};
    pub use floem_reactive::{
        create_rw_signal, create_signal, RwSignal, SignalGet, SignalTrack, SignalUpdate, SignalWith,
    };
    pub use peniko::color::palette;
    pub use peniko::Color;
}
