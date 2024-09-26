use std::{hash::Hash, marker::PhantomData};
use floem_reactive::{as_child_of_current_scope, create_effect, Scope};
use smallvec::SmallVec;
use taffy::style::Display;

use crate::{
    context::{StyleCx, UpdateCx},
    id::ViewId,
    style::DisplayProp,
    view::{IntoView, View},
};

use super::{apply_diff, diff, Diff, DiffOpAdd, FxIndexSet, HashRun};

type ViewFn<T> = Box<dyn Fn(T) -> (Box<dyn View>, Scope)>;

enum TabState<V> {
    Diff(Box<Diff<V>>),
    Active(usize),
    None
}

pub struct Tab<T>
where
    T: 'static,
{
    id: ViewId,
    active: Option<usize>,
    children: Vec<Option<(ViewId, Scope)>>,
    view_fn: ViewFn<T>,
    phatom: PhantomData<T>,
}

pub fn tab<IF, I, T, KF, K, VF, V>(
    active_fn: impl Fn() -> Option<usize> + 'static,
    each_fn: IF,
    key_fn: KF,
    view_fn: VF,
) -> Tab<T>
where
    IF: Fn() -> I + 'static,
    I: IntoIterator<Item = T>,
    KF: Fn(&T) -> K + 'static,
    K: Eq + Hash + 'static,
    VF: Fn(T) -> V + 'static,
    V: IntoView + 'static,
    T: 'static,
{
    let id = ViewId::new();

    create_effect(move |prev_hash_run| {
        let items = each_fn();
        let items = items.into_iter().collect::<SmallVec<[_; 128]>>();
        let hashed_items = items.iter().map(&key_fn).collect::<FxIndexSet<_>>();
        let diff = if let Some(HashRun(prev_hash_run)) = prev_hash_run {
            let mut cmds = diff(&prev_hash_run, &hashed_items);
            let mut items = items
                .into_iter()
                .map(|i| Some(i))
                .collect::<SmallVec<[Option<_>; 128]>>();
            for added in &mut cmds.added {
                added.view = Some(items[added.at].take().unwrap());
            }
            cmds
        } else {
            let mut diff = Diff::default();
            for (i, item) in each_fn().into_iter().enumerate() {
                diff.added.push(DiffOpAdd {
                    at: i,
                    view: Some(item),
                });
            }
            diff
        };
        id.update_state(TabState::<T>::Diff(Box::new(diff)));
        HashRun(hashed_items)
    });

    create_effect(move |_| {
        let active_key = active_fn();
        match active_key {
            Some(key) => id.update_state(TabState::Active::<T>(key)),
            None => id.update_state(TabState::None::<T>),
        }
    });

    let view_fn = Box::new(as_child_of_current_scope(move |e| view_fn(e).into_any()));

    Tab {
        id,
        active: None,
        children: Vec::new(),
        view_fn,
        phatom: PhantomData,
    }
}

impl<T> View for Tab<T> {
    fn id(&self) -> ViewId {
        self.id
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        format!("Tab: {:?}", self.active).into()
    }

    fn update(&mut self, cx: &mut UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(state) = state.downcast::<TabState<T>>() {
            match *state {
                TabState::Diff(diff) => {
                    apply_diff(
                        self.id(),
                        cx.app_state,
                        *diff,
                        &mut self.children,
                        &self.view_fn,
                    );
                }
                TabState::Active(active) => {
                    self.active.replace(active);
                }
                TabState::None => {
                    self.active.take();
                }
            }
            self.id.request_all();
            for (child, _) in self.children.iter().flatten() {
                child.request_all();
            }
        }
    }

    fn style_pass(&mut self, cx: &mut StyleCx<'_>) {
        for (i, child) in self.id.children().into_iter().enumerate() {
            cx.style_view(child);
            let child_view = child.state();
            let mut child_view = child_view.borrow_mut();
            child_view.combined_style = child_view.combined_style.clone().set(
                DisplayProp,

                match self.active {
                    None => {
                        Display::None
                    }
                    Some(active_index) if active_index == i => {
                        Display::Flex
                    }
                    Some(_active_index) => {
                        // set display to none for non-active child
                        Display::None
                    }
                }
            );
        }
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        if let Some(active_index) = self.active {
            if let Some(Some((active, _))) = self
                .children
                .get(active_index)
                .or_else(|| self.children.first())
            {
                cx.paint_view(*active);
            }
        }
    }
}
