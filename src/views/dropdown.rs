#![deny(missing_docs)]
//! A view that allows the user to select an item from a list of items.
//!
//! The [`Dropdown`] struct provides several constructors, each offering different levels of customization and ease of use.
//!
//! The [`DropdownCustomStyle`] struct allows for easy and advanced customization of the dropdown's appearance.
use std::{any::Any, rc::Rc};

use floem_reactive::{
    as_child_of_current_scope, create_effect, create_updater, RwSignal, Scope, SignalGet,
    SignalUpdate,
};
use peniko::{
    color::palette,
    kurbo::{Point, Rect, Size},
};
use winit::keyboard::{Key, NamedKey};

use crate::{
    action::{add_overlay, remove_overlay},
    event::{Event, EventListener, EventPropagation},
    id::ViewId,
    prop, prop_extractor,
    style::{CustomStylable, CustomStyle, Style, StyleClass, Width},
    style_class,
    unit::PxPctAuto,
    view::{default_compute_layout, IntoView, View},
    views::{container, scroll, stack, svg, text, Decorators},
    AnyView,
};

use super::list;

type ChildFn<T> = dyn Fn(T) -> (AnyView, Scope);
type ListViewFn<T> = Rc<dyn Fn(&dyn Fn(T) -> AnyView) -> AnyView>;

style_class!(
    /// A Style class that is applied to all dropdowns.
    pub DropdownClass
);

prop!(
    /// A property that determines whether the dropdown should close automatically when an item is selected.
    pub CloseOnAccept: bool {} = true
);
prop_extractor!(DropdownStyle {
    close_on_accept: CloseOnAccept,
});

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
    list_view: ListViewFn<T>,
    list_item_fn: Rc<dyn Fn(T) -> AnyView>,
    list_style: Style,
    overlay_id: Option<ViewId>,
    window_origin: Option<Point>,
    on_accept: Option<Box<dyn Fn(T)>>,
    on_open: Option<Box<dyn Fn(bool)>>,
    style: DropdownStyle,
}

enum Message {
    OpenState(bool),
    ActiveElement(Box<dyn Any>),
    ListFocusLost,
    ListSelect(Box<dyn Any>),
}

impl<T: 'static + Clone> View for Dropdown<T> {
    fn id(&self) -> ViewId {
        self.id
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "DropDown".into()
    }

    fn style_pass(&mut self, cx: &mut crate::context::StyleCx<'_>) {
        if self.style.read(cx) {
            cx.app_state_mut().request_paint(self.id);
        }
        self.list_style = cx
            .style()
            .get_nested_map(scroll::ScrollClass::key())
            .unwrap_or_default();

        for child in self.id.children() {
            cx.style_view(child);
        }
    }

    fn compute_layout(&mut self, cx: &mut crate::context::ComputeLayoutCx) -> Option<Rect> {
        self.window_origin = Some(cx.window_origin);

        default_compute_layout(self.id, cx)
    }

    fn update(&mut self, cx: &mut crate::context::UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(state) = state.downcast::<Message>() {
            match *state {
                Message::OpenState(true) => self.open_dropdown(cx),
                Message::OpenState(false) => self.close_dropdown(),
                Message::ListFocusLost => self.close_dropdown(),
                Message::ListSelect(val) => {
                    if let Ok(val) = val.downcast::<T>() {
                        if self.style.close_on_accept() {
                            self.close_dropdown();
                        }
                        if let Some(on_select) = &self.on_accept {
                            on_select(*val);
                        }
                    }
                }
                Message::ActiveElement(val) => {
                    if let Ok(val) = val.downcast::<T>() {
                        let old_child_scope = self.main_view_scope;
                        let old_main_view = self.main_view;
                        let (main_view, main_view_scope) = (self.main_fn)(*val);
                        let main_view_id = main_view.id();
                        self.id.set_children([main_view]);
                        self.main_view = main_view_id;
                        self.main_view_scope = main_view_scope;

                        cx.app_state_mut().remove_view(old_main_view);
                        old_child_scope.dispose();
                        self.id.request_all();
                    }
                }
            }
        }
    }

    fn event_before_children(
        &mut self,
        _cx: &mut crate::context::EventCx,
        event: &Event,
    ) -> EventPropagation {
        match event {
            Event::PointerDown(_) => {
                self.swap_state();
                return EventPropagation::Stop;
            }
            Event::KeyUp(ref key_event)
                if matches!(key_event.key.logical_key, Key::Named(NamedKey::Enter))
                    | matches!(
                        key_event.key.logical_key,
                        Key::Character(ref c) if c == " "
                    ) =>
            {
                self.swap_state()
            }
            _ => {}
        }

        EventPropagation::Continue
    }
}

