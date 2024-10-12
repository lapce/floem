use std::{any::Any, rc::Rc};

use floem_reactive::{
    as_child_of_current_scope, create_effect, create_updater, Scope, SignalGet, SignalUpdate,
};
use floem_winit::keyboard::{Key, NamedKey};
use peniko::{
    kurbo::{Point, Rect},
    Color,
};

use crate::{
    action::{add_overlay, remove_overlay},
    event::{Event, EventListener, EventPropagation},
    id::ViewId,
    prop, prop_extractor,
    style::{CustomStylable, Style, StyleClass, Width},
    style_class,
    unit::{PxPctAuto, UnitExt},
    view::{default_compute_layout, IntoView, View},
    views::{container, scroll, stack, svg, text, Decorators},
    AnyView,
};

use super::list;

type ChildFn<T> = dyn Fn(T) -> (Box<dyn View>, Scope);

style_class!(pub DropdownClass);

prop!(pub CloseOnAccept: bool {} = true);
prop_extractor!(DropdownStyle {
    close_on_accept: CloseOnAccept,
});

pub fn dropdown<T, MF, I, LF, AIF>(
    active_item: AIF,
    main_view: MF,
    iterator: I,
    list_item_fn: LF,
) -> Dropdown<T>
where
    MF: Fn(T) -> Box<dyn View> + 'static,
    I: IntoIterator<Item = T> + Clone + 'static,
    LF: Fn(T) -> Box<dyn View> + Clone + 'static,
    T: Clone + 'static,
    AIF: Fn() -> T + 'static,
{
    Dropdown::new(active_item, main_view, iterator, list_item_fn)
}

/// A dropdown widget
///
/// **Styling**:
/// You can modify the behavior of the dropdown through the `CloseOnAccept` property.
/// If the property is set to `true` the dropdown will automatically close when an item is selected.
/// If the property is set to `false` the dropwown will not automatically close when an item is selected.
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
    main_view: ViewId,
    main_view_scope: Scope,
    main_fn: Box<ChildFn<T>>,
    list_view: Rc<dyn Fn() -> Box<dyn View>>,
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

impl<T: 'static> View for Dropdown<T> {
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
            .indirect_style()
            .clone()
            .apply_classes_from_context(&[scroll::ScrollClass::class_ref()], cx.indirect_style());

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
                        self.id.set_children(vec![main_view]);
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
                if matches!(
                    key_event.key.logical_key,
                    Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space)
                ) =>
            {
                self.swap_state()
            }
            _ => {}
        }

        EventPropagation::Continue
    }
}

impl<T> Dropdown<T> {
    pub fn default_main_view(item: T) -> AnyView
    where
        T: std::fmt::Display,
    {
        const CHEVRON_DOWN: &str = r##"
            <svg xmlns="http://www.w3.org/2000/svg" xml:space="preserve" viewBox="0 0 185.344 185.344">
                <path fill="#010002" d="M92.672 144.373a10.707 10.707 0 0 1-7.593-3.138L3.145 59.301c-4.194-4.199
                -4.194-10.992 0-15.18a10.72 10.72 0 0 1 15.18 0l74.347 74.341 74.347-74.341a10.72 10.72 0 0 1
                15.18 0c4.194 4.194 4.194 10.981 0 15.18l-81.939 81.934a10.694 10.694 0 0 1-7.588 3.138z"/>
            </svg>
        "##;

        stack((
            text(item),
            container(svg(CHEVRON_DOWN).style(|s| s.size(12, 12).color(Color::BLACK))).style(|s| {
                s.items_center()
                    .padding(3.)
                    .border_radius(7.pct())
                    .hover(move |s| s.background(Color::LIGHT_GRAY))
            }),
        ))
        .style(|s| s.items_center().justify_between().size_full())
        .into_any()
    }

