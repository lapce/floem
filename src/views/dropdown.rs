#![deny(missing_docs)]
//! A view that allows the user to select an item from a list of items.
//!
//! The [`Dropdown`] struct provides several constructors, each offering different levels of customization and ease of use.
//!
//! The [`DropdownCustomStyle`] struct allows for easy and advanced customization of the dropdown's appearance.
use std::{any::Any, rc::Rc};

use floem_reactive::{Effect, RwSignal, Scope, SignalGet, SignalUpdate, UpdaterEffect};
use imbl::OrdMap;
use peniko::kurbo::{Point, Size};

use crate::{
    AnyView,
    action::{add_overlay, remove_overlay},
    context::{Phases, VisualChangedListener},
    custom_event,
    event::{DispatchKind, Event, EventPropagation, Phase, listener},
    prelude::{EventListenerTrait, ViewTuple},
    prop, prop_extractor,
    style::{CustomStylable, CustomStyle, Style, StyleClass},
    style_class,
    view::{IntoView, View, ViewId},
    views::{ContainerExt, Decorators, Label, ScrollExt, svg},
};

use super::list;

type ChildFn<T> = dyn Fn(T) -> (AnyView, Scope);

style_class!(
    /// A Style class that is applied to all dropdowns.
    pub DropdownClass
);

style_class!(
    /// A Style class that is applied to all dropdown previews.
    pub DropdownPreviewClass
);

prop!(
    /// A property that determines whether the dropdown should close automatically when an item is selected.
    pub CloseOnAccept: bool {} = true
);
prop_extractor!(DropdownStyle {
    close_on_accept: CloseOnAccept,
});

/// Event fired when the dropdown open state changes
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DropdownOpenChanged {
    /// Whether the dropdown is now open
    pub is_open: bool,
}
impl DropdownOpenChanged {
    fn extract(&self) -> Option<&bool> {
        Some(&self.is_open)
    }
}
custom_event!(DropdownOpenChanged, bool, DropdownOpenChanged::extract);

/// Event fired when an item is accepted/selected from the dropdown.
/// Contains the selected value.
///
/// Note: Prefer using [`Dropdown::on_accept`] instead of listening for this event directly,
/// as it provides properly typed access to the selected value.
///
/// If you instead manually specify the incorrect type, a downcast will fail and your handler will not run.
#[derive(Debug, Clone, PartialEq)]
pub struct DropdownAccept<T: 'static> {
    /// The accepted value
    pub value: T,
}
custom_event!(DropdownAccept<T>);

