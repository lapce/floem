use std::{hash::Hash, marker::PhantomData};

use floem_reactive::{as_child_of_current_scope, create_effect, Scope};
use smallvec::SmallVec;
use taffy::style::Display;

use crate::{
    context::{StyleCx, UpdateCx},
    id::Id,
    style::DisplayProp,
    view::{AnyWidget, View, ViewData, Widget},
};

use super::{apply_diff, diff, Diff, DiffOpAdd, FxIndexSet, HashRun};

enum TabState<V> {
    Diff(Box<Diff<V>>),
    Active(usize),
}

pub struct Tab<T>
where
    T: 'static,
{
    data: ViewData,
    active: usize,
    children: Vec<Option<(AnyWidget, Scope)>>,
    view_fn: Box<dyn Fn(T) -> (AnyWidget, Scope)>,
    phatom: PhantomData<T>,
}

pub fn tab<IF, I, T, KF, K, VF, V>(
    active_fn: impl Fn() -> usize + 'static,
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
    V: View + 'static,
    T: 'static,
{
    let id = Id::next();

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
        id.update_state(TabState::Diff(Box::new(diff)));
        HashRun(hashed_items)
    });

    create_effect(move |_| {
        let active = active_fn();
        id.update_state(TabState::Active::<T>(active));
    });

    let view_fn = Box::new(as_child_of_current_scope(move |e| view_fn(e).build()));

    Tab {
        data: ViewData::new(id),
        active: 0,
        children: Vec::new(),
        view_fn,
        phatom: PhantomData,
    }
}

impl<T> View for Tab<T> {
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

impl<T> Widget for Tab<T> {
    fn view_data(&self) -> &ViewData {
        &self.data
    }

    fn view_data_mut(&mut self) -> &mut ViewData {
        &mut self.data
    }

    fn for_each_child<'a>(&'a self, for_each: &mut dyn FnMut(&'a dyn Widget) -> bool) {
        for child in self.children.iter().filter_map(|child| child.as_ref()) {
            if for_each(&child.0) {
                break;
            }
        }
    }

    fn for_each_child_mut<'a>(&'a mut self, for_each: &mut dyn FnMut(&'a mut dyn Widget) -> bool) {
        for child in self.children.iter_mut().filter_map(|child| child.as_mut()) {
            if for_each(&mut child.0) {
                break;
            }
        }
    }

    fn for_each_child_rev_mut<'a>(
        &'a mut self,
        for_each: &mut dyn FnMut(&'a mut dyn Widget) -> bool,
    ) {
        for child in self
            .children
            .iter_mut()
            .rev()
            .filter_map(|child| child.as_mut())
        {
            if for_each(&mut child.0) {
                break;
            }
        }
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        format!("Tab: {}", self.active).into()
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
                    self.active = active;
                }
            }
            cx.request_all(self.id());
            for (child, _) in self.children.iter().flatten() {
                cx.request_all(child.view_data().id());
            }
        }
    }

    fn style(&mut self, cx: &mut StyleCx<'_>) {
        for (i, child) in self
            .children
            .iter_mut()
            .enumerate()
            .filter_map(|(i, child)| child.as_mut().map(|child| (i, &mut child.0)))
        {
            cx.style_view(child);
            let child_view = cx.app_state_mut().view_state(child.view_data().id());
            child_view.combined_style = child_view.combined_style.clone().set(
                DisplayProp,
                if i != self.active {
                    // set display to none for non active child
                    Display::None
                } else {
                    Display::Flex
                },
            );
        }
    }
}