    /// Creates a new dropdown
    pub fn new<MF, I, LF, AIF>(
        active_item: AIF,
        main_view: MF,
        iterator: I,
        list_item_fn: LF,
    ) -> Dropdown<T>
    where
        MF: Fn(T) -> Box<dyn View> + 'static,
        I: IntoIterator<Item = T> + Clone + 'static,
        LF: Fn(T) -> Box<dyn View> + Clone + 'static,
        T: Clone + 'static,
        AIF: Fn() -> T + 'static,
    {
        let dropdown_id = ViewId::new();

        let list_view = Rc::new(move || {
            let iterator = iterator.clone();
            let iter_clone = iterator.clone();
            let list_item_fn = list_item_fn.clone();
            let inner_list = list(iterator.into_iter().map(list_item_fn))
                .on_accept(move |opt_idx| {
                    if let Some(idx) = opt_idx {
                        let val = iter_clone.clone().into_iter().nth(idx).unwrap();
                        dropdown_id.update_state(Message::ActiveElement(Box::new(val.clone())));
                        dropdown_id.update_state(Message::ListSelect(Box::new(val)));
                    }
                })
                .style(|s| s.size_full())
                .keyboard_navigable()
                .on_event_stop(EventListener::PointerDown, move |_| {})
                .on_event_stop(EventListener::FocusLost, move |_| {
                    dropdown_id.update_state(Message::ListFocusLost);
                });
            let inner_list_id = inner_list.id();
            scroll(inner_list)
                .on_event_stop(EventListener::FocusGained, move |_| {
                    inner_list_id.request_focus();
                })
                .into_any()
        });

        let initial = create_updater(active_item, move |new_state| {
            dropdown_id.update_state(Message::ActiveElement(Box::new(new_state)));
        });

        let main_fn = Box::new(as_child_of_current_scope(main_view));

        let (child, main_view_scope) = main_fn(initial);
        let main_view = child.id();

        dropdown_id.set_children(vec![child]);

        Self {
            id: dropdown_id,
            main_view,
            main_view_scope,
            main_fn,
            list_view,
            list_style: Style::new(),
            overlay_id: None,
            window_origin: None,
            on_accept: None,
            on_open: None,
            style: Default::default(),
        }
        .class(DropdownClass)
    }

    /// Creates a basic dropdown with a read-only function for the active item.
    ///
    /// # Example
    /// ```rust
    /// # use floem::{*, views::*, reactive::*};
    /// # use floem::views::dropdown::*;
    /// let active_item = RwSignal::new(3);
    ///
    /// Dropdown::basic(move || active_item.get(), 1..=5).on_accept(move |val| active_item.set(val)));
    /// ```
    ///
    /// This function is a convenience wrapper around `Dropdown::new` that uses default views
    /// for the main and list items.
    ///
    /// See also [Dropdown::basic_rw].
    ///
    /// # Arguments
    ///
    /// * `active_item` - A function that returns the currently selected item.
    ///     * `AIF` - The type of the active item function of type `T`.
    ///     * `T` - The type of items in the dropdown. Must implement `Clone` and `std::fmt::Display`.
    ///
    /// * `iterator` - An iterator that provides the items to be displayed in the dropdown list.
    ///                It must be `Clone` and iterate over items of type `T`.
    ///     * `I` - The type of the iterator.
    pub fn basic<AIF, I>(active_item: AIF, iterator: I) -> Dropdown<T>
    where
        AIF: Fn() -> T + 'static,
        I: IntoIterator<Item = T> + Clone + 'static,
        T: Clone + std::fmt::Display + 'static,
    {
        Self::new(active_item, Self::default_main_view, iterator, |v| {
            crate::views::text(v).into_any()
        })
    }

