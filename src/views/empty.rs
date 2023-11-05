use crate::{id::Id, view::View};

pub struct Empty {
    id: Id,
}

pub fn empty() -> Empty {
    Empty { id: Id::next() }
}

impl View for Empty {
    fn id(&self) -> Id {
        self.id
    }
}
