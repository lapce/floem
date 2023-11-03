use floem_reactive::{as_child_of_current_scope, create_effect, Scope};
use kurbo::Rect;

use crate::{
    id::Id,
    view::{view_children_set_parent_id, ChangeFlags, View},
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

    fn child(&self, id: Id) -> Option<&dyn View> {
        if self.child.id() == id {
            Some(&*self.child)
        } else {
            None
        }
    }

    fn child_mut(&mut self, id: Id) -> Option<&mut dyn View> {
        if self.child.id() == id {
            Some(&mut *self.child)
        } else {
            None
        }
    }

    fn children(&self) -> Vec<&dyn View> {
        vec![&*self.child]
    }

    fn children_mut(&mut self) -> Vec<&mut dyn View> {
        vec![&mut *self.child]
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "DynamicContainer".into()
    }

    fn update(
        &mut self,
        cx: &mut crate::context::UpdateCx,
        state: Box<dyn std::any::Any>,
    ) -> crate::view::ChangeFlags {
        if let Ok(val) = state.downcast::<T>() {
            let old_child_scope = self.child_scope;
            (self.child, self.child_scope) = (self.child_fn)(*val);
            old_child_scope.dispose();
            self.child.id().set_parent(self.id);
            view_children_set_parent_id(&*self.child);
            cx.request_all(self.id());
            ChangeFlags::all()
        } else {
            ChangeFlags::empty()
        }
    }

    fn layout(&mut self, cx: &mut crate::context::LayoutCx) -> taffy::prelude::Node {
        cx.layout_node(self.id, true, |cx| vec![self.child.layout_main(cx)])
    }

    fn compute_layout(&mut self, cx: &mut crate::context::LayoutCx) -> Option<Rect> {
        Some(self.child.compute_layout_main(cx))
    }

    fn event(
        &mut self,
        cx: &mut crate::context::EventCx,
        id_path: Option<&[Id]>,
        event: crate::event::Event,
    ) -> bool {
        if cx.should_send(self.child.id(), &event) {
            self.child.event_main(cx, id_path, event)
        } else {
            false
        }
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        self.child.paint_main(cx);
    }
}
