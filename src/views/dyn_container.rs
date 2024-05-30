use floem_reactive::{as_child_of_current_scope, create_updater, Scope};

use crate::{
    view::{AnyView, View},
    IntoView, ViewId,
};

type ChildFn<T> = dyn Fn(T) -> (AnyView, Scope);

/// A container for a dynamically updating View. See [`dyn_container`]
pub struct DynamicContainer<T: 'static> {
    id: ViewId,
    child_id: ViewId,
    child_scope: Scope,
    child_fn: Box<ChildFn<T>>,
}

/// A container for a dynamically updating View
///
/// ## Example
/// ```
/// use floem::{
///     reactive::create_rw_signal,
///     View, IntoView,
///     views::{dyn_container, label, v_stack, Decorators, toggle_button},
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
///                 ViewSwitcher::One => label(|| "One").into_any(),
///                 ViewSwitcher::Two => v_stack((label(|| "Stacked"), label(|| "Two"))).into_any(),
///             },
///         ),
///     ))
///     .style(|s| {
///         s.width_full()
///             .height_full()
///             .items_center()
///             .justify_center()
///             .row_gap(10)
///     })
/// }
///
/// ```
///
pub fn dyn_container<CF: Fn(T) -> IV + 'static, T: 'static, IV: IntoView>(
    update_view: impl Fn() -> T + 'static,
    child_fn: CF,
) -> DynamicContainer<T> {
    let id = ViewId::new();

    let initial = create_updater(update_view, move |new_state| id.update_state(new_state));

    let child_fn = Box::new(as_child_of_current_scope(move |e| child_fn(e).into_any()));
    let (child, child_scope) = child_fn(initial);
    let child_id = child.id();
    id.set_children(vec![child]);
    DynamicContainer {
        id,
        child_scope,
        child_id,
        child_fn,
    }
}

impl<T: 'static> View for DynamicContainer<T> {
    fn id(&self) -> ViewId {
        self.id
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "DynamicContainer".into()
    }

    fn update(&mut self, cx: &mut crate::context::UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(val) = state.downcast::<T>() {
            let old_child_scope = self.child_scope;
            let old_child_id = self.child_id;
            let (new_child, new_child_scope) = (self.child_fn)(*val);
            self.child_id = new_child.id();
            self.id.set_children(vec![new_child]);
            self.child_scope = new_child_scope;
            cx.app_state_mut().remove_view(old_child_id);
            old_child_scope.dispose();
            self.id.request_all();
        }
    }
}
