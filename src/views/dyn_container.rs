use floem_reactive::{as_child_of_current_scope, create_updater, Scope};

use crate::{
    id::ViewId,
    view::{IntoView, View},
};

type ChildFn<T> = dyn Fn(T) -> (Box<dyn View>, Scope);

/// A container for a dynamically updating View. See [`dyn_container`]
pub struct DynamicContainer<T: 'static> {
    id: ViewId,
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
pub fn dyn_container<CF: Fn(T) -> Box<dyn View> + 'static, T: 'static>(
    update_view: impl Fn() -> T + 'static,
    child_fn: CF,
) -> DynamicContainer<T> {
    let id = ViewId::new();

    let initial = create_updater(update_view, move |new_state| id.update_state(new_state));

    let child_fn = Box::new(as_child_of_current_scope(move |e| {
        child_fn(e).into_any_view()
    }));
    let (child, child_scope) = child_fn(initial);
    id.set_children(vec![child]);
    DynamicContainer {
        id,
        child_scope,
        child_fn,
    }
}

impl<T: 'static> View for DynamicContainer<T> {
    fn id(&self) -> ViewId {
        self.id
    }

    fn update(&mut self, cx: &mut crate::context::UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(val) = state.downcast::<T>() {
            let old_child_scope = self.child_scope;
            for child in self.id.children() {
                cx.app_state_mut().remove_view(child);
            }
            let (child, child_scope) = (self.child_fn)(*val);
            self.child_scope = child_scope;
            self.id.set_children(vec![child]);
            old_child_scope.dispose();
            self.id.request_all();
        }
    }
}
