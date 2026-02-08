#![deny(missing_docs)]
use std::{hash::Hash, marker::PhantomData};

use floem_reactive::{Effect, Scope};
use smallvec::SmallVec;

use crate::{
    context::{StyleCx, UpdateCx},
    style_class,
    view::ViewId,
    view::{IntoView, View},
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
/// # use floem::theme;
/// # use floem::theme::StyleThemeExt;
/// # use floem_reactive::create_effect;
/// // Tabs from dynamic list
/// #[derive(Clone)]
/// struct TabContent {
///     idx: usize,
///     name: String,
/// }
///
/// impl TabContent {
///     fn new(tabs_count: usize) -> Self {
///         Self {
///             idx: tabs_count,
///             name: format!("Tab with index"),
///         }
///     }
/// }
///
/// #[derive(Clone)]
/// enum Action {
///     Add,
///     Remove,
///     None,
/// }
/// let tabs = RwSignal::new(vec![]);
/// let active_tab = RwSignal::new(None::<usize>);
/// let tab_action = RwSignal::new(Action::None);
/// create_effect(move |_| match tab_action.get() {
///     Action::Add => {
///         tabs.update(|tabs| tabs.push(TabContent::new(tabs.len())));
///     }
///     Action::Remove => {
///         tabs.update(|tabs| { tabs.pop(); });
///     }
///     Action::None => ()
/// });///
/// let tabs_view = stack((dyn_stack(
///     move || tabs.get(),
///     |tab| tab.idx,
///     move |tab| {
///         text(format!("{} {}", tab.name, tab.idx)).button().style(move |s| s
///             .width_full()
///             .height(36.px())
///             .apply_if(active_tab.get().is_some_and(|a| a == tab.idx), |s| {
///                 s.with_theme(|s, t| s.border_color(t.primary()))
///             })
///         )
///         .on_click_stop(move |_| {
///             active_tab.update(|a| {
///                 *a = Some(tab.idx);
///             });
///         })
///     },
/// )
/// .style(|s| s.flex_col().width_full().row_gap(5.))
/// .scroll()
/// .on_click_stop(move |_| {
///     if active_tab.with_untracked(|act| act.is_some()) {
///         active_tab.set(None)
///     }
/// })
/// .style(|s| s.size_full().padding(5.).padding_right(7.))
/// .scroll_style(|s| s.handle_thickness(6.).shrink_to_fit()),))
/// .style(|s| s
///     .width(140.)
///     .min_width(140.)
///     .height_full()
///     .border_right(1.)
///     .with_theme(|s, t| s.border_color(t.border_muted()))
/// );
/// let tabs_content_view = stack((
///     tab(
///         move || active_tab.get(),
///         move || tabs.get(),
///         |tab| tab.idx,
///         move |tab| {
///          v_stack((
///             label(move || format!("{}", tab.name)).style(|s| s
///                 .font_size(15.)
///                 .font_bold()),
///             label(move || format!("{}", tab.idx)).style(|s| s
///                 .font_size(20.)
///                 .font_bold()),
///             label(move || "is now active").style(|s| s
///                 .font_size(13.)),
///         )).style(|s| s
///             .size(150.px(), 150.px())
///             .items_center()
///             .justify_center()
///             .row_gap(10.))
///         },
///     ).style(|s| s.size_full()),
/// ))
/// .style(|s| s.size_full());
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
                    self.id.request_style_recursive();
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

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        if let Some(active_tab) = self.active
            && let Some(Some((active, _))) = self
                .children
                .get(active_tab)
                .or_else(|| self.children.first())
        {
            cx.paint_view(*active);
        }
    }
}
