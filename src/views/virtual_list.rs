use super::{
    virtual_stack, Decorators, Item, VirtualDirection, VirtualItemSize, VirtualStack, VirtualVector,
};
use crate::context::ComputeLayoutCx;
use crate::reactive::create_effect;
use crate::view::View;
use crate::EventPropagation;
use crate::{
    event::{Event, EventListener},
    id::Id,
    keyboard::{Key, NamedKey},
    view::{ViewData, Widget},
};
use floem_reactive::{create_rw_signal, RwSignal};
use kurbo::{Rect, Size};
use std::hash::Hash;
use std::rc::Rc;

enum ListUpdate {
    SelectionChanged,
    ScrollToSelected,
}

/// A view that is like a [`virtual_stack`](super::virtual_stack()) but also supports item selection.
/// See [`virtual_list`] and [`virtual_stack`](super::virtual_stack()).
pub struct VirtualList<T: 'static> {
    data: ViewData,
    direction: VirtualDirection,
    child_size: Size,
    selection: RwSignal<Option<usize>>,
    offsets: RwSignal<Vec<f64>>,
    child: VirtualStack<(usize, T)>,
}

impl<T> VirtualList<T> {
    pub fn selection(&self) -> RwSignal<Option<usize>> {
        self.selection
    }

    pub fn on_select(self, on_select: impl Fn(Option<usize>) + 'static) -> Self {
        create_effect(move |_| {
            let selection = self.selection.get();
            on_select(selection);
        });
        self
    }
}

/// A view that is like a [`virtual_stack`](super::virtual_stack()) but also supports item selection.
/// See the [`virtual_stack`](super::virtual_stack()) for more documentation and an example.
///
/// Selection is done using the following: up and down using arrow keys, top and bottom control using the home and end keys, and for the “acceptance” of an item using the Enter key.
pub fn virtual_list<T, IF, I, KF, K, VF, V>(
    direction: VirtualDirection,
    item_size: VirtualItemSize<T>,
    each_fn: IF,
    key_fn: KF,
    view_fn: VF,
) -> VirtualList<T>
where
    T: 'static,
    IF: Fn() -> I + 'static,
    I: VirtualVector<T>,
    KF: Fn(&T) -> K + 'static,
    K: Eq + Hash + 'static,
    VF: Fn(T) -> V + 'static,
    V: View + 'static,
{
    let id = Id::next();
    let selection = create_rw_signal(None);
    let length = create_rw_signal(0);
    let offsets = create_rw_signal(Vec::new());
    create_effect(move |_| {
        selection.track();
        id.update_state(ListUpdate::SelectionChanged);
    });

    let shared = Rc::new((each_fn, item_size));
    let shared_ = shared.clone();

    create_effect(move |_| {
        let mut items = (shared_.0)();

        let mut new_offsets = Vec::with_capacity(items.total_len());
        let mut current = 0.0;

        match &shared_.1 {
            VirtualItemSize::Fixed(item_size) => {
                let item_size = item_size();
                for _ in 0..items.total_len() {
                    new_offsets.push(current);
                    current += item_size;
                }
            }
            VirtualItemSize::Fn(size_fn) => {
                for item in items.slice(0..(items.total_len())) {
                    new_offsets.push(current);
                    current += size_fn(&item);
                }
            }
        };

        new_offsets.push(current);

        offsets.set(new_offsets);
    });

    let shared_ = shared.clone();
    let item_size = match shared.1 {
        VirtualItemSize::Fixed(..) => VirtualItemSize::Fixed(Box::new(move || match shared_.1 {
            VirtualItemSize::Fixed(ref f) => f(),
            VirtualItemSize::Fn(..) => panic!(),
        })),
        VirtualItemSize::Fn(..) => VirtualItemSize::Fn(Box::new(move |(_, e)| match shared_.1 {
            VirtualItemSize::Fixed(..) => panic!(),
            VirtualItemSize::Fn(ref f) => f(e),
        })),
    };
    let stack = virtual_stack(
        direction,
        item_size,
        move || {
            let vector = (shared.0)().enumerate();
            length.set(vector.total_len());
            vector
        },
        move |(_, e)| key_fn(e),
        move |(index, e)| {
            Item {
                data: ViewData::new(Id::next()),
                selection,
                index,
                child: view_fn(e).build(),
            }
            .on_click_stop(move |_| {
                if selection.get_untracked() != Some(index) {
                    selection.set(Some(index))
                }
            })
            .style(|s| s.width_full())
        },
    )
    .style(move |s| match direction {
        VirtualDirection::Horizontal => s.flex_row(),
        VirtualDirection::Vertical => s.flex_col(),
    });
    VirtualList {
        data: ViewData::new(id),
        selection,
        direction,
        offsets,
        child_size: Size::ZERO,
        child: stack,
    }
    .keyboard_navigatable()
    .on_event(EventListener::KeyDown, move |e| {
        if let Event::KeyDown(key_event) = e {
            match key_event.key.logical_key {
                Key::Named(NamedKey::Home) => {
                    if length.get_untracked() > 0 {
                        selection.set(Some(0));
                        id.update_state(ListUpdate::ScrollToSelected);
                    }
                    EventPropagation::Stop
                }
                Key::Named(NamedKey::End) => {
                    let length = length.get_untracked();
                    if length > 0 {
                        selection.set(Some(length - 1));
                        id.update_state(ListUpdate::ScrollToSelected);
                    }
                    EventPropagation::Stop
                }
                Key::Named(NamedKey::ArrowUp) => {
                    let current = selection.get_untracked();
                    match current {
                        Some(i) => {
                            if i > 0 {
                                selection.set(Some(i - 1));
                                id.update_state(ListUpdate::ScrollToSelected);
                            }
                        }
                        None => {
                            let length = length.get_untracked();
                            if length > 0 {
                                selection.set(Some(length - 1));
                                id.update_state(ListUpdate::ScrollToSelected);
                            }
                        }
                    }
                    EventPropagation::Stop
                }
                Key::Named(NamedKey::ArrowDown) => {
                    let current = selection.get_untracked();
                    match current {
                        Some(i) => {
                            if i < length.get_untracked() - 1 {
                                selection.set(Some(i + 1));
                                id.update_state(ListUpdate::ScrollToSelected);
                            }
                        }
                        None => {
                            if length.get_untracked() > 0 {
                                selection.set(Some(0));
                                id.update_state(ListUpdate::ScrollToSelected);
                            }
                        }
                    }
                    EventPropagation::Stop
                }
                _ => EventPropagation::Continue,
            }
        } else {
            EventPropagation::Continue
        }
    })
}

