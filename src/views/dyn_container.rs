use std::any::Any;

use floem_reactive::{as_child_of_current_scope, create_updater, Scope};

use crate::{
    animate::RepeatMode,
    context::UpdateCx,
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
    next_val_state: Option<(T, ViewId, Scope)>,
    num_started_animations: u16,
}

/// A container for a dynamically updating View
///
/// ## Example
/// ```
/// use floem::{
///     reactive::{create_rw_signal, SignalUpdate, SignalGet},
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

    let initial = create_updater(update_view, move |new_state| {
        id.update_state(DynMessage::Val(Box::new(new_state)));
    });

    let child_fn = Box::new(as_child_of_current_scope(move |e| child_fn(e).into_any()));
    let (child, child_scope) = child_fn(initial);
    let child_id = child.id();
    id.set_children(vec![child]);
    DynamicContainer {
        id,
        child_scope,
        child_id,
        child_fn,
        next_val_state: None,
        num_started_animations: 0,
    }
}
enum DynMessage {
    Val(Box<dyn Any>),
    CompletedAnimation,
}

impl<T: 'static> View for DynamicContainer<T> {
    fn id(&self) -> ViewId {
        self.id
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "Dynamic Container".into()
    }

    fn update(&mut self, cx: &mut UpdateCx, state: Box<dyn Any>) {
        if let Ok(message) = state.downcast::<DynMessage>() {
            match *message {
                DynMessage::Val(val) => {
                    if let Ok(val) = val.downcast::<T>() {
                        self.new_val(cx, *val);
                    }
                }
                DynMessage::CompletedAnimation => {
                    self.num_started_animations = self.num_started_animations.saturating_sub(1);
                    if self.num_started_animations == 0 {
                        let next_val_state = self
                            .next_val_state
                            .take()
                            .expect("when waiting for animations the next value will be stored and all message effects should have been dropped by dropping the child id if another value was sent before the animations finished");
                        self.swap_val(cx, next_val_state);
                    }
                }
            }
        }
    }
}
impl<T> DynamicContainer<T> {
    fn new_val(&mut self, cx: &mut UpdateCx, val: T) {
        let id = self.id;

        let old_child_scope = self.child_scope;
        let old_child_id = self.child_id;

        if self.num_started_animations > 0 {
            // another update was sent before the animations finished processing
            let next_state = self
                .next_val_state
                .take()
                .expect("valid when waiting for animations");
            // force swap
            self.swap_val(cx, next_state);
            self.num_started_animations = 0;
        }

        self.num_started_animations =
            animations_recursive_on_remove(id, old_child_id, old_child_scope);

        let next_state = (val, old_child_id, old_child_scope);
        if self.num_started_animations == 0 {
            // after recursively checking, no animations were found that needed to be started
            self.swap_val(cx, next_state);
        } else {
            self.next_val_state = Some(next_state);
        }
    }

    fn swap_val(&mut self, cx: &mut UpdateCx, next_val_state: (T, ViewId, Scope)) {
        let (val, old_child_id, old_child_scope) = next_val_state;
        let (new_child, new_child_scope) = (self.child_fn)(val);
        self.child_id = new_child.id();
        self.id.set_children(vec![new_child]);
        self.child_scope = new_child_scope;
        cx.app_state_mut().remove_view(old_child_id);
        old_child_scope.dispose();
        animations_recursive_on_create(self.child_id);
        self.id.request_all();
    }
}

fn animations_recursive_on_remove(id: ViewId, child_id: ViewId, child_scope: Scope) -> u16 {
    let mut wait_for = 0;
    let state = child_id.state();
    let mut state = state.borrow_mut();
    let animations = &mut state.animations.stack;
    let mut request_style = false;
    for anim in animations {
        if anim.run_on_remove && !matches!(anim.repeat_mode, RepeatMode::LoopForever) {
            anim.reverse_once = true;
            anim.start_mut();
            request_style = true;
            wait_for += 1;
            let trigger = anim.on_complete_trigger;
            child_scope.create_updater(
                move || trigger.track(),
                move |_| {
                    id.update_state(DynMessage::CompletedAnimation);
                },
            );
        }
    }
    drop(state);
    if request_style {
        child_id.request_style();
    }

    child_id
        .children()
        .into_iter()
        .fold(wait_for, |acc, child_id| {
            acc + animations_recursive_on_remove(id, child_id, child_scope)
        })
}
fn animations_recursive_on_create(child_id: ViewId) {
    let state = child_id.state();
    let mut state = state.borrow_mut();
    let animations = &mut state.animations.stack;
    let mut request_style = false;
    for anim in animations {
        if anim.run_on_create && !matches!(anim.repeat_mode, RepeatMode::LoopForever) {
            anim.start_mut();
            request_style = true;
        }
    }
    drop(state);
    if request_style {
        child_id.request_style();
    }

    child_id
        .children()
        .into_iter()
        .for_each(animations_recursive_on_create);
}