    /// Creates a new dropdown with a read-write signal for the active item.
    ///
    /// # Example
    /// ```rust
    /// # use floem::{*, views::*, reactive::*};
    /// # use floem::{views::dropdown::*};
    /// let active_item = RwSignal::new(3);
    ///
    /// Dropdown::new_rw(
    ///     active_item,
    ///     |item| text(item).into_any(),
    ///     1..=5,
    ///     |item| text(item).into_any(),
    /// );
    /// ```
    ///
    /// This function allows for more customization compared to `basic_rw` by letting you specify
    /// custom view functions for both the main dropdown display and the list items.
    ///
    /// # Arguments
    ///
    /// * `active_item` - A read-write signal representing the currently selected item.
    ///                   It must implement both `SignalGet<T>` and `SignalUpdate<T>`.
    ///     * `T` - The type of items in the dropdown. Must implement `Clone`.
    ///     * `AI` - The type of the active item signal.
    ///
    /// * `main_view` - A function that takes a value of type `T` and returns an `AnyView`
    ///                 to be used as the main dropdown display.
    /// * `iterator` - An iterator that provides the items to be displayed in the dropdown list.
    ///                It must be `Clone` and iterate over items of type `T`.
    /// * `list_item_fn` - A function that takes a value of type `T` and returns an `AnyView`
    ///                    to be used for each item in the dropdown list.
    ///
    /// # Type Parameters
    ///
    /// * `MF` - The type of the main view function.
    /// * `I` - The type of the iterator.
    /// * `LF` - The type of the list item function.
    pub fn new_rw<AI, MF, I, LF>(
        active_item: AI,
        main_view: MF,
        iterator: I,
        list_item_fn: LF,
    ) -> Dropdown<T>
    where
        AI: SignalGet<T> + SignalUpdate<T> + Copy + 'static,
        MF: Fn(T) -> AnyView + 'static,
        I: IntoIterator<Item = T> + Clone + 'static,
        LF: Fn(T) -> AnyView + Clone + 'static,
        T: Clone + 'static,
    {
        Self::new(move || active_item.get(), main_view, iterator, list_item_fn)
            .on_accept(move |nv| active_item.set(nv))
    }

    /// Creates a basic dropdown with a read-write signal for the active item.
    ///
    /// # Example:
    /// ```rust
    /// # use floem::{*, views::*, reactive::*};
    /// # use floem::{views::dropdown::*};
    /// let dropdown_active_item = RwSignal::new(3);
    ///
    /// Dropdown::basic_rw(dropdown_active_item, 1..=5);
    /// ```
    ///
    /// This function is a convenience wrapper around `Dropdown::new_rw` that uses default views
    /// for the main and list items.
    ///
    /// # Arguments
    ///
    /// * `active_item` - A read-write signal representing the currently selected item.
    ///                   It must implement `SignalGet<T>` and `SignalUpdate<T>`.
    ///     * `T` - The type of items in the dropdown. Must implement `Clone` and `std::fmt::Display`.
    ///     * `AI` - The type of the active item signal.
    /// * `iterator` - An iterator that provides the items to be displayed in the dropdown list.
    ///                It must be `Clone` and iterate over items of type `T`.
    ///     * `I` - The type of the iterator.
    pub fn basic_rw<AI, I>(active_item: AI, iterator: I) -> Dropdown<T>
    where
        AI: SignalGet<T> + SignalUpdate<T> + Copy + 'static,
        I: IntoIterator<Item = T> + Clone + 'static,
        T: Clone + std::fmt::Display + 'static,
    {
        Self::new_rw(active_item, Self::default_main_view, iterator, |v| {
            text(v).into_any()
        })
    }

    pub fn show_list(self, show: impl Fn() -> bool + 'static) -> Self {
        let id = self.id();
        create_effect(move |_| {
            let state = show();
            id.update_state(Message::OpenState(state));
        });
        self
    }

    pub fn on_accept(mut self, on_accept: impl Fn(T) + 'static) -> Self {
        self.on_accept = Some(Box::new(on_accept));
        self
    }

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
        self.overlay_id = Some(add_overlay(point, move |_| {
            let list = list()
                .style(move |s| s.apply(list_style.clone()))
                .into_view();
            let list_id = list.id();
            list_id.request_focus();
            list
        }));
    }

    /// Sets the custom style properties of the `DropDown`.
    pub fn dropdown_style(
        self,
        style: impl Fn(DropDownCustomStyle) -> DropDownCustomStyle + 'static,
    ) -> Self {
        self.custom_style(style)
    }
}

#[derive(Debug, Clone, Default)]
pub struct DropDownCustomStyle(Style);
impl From<DropDownCustomStyle> for Style {
    fn from(val: DropDownCustomStyle) -> Self {
        val.0
    }
}
impl<T> CustomStylable<DropDownCustomStyle> for Dropdown<T> {
    type DV = Self;
}

impl DropDownCustomStyle {
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