impl<T> View for VirtualList<T> {
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

impl<T> Widget for VirtualList<T> {
    fn view_data(&self) -> &ViewData {
        &self.data
    }

    fn view_data_mut(&mut self) -> &mut ViewData {
        &mut self.data
    }

    fn for_each_child<'a>(&'a self, for_each: &mut dyn FnMut(&'a dyn Widget) -> bool) {
        for_each(&self.child);
    }

    fn for_each_child_mut<'a>(&'a mut self, for_each: &mut dyn FnMut(&'a mut dyn Widget) -> bool) {
        for_each(&mut self.child);
    }

    fn for_each_child_rev_mut<'a>(
        &'a mut self,
        for_each: &mut dyn FnMut(&'a mut dyn Widget) -> bool,
    ) {
        for_each(&mut self.child);
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "VirtualList".into()
    }

    fn update(&mut self, cx: &mut crate::context::UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(change) = state.downcast::<ListUpdate>() {
            match *change {
                ListUpdate::SelectionChanged => {
                    cx.app_state_mut().request_style_recursive(self.id())
                }
                ListUpdate::ScrollToSelected => {
                    if let Some(index) = self.selection.get_untracked() {
                        self.offsets.with_untracked(|offsets| {
                            if let Some([before, after]) = offsets.get(index..index + 2) {
                                let rect = match self.direction {
                                    VirtualDirection::Vertical => {
                                        Rect::new(0.0, *before, self.child_size.width, *after)
                                    }
                                    VirtualDirection::Horizontal => {
                                        Rect::new(*before, 0.0, *after, self.child_size.height)
                                    }
                                };
                                self.child.id().scroll_to(Some(rect));
                            }
                        });
                    }
                }
            }
        }
    }

    fn compute_layout(&mut self, cx: &mut ComputeLayoutCx) -> Option<Rect> {
        self.child_size = cx
            .app_state
            .get_layout(self.child.id())
            .map(|layout| Size::new(layout.size.width as f64, layout.size.height as f64))
            .unwrap();

        cx.compute_view_layout(&mut self.child)
    }
}
