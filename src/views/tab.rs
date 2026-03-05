#![deny(missing_docs)]
use std::{hash::Hash, marker::PhantomData};

use floem_reactive::{Effect, Scope};
use smallvec::SmallVec;

use crate::{
    context::{StyleCx, UpdateCx},
    style::recalc::StyleReason,
    style_class,
    view::{IntoView, View, ViewId},
};

use super::{Diff, DiffOpAdd, FxIndexSet, HashRun, apply_diff, diff};

type ViewFn<T> = Box<dyn Fn(T) -> (Box<dyn View>, Scope)>;

style_class!(
    /// Set class to TabSelector.
    pub TabSelectorClass
);

enum TabState<V> {
    Diff(Box<Diff<V>>),
    Active(usize),
    None,
}

/// Tab widget.
///
/// See [tab] for examples.
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

/// A tab widget. Create tabs from static or dynamic lists.
///
/// ### Simple example
/// ```rust
/// # use floem::prelude::*;
/// # use floem::theme;
/// // Tabs from static list:
/// let tabs = RwSignal::new(vec!["tab1, tab2, tab3"]);
/// let active_tab = RwSignal::new(0);
///
/// let side_bar = tabs
///     .get()
///     .into_iter()
///     .enumerate()
///     .map(move |(idx, item)| {
///         item.style(move |s| s
///             .height(36.)
///             .apply_if(idx != active_tab.get(), |s| s.apply(theme::hover_style()))
///         )
///     })
///     .list()
///     .on_select(move |idx| {
///         if let Some(idx) = idx {
///             active_tab.set(idx);
///         }
/// });
///
/// let static_tabs = tab(
///     move || Some(active_tab.get()),
///     move || tabs.get(),
///     |it| *it,
///     |tab_content| tab_content
///         .container()
///         .style(|s| s.size(150., 150.).padding(10.))
/// );
///
/// stack((side_bar, static_tabs));
/// ```
/// ### Complex example
/// ```rust
/// # use floem::prelude::*;
/// # use floem::event::listener;
/// # use floem::reactive::Effect;
/// # use floem::views::{Button, Label, Stack, dyn_stack};
/// // Tabs from a dynamic list using current APIs.
/// #[derive(Clone)]
/// struct TabContent {
///     idx: usize,
///     name: String,
/// }
///
/// #[derive(Clone, Copy)]
/// enum Action {
///     Add,
///     Remove,
///     None,
/// }
///
/// let tabs = RwSignal::new(Vec::<TabContent>::new());
/// let active_tab = RwSignal::new(Some(0usize));
/// let tab_action = RwSignal::new(Action::None);
///
/// Effect::new(move |_| match tab_action.get() {
///     Action::Add => {
///         tabs.update(|tabs| {
///             let idx = tabs.len();
///             tabs.push(TabContent {
///                 idx,
///                 name: format!("Tab {idx}"),
///             });
///         });
///         tab_action.set(Action::None);
///     }
///     Action::Remove => {
///         tabs.update(|tabs| {
///             tabs.pop();
///         });
///         tab_action.set(Action::None);
///     }
///     Action::None => {}
/// });
///
/// let tabs_view = dyn_stack(
///     move || tabs.get(),
///     |tab| tab.idx,
///     move |tab| {
///         let idx = tab.idx;
///         Label::new(format!("{} {}", tab.name, tab.idx))
///             .style(move |s| {
///                 s.width_full()
///                     .height(36.0)
///                     .apply_if(active_tab.get() == Some(idx), |s| s.font_bold())
///             })
///             .on_event_stop(listener::Click, move |_cx, _| active_tab.set(Some(idx)))
///     },
/// )
/// .scroll()
/// .style(|s| s.width(140.).height_full().padding(5.0));
///
/// let tabs_content_view = tab(
///     move || active_tab.get(),
///     move || tabs.get(),
///     |tab| tab.idx,
///     move |tab| {
///         Stack::vertical((
///             Label::new(tab.name.clone()).style(|s| s.font_size(15.0).font_bold()),
///             Label::new(format!("{}", tab.idx)).style(|s| s.font_size(20.0).font_bold()),
///             Label::new("is now active"),
///         ))
///         .style(|s| {
///             s.size(150.0, 150.0)
///                 .items_center()
///                 .justify_center()
///                 .row_gap(10.0)
///         })
///     },
/// );
///
/// let controls = Stack::new((
///     Button::new("Add tab").on_event_stop(listener::Click, move |_cx, _| {
///         tab_action.set(Action::Add)
///     }),
///     Button::new("Remove tab").on_event_stop(listener::Click, move |_cx, _| {
///         tab_action.set(Action::Remove)
///     }),
/// ));
///
/// let _layout = Stack::vertical((controls, Stack::new((tabs_view, tabs_content_view))))
///     .style(|s| s.size_full().row_gap(10.0));
/// ```
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

    Effect::new(move |prev_hash_run| {
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

    Effect::new(move |_| {
        let active = active_fn();
        match active {
            Some(idx) => id.update_state(TabState::Active::<T>(idx)),
            None => id.update_state(TabState::None::<T>),
        }
    });

    let view_fn = Box::new(Scope::current().enter_child(move |e| view_fn(e).into_any()));

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
                        cx.window_state,
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
            self.id.request_style(StyleReason::style_pass());
        }
    }

    fn style_pass(&mut self, _cx: &mut StyleCx<'_>) {
        for (i, child) in self.id.children().into_iter().enumerate() {
            match self.active {
                Some(act_idx) if act_idx == i => {
                    child.set_visible();
                }
                _ => {
                    child.set_hidden();
                }
            }
        }
    }
}
