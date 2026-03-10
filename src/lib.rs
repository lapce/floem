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
pub mod context;
pub mod event;
pub mod ext_event;
mod inspector;
pub mod layout;
mod message;
pub mod paint;
pub mod platform;
pub mod text;
/// Re-export easing module from animate for backward compatibility.
pub use animate::easing;
/// Re-export dropped_file module from event for backward compatibility.
pub use event::dropped_file;
/// Re-export responsive module from layout for backward compatibility.
pub use layout::responsive;
/// Re-export file module from platform for backward compatibility.
pub use platform::file;
/// Re-export menu module from platform for backward compatibility.
pub use platform::menu;
/// Re-export view_tuple module from view for backward compatibility.
pub use view::tuple as view_tuple;
pub mod headless;
pub mod style;
pub mod view;
pub mod views;
pub mod window;
pub mod receiver_signal {
    //! Signals from Channels, Futures, and Streams.
    mod channel_signal;
    mod common;
    mod future_signal;
    mod resource;
    mod stream_signal;
    pub use channel_signal::*;
    pub use common::*;
    pub use future_signal::*;
    pub use resource::*;
    pub use stream_signal::*;
}

mod element_id {
    use crate::ViewId;

    /// A visual identifier that represents a rectangle in the box tree.
    ///
    /// # ViewId vs ElementId Relationship
    ///
    /// **ViewId** represents a logical view in the view tree (1:1 with View instances).
    /// **ElementId** represents a visual rectangle in the box tree (can be many per View).
    ///
    /// ## Key Relationships:
    /// - Each **View** has exactly one primary **ViewId** (1:1)
    /// - Each **View** can create multiple **ElementIds** for sub-widget rectangles (1:many)
    ///   - Example: A scroll view creates VisualIds for content area, vertical scrollbar, horizontal scrollbar
    /// - Each **VisualId** maps back to exactly one **ViewId** for event routing (many:1)
    ///   - Call `element_id.view_id()` to get the owning ViewId
    ///
    /// ## Usage:
    /// - **Hit testing** operates on VisualIds (tests against individual rectangles in box tree)
    /// - **Event handling** happens on ViewIds (the view receives events with target VisualId)
    /// - **Painting** iterates through VisualIds in z-index order from the box tree
    /// - **View hierarchy** uses ViewIds for parent/child relationships
    ///
    /// ## Structure:
    /// - `.0`: The box tree NodeId (identifies the rectangle in the spatial index)
    /// - `.1`: The owning ViewId (identifies which view this rectangle belongs to)
    ///
    /// ## Example:
    /// ```ignore
    /// // A scroll view might create these VisualIds:
    /// let scroll_view_id = ViewId::new();
    /// let content_element_id = VisualId(node_id_1, scroll_view_id);     // content area
    /// let vscroll_element_id = VisualId(node_id_2, scroll_view_id);     // vertical scrollbar
    /// let hscroll_element_id = VisualId(node_id_3, scroll_view_id);     // horizontal scrollbar
    ///
    /// // All three VisualIds route events to the same scroll_view_id:
    /// assert_eq!(content_element_id.view_id(), scroll_view_id);
    /// assert_eq!(vscroll_element_id.view_id(), scroll_view_id);
    /// assert_eq!(hscroll_element_id.view_id(), scroll_view_id);
    ///
    /// // But hit testing can distinguish which specific rectangle was hit
    /// ```
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    // #[repr(transparent)]
    pub struct ElementId(
        pub(crate) understory_box_tree::NodeId,
        pub(crate) ViewId,
        pub(crate) bool,
    );
    impl ElementId {
        pub fn owning_id(&self) -> crate::ViewId {
            self.1
        }

        /// returns true if the element id is the id for a view
        pub fn is_view(&self) -> bool {
            self.2
        }
    }

