use std::collections::HashSet;

use super::{
    dyn_stack::{diff, Diff, DiffOpAdd, FxIndexSet, HashRun},
    VirtualVector,
};
use floem_reactive::{as_child_of_current_scope2, create_effect, Scope, WriteSignal};
use peniko::kurbo::{Affine, Rect, Vec2};

use crate::{
    app_state::AppState,
    context::{ComputeLayoutCx, UpdateCx},
    prelude::*,
    prop_extractor,
    style::{RowGap, Style},
    style_class,
    view::{self},
    AnyView, ViewId,
};

pub struct Column<T> {
    title_id: ViewId,
    view: Option<AnyView>,
    func: Box<dyn Fn(&T) -> (AnyView, Scope)>,
}
impl<T: 'static> Column<T> {
    /// a column needs
    /// 1. a header which can be any view
    /// 2. a closure that can generate the data for the column given the row as input
    /// It is assumed that all rows, including the column headers have the same height.
    /// There wil be layout issues if this isn't respected
    pub fn new<V: IntoView>(title: impl IntoView, func: impl Fn(&T) -> V + 'static) -> Self {
        use crate::views::Decorators;
        let title = title.into_view().class(ColumnHeaderClass);
        let title_id = title.id();
        let view_fn = Box::new(as_child_of_current_scope2(move |e| func(e).into_any()));

        Self {
            title_id,
            view: Some(title.into_any()),
            func: view_fn,
        }
    }
}

prop_extractor! {
    TableExtractor {
        pub row_gap: RowGap
    }
}

pub struct Table<T> {
    id: ViewId,
    style: TableExtractor,
    columns: Vec<Column<T>>,
    viewport: Rect,
    set_viewport: WriteSignal<Rect>,
    row_views: Vec<Vec<(ViewId, Scope)>>,
    // before_node: Option<NodeId>,
    before_size: f64,
    content_size: f64,
    row_height: RwSignal<VirtualItemSize<T>>,
    row_h: Option<f64>,
    first_content_id: Option<ViewId>,
}

pub fn table<T, DF, I, KF, K>(data_fn: DF, key_fn: KF) -> Table<T>
where
    T: 'static + std::fmt::Debug,
    DF: Fn() -> I + 'static,
    I: VirtualVector<T>,
    KF: Fn(&T) -> K + 'static,
    K: Eq + std::hash::Hash + 'static,
{
    let id = ViewId::new();
    let (viewport, set_viewport) = create_signal(Rect::ZERO);
    let row_height = RwSignal::new(VirtualItemSize::Assume(None));

    let table = Table {
        id,
        style: Default::default(),
        columns: Vec::new(),
        viewport: Rect::ZERO,
        set_viewport,
        row_views: Vec::new(),
        before_size: 0.0,
        // before_node: None,
        content_size: 0.0,
        row_height,
        row_h: None,
        first_content_id: None,
    };

    create_effect(move |prev| {
        let mut items_vector = data_fn();
        let viewport = viewport.get();
        let viewport_start = viewport.y0;
        let viewport_end = viewport.height() + viewport.y0;
        let mut items = Vec::new();
        let total_num_rows = items_vector.total_len();

        let mut before_size = 0.0;
        let mut content_size = 0.0;
        let mut start_idx = 0;

        row_height.with(|s| match s {
            VirtualItemSize::Fixed(row_height) => {
                let row_height = row_height();
                // Account for header row in viewport calculations

                start_idx = if row_height > 0.0 {
                    (viewport_start / row_height).floor().max(0.0) as usize
                } else {
                    0
                };

                let end_idx = if row_height > 0.0 {
                    (((viewport_end - row_height) / row_height).ceil() as usize).min(total_num_rows)
                } else {
                    usize::MAX
                };

                // Add visible content items
                for item in items_vector.slice(start_idx..end_idx) {
                    items.push(item);
                }

                // before_size represents space before visible items (after header)
                before_size = row_height * (start_idx.min(total_num_rows) as f64);
                // content_size includes header row plus all content rows
                content_size = row_height * (total_num_rows as f64 + 1.);
            }
            VirtualItemSize::Assume(None) => {
                // For initial run, render at least one item
                if total_num_rows > 0 {
                    items.push(items_vector.slice(0..1).next().unwrap());
                    before_size = 0.0;
                    // Add 1 to account for header row
                    content_size = (total_num_rows as f64) * 10.0;
                }
            }
            VirtualItemSize::Assume(Some(row_height)) => {
                // Account for header row in viewport calculations

                start_idx = if *row_height > 0.0 {
                    (viewport_start / row_height).floor().max(0.0) as usize
                } else {
                    0
                };

                let end_idx = if *row_height > 0.0 {
                    (((viewport_end - row_height) / row_height).ceil() as usize).min(total_num_rows)
                } else {
                    usize::MAX
                };

                // Add visible content items
                for item in items_vector.slice(start_idx..end_idx) {
                    items.push(item);
                }

                before_size = row_height * start_idx.min(total_num_rows) as f64;
                // Add 1 to account for header row
                content_size = row_height * (total_num_rows as f64 + 1.);
            }
            VirtualItemSize::Fn(size_fn) => {
                let mut main_axis = 0.0;
                // Start measuring after header height
                let header_height = size_fn(&items_vector.slice(0..1).next().unwrap());
                main_axis += header_height;
                content_size += header_height;

                for (idx, item) in items_vector.slice(0..total_num_rows).enumerate() {
                    let item_height = size_fn(&item);
                    content_size += item_height;

                    if main_axis + item_height < viewport_start {
                        main_axis += item_height;
                        before_size += item_height;
                        start_idx = idx;
                        continue;
                    }

                    if main_axis <= viewport_end {
                        main_axis += item_height;
                        items.push(item);
                    }
                }
            }
        });

        let hashed_items = items.iter().map(&key_fn).collect::<FxIndexSet<_>>();

        let (prev_before_size, prev_content_size, diff) =
            if let Some((prev_before_size, prev_content_size, HashRun(prev_hashes))) = prev {
                let mut diff = diff(&prev_hashes, &hashed_items);
                for added in &mut diff.added {
                    added.view = Some(unsafe { std::ptr::read(&items[added.at]) });
                }
                (prev_before_size, prev_content_size, diff)
            } else {
                let mut diff = Diff::default();
                for (i, item) in items.into_iter().enumerate() {
                    diff.added.push(DiffOpAdd {
                        at: i,
                        view: Some(item),
                    });
                }
                (0.0, 0.0, diff)
            };

        if !diff.is_empty() || prev_before_size != before_size || prev_content_size != content_size
        {
            id.update_state(TableState {
                diff,
                first_idx: start_idx,
                before_size,
                content_size,
            });
        }

        (before_size, content_size, HashRun(hashed_items))
    });

    table
}

