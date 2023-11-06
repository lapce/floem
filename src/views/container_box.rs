use crate::{id::Id, view::View};

/// A wrapper around any type that implements View. See [`container_box`]
pub struct ContainerBox {
    id: Id,
    child: Box<dyn View>,
}

/// A wrapper around any type that implements View.
///
/// Views in Floem are strongly typed. A [`ContainerBox`] allows you to escape the strongly typed View and contain a `Box<dyn View>`.
///
/// ## Bad Example
///```compile_fail
/// use floem::views::*;
/// use floem_reactive::*;
/// let check = true;
///
/// container(|| {
///     if check == true {
///         checkbox(create_rw_signal(true).read_only())
///     } else {
///         label(|| "no check".to_string())
///     }
/// });
/// ```
/// The above example will fail to compile because the container is strongly typed so the if and
/// the else must return the same type. The problem is that checkbox is an [Svg](crate::views::Svg)
/// and the else returns a [Label](crate::views::Label). The solution to this is to use a
/// [`ContainerBox`] to escape the strongly typed requirement.
///
/// ```
/// use floem::views::*;
/// use floem_reactive::*;
/// let check = true;
///
/// if check == true {
///     container_box(checkbox(create_rw_signal(true).read_only()))
/// } else {
///     container_box(label(|| "no check".to_string()))
/// };
/// ```
pub fn container_box(child: impl View + 'static) -> ContainerBox {
    ContainerBox {
        id: Id::next(),
        child: Box::new(child),
    }
}

impl View for ContainerBox {
    fn id(&self) -> Id {
        self.id
    }

    fn for_each_child<'a>(&'a self, for_each: &mut dyn FnMut(&'a dyn View) -> bool) {
        for_each(&self.child);
    }

    fn for_each_child_mut<'a>(&'a mut self, for_each: &mut dyn FnMut(&'a mut dyn View) -> bool) {
        for_each(&mut self.child);
    }

    fn for_each_child_rev_mut<'a>(
        &'a mut self,
        for_each: &mut dyn FnMut(&'a mut dyn View) -> bool,
    ) {
        for_each(&mut self.child);
    }

    fn debug_name(&self) -> std::borrow::Cow<'static, str> {
        "ContainerBox".into()
    }
}