    /// Per-element focus navigation metadata kept in the box tree.
    ///
    /// This is intentionally lightweight (`Copy`) so event dispatch can read it
    /// with minimal overhead while building transient focus spaces.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct FocusNavMeta {
        /// Optional explicit linear order key (used for tab traversal).
        pub order: Option<i32>,
        /// Optional logical group for policy-aware navigation.
        pub group: Option<understory_focus::FocusSymbol>,
        /// Optional policy selection hint for host-level policy switching.
        pub policy_hint: Option<understory_focus::FocusSymbol>,
        /// Depth within an app-defined focus scope.
        pub scope_depth: u8,
        /// Preferred initial focus candidate for a scope.
        pub autofocus: bool,
        /// Additional enable/disable gate independent of style flags.
        pub enabled: bool,
    }

    /// Metadata stored per box tree node.
    ///
    /// Keeps the `ElementId` used by hit-testing/event routing plus optional
    /// navigation hints for high-quality keyboard focus behavior.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ElementMeta {
        pub element_id: ElementId,
        pub focus: FocusNavMeta,
    }

    impl ElementMeta {
        pub const fn new(element_id: ElementId) -> Self {
            Self {
                element_id,
                focus: FocusNavMeta {
                    order: None,
                    group: None,
                    policy_hint: None,
                    scope_depth: 0,
                    autofocus: false,
                    enabled: true,
                },
            }
        }
    }
}
pub use element_id::ElementId;
pub use element_id::{ElementMeta, FocusNavMeta};

pub type BoxTree = understory_box_tree::Tree<understory_index::backends::GridF64, ElementMeta>;
// pub type BoxTree = understory_box_tree::Tree;

static FOCUS_NAV_META_REVISION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

pub(crate) fn bump_focus_nav_meta_revision() {
    FOCUS_NAV_META_REVISION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

pub(crate) fn focus_nav_meta_revision() -> u64 {
    FOCUS_NAV_META_REVISION.load(std::sync::atomic::Ordering::Relaxed)
}

pub use app::{AppConfig, AppEvent, Application, launch, quit_app, reopen};
pub use floem_reactive as reactive;
pub use floem_renderer::Renderer;
pub use floem_renderer::Svg as RendererSvg;
pub use floem_renderer::gpu_resources::GpuResources;
pub use imbl;
pub use layout::ScreenLayout;
#[cfg(not(target_arch = "wasm32"))]
pub use muda;
pub use peniko;
pub use peniko::kurbo;
#[cfg(not(target_arch = "wasm32"))]
pub use platform::open_file;
#[cfg(not(target_arch = "wasm32"))]
pub use platform::save_as;
pub use platform::{Clipboard, ClipboardError, FileDialogOptions, FileInfo, FileSpec};
pub use platform::{Menu, SubMenu};
pub use taffy;
pub use ui_events;
pub use understory_focus;
pub use view::ViewId;
pub use view::{AnyView, HasViewId, IntoView, LazyView, ParentView, View};
pub use view::{Stack, StackOffset};
pub use window::{Urgency, WindowIdExt, WindowState, close_window, new_window};

/// Re-export unit and theme modules from style for backward compatibility.
pub use style::{theme, unit};

pub mod prelude {
    pub use crate::Renderer;
    pub use crate::event::listener as el;
    pub use crate::event::listener;
    pub use crate::event::listener::EventListenerTrait;
    pub use crate::unit::{DurationUnitExt, UnitExt};
    pub use crate::view::IntoViewIter;
    pub use crate::view::ViewTuple;
    pub use crate::views::*;
    pub use crate::{HasViewId, IntoView, ParentView, View};
    #[allow(deprecated)]
    pub use floem_reactive::{
        RwSignal, SignalGet, SignalTrack, SignalUpdate, SignalWith, create_rw_signal, create_signal,
    };
    pub use palette::css;
    pub use peniko::Color;
    pub use peniko::color::palette;
    pub use ui_events::{
        keyboard::{Code, Key, KeyState, KeyboardEvent, Modifiers, NamedKey},
        pointer::{PointerButtonEvent, PointerEvent},
    };
}