/// # A customizable dropdown view for selecting an item from a list.
///
/// The `Dropdown` struct provides several constructors, each offering different levels of
/// customization and ease of use:
///
/// - [`Dropdown::new_rw`]: The simplest constructor, ideal for quick setup with minimal customization.
///   It uses default views and assumes direct access to a signal that can be both read from and written to for driving the selection of an item.
///
/// - [`Dropdown::new`]: Similar to `new_rw`, but uses a read-only function for the active item, and requires that you manually provide an `on_accept` callback.
///
/// - [`Dropdown::custom`]: Offers full customization, letting you define custom view functions for
///   both the main display and list items. Uses a read-only function for the active item and requires that you manually provide an `on_accept` callback.
///
/// - The dropdown also has methods [`Dropdown::main_view`] and [`Dropdown::list_item_view`] that let you override the main view function and list item view function respectively.
///
/// Choose the constructor that best fits your needs based on the level of customization required.
///
/// ## Usage with Enums
///
/// A common scenario is populating a dropdown menu from an enum. The `widget-gallery` example does this.
///
/// The below example creates a dropdown with three items, one for each character in our `Character` enum.
///
/// The `strum` crate is handy for this use case. This example uses the `strum` crate to create an iterator for our `Character` enum.
///
/// First, define the enum and implement `Clone`, `strum::EnumIter`, and `Display` on it:
/// ```rust
/// use strum::IntoEnumIterator;
///
/// #[derive(Clone, strum::EnumIter)]
/// enum Character {
///     Ori,
///     Naru,
///     Gumo,
/// }
///
/// impl std::fmt::Display for Character {
///     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
///         match self {
///             Self::Ori => write!(f, "Ori"),
///             Self::Naru => write!(f, "Naru"),
///             Self::Gumo => write!(f, "Gumo"),
///         }
///     }
/// }
/// ```
///
/// Then, create a signal:
/// ```rust
/// # use strum::IntoEnumIterator;
/// #
/// # #[derive(Clone, strum::EnumIter)]
/// # enum Character {
/// #     Ori,
/// #     Naru,
/// #     Gumo,
/// # }
/// #
/// # impl std::fmt::Display for Character {
/// #     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
/// #         match self {
/// #             Self::Ori => write!(f, "Ori"),
/// #             Self::Naru => write!(f, "Naru"),
/// #             Self::Gumo => write!(f, "Gumo"),
/// #         }
/// #     }
/// # }
/// #
/// # use floem::reactive::RwSignal;
/// let selected = RwSignal::new(Character::Ori);
/// ```
///
/// Finally, create the dropdown using one of the available constructors, like [`Dropdown::new_rw`]:
///
/// ```rust
/// # use strum::IntoEnumIterator;
/// #
/// # #[derive(Clone, strum::EnumIter, PartialEq)]
/// # enum Character {
/// #     Ori,
/// #     Naru,
/// #     Gumo,
/// # }
/// #
/// # impl std::fmt::Display for Character {
/// #     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
/// #         match self {
/// #             Self::Ori => write!(f, "Ori"),
/// #             Self::Naru => write!(f, "Naru"),
/// #             Self::Gumo => write!(f, "Gumo"),
/// #         }
/// #     }
/// # }
/// #
/// # fn character_select() -> impl floem::IntoView {
/// #     use floem::{prelude::*, views::dropdown::Dropdown};
/// # let selected = RwSignal::new(Character::Ori);
/// Dropdown::new_rw(selected, Character::iter())
/// # }
/// ```
///
/// ## Styling
///
/// You can modify the behavior of the dropdown through the `CloseOnAccept` property.
/// If the property is set to `true`, the dropdown will automatically close when an item is selected.
/// If the property is set to `false`, the dropdown will not automatically close when an item is selected.
/// The default is `true`.
/// Styling Example:
/// ```rust
/// # use floem::views::dropdown;
/// # use floem::views::empty;
/// # use floem::views::Decorators;
/// // root view
/// empty().style(|s| {
///     s.class(dropdown::DropdownClass, |s| {
///         s.set(dropdown::CloseOnAccept, false)
///     })
/// });
/// ```
pub struct Dropdown<T: 'static> {
    id: ViewId,
    current_value: T,
    main_view: ViewId,
    main_view_scope: Scope,
    main_fn: Box<ChildFn<T>>,
    list_item_fn: Rc<dyn Fn(&T) -> AnyView>,
    overlay_id: Option<ViewId>,
    window_origin: Option<Point>,
    style: DropdownStyle,
    index_to_item: OrdMap<usize, T>,
    width: RwSignal<f64>,
}

enum Message {
    OpenState(bool),
    ActiveElement(Box<dyn Any>),
    ListFocusLost,
    ListSelect(Box<dyn Any>),
}

