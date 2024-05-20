use floem_reactive::{as_child_of_current_scope, create_updater, Scope};

use crate::{
    id::ViewId,
    view::{IntoView, View},
};

/// A container for a dynamically updating View. See [`dyn_view`]
pub struct DynamicView {
    id: ViewId,
    child_scope: Scope,
}

/// A container for a dynamically updating View
pub fn dyn_view<VF, IV>(view_fn: VF) -> DynamicView
where
    VF: Fn() -> IV + 'static,
    IV: IntoView,
{
    let id = ViewId::new();
    let view_fn = Box::new(as_child_of_current_scope(move |_| view_fn().into_any()));

    let (child, child_scope) = create_updater(
        move || view_fn(()),
        move |(new_view, new_scope)| {
            let current_children = id.children();
            id.set_children(vec![new_view]);
            id.update_state((current_children, new_scope));
        },
    );

    id.set_children(vec![child]);
    DynamicView { id, child_scope }
}

impl View for DynamicView {
    fn id(&self) -> ViewId {
        self.id
    }

    fn update(&mut self, cx: &mut crate::context::UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(val) = state.downcast::<(Vec<ViewId>, Scope)>() {
            let old_child_scope = self.child_scope;
            let (old_children, child_scope) = *val;
            self.child_scope = child_scope;
            for child in old_children {
                cx.app_state_mut().remove_view(child);
            }
            old_child_scope.dispose();
            self.id.request_all();
        }
    }
}
