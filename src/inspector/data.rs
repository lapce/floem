use crate::inspector::CapturedView;
use crate::views::VirtualVector;
use crate::ViewId;
use floem_reactive::{create_rw_signal, RwSignal, SignalGet, SignalUpdate};
use std::ops::AddAssign;
use std::rc::Rc;

#[derive(Clone)]
pub struct CapturedDatas {
    pub root: CapturedData,
    pub focus_line: RwSignal<usize>,
}

impl CapturedDatas {
    pub fn init_from_view(view: Rc<CapturedView>) -> Self {
        let root = CapturedData::init_from_view(view);
        Self {
            root,
            focus_line: create_rw_signal(0),
        }
    }

    pub fn focus(&mut self, id: ViewId) {
        if self.root.focus(id) {
            self.focus_line.set(self.count_line(id));
        }
    }

    fn count_line(&self, id: ViewId) -> usize {
        let mut line = 0;
        self.root.count_line(id, &mut line);
        line
    }

    fn total(&self) -> usize {
        self.root.total()
    }

    fn get_children(
        &self,
        next: &mut usize,
        min: usize,
        max: usize,
        level: usize,
    ) -> Vec<(usize, usize, CapturedData)> {
        self.root.get_children(next, min, max, level)
    }
}
#[derive(Clone, Debug)]
pub enum DataType {
    Internal {
        children: Vec<CapturedData>,
        expanded: RwSignal<bool>,
    },
    Leaf,
}
#[derive(Clone, Debug)]
pub struct CapturedData {
    pub id: ViewId,
    pub view_conf: Rc<CapturedView>,
    pub ty: DataType,
}

impl CapturedData {
    pub fn init_from_view(view: Rc<CapturedView>) -> Self {
        if view.children.is_empty() {
            Self {
                id: view.id,
                view_conf: view,
                ty: DataType::Leaf,
            }
        } else {
            let mut children = Vec::with_capacity(view.children.len());
            for child in &view.children {
                children.push(CapturedData::init_from_view(child.clone()));
            }
            let expanded = create_rw_signal(false);
            Self {
                id: view.id,
                view_conf: view,
                ty: DataType::Internal { children, expanded },
            }
        }
    }
    pub fn count_line(&self, id: ViewId, line: &mut usize) -> bool {
        line.add_assign(1);
        if self.id == id {
            return true;
        }
        match &self.ty {
            DataType::Internal { children, expanded } => {
                if expanded.get_untracked() {
                    for child in children {
                        if child.count_line(id, line) {
                            return true;
                        }
                    }
                }
            }
            DataType::Leaf => {}
        }
        false
    }

    pub fn focus(&mut self, id: ViewId) -> bool {
        if self.id == id {
            return true;
        }
        match &mut self.ty {
            DataType::Internal { children, expanded } => {
                for child in children.iter_mut() {
                    if child.focus(id) {
                        expanded.set(true);
                        return true;
                    }
                }
            }
            DataType::Leaf => {}
        }
        false
    }

    pub fn expanded(&self) -> Option<RwSignal<bool>> {
        match &self.ty {
            DataType::Internal { expanded, .. } => Some(*expanded),
            DataType::Leaf => None,
        }
    }

    /// contain self?
    fn total(&self) -> usize {
        match &self.ty {
            DataType::Internal { expanded, children } => {
                if expanded.get() {
                    let mut total = 1;
                    for child in children {
                        total += child.total();
                    }
                    total
                } else {
                    1
                }
            }
            DataType::Leaf => 1,
        }
    }

    fn get_children(
        &self,
        next: &mut usize,
        min: usize,
        max: usize,
        level: usize,
    ) -> Vec<(usize, usize, CapturedData)> {
        let mut children_data = Vec::new();
        if *next >= min && *next < max {
            children_data.push((*next, level, self.clone()));
        } else if *next >= max {
            return children_data;
        }
        next.add_assign(1);
        match &self.ty {
            DataType::Internal { expanded, children } => {
                if expanded.get() {
                    for child in children {
                        let child_children = child.get_children(next, min, max, level + 1);
                        if !child_children.is_empty() {
                            children_data.extend(child_children);
                        }
                        if *next > max {
                            break;
                        }
                    }
                }
            }
            DataType::Leaf => {}
        }
        children_data
    }
}

impl VirtualVector<(usize, usize, CapturedData)> for CapturedDatas {
    fn total_len(&self) -> usize {
        self.total()
    }

    fn slice(
        &mut self,
        range: std::ops::Range<usize>,
    ) -> impl Iterator<Item = (usize, usize, CapturedData)> {
        let min = range.start;
        let max = range.end;
        let children = self.get_children(&mut 0, min, max, 0);
        children.into_iter()
    }
}