impl<T: 'static + Clone + PartialEq + core::fmt::Debug> View for Dropdown<T> {
    fn id(&self) -> ViewId {
        self.id
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "Dropdown".into()
    }

    fn style_pass(&mut self, cx: &mut crate::context::StyleCx<'_>) {
        if self.style.read(cx) {
            cx.window_state.request_paint(self.id);
        }
    }

    fn update(&mut self, cx: &mut crate::context::UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(state) = state.downcast::<Message>() {
            match *state {
                Message::OpenState(true) => self.open_dropdown(),
                Message::OpenState(false) => self.close_dropdown(),
                Message::ListFocusLost => self.close_dropdown(),
                Message::ListSelect(val) => {
                    if let Ok(val) = val.downcast::<T>() {
                        if self.style.close_on_accept() {
                            self.close_dropdown();
                        }
                        self.id.dispatch_event(
                            Event::new_custom(DropdownAccept { value: *val }),
                            DispatchKind::Directed {
                                target: self.id.get_element_id(),
                                phases: Phases::TARGET,
                            },
                        );
                    }
                }
                Message::ActiveElement(val) => {
                    if let Ok(val) = val.downcast::<T>() {
                        let old_child_scope = self.main_view_scope;
                        let old_main_view = self.main_view;
                        self.current_value = *val.clone();
                        let (main_view, main_view_scope) = (self.main_fn)(*val);
                        let main_view_id = main_view.id();
                        main_view_id.add_class(DropdownPreviewClass::class_ref());
                        self.id.set_children([main_view]);
                        self.main_view = main_view_id;
                        self.main_view_scope = main_view_scope;

                        cx.window_state.remove_view(old_main_view);
                        old_child_scope.dispose();
                        self.id.request_all();
                    }
                }
            }
        }
    }

    fn event(&mut self, cx: &mut crate::context::EventCx) -> EventPropagation {
        if cx.phase == Phase::Target {
            if let Some(new_vis) = VisualChangedListener::extract(&cx.event) {
                self.window_origin = Some(new_vis.new_visual_aabb.origin());
                self.width.set(new_vis.new_visual_aabb.width());
            }
        }

        if (cx.phase != Phase::Capture && cx.event.is_pointer_down())
            || (cx.phase == Phase::Target && cx.event.is_keyboard_trigger())
        {
            self.swap_state();
            return EventPropagation::Stop;
        }

        EventPropagation::Continue
    }
}

impl<T: Clone + std::cmp::PartialEq + std::fmt::Debug> Dropdown<T> {
    /// Creates a default main view for the dropdown.
    ///
    /// This function generates a view that displays the given item as text,
    /// along with a chevron-down icon to indicate that it's a dropdown.
    pub fn default_main_view(item: T) -> AnyView
    where
        T: std::fmt::Display,
    {
        const CHEVRON_DOWN: &str = r##"
            <svg xmlns="http://www.w3.org/2000/svg" xml:space="preserve" viewBox="-46.336 -46.336 278.016 278.016">
                <path fill="#010002" d="M92.672 144.373a10.707 10.707 0 0 1-7.593-3.138L3.145 59.301c-4.194-4.199
                -4.194-10.992 0-15.18a10.72 10.72 0 0 1 15.18 0l74.347 74.341 74.347-74.341a10.72 10.72 0 0 1
                15.18 0c4.194 4.194 4.194 10.981 0 15.18l-81.939 81.934a10.694 10.694 0 0 1-7.588 3.138z"/>
            </svg>
        "##;

        // TODO: this should be more customizable
        (
            Label::new(item),
            svg(CHEVRON_DOWN).style(|s| s.items_center()),
        )
            .h_stack()
            .style(|s| s.items_center().justify_between().size_full())
            .into_any()
    }