struct TableState<T> {
    diff: Diff<T>,
    #[allow(unused)]
    first_idx: usize,
    before_size: f64,
    content_size: f64,
}

impl<T: 'static + std::fmt::Debug> View for Table<T> {
    fn id(&self) -> ViewId {
        self.id
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "Table".into()
    }

    fn style_pass(&mut self, cx: &mut crate::context::StyleCx<'_>) {
        if self.style.read(cx) {
            cx.app_state_mut().request_paint(self.id);
        }
        for child in self.id().children() {
            cx.style_view(child);
        }
    }

    fn view_style(&self) -> Option<Style> {
        use taffy::prelude::*;
        let row_gap = match self.style.row_gap() {
            crate::unit::PxPct::Px(px) => px,
            crate::unit::PxPct::Pct(_) => todo!(),
        };
        Some(
            crate::style::Style::new()
                .grid()
                .min_height(self.content_size)
                .padding_top(self.before_size)
                .margin_top(row_gap)
                .grid_template_columns([repeat(self.columns.len() as u16, [fr(1.)].to_vec())])
                .grid_auto_rows([min_content()]),
        )
    }

    fn layout(&mut self, cx: &mut crate::context::LayoutCx) -> taffy::NodeId {
        cx.layout_node(self.id, true, |cx| {
            let mut nodes = Vec::new();

            let columns = self
                .columns
                .iter()
                .map(|c| c.title_id)
                .collect::<HashSet<_>>();
            for child in self.id.children() {
                if self.first_content_id.is_none() {
                    if !columns.contains(&child) {
                        self.first_content_id = Some(child);
                    }
                }
                let view = child.view();
                let mut view = view.borrow_mut();
                nodes.push(view.layout(cx));
            }
            nodes
        })
    }

    fn compute_layout(&mut self, cx: &mut ComputeLayoutCx) -> Option<Rect> {
        let new_viewport = cx.current_viewport();
        if self.viewport != new_viewport {
            self.viewport = new_viewport;
            self.set_viewport.set(new_viewport);
        }
        let layout = view::default_compute_layout(self.id, cx);

        let new_size = self.row_height.with(|s| match s {
            VirtualItemSize::Assume(None) => {
                if let Some(first_content) = self.first_content_id {
                    let rect = first_content.layout_rect();
                    let row_gap = match self.style.row_gap() {
                        crate::unit::PxPct::Px(px) => px,
                        crate::unit::PxPct::Pct(_) => todo!(),
                    };
                    Some(rect.height() + row_gap)
                } else {
                    None
                }
            }
            _ => None,
        });
        if let Some(new_size) = new_size {
            self.row_h = Some(new_size);
            self.row_height
                .set(VirtualItemSize::Assume(Some(new_size as f64)));
        }

        layout
    }

    fn update(&mut self, cx: &mut UpdateCx, state: Box<dyn std::any::Any>) {
        if let Ok(state) = state.downcast::<TableState<T>>() {
            if !state.diff.is_empty()
                || state.before_size != self.before_size
                || state.content_size != self.content_size
            {
                self.before_size = state.before_size;
                self.content_size = state.content_size;
                self.apply_table_diff(cx.app_state, state.diff);
                self.id.request_all();
            }
        }
    }

    fn paint(&mut self, cx: &mut crate::context::PaintCx) {
        let column_ids = self
            .columns
            .iter()
            .map(|c| c.title_id)
            .collect::<HashSet<_>>();
        let children = self.id.children();
        let layout = self.id.get_layout().unwrap();

        let row_gap = match self.style.row_gap() {
            crate::unit::PxPct::Px(px) => px,
            crate::unit::PxPct::Pct(_) => todo!(),
        };
        let header_rect = Rect::new(
            0.,
            self.viewport.y0 + self.row_h.unwrap() - row_gap,
            layout.size.width as f64,
            self.content_size,
        );

        for child in children {
            if column_ids.contains(&child) {
                let scroll_offset = self.viewport.y0;
                let layout = child.get_layout().unwrap();
                let offset = layout.location;
                cx.save();
                // stick columns to the top by undoing it's y offset and adding just the scroll_offset
                cx.transform *= Affine::translate(Vec2 {
                    x: 0.,
                    y: -offset.y as f64 + scroll_offset,
                });
                cx.paint_view(child);
                cx.restore();
            } else {
                cx.save();
                cx.clip(&header_rect);
                cx.paint_view(child);
                cx.restore();
            }
        }
    }
}

