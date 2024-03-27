use crate::{
    id::Id,
    view::{View, ViewData, Widget},
};

/// An empty View. See [`empty`].
pub struct Empty {
    data: ViewData,
}

/// An empty View. This view can still have a size, background, border radius, and outline.
///
/// This view can also be useful if you have another view that requires a child element but there is not a meaningful child element that needs to be provided.
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

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "Empty".into()
    }
}