    /// Creates a new customizable dropdown.
    ///
    /// You might want to use some of the simpler constructors like [`Dropdown::new`] or [`Dropdown::new_rw`].
    ///
    /// # Example
    /// ```rust
    /// # use floem::{*, views::*, reactive::*};
    /// # use floem::views::dropdown::*;
    /// let active_item = RwSignal::new(3);
    ///
    /// Dropdown::custom(
    ///     move || active_item.get(),
    ///     |main_item| text(main_item).into_any(),
    ///     1..=5,
    ///     |list_item| text(list_item).into_any(),
    /// )
    /// .on_accept(move |item| active_item.set(item));
    /// ```
    ///
    /// This function provides full control over the dropdown's appearance and behavior
    /// by allowing custom view functions for both the main display and list items.
    ///
    /// # Arguments
    ///
    /// * `active_item` - A function that returns the currently selected item.
    ///
    /// * `main_view` - A function that takes a value of type `T` and returns an `AnyView`
    ///   to be used as the main dropdown display.
    ///
    /// * `iterator` - An iterator that provides the items to be displayed in the dropdown list.
    ///
    /// * `list_item_fn` - A function that takes a value of type `T` and returns an `AnyView`
    ///   to be used for each item in the dropdown list.
    pub fn custom<MF, I, LF, AIF>(
        active_item: AIF,
        main_view: MF,
        iterator: I,
        list_item_fn: LF,
    ) -> Dropdown<T>
    where
        MF: Fn(T) -> AnyView + 'static,
        I: IntoIterator<Item = T> + 'static,
        LF: Fn(&T) -> AnyView + Clone + 'static,
        T: PartialEq + Clone + 'static,
        AIF: Fn() -> T + 'static,
    {
        let dropdown_id = ViewId::new();
        dropdown_id.register_listener(VisualChangedListener::listener_key());

        // Process the iterator once, building a map from indices to items
        let mut index_to_item = OrdMap::new();

        for (idx, item) in iterator.into_iter().enumerate() {
            index_to_item.insert(idx, item);
        }

        let list_item_fn = Rc::new(list_item_fn);

        let initial = UpdaterEffect::new(active_item, move |new_state| {
            dropdown_id.update_state(Message::ActiveElement(Box::new(new_state)));
        });

        let main_fn = Box::new(Scope::current().enter_child(main_view));
        let (child, main_view_scope) = main_fn(initial.clone());
        let main_view = child.id();
        main_view.add_class(DropdownPreviewClass::class_ref());

        dropdown_id.set_children([child]);

        Self {
            id: dropdown_id,
            current_value: initial,
            main_view,
            main_view_scope,
            main_fn,
            list_item_fn,
            index_to_item,
            overlay_id: None,
            window_origin: None,
            style: Default::default(),
            width: RwSignal::new(0.),
        }
        .class(DropdownClass)
    }

    /// Creates a new dropdown with a read-only function for the active item.
    ///
    /// # Example
    /// ```rust
    /// # use floem::{*, views::*, reactive::*};
    /// # use floem::views::dropdown::*;
    /// let active_item = RwSignal::new(3);
    ///
    /// Dropdown::new(move || active_item.get(), 1..=5).on_accept(move |val| active_item.set(val));
    /// ```
    ///
    /// This function is a convenience wrapper around `Dropdown::new` that uses default views
    /// for the main and list items.
    ///
    /// See also [`Dropdown::new_rw`].
    ///
    /// # Arguments
    ///
    /// * `active_item` - A function that returns the currently selected item.
    ///
    /// * `iterator` - An iterator that provides the items to be displayed in the dropdown list.
    pub fn new<AIF, I>(active_item: AIF, iterator: I) -> Dropdown<T>
    where
        AIF: Fn() -> T + 'static,
        I: IntoIterator<Item = T> + 'static,
        T: Clone + PartialEq + std::fmt::Display + 'static,
    {
        Self::custom(active_item, Self::default_main_view, iterator, |v| {
            crate::views::Label::new(v).into_any()
        })
    }

    /// Creates a new dropdown with a read-write signal for the active item.
    ///
    /// # Example:
    /// ```rust
    /// # use floem::{*, views::*, reactive::*};
    /// # use floem::{views::dropdown::*};
    /// let dropdown_active_item = RwSignal::new(3);
    ///
    /// Dropdown::new_rw(dropdown_active_item, 1..=5);
    /// ```
    ///
    /// This function is a convenience wrapper around `Dropdown::custom` that uses default views
    /// for the main and list items.
    ///
    /// # Arguments
    ///
    /// * `active_item` - A read-write signal representing the currently selected item.
    ///   It must implement `SignalGet<T>` and `SignalUpdate<T>`.
    ///
    /// * `iterator` - An iterator that provides the items to be displayed in the dropdown list.
    pub fn new_rw<AI, I>(active_item: AI, iterator: I) -> Dropdown<T>
    where
        AI: SignalGet<T> + SignalUpdate<T> + Copy + 'static,
        I: IntoIterator<Item = T> + 'static,
        T: Clone + PartialEq + std::fmt::Display + 'static,
    {
        Self::custom(
            move || active_item.get(),
            Self::default_main_view,
            iterator,
            |t| Label::new(t).into_any(),
        )
        .on_accept(move |nv| active_item.set(nv))
    }

