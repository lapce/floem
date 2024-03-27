use std::{any::Any, rc::Rc};

use floem_reactive::{as_child_of_current_scope, create_effect, create_updater, Scope};
use floem_winit::keyboard::{Key, NamedKey};
use kurbo::{Point, Rect};

use crate::{
    action::{add_overlay, remove_overlay},
    event::{Event, EventListener},
    id::Id,
    prop, prop_extractor,
    style::{Style, StyleClass, Width},
    style_class,
    unit::PxPctAuto,
    view::{
        default_compute_layout, default_event, view_children_set_parent_id, AnyView, View,
        ViewData, Widget,
    },
    views::{scroll, Decorators},
    EventPropagation,
};

use super::list;

type ChildFn<T> = dyn Fn(T) -> (AnyView, Scope);

style_class!(pub DropDownClass);
style_class!(pub DropDownScrollClass);

prop!(pub CloseOnAccept: bool {} = true);
prop_extractor!(DropDownStyle {
    close_on_accept: CloseOnAccept,
});

pub struct DropDown<T: 'static> {
    view_data: ViewData,
    main_view: Box<dyn Widget>,
    main_view_scope: Scope,
    main_fn: Box<ChildFn<T>>,
    list_view: Rc<dyn Fn() -> AnyView>,
    list_style: Style,
    overlay_id: Option<Id>,
    window_origin: Option<Point>,
    on_accept: Option<Box<dyn Fn(T)>>,
    on_open: Option<Box<dyn Fn(bool)>>,
    style: DropDownStyle,
}

enum Message {
    OpenState(bool),
    ActiveElement(Box<dyn Any>),
    ListFocusLost,
    ListSelect(Box<dyn Any>),
}

impl<T: 'static> View for DropDown<T> {
    fn view_data(&self) -> &ViewData {
        &self.view_data
    }

    fn view_data_mut(&mut self) -> &mut ViewData {
        &mut self.view_data
    }

    fn build(self) -> crate::view::AnyWidget {
        Box::new(self.keyboard_navigatable())
    }
}

impl<T: 'static> Widget for DropDown<T> {
    fn view_data(&self) -> &ViewData {
        &self.view_data
    }

    fn view_data_mut(&mut self) -> &mut ViewData {
        &mut self.view_data
    }

    fn for_each_child<'a>(&'a self, for_each: &mut dyn FnMut(&'a dyn Widget) -> bool) {
        for_each(&self.main_view);
    }

    fn for_each_child_mut<'a>(&'a mut self, for_each: &mut dyn FnMut(&'a mut dyn Widget) -> bool) {
        for_each(&mut self.main_view);
    }

    fn for_each_child_rev_mut<'a>(
        &'a mut self,
        for_each: &mut dyn FnMut(&'a mut dyn Widget) -> bool,
    ) {
        for_each(&mut self.main_view);
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "DropDown".into()
    }

    fn style(&mut self, cx: &mut crate::context::StyleCx<'_>) {
        if self.style.read(cx) {
            cx.app_state_mut().request_paint(self.id());
        }
        cx.save();
        self.list_style =
            Style::new().apply_classes_from_context(&[super::ListClass::class_ref()], &cx.current);
        cx.restore();

        self.for_each_child_mut(&mut |child| {
            cx.style_view(child);
            false
        });
    }

    fn compute_layout(&mut self, cx: &mut crate::context::ComputeLayoutCx) -> Option<Rect> {
        self.window_origin = Some(cx.window_origin);

        default_compute_layout(self, cx)
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
                        cx.app_state_mut().remove_view(&mut self.main_view);
                        let (main_view, main_view_scope) = (self.main_fn)(*val);
                        self.main_view = main_view.build();
                        self.main_view_scope = main_view_scope;

                        old_child_scope.dispose();
                        self.main_view.view_data().id.set_parent(self.id());
                        view_children_set_parent_id(&*self.main_view);
                        cx.request_all(self.id());
                    }
                }
            }
        }
    }

    fn event(
        &mut self,
        cx: &mut crate::context::EventCx,
        id_path: Option<&[Id]>,
        event: Event,
    ) -> crate::EventPropagation {
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
        default_event(self, cx, id_path, event)
    }
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
/// # use floem::widgets::dropdown;
/// # use floem::views::empty;
/// # use floem::views::Decorators;
/// // root view
/// empty()
///     .style(|s|
///         s.class(dropdown::DropDownClass, |s| {
///             s.set(dropdown::CloseOnAccept, false)
///         })
///  );
///```
pub fn dropdown<MF, I, T, LF, AIF>(
    active_item: AIF,
    main_view: MF,
    iterator: I,
    list_item_fn: LF,
) -> DropDown<T>
where
    MF: Fn(T) -> AnyView + 'static,
    I: IntoIterator<Item = T> + Clone + 'static,
    LF: Fn(T) -> AnyView + Clone + 'static,
    T: Clone + 'static,
    AIF: Fn() -> T + 'static,
{
    let dropdown_id = Id::next();

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
            .on_event_stop(EventListener::FocusLost, move |_| {
                dropdown_id.update_state(Message::ListFocusLost);
            });
        let inner_list_id = View::view_data(&inner_list).id();
        scroll(inner_list)
            .on_event_stop(EventListener::FocusGained, move |_| {
                inner_list_id.request_focus();
            })
            .class(DropDownScrollClass)
            .any()
    });

    let initial = create_updater(active_item, move |new_state| {
        dropdown_id.update_state(Message::ActiveElement(Box::new(new_state)));
    });

    let main_fn = Box::new(as_child_of_current_scope(main_view));

    let (child, main_view_scope) = main_fn(initial);

    DropDown {
        view_data: ViewData::new(dropdown_id),
        main_view: Box::new(child.build()),
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
    .class(DropDownClass)
}

