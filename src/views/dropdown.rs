use std::{any::Any, rc::Rc};

use floem_reactive::{as_child_of_current_scope, create_effect, create_updater, Scope};
use floem_winit::keyboard::{Key, NamedKey};
use kurbo::{Point, Rect};

use crate::{
    action::{add_overlay, remove_overlay},
    id::Id,
    style::{Style, StyleClass, Width},
    style_class,
    unit::PxPctAuto,
    view::{
        default_compute_layout, default_event, view_children_set_parent_id, IntoAnyView, IntoView,
        View, ViewData,
    },
    views::{list, Decorators, ListClass},
    EventPropagation,
};

type ChildFn<T> = dyn Fn(T) -> (Box<dyn View>, Scope);

style_class!(pub DropDownClass);

pub struct DropDown<T: 'static> {
    view_data: ViewData,
    main_view: Box<dyn View>,
    main_view_scope: Scope,
    main_fn: Box<ChildFn<T>>,
    list_view: Rc<dyn Fn() -> Box<dyn View>>,
    list_style: Style,
    overlay_id: Option<Id>,
    window_origin: Option<Point>,
}

enum Message {
    OpenState(bool),
    ActiveElement(Box<dyn Any>),
}

impl<T: 'static> View for DropDown<T> {
    fn view_data(&self) -> &ViewData {
        &self.view_data
    }

    fn view_data_mut(&mut self) -> &mut ViewData {
        &mut self.view_data
    }

    fn for_each_child<'a>(&'a self, for_each: &mut dyn FnMut(&'a dyn View) -> bool) {
        for_each(&self.main_view);
    }

    fn for_each_child_mut<'a>(&'a mut self, for_each: &mut dyn FnMut(&'a mut dyn View) -> bool) {
        for_each(&mut self.main_view);
    }

    fn for_each_child_rev_mut<'a>(
        &'a mut self,
        for_each: &mut dyn FnMut(&'a mut dyn View) -> bool,
    ) {
        for_each(&mut self.main_view);
    }

    fn style(&mut self, cx: &mut crate::context::StyleCx<'_>) {
        cx.save();
        self.list_style =
            Style::new().apply_classes_from_context(&[ListClass::class_ref()], &cx.current);
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
                Message::ActiveElement(val) => {
                    if let Ok(val) = val.downcast::<T>() {
                        let old_child_scope = self.main_view_scope;
                        cx.app_state_mut().remove_view(&mut self.main_view);
                        let (main_view, main_view_scope) = (self.main_fn)(*val);
                        self.main_view = main_view.into_view().any();
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
        event: crate::event::Event,
    ) -> crate::EventPropagation {
        #[allow(clippy::single_match)]
        match event {
            crate::event::Event::PointerDown(_) => {
                self.swap_state();
                return EventPropagation::Stop;
            }
            crate::event::Event::KeyUp(ref key_event)
                if key_event.key.logical_key == Key::Named(NamedKey::Enter) =>
            {
                self.swap_state()
            }
            _ => {}
        }
        default_event(self, cx, id_path, event.clone())
    }
}

pub fn dropdown<MF, I, T, V2, AIF>(main_view: MF, iterator: I, active_item: AIF) -> DropDown<T>
where
    MF: Fn(T) -> Box<dyn View> + 'static,
    I: IntoIterator<Item = V2> + Clone + 'static,
    V2: IntoView + 'static,
    T: Clone + 'static,
    AIF: Fn() -> T + 'static,
{
    let dropdown_id = Id::next();

    let list_view = Rc::new(move || {
        let iterator = iterator.clone();
        list(iterator).keyboard_navigatable().any()
    });

    let initial = create_updater(active_item, move |new_state| {
        dropdown_id.update_state(Message::ActiveElement(Box::new(new_state)));
    });

    let main_fn = Box::new(as_child_of_current_scope(main_view));

    let (child, main_view_scope) = main_fn(initial);

    DropDown {
        view_data: ViewData::new(dropdown_id),
        main_view: child.any(),
        main_view_scope,
        main_fn,
        list_view,
        list_style: Style::new(),
        overlay_id: None,
        window_origin: None,
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
            if let Some(layout) = cx.app_state.get_layout(self.id()) {
                self.update_list_style(layout.size.width as f64);
                let point =
                    self.window_origin.unwrap_or_default() + (0., layout.size.height as f64);
                self.create_overlay(point);
            }
        }
    }

    fn close_dropdown(&mut self) {
        if let Some(id) = self.overlay_id.take() {
            remove_overlay(id);
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
            let list_id = list.view_data().id;
            list_id.request_focus();
            list
        }));
    }
}

impl<T> Drop for DropDown<T> {
    fn drop(&mut self) {
        if let Some(id) = self.overlay_id {
            remove_overlay(id)
        }
    }
}
