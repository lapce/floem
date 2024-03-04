use floem_reactive::{as_child_of_current_scope, create_updater, Scope};

use crate::{
    id::Id,
    view::{view_children_set_parent_id, AnyView, View, ViewData, Widget},
};

type ChildFn<T> = dyn Fn(T) -> (Box<dyn Widget>, Scope);

/// A container for a dynamically updating View. See [`dyn_container`]
pub struct DynamicContainer<T: 'static> {
    data: ViewData,
    child: Box<dyn Widget>,
    child_scope: Scope,
    child_fn: Box<ChildFn<T>>,
}

/// A container for a dynamically updating View
///
/// ## Example
/// ```
/// use floem::{
///     reactive::create_rw_signal,
///     view::View,
///     views::{dyn_container, label, v_stack, Decorators},
///     widgets::toggle_button,
/// };
///
/// #[derive(Clone)]
/// enum ViewSwitcher {
///     One,
///     Two,
/// }
///
/// fn app_view() -> impl View {
///     let view = create_rw_signal(ViewSwitcher::One);
///     v_stack((
///         toggle_button(|| true)
///             .on_toggle(move |is_on| {
///                 if is_on {
///                     view.update(|val| *val = ViewSwitcher::One);
///                 } else {
///                     view.update(|val| *val = ViewSwitcher::Two);
///                 }
///             })
///             .style(|s| s.margin_bottom(20)),
///         dyn_container(
///             move || view.get(),
///             move |value| match value {
///                 ViewSwitcher::One => label(|| "One").any(),
///                 ViewSwitcher::Two => v_stack((label(|| "Stacked"), label(|| "Two"))).any(),
///             },
///         ),
///     ))
///     .style(|s| {
///         s.width_full()
///             .height_full()
///             .items_center()
///             .justify_center()
///             .gap(10, 0)
///     })
/// }
///
/// ```
///
pub fn dyn_container<CF: Fn(T) -> AnyView + 'static, T: 'static>(
    update_view: impl Fn() -> T + 'static,
    child_fn: CF,
) -> DynamicContainer<T> {
    let id = Id::next();

    let initial = create_updater(update_view, move |new_state| id.update_state(new_state));

    let child_fn = Box::new(as_child_of_current_scope(move |e| child_fn(e).build()));
    let (child, child_scope) = child_fn(initial);
    DynamicContainer {
        data: ViewData::new(id),
        child,
        child_scope,
        child_fn,
    }
}

impl<T: 'static> View for DynamicContainer<T> {
    fn view_data(&self) -> &ViewData {
        &self.data
    }

    fn view_data_mut(&mut self) -> &mut ViewData {
        &mut self.data
    }

    fn build(self) -> Box<dyn Widget> {
        Box::new(self)
    }
}

impl<T: 'static> Widget for DynamicContainer<T> {
    fn view_data(&self) -> &ViewData {
        &self.data
    }

    fn view_data_mut(&mut self) -> &mut ViewData {
        &mut self.data
    }

    fn for_each_child<'a>(&'a self, for_each: &mut dyn FnMut(&'a dyn Widget) -> bool) {
        for_each(&self.child);
    }

    fn for_each_child_mut<'a>(&'a mut self, for_each: &mut dyn FnMut(&'a mut dyn Widget) -> bool) {
        for_each(&mut self.child);
    }

    fn for_each_child_rev_mut<'a>(
        &'a mut self,
        for_each: &mut dyn FnMut(&'a mut dyn Widget) -> bool,
    ) {
        for_each(&mut self.child);
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "DynamicContainer".into()
    }

    fn update(&mut self, cx: &mut crate::context::UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(val) = state.downcast::<T>() {
            let old_child_scope = self.child_scope;
            cx.app_state_mut().remove_view(&mut self.child);
            (self.child, self.child_scope) = (self.child_fn)(*val);
            old_child_scope.dispose();
            self.child.view_data().id().set_parent(self.id());
            view_children_set_parent_id(&*self.child);
            cx.request_all(self.id());
        }
    }
}