impl<T> DropDown<T> {
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
            self.id().update_state(Message::OpenState(false));
        } else {
            self.id().request_layout();
            self.id().update_state(Message::OpenState(true));
        }
    }

    fn open_dropdown(&mut self, cx: &mut crate::context::UpdateCx) {
        if self.overlay_id.is_none() {
            self.id().request_layout();
            cx.app_state.compute_layout();
            if let Some(layout) = cx.app_state.get_layout(self.id()) {
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
            let list = list().style(move |s| s.apply(list_style.clone())).build();
            let list_id = list.view_data().id;
            list_id.request_focus();
            list
        }));
    }

    /// Sets the custom style properties of the `DropDown`.
    pub fn dropdown_style(
        mut self,
        style: impl Fn(DropDownCustomStyle) -> DropDownCustomStyle + 'static,
    ) -> Self {
        let id = self.id();
        let offset = Widget::view_data_mut(&mut self).style.next_offset();
        let style = create_updater(
            move || style(DropDownCustomStyle(Style::new())),
            move |style| id.update_style(style.0, offset),
        );
        Widget::view_data_mut(&mut self).style.push(style.0);
        self
    }
}

pub struct DropDownCustomStyle(Style);

impl DropDownCustomStyle {
    /// Sets the `CloseOnAccept` property for the dropdown, which determines whether the dropdown
    /// should automatically close when an item is selected. The default value is `true`.
    ///
    /// # Arguments
    /// * `close`: If set to `true`, the dropdown will close upon item selection. If `false`, it
    /// will remain open after an item is selected.
    pub fn close_on_accept(mut self, close: bool) -> Self {
        self = Self(self.0.set(CloseOnAccept, close));
        self
    }

    /// Apply regular style properties
    pub fn style(mut self, style: impl Fn(Style) -> Style + 'static) -> Self {
        self = Self(self.0.apply(style(Style::new())));
        self
    }
}

impl<T> Drop for DropDown<T> {
    fn drop(&mut self) {
        if let Some(id) = self.overlay_id {
            remove_overlay(id)
        }
    }
}
