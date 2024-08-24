use std::{any::Any, rc::Rc};

use floem_reactive::{
    as_child_of_current_scope, create_effect, create_updater, Scope, SignalGet, SignalUpdate,
};
use floem_winit::keyboard::{Key, NamedKey};
use peniko::kurbo::{Point, Rect};

use crate::{
    action::{add_overlay, remove_overlay},
    event::{Event, EventListener, EventPropagation},
    id::ViewId,
    prop, prop_extractor,
    style::{CustomStylable, Style, StyleClass, Width},
    style_class,
    unit::PxPctAuto,
    view::{default_compute_layout, IntoView, View},
    views::{scroll, Decorators},
};

use super::list;

type ChildFn<T> = dyn Fn(T) -> (Box<dyn View>, Scope);

style_class!(pub DropdownClass);
style_class!(pub DropdownScrollClass);

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
/// empty()
///     .style(|s|
///         s.class(dropdown::DropdownClass, |s| {
///             s.set(dropdown::CloseOnAccept, false)
///         })
///  );
///```
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
                if key_event.key.logical_key == Key::Named(NamedKey::Enter) =>
            {
                self.swap_state()
            }
            _ => {}
        }

        EventPropagation::Continue
    }
}

impl<T> Dropdown<T> {
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
                .keyboard_navigatable()
                .on_event_stop(EventListener::PointerDown, move |_| {})
                .on_event_stop(EventListener::FocusLost, move |_| {
                    dropdown_id.update_state(Message::ListFocusLost);
                });
            let inner_list_id = inner_list.id();
            scroll(inner_list)
                .on_event_stop(EventListener::FocusGained, move |_| {
                    inner_list_id.request_focus();
                })
                .class(DropdownScrollClass)
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

    pub fn new_get_set<MF, I, LF>(
        active_item: impl SignalGet<T> + SignalUpdate<T> + Copy + 'static,
        main_view: MF,
        iterator: I,
        list_item_fn: LF,
    ) -> Dropdown<T>
    where
        MF: Fn(T) -> Box<dyn View> + 'static,
        I: IntoIterator<Item = T> + Clone + 'static,
        LF: Fn(T) -> Box<dyn View> + Clone + 'static,
        T: Clone + 'static,
    {
        Self::new(move || active_item.get(), main_view, iterator, list_item_fn)
            .on_accept(move |nv| active_item.set(nv))
    }

    pub fn new_get<MF, I, LF>(
        active_item: impl SignalGet<T> + Copy + 'static,
        main_view: MF,
        iterator: I,
        list_item_fn: LF,
    ) -> Dropdown<T>
    where
        MF: Fn(T) -> Box<dyn View> + 'static,
        I: IntoIterator<Item = T> + Clone + 'static,
        LF: Fn(T) -> Box<dyn View> + Clone + 'static,
        T: Clone + 'static,
    {
        Self::new(move || active_item.get(), main_view, iterator, list_item_fn)
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
