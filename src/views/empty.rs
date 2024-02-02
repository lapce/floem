use crate::{
    id::Id,
    view::{View, ViewData},
};

pub struct Empty {
    data: ViewData,
}

pub fn empty() -> Empty {
    Empty {
        data: ViewData::new(Id::next()),
    }
}

impl View for Empty {
    fn view_data(&self) -> &ViewData {
        &self.data
    }

    fn view_data_mut(&mut self) -> &mut ViewData {
        &mut self.data
    }
}
