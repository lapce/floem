use floem_reactive::{as_child_of_current_scope, create_updater, Scope};

use crate::{
    id::ViewId,
    view::{IntoView, View},
    AnyView,
};

/// A container for a dynamically updating View. See [`dyn_container`]
pub struct DynamicContainer {
    id: ViewId,
    child_scope: Scope,
}

/// A container for a dynamically updating View
///
/// ## Example
/// ```
/// use floem::{
///     reactive::create_rw_signal,
///     View, IntoView,
///     views::{dyn_container, label, v_stack, toggle_button, Decorators},
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
///             move || match view.get() {
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
///             .gap(10, 0)
///     })
/// }
///
/// ```
///
pub fn dyn_container<VF, IV>(view_fn: VF) -> DynamicContainer
where
    VF: Fn() -> IV + 'static,
    IV: IntoView,
{
    let id = ViewId::new();
    let view_fn = Box::new(as_child_of_current_scope(move |_| view_fn().into_any()));

    let (child, child_scope) = create_updater(
        move || view_fn(()),
        move |new_state| id.update_state(new_state),
    );

    id.set_children(vec![child]);
    DynamicContainer { id, child_scope }
}

impl View for DynamicContainer {
    fn id(&self) -> ViewId {
        self.id
    }

    fn update(&mut self, cx: &mut crate::context::UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(val) = state.downcast::<(AnyView, Scope)>() {
            let old_child_scope = self.child_scope;
            for child in self.id.children() {
                cx.app_state_mut().remove_view(child);
            }
            let (child, child_scope) = *val;
            self.child_scope = child_scope;
            self.id.set_children(vec![child]);
            old_child_scope.dispose();
            self.id.request_all();
        }
    }
}
