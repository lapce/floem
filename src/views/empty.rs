use crate::{
    id::Id,
    view::{View, ViewData, Widget},
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

    fn build(self) -> Box<dyn Widget> {
        Box::new(self)
    }
}

impl Widget for Empty {
    fn view_data(&self) -> &ViewData {
        &self.data
    }

    fn view_data_mut(&mut self) -> &mut ViewData {
        &mut self.data
    }
}
