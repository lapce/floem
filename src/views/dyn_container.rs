use floem_reactive::{as_child_of_current_scope, create_effect, Scope};

use crate::{
    id::Id,
    view::{view_children_set_parent_id, View},
};

type ChildFn<T> = dyn Fn(T) -> (Box<dyn View>, Scope);

/// A container for a dynamically updating View. See [`dyn_container`]
pub struct DynamicContainer<T: 'static> {
    id: Id,
    child: Box<dyn View>,
    child_scope: Scope,
    child_fn: Box<ChildFn<T>>,
}

/// A container for a dynamically updating View
///
/// ## Example
/// ```ignore
/// #[derive(Debug, Clone)]
/// enum ViewSwitcher {
///     One,
///     Two,
/// }
///
/// fn app() -> impl View {
///
///     let view = create_rw_signal(ViewSwitcher::One);
///
///     let button = || {
///         // imaginary toggle button and state
///         toggle_button(
///             // on toggle function
///             move |state| match state {
///                 State::On => view.update(|val| *val = Views::One),
///                 State::Off => view.update(|val| *val = Views::Two),
///             },
///         )
///     };
///
///     stack(|| (
///         button(),
///         dyn_container(
///             move || view.get(),
///             move |val: ViewSwitcher| match val {
///                 ViewSwitcher::One => Box::new(label(|| "one".into())),
///                 ViewSwitcher::Two => {
///                     Box::new(
///                       stack(|| (
///                           label(|| "stacked".into()),
///                           label(|| "two".into())
///                       ))
///                     ),
///                 }
///             },
///         )
///     ))
///     .style(|| {
///         Style::new()
///             .size(100.pct(), 100.pct())
///             .items_center()
///             .justify_center()
///             .gap(points(10.))
///     })
/// }
/// ```
///
/// See [container_box](crate::views::container_box()) for more documentation on a general container
pub fn dyn_container<CF: Fn(T) -> Box<dyn View> + 'static, T: 'static>(
    update_view: impl Fn() -> T + 'static,
    child_fn: CF,
) -> DynamicContainer<T> {
    let id = Id::next();

    create_effect(move |_| {
        id.update_state(update_view(), false);
    });

    let child_fn = Box::new(as_child_of_current_scope(child_fn));
    DynamicContainer {
        id,
        child: Box::new(crate::views::empty()),
        child_scope: Scope::new(),
        child_fn,
    }
}

impl<T: 'static> View for DynamicContainer<T> {
    fn id(&self) -> Id {
        self.id
    }

    fn for_each_child<'a>(&'a self, for_each: &mut dyn FnMut(&'a dyn View) -> bool) {
        for_each(&self.child);
    }

    fn for_each_child_mut<'a>(&'a mut self, for_each: &mut dyn FnMut(&'a mut dyn View) -> bool) {
        for_each(&mut self.child);
    }

    fn for_each_child_rev_mut<'a>(
        &'a mut self,
        for_each: &mut dyn FnMut(&'a mut dyn View) -> bool,
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
            self.child.id().set_parent(self.id);
            view_children_set_parent_id(&*self.child);
            cx.request_all(self.id());
        }
    }
}