    /// Overrides the main view for the dropdown.
    pub fn main_view(mut self, main_view: impl Fn(T) -> Box<dyn View> + 'static) -> Self {
        self.main_fn = Box::new(Scope::current().enter_child(main_view));
        let (child, main_view_scope) = (self.main_fn)(self.current_value.clone());
        let main_view = child.id();
        self.main_view_scope = main_view_scope;
        self.main_view = main_view;
        self.id.set_children([child]);
        self
    }

    /// Overrides the list view for each item in the dropdown list.
    pub fn list_item_view(mut self, list_item_fn: impl Fn(&T) -> Box<dyn View> + 'static) -> Self {
        self.list_item_fn = Rc::new(list_item_fn);
        self
    }

    /// Sets a reactive condition for showing or hiding the dropdown list.
    ///
    /// # Reactivity
    /// The `show` function will be re-run whenever any signal it depends on changes.
    pub fn show_list(self, show: impl Fn() -> bool + 'static) -> Self {
        let id = self.id();
        Effect::new(move |_| {
            let state = show();
            id.update_state(Message::OpenState(state));
        });
        self
    }

    /// Add a callback to be called when an item is selected from the dropdown.
    ///
    /// This is the preferred way to handle dropdown selections, as it provides
    /// direct typed access to the selected value without needing to correctly specify the generics on [DropdownAccept].
    ///
    /// # Example
    /// ```rust,ignore
    /// dropdown.on_accept(|value| {
    ///     println!("Selected: {value}");
    /// })
    /// ```
    pub fn on_accept(self, on_accept: impl Fn(T) + 'static) -> Self {
        self.on_event_stop(
            DropdownAccept::listener(),
            move |_cx, t: &DropdownAccept<T>| on_accept(t.value.clone()),
        )
    }