impl<T: 'static + std::fmt::Debug> Table<T> {
    fn apply_table_diff(&mut self, app_state: &mut AppState, mut diff: Diff<T>) {
        // Handle clear operation first
        if diff.clear {
            for row in self.row_views.iter() {
                for (view_id, scope) in row {
                    app_state.remove_view(*view_id);
                    scope.dispose();
                }
            }
            self.row_views.clear();
            diff.removed.clear();
        }

        // Resize the table if needed (pre-allocate space)
        if let Some(size_diff) = diff.added.len().checked_sub(diff.removed.len()) {
            let target_size = self.row_views.len() + size_diff;
            self.row_views.resize_with(target_size, Vec::new);
        }

        // Handle removes
        for DiffOpRemove { at } in &diff.removed {
            if let Some(row) = self.row_views.get_mut(*at) {
                for (view_id, scope) in row.iter() {
                    app_state.remove_view(*view_id);
                    scope.dispose();
                }
                row.clear();
            }
        }

        // Store moves to apply later to prevent overwrites
        let mut moves_to_apply = Vec::with_capacity(diff.moved.len());
        for DiffOpMove { from, to } in diff.moved {
            if let Some(row) = self.row_views.get_mut(from) {
                let row_views = std::mem::take(row);
                moves_to_apply.push((to, row_views));
            }
        }

        // Handle adds
        for DiffOpAdd { at, view } in diff.added {
            if let Some(item) = view {
                let mut row_views = Vec::with_capacity(self.columns.len());

                // Create views for each column
                for column in &self.columns {
                    let (view, scope) = (column.func)(&item);
                    let view = view.class(CellClass);
                    let id = view.id();
                    id.set_view(view);
                    id.set_parent(self.id);
                    row_views.push((id, scope));
                }

                if at < self.row_views.len() {
                    self.row_views[at] = row_views;
                }
            }
        }

        // Apply stored moves
        for (to, row_views) in moves_to_apply {
            self.row_views[to] = row_views;
        }

        // Clean up empty rows
        self.row_views.retain(|row| !row.is_empty());

        // Update children IDs
        let mut all_view_ids =
            Vec::with_capacity(self.columns.len() + (self.row_views.len() * self.columns.len()));

        // Add column headers
        all_view_ids.extend(self.columns.iter().map(|col| col.title_id));

        // Add all column views for each row
        for row in &self.row_views {
            all_view_ids.extend(row.iter().map(|(id, _)| *id));
        }

        self.id.set_children_ids(all_view_ids);
        self.id.request_layout();
    }

    pub fn column<V: IntoView + 'static>(
        mut self,
        title: impl IntoView,
        func: impl Fn(&T) -> V + 'static,
    ) -> Self {
        let mut column = Column::new(title, func);
        let view = column.view.take().unwrap();
        self.id.add_child(view);
        self.columns.push(column);
        self
    }

    pub fn columns<I: Iterator<Item = Column<T>>>(mut self, columns: I) -> Self {
        let id = self.id;
        self.columns.extend(columns.into_iter().map(|mut c| {
            let view = c.view.take().unwrap();
            id.add_child(view);
            c
        }));
        self
    }
}

style_class!(pub ColumnHeaderClass);
style_class!(pub CellClass);