impl<T: Clone> Dropdown<T> {
    /// Creates a default main view for the dropdown.
    ///
    /// This function generates a view that displays the given item as text,
    /// along with a chevron-down icon to indicate that it's a dropdown.
    pub fn default_main_view(item: T) -> AnyView
    where
        T: std::fmt::Display,
    {
        const CHEVRON_DOWN: &str = r##"
<svg
   width="12"
   height="12"
   viewBox="0 0 12 12"
   version="1.1"
   xmlns:svg="http://www.w3.org/2000/svg">
  <g
     style="display:inline;opacity:1;mix-blend-mode:normal"
     transform="translate(-42.144408,-102.78125)">
    <path
       style="fill:none;stroke:#333333;stroke-width:1.5;stroke-linecap:round;stroke-linejoin:round;stroke-dasharray:none"
       d="m 43.978404,107.53126 4.194255,2.5 4.137753,-2.5" />
  </g>
</svg>"##;

        // TODO: this should be more customizable
        stack((
            text(item),
            container(svg(CHEVRON_DOWN).style(|s| s.size(12, 12).color(palette::css::BLACK)))
                .style(|s| {
                    s.items_center()
                        .padding(3.)
                        .border_radius(5)
                        .hover(move |s| s.background(palette::css::LIGHT_GRAY))
                }),
        ))
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
        I: IntoIterator<Item = T> + Clone + 'static,
        LF: Fn(T) -> AnyView + Clone + 'static,
        T: Clone + 'static,
        AIF: Fn() -> T + 'static,
    {
        let dropdown_id = ViewId::new();

        let list_item_fn = Rc::new(list_item_fn);

        let list_view = Rc::new(move |list_item_fn: &dyn Fn(T) -> AnyView| {
            let iterator = iterator.clone();
            let iter_clone = iterator.clone();
            list(iterator.into_iter().map(list_item_fn))
                .on_accept(move |opt_idx| {
                    if let Some(idx) = opt_idx {
                        let val = iter_clone.clone().into_iter().nth(idx).unwrap();
                        dropdown_id.update_state(Message::ActiveElement(Box::new(val.clone())));
                        dropdown_id.update_state(Message::ListSelect(Box::new(val)));
                    }
                })
                .style(|s| s.width_full())
                .keyboard_navigable()
                .on_event_stop(EventListener::FocusLost, move |_| {
                    dropdown_id.update_state(Message::ListFocusLost);
                })
                .on_event_stop(EventListener::PointerMove, |_| {})
                .into_any()
        });

        let initial = create_updater(active_item, move |new_state| {
            dropdown_id.update_state(Message::ActiveElement(Box::new(new_state)));
        });

        let main_fn = Box::new(as_child_of_current_scope(main_view));

        let (child, main_view_scope) = main_fn(initial.clone());
        let main_view = child.id();

        dropdown_id.set_children([child]);

        Self {
            id: dropdown_id,
            current_value: initial,
            main_view,
            main_view_scope,
            main_fn,
            list_view,
            list_item_fn,
            list_style: Style::new(),
            overlay_id: None,
            window_origin: None,
            on_accept: None,
            on_open: None,
            style: Default::default(),
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
        I: IntoIterator<Item = T> + Clone + 'static,
        T: Clone + std::fmt::Display + 'static,
    {
        Self::custom(active_item, Self::default_main_view, iterator, |v| {
            crate::views::text(v).into_any()
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
        I: IntoIterator<Item = T> + Clone + 'static,
        T: Clone + std::fmt::Display + 'static,
    {
        Self::custom(
            move || active_item.get(),
            Self::default_main_view,
            iterator,
            |t| text(t).into_any(),
        )
        .on_accept(move |nv| active_item.set(nv))
    }

    /// Overrides the main view for the dropdown.
    pub fn main_view(mut self, main_view: impl Fn(T) -> Box<dyn View> + 'static) -> Self {
        self.main_fn = Box::new(as_child_of_current_scope(main_view));
        let (child, main_view_scope) = (self.main_fn)(self.current_value.clone());
        let main_view = child.id();
        self.main_view_scope = main_view_scope;
        self.main_view = main_view;
        self.id.set_children([child]);
        self
    }

    /// Overrides the list view for each item in the dropdown list.
    pub fn list_item_view(mut self, list_item_fn: impl Fn(T) -> Box<dyn View> + 'static) -> Self {
        self.list_item_fn = Rc::new(list_item_fn);
        self
    }

    /// Sets a reactive condition for showing or hiding the dropdown list.
    ///
    /// # Reactivity
    /// The `show` function will be re-run whenever any signal it depends on changes.
    pub fn show_list(self, show: impl Fn() -> bool + 'static) -> Self {
        let id = self.id();
        create_effect(move |_| {
            let state = show();
            id.update_state(Message::OpenState(state));
        });
        self
    }

    /// Sets a callback function to be called when an item is selected from the dropdown.
    ///
    /// Only one `on_accept` callback can be set at a time.
    pub fn on_accept(mut self, on_accept: impl Fn(T) + 'static) -> Self {
        self.on_accept = Some(Box::new(on_accept));
        self
    }

    /// Sets a callback function to be called when the dropdown is opened.
    ///
    /// Only one `on_open` callback can be set at a time.
    pub fn on_open(mut self, on_open: impl Fn(bool) + 'static) -> Self {
        self.on_open = Some(Box::new(on_open));
        self
    }

    fn swap_state(&self) {
        if self.overlay_id.is_some() {
            self.id.update_state(Message::OpenState(false));
        } else {
            self.id.request_layout();
            self.id.update_state(Message::OpenState(true));
        }
    }

    fn open_dropdown(&mut self, cx: &mut crate::context::UpdateCx) {
        if self.overlay_id.is_none() {
            self.id.request_layout();
            cx.app_state.compute_layout();
            if let Some(layout) = self.id.get_layout() {
                self.update_list_style(layout.size.width as f64);
                let point =
                    self.window_origin.unwrap_or_default() + (0., layout.size.height as f64);
                self.create_overlay(point);

                if let Some(on_open) = &self.on_open {
                    on_open(true);
                }
            }
        }
    }

    fn close_dropdown(&mut self) {
        if let Some(id) = self.overlay_id.take() {
            remove_overlay(id);
            if let Some(on_open) = &self.on_open {
                on_open(false);
            }
        }
    }

    fn update_list_style(&mut self, width: f64) {
        if let PxPctAuto::Pct(pct) = self.list_style.get(Width) {
            let new_width = width * pct / 100.0;
            self.list_style = self.list_style.clone().width(new_width);
        }
    }

    fn create_overlay(&mut self, point: Point) {
        let list = self.list_view.clone();
        let list_style = self.list_style.clone();
        let list_item_fn = self.list_item_fn.clone();
        self.overlay_id = Some(add_overlay(Point::ZERO, {
            const DEFAULT_PADDING: f64 = 5.0;
            let list_size = RwSignal::new(None);
            let overlay_size = RwSignal::new(None);
            let initial_padding = Size::new(point.x, point.y);
            let top_left_padding = RwSignal::new(initial_padding);

            create_effect(move |_| {
                let (Some(list_size), Some(overlay_size)) = (list_size.get(), overlay_size.get())
                else {
                    return;
                };

                let default_padding_size = Size::new(DEFAULT_PADDING, DEFAULT_PADDING);
                let new_padding = initial_padding
                    .min(overlay_size - list_size - default_padding_size)
                    .max(default_padding_size);

                if new_padding != top_left_padding.get_untracked() {
                    top_left_padding.set(new_padding);
                }
            });

            let list = list(&*list_item_fn.clone());
            let list_id = list.id();

            let list = list.on_resize(move |rect| {
                if let Some(parent_layout) = list_id.parent().and_then(|p| p.get_layout()) {
                    // resolve size of the scroll view if it wasn't squished
                    let margin = parent_layout.margin;
                    let padding = parent_layout.padding;
                    let border = parent_layout.border;

                    let indent_size = Size::new(
                        (margin.horizontal_components().sum()
                            + border.horizontal_components().sum()
                            + padding.horizontal_components().sum()) as _,
                        (margin.vertical_components().sum()
                            + padding.vertical_components().sum()
                            + border.vertical_components().sum()) as _,
                    );
                    let size = rect.size() + indent_size;

                    list_size.set(Some(size));
                }
            });

            list_id.request_focus();

            container(scroll(list).style(move |s| {
                s.flex_col()
                    .pointer_events_auto()
                    .flex_grow(0.0)
                    .flex_shrink(1.0)
                    .apply(list_style.clone())
            }))
            .on_resize(move |rect| {
                overlay_size.set(Some(rect.size()));
            })
            .style(move |s| {
                let padding = top_left_padding.get();
                s.absolute()
                    .flex_col()
                    .size_full()
                    .padding_left(padding.width)
                    .padding_top(padding.height)
                    .padding_bottom(DEFAULT_PADDING)
                    .padding_right(DEFAULT_PADDING)
                    .pointer_events_none()
            })
        }));
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

impl<T: Clone> CustomStylable<DropdownCustomStyle> for Dropdown<T> {
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