    /// Add a callback function to be called when the dropdown is opened.
    #[deprecated(
        note = "use .on_event_stop(DropdownOpenChanged::listener(), |_, _|) directly instead"
    )]
    pub fn on_open(self, on_open: impl Fn(bool) + 'static) -> Self {
        self.on_event_stop(DropdownOpenChanged::listener(), move |_cx, t| on_open(*t))
    }

    fn swap_state(&mut self) {
        if self.overlay_id.is_some() {
            self.close_dropdown();
        } else {
            self.open_dropdown();
        }
    }

    fn dispatch_open_changed(&self, is_open: bool) {
        self.id.dispatch_event(
            Event::new_custom(DropdownOpenChanged { is_open }),
            DispatchKind::Directed {
                target: self.id.get_element_id(),
                phases: Phases::TARGET,
            },
        );
    }

    fn open_dropdown(&mut self) {
        if self.overlay_id.is_none() {
            self.create_overlay();
            self.dispatch_open_changed(true);
        }
    }

    fn close_dropdown(&mut self) {
        if let Some(id) = self.overlay_id.take() {
            remove_overlay(id);
            self.dispatch_open_changed(false);
        }
    }

    fn build_list_view(&self) -> impl View + use<T> {
        let dropdown_id = self.id;
        let index_to_item = self.index_to_item.clone();
        let list_item_fn = self.list_item_fn.clone();

        let items_view = self.index_to_item.values().map(|v| (list_item_fn)(v));
        let active = self
            .index_to_item
            .values()
            .position(|v| *v == self.current_value);

        let list = list(items_view)
            .on_event_stop(
                crate::views::list::ListAccept::listener(),
                move |_, event| {
                    if let Some(idx) = event.selection {
                        let val = index_to_item
                            .get(&idx)
                            .expect("Index should exist in the map")
                            .clone();
                        dropdown_id.update_state(Message::ActiveElement(Box::new(val.clone())));
                        dropdown_id.update_state(Message::ListSelect(Box::new(val)));
                    }
                },
            )
            .style(|s| s.width_full())
            .on_event_stop(listener::FocusLeftSubtree, move |_, _| {
                dropdown_id.update_state(Message::ListFocusLost);
            })
            .on_event_stop(listener::FocusLost, move |_, _| {
                dropdown_id.update_state(Message::ListFocusLost);
            });

        list.selection().set(active);
        list
    }

    fn create_overlay(&mut self) {
        let anchor_rect = self.id.get_visual_rect();
        let width = self.width;
        let point = Point::new(anchor_rect.x0, anchor_rect.y1);

        let list = self.build_list_view();
        let list_id = list.id();
        list_id.request_focus();

        let scroll = list.scroll().style(move |s| {
            s.flex_col()
                // constrains the scroll width to match
                // the dropdown trigger. Without this, the scroll would expand to
                // fill the full overlay due to width_full() on ScrollClass.
                .width_full()
                .max_height_full()
        });

        let anchor_id = self.id;
        let inset = RwSignal::new(Size::new(point.x, point.y));

        self.overlay_id = Some(add_overlay(
            scroll
                .on_event_stop(listener::WindowResized, move |cx, size| {
                    let anchor = anchor_id.get_visual_rect();
                    let container_size = size;
                    let list_size = cx.view_id.get_visual_rect_no_clip().size();
                    let padding = 5.0;

                    let ideal = Size::new(anchor.x0, anchor.y1);
                    let clamped = Size::new(
                        ideal.width.clamp(
                            padding,
                            (container_size.width - list_size.width - padding).max(padding),
                        ),
                        ideal.height.clamp(
                            padding,
                            (container_size.height - list_size.height - padding).max(padding),
                        ),
                    );

                    if inset != clamped {
                        inset.set(clamped);
                    }
                })
                .container()
                // Positioning container: uses absolute
                // inset to position the width-constrained list. Also listens to
                // VisualChanged to recompute position when the anchor or overlay moves.
                .style(move |s| {
                    let inset = inset.get();
                    s.absolute()
                        .inset_left(inset.width)
                        .inset_top(inset.height)
                        .min_width(width.get())
                        .flex_shrink(0.)
                }),
        ));
        self.overlay_id.unwrap().set_style_parent(self.id);
    }

    /// Sets the custom style properties of the `Dropdown`.
    pub fn dropdown_style(
        self,
        style: impl Fn(DropdownCustomStyle) -> DropdownCustomStyle + 'static,
    ) -> Self {
        self.custom_style(style)
    }
}

#[derive(Debug, Clone, Default)]
/// A struct that allows for easy custom styling of the `Dropdown` using the [`Dropdown::dropdown_style`] method or the [`Style::custom_style`](crate::style::CustomStylable::custom_style) method.
pub struct DropdownCustomStyle(Style);
impl From<DropdownCustomStyle> for Style {
    fn from(val: DropdownCustomStyle) -> Self {
        val.0
    }
}
impl From<Style> for DropdownCustomStyle {
    fn from(val: Style) -> Self {
        Self(val)
    }
}
impl CustomStyle for DropdownCustomStyle {
    type StyleClass = DropdownClass;
}

impl<T: Clone + PartialEq + std::fmt::Debug> CustomStylable<DropdownCustomStyle> for Dropdown<T> {
    type DV = Self;
}

impl DropdownCustomStyle {
    /// Creates a new `DropDownCustomStyle` with default values.
    pub fn new() -> Self {
        Self::default()
    }
    /// Sets the `CloseOnAccept` property for the dropdown, which determines whether the dropdown
    /// should automatically close when an item is selected. The default value is `true`.
    ///
    /// # Arguments
    /// * `close`: If set to `true`, the dropdown will close upon item selection. If `false`, it
    ///   will remain open after an item is selected.
    pub fn close_on_accept(mut self, close: bool) -> Self {
        self = Self(self.0.set(CloseOnAccept, close));
        self
    }
}

impl<T> Drop for Dropdown<T> {
    fn drop(&mut self) {
        if let Some(id) = self.overlay_id {
            remove_overlay(id)
        }
    }
}
